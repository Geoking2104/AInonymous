# Spécification Réseau — Canal QUIC Inter-Nœuds

> Plan de données : transport des activations tensorielles, tokens et streams entre nœuds du mesh.

---

## Principe Dual-Canal

AInonymous sépare strictement les deux plans réseau :

```
PLAN DE CONTRÔLE                    PLAN DE DONNÉES
────────────────                    ────────────────
Holochain / kitsune2                QUIC direct (iroh-net)

• Découverte de pairs               • Activations tensorielles
• Négociation de sessions           • Tokens générés (stream)
• Routage et planification          • Embeddings d'entrée
• Métriques et monitoring           • Logits de sortie
• Blackboard                        • Draft tokens (spéculatif)
• État global DHT                   • KV-cache (futur)

Latence : 50-500ms acceptable       Latence : < 5ms requis
Volume : < 10 KB par message        Volume : 10 MB — 500 MB par requête
```

---

## Stack QUIC

**Librairie** : `iroh-net` (Rust) — la même que Holochain utilise pour kitsune2, mais exposée directement pour les connexions de données.

**iroh-net** fournit :
- QUIC sur UDP avec NAT traversal automatique (hole punching)
- Chiffrement TLS 1.3 de bout en bout (clés ed25519 Holochain réutilisées)
- Multiplexage de streams sur une même connexion
- 0-RTT reconnexion entre pairs déjà connus

**Pourquoi QUIC et pas TCP ou WebRTC :**
- TCP : pas de multiplexing natif, head-of-line blocking sur les gros tenseurs
- WebRTC : overhead de signaling, conçu pour media temps-réel pas pour blobs tensoriels
- QUIC : streams indépendants, contrôle de congestion, 0-RTT, idéal pour gros transferts binaires

---

## Cycle de Vie d'une Session QUIC

### 1. Négociation (via Holochain)

```
Nœud A (coordinateur)                    Nœud B (worker)
       │                                       │
       │── call_remote() Holochain ───────────►│
       │   "inference-mesh::negotiate_quic"    │
       │   {request_id, layer_range: 24-47}    │
       │                                       │── ouvre QUIC listener
       │                                       │   sur port éphémère UDP
       │◄── QuicSessionOffer ─────────────────│
       │   {endpoint: "203.0.113.42:54xxx",   │
       │    session_token: [32 bytes],         │
       │    expires_in: 30s}                   │
```

### 2. Établissement QUIC (direct, sans Holochain)

```rust
// Côté Nœud A (initiateur)
let endpoint = iroh_net::Endpoint::builder()
    .secret_key(holochain_agent_secret_key())  // clé ed25519 de l'agent Holochain
    .bind()
    .await?;

let connection = endpoint
    .connect(node_b_addr, node_b_public_key)
    .await?;

// Authentifier la session avec le token négocié via Holochain
let auth_stream = connection.open_uni().await?;
auth_stream.write_all(&session_token).await?;
```

```rust
// Côté Nœud B (listener)
let incoming = endpoint.accept().await?;
let connection = incoming.await?;

// Vérifier token de session
let auth_stream = connection.accept_uni().await?;
let token = read_exact(auth_stream, 32).await?;
if token != expected_session_token {
    connection.close(1u32.into(), b"invalid session token");
    return Err(anyhow!("Session token invalide"));
}
```

### 3. Transfer des Activations

```rust
// Format du stream d'activations
// Header (fixe, 64 bytes)
struct ActivationHeader {
    request_id: [u8; 36],          // UUID
    layer_start: u32,
    layer_end: u32,
    seq_len: u32,                  // longueur de séquence
    hidden_size: u32,              // taille cachée (ex: 5120 pour gemma4-31b)
    dtype: u8,                     // 0=f32, 1=f16, 2=bf16
    compression: u8,               // 0=none, 1=zstd
    _reserved: [u8; 14],
}

// Body : tenseur [seq_len × hidden_size] en dtype spécifié
// Taille typique : 2048 × 5120 × 2 bytes (bf16) = 20 MB

// Côté Nœud A : envoyer activations
let mut send_stream = connection.open_uni().await?;
send_stream.write_all(&header.to_bytes()).await?;
if header.compression == 1 {
    let compressed = zstd::encode_all(&activations_bytes, 1)?;  // niveau 1 : vitesse
    send_stream.write_all(&compressed).await?;
} else {
    send_stream.write_all(&activations_bytes).await?;
}
send_stream.finish().await?;

// Côté Nœud B : recevoir et traiter
let mut recv_stream = connection.accept_uni().await?;
let header = ActivationHeader::from_bytes(&read_exact(&mut recv_stream, 64).await?);
let body = recv_stream.read_to_end(MAX_ACTIVATION_SIZE).await?;
let activations = if header.compression == 1 {
    zstd::decode_all(&body[..])?
} else { body };
// → passer à llama-server via shared memory ou stdin
```

### 4. Stream de Tokens (génération)

```rust
// Stream séparé sur la même connexion QUIC (multiplexing)
// Nœud final → Nœud coordinateur → Client SSE

// Format : NDJSON compatible OpenAI streaming
// {"id":"chatcmpl-X","choices":[{"delta":{"content":"Le "}}]}
// {"id":"chatcmpl-X","choices":[{"delta":{"content":"sharding"}}]}
// {"id":"chatcmpl-X","choices":[{"finish_reason":"stop"}]}

let mut token_stream = connection.open_uni().await?;
for token in generated_tokens {
    let chunk = serde_json::to_vec(&ChatCompletionChunk::from(token))?;
    token_stream.write_all(&(chunk.len() as u32).to_le_bytes()).await?;
    token_stream.write_all(&chunk).await?;
}
token_stream.finish().await?;
```

### 5. Fermeture

```rust
// Nœud A ferme la connexion proprement après réception complète
connection.close(0u32.into(), b"done");

// Métriques publiées sur Holochain (plan de contrôle)
call_zome("inference-mesh", "publish_metrics", InferenceMetrics {
    request_id,
    total_latency_ms: elapsed.as_millis() as u32,
    tokens_per_second: tokens_count as f32 / elapsed.as_secs_f32(),
    nodes_used: 2,
    model_id: "gemma4-31b".into(),
    success: true,
    error_reason: None,
}).await?;
```

---

## Compression des Activations

Les activations tensorielles peuvent être significativement compressées car elles contiennent des patterns répétitifs :

| Modèle | Séq. | Taille brute (bf16) | Zstd-1 | Gain | Latence compression |
|---|---|---|---|---|---|
| Gemma4-31B | 512 | 5 MB | 2.1 MB | 58% | ~3ms |
| Gemma4-31B | 2048 | 20 MB | 7.8 MB | 61% | ~12ms |
| Gemma4-26B-MoE | 2048 | 16 MB | 6.2 MB | 61% | ~9ms |
| Gemma4-E4B | 512 | 1.4 MB | 0.6 MB | 57% | ~1ms |

**Décision automatique** : compression activée si bande passante estimée < 1 Gbps entre les nœuds.

```rust
fn should_compress(node_a: &NodeInfo, node_b: &NodeInfo) -> bool {
    let bandwidth_estimate = estimate_bandwidth(node_a, node_b);
    bandwidth_estimate < 1_000_000_000  // < 1 Gbps
}
```

---

## NAT Traversal

iroh-net gère le NAT traversal automatiquement via **hole punching** (STUN/TURN). La séquence :

```
1. Les deux nœuds connaissent leurs IP publiques
   (obtenues via le DHT Holochain au moment de l'annonce de capabilities)

2. iroh-net tente la connexion directe UDP (hole punching)
   → succès dans ~85% des cas (NAT symétrique → échec)

3. Si échec direct : iroh-net utilise un relay QUIC
   → relay public iroh (open source, opérés par la communauté)
   → performance dégradée mais fonctionnel

4. Résultats enregistrés localement pour optimiser les futures connexions
```

**Ports utilisés :**
- Port éphémère UDP pour chaque session QUIC de données
- Le port est communiqué via Holochain (`QuicSessionOffer.endpoint`)
- Pas de port fixe à ouvrir en firewall (sauf si relay forcé)

---

## Gestion des Échecs

### Timeout et retry

```rust
const QUIC_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const QUIC_STREAM_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_ACTIVATION_SIZE: usize = 512 * 1024 * 1024;  // 512 MB max

// Si connexion QUIC échoue → signaler via Holochain et prendre un autre nœud
match timeout(QUIC_CONNECT_TIMEOUT, connect_quic(&offer)).await {
    Ok(Ok(conn)) => conn,
    Ok(Err(e)) | Err(_) => {
        // Publier échec sur Holochain
        report_node_failure(offer.node_id, "quic_connect_failed").await?;
        // Re-router vers un autre nœud disponible
        let new_node = call_zome("agent-registry", "get_fallback_node", model_id).await?;
        connect_quic_to(new_node).await?
    }
}
```

### Scénarios d'échec

| Scénario | Détection | Action |
|---|---|---|
| Nœud B inaccessible (QUIC timeout) | 5s timeout connexion | Holochain → nœud alternatif |
| Stream coupé en cours (transfert partiel) | EOF inattendu | Abandon requête + métriques failure |
| Session token expiré (> 30s) | Rejet côté B | Re-négocier via Holochain |
| Nœud B OOM pendant calcul | Signal Holochain NodeError | Nœud B publie heartbeat load=1.0, exclu des futures requêtes |
| Réseau lent (latence > 10s) | Stream timeout | Abandon + log dans métriques |

---

## Sécurité du Canal QUIC

Même en réseau public, le canal QUIC est sécurisé :

- **Chiffrement** : TLS 1.3 obligatoire (intégré dans QUIC) — les activations sont chiffrées en transit
- **Authentification mutuelle** : les deux nœuds s'authentifient avec leur clé ed25519 Holochain (la même clé que leur AgentPubKey)
- **Token de session** : 32 bytes aléatoires négociés via Holochain — empêche les connexions non sollicitées même si l'endpoint QUIC est connu
- **Pas de stockage** : les activations ne sont jamais écrites sur disque, uniquement en mémoire → pas de fuite post-inférence

```
Ce que voit un observateur réseau :
  • Connexion QUIC chiffrée entre deux IP
  • Pas de contenu lisible (TLS 1.3)
  • Pas d'identification du modèle ou du prompt
  • Taille du transfert visible (mais pas le contenu)
```

---

## Configuration Daemon Local

Le daemon `ainonymous` gère le canal QUIC en parallèle du conducteur Holochain :

```toml
# ~/.config/ainonymous/config.toml

[network]
# Holochain conductor
holochain_port = 8888
holochain_app_port = 8889

# QUIC data plane
quic_bind_addr = "0.0.0.0:0"      # port éphémère automatique
quic_relay_fallback = true         # utiliser relay si NAT symétrique
max_session_duration_seconds = 120

# Compression des activations
activation_compression = "auto"   # "auto" | "zstd" | "none"
compression_threshold_gbps = 1.0  # activer si bande passante estimée < 1 Gbps

# Limites
max_activation_size_mb = 512
max_concurrent_quic_sessions = 4

[inference]
llama_server_port = 9337
llama_server_host = "127.0.0.1"
api_port = 9337                    # endpoint OpenAI-compat exposé au client
```
