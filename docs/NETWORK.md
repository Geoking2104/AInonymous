# Spécification Réseau — Canal QUIC Inter-Nœuds

> Plan de données : transport des activations tensorielles, tokens et streams entre nœuds du mesh.

---

## Principe Dual-Canal

AInonymous sépare strictement les deux plans réseau :

```
PLAN DE CONTRÔLE                    PLAN DE DONNÉES
────────────────                    ────────────────
Holochain / iroh (DHT)              QUIC mTLS direct (iroh-net)

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

**Librairie** : `iroh-net` (Rust) — le même transport qu'utilise Holochain 0.6.1 pour le DHT, exposé directement pour les connexions de données.

**iroh-net** fournit :
- QUIC sur UDP avec NAT traversal automatique (hole punching STUN/TURN)
- **TLS 1.3 obligatoire** avec authentification mutuelle (mTLS) basée sur les clés ed25519 Holochain
- Multiplexage de streams sur une même connexion QUIC
- 0-RTT reconnexion entre pairs déjà connus

**Pourquoi QUIC et pas TCP ou WebRTC :**
- TCP : pas de multiplexing natif, head-of-line blocking sur les gros tenseurs
- WebRTC : overhead de signaling, conçu pour media temps-réel
- QUIC : streams indépendants, contrôle de congestion, 0-RTT, idéal pour gros transferts binaires

---

## mTLS QUIC — Authentification Mutuelle

> **Problème résolu** : l'ancienne implémentation (`ainonymous-quic`) utilisait des certificats auto-signés avec vérification TLS désactivée (`dangerous_accept_any_server_cert`). C'est corrigé.

### Principe

Chaque nœud présente son **AgentPubKey Holochain (ed25519)** comme certificat TLS lors de l'établissement QUIC. Les deux extrémités vérifient mutuellement l'identité de leur pair avant d'échanger des activations :

```
Nœud A                              Nœud B
  │                                    │
  │──── ClientHello (TLS 1.3) ────────►│
  │     + certificat ed25519(A)         │
  │                                    │
  │◄─── ServerHello + cert ed25519(B) ─│
  │                                    │
  │ Vérifications mutuelles :           │
  │ A vérifie : cert(B) == AgentPubKey(B) connu dans le DHT
  │ B vérifie : cert(A) == AgentPubKey(A) connu dans le DHT
  │ + B vérifie le session_token reçu via Holochain
  │                                    │
  │◄══ QUIC chiffré mTLS établi ══════►│
```

### Implémentation

```rust
use iroh_net::tls::Keypair;

// Côté Nœud A (initiateur)
pub async fn connect_quic_mtls(
    local_key: &ed25519_dalek::SigningKey,
    remote_pubkey: &AgentPubKey,
    remote_addr: SocketAddr,
    session_token: &[u8; 32],
) -> anyhow::Result<quinn::Connection> {
    // Construire le certificat TLS depuis la clé ed25519 Holochain
    let keypair = Keypair::from_ed25519_bytes(local_key.to_bytes())?;

    let endpoint = iroh_net::Endpoint::builder()
        .secret_key(keypair)
        // Vérification stricte du pair distant
        .tls_client_config(build_mtls_client_config(remote_pubkey)?)
        .bind()
        .await?;

    let connection = endpoint
        .connect(remote_addr, &remote_pubkey.into_node_id())
        .await?;

    // Envoyer le session_token pour authentifier cette session spécifique
    let mut auth_stream = connection.open_uni().await?;
    auth_stream.write_all(session_token).await?;
    auth_stream.finish().await?;

    Ok(connection)
}

fn build_mtls_client_config(
    expected_peer: &AgentPubKey,
) -> anyhow::Result<rustls::ClientConfig> {
    let verifier = PeerKeyVerifier::new(expected_peer.clone());
    Ok(rustls::ClientConfig::builder()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth()) // l'auth client est faite via le keypair iroh
}

// Vérificateur de clé pair (remplace le danger accept-any)
struct PeerKeyVerifier {
    expected_pubkey: AgentPubKey,
}

impl rustls::client::ServerCertVerifier for PeerKeyVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _server_name: &rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        // Extraire la clé publique ed25519 du certificat iroh
        let peer_pubkey = extract_ed25519_from_cert(end_entity)?;
        // Comparer avec l'AgentPubKey attendue (obtenue via Holochain DHT)
        if peer_pubkey != self.expected_pubkey.get_raw_32() {
            return Err(rustls::Error::General(
                "Clé publique pair ne correspond pas à l'AgentPubKey DHT".into()
            ));
        }
        Ok(rustls::client::ServerCertVerified::assertion())
    }
}

// Côté Nœud B (listener)
pub async fn accept_quic_mtls(
    local_key: &ed25519_dalek::SigningKey,
    expected_session_token: &[u8; 32],
    expected_caller: &AgentPubKey,
) -> anyhow::Result<quinn::Connection> {
    let keypair = Keypair::from_ed25519_bytes(local_key.to_bytes())?;

    let endpoint = iroh_net::Endpoint::builder()
        .secret_key(keypair)
        .tls_server_config(build_mtls_server_config()?)
        .bind()
        .await?;

    let incoming = endpoint.accept().await
        .ok_or(anyhow::anyhow!("Pas de connexion entrante"))?;
    let connection = incoming.await?;

    // Vérifier l'identité du pair appelant
    let peer_pubkey = extract_peer_pubkey(&connection)?;
    if peer_pubkey != expected_caller.get_raw_32() {
        connection.close(1u32.into(), b"unauthorized peer");
        anyhow::bail!("Pair non autorisé");
    }

    // Vérifier le session_token
    let mut auth_stream = connection.accept_uni().await?;
    let mut token = [0u8; 32];
    auth_stream.read_exact(&mut token).await?;
    if &token != expected_session_token {
        connection.close(2u32.into(), b"invalid session token");
        anyhow::bail!("Session token invalide");
    }

    Ok(connection)
}
```

---

## Attestation des Nœuds côté Transport

Avant toute connexion QUIC, le coordinateur vérifie l'attestation du nœud worker dans le DHT Holochain :

```rust
pub async fn verify_node_before_connect(
    node: &AgentPubKey,
    required_model: &str,
) -> anyhow::Result<()> {
    // 1. Vérifier NodeAttestation récente (< 24h) dans le DHT
    let attestation = get_node_attestation(node).await?
        .ok_or(anyhow::anyhow!("Pas d'attestation pour ce nœud"))?;

    if attestation.age_hours() > 24 {
        anyhow::bail!("Attestation expirée ({} h)", attestation.age_hours());
    }

    // 2. Vérifier la signature de l'attestation
    verify_ed25519_signature(
        node,
        &attestation.attestation_signature,
        &attestation.content_bytes(),
    )?;

    // 3. Vérifier que le nœud a un ModelClaim valide pour ce modèle
    let claim = get_model_claim(node, required_model).await?
        .ok_or(anyhow::anyhow!("Nœud n'a pas ce modèle attesté"))?;

    if !claim.verified_locally {
        anyhow::bail!("Modèle non vérifié localement par le nœud");
    }

    // 4. Vérifier absence de warrant actif
    let warrants = get_active_warrants(node).await?;
    if !warrants.is_empty() {
        anyhow::bail!("{} warrant(s) actif(s) sur ce nœud", warrants.len());
    }

    Ok(())
}
```

---

## Cycle de Vie d'une Session QUIC

### 1. Négociation (via Holochain)

```
Nœud A (coordinateur)                    Nœud B (worker)
       │                                       │
       │── call_remote() Holochain ───────────►│
       │   "inference-mesh::negotiate_quic"    │
       │   {request_id, layer_range: 24-47}    │
       │                                       │── ouvre QUIC mTLS listener
       │                                       │   sur port éphémère UDP
       │◄── QuicSessionOffer ─────────────────│
       │   {endpoint: "203.0.113.42:54xxx",   │
       │    session_token: [32 bytes],         │
       │    expires_in: 30s}                   │
```

### 2. Vérifications pré-connexion

```
A vérifie (via DHT Holochain) :
  ✅ NodeAttestation de B valide et récente (< 24h)
  ✅ ModelClaim de B pour le modèle demandé
  ✅ Aucun warrant actif sur B
  ✅ Heartbeat récent de B (< 60s)
→ Si une vérification échoue : sélection d'un autre nœud
```

### 3. Établissement QUIC mTLS

```rust
// A se connecte à B avec authentification mutuelle ed25519
let connection = connect_quic_mtls(&agent_key, &node_b_pubkey, offer.endpoint, &offer.session_token).await?;
```

### 4. Transfer des Activations

```rust
// Format du stream d'activations (header 64 bytes + body tenseur)
struct ActivationHeader {
    request_id: [u8; 36],
    layer_start: u32,
    layer_end: u32,
    seq_len: u32,
    hidden_size: u32,
    dtype: u8,          // 0=f32, 1=f16, 2=bf16
    compression: u8,    // 0=none, 1=zstd
    _reserved: [u8; 14],
}
// Body : tenseur [seq_len × hidden_size] en dtype spécifié
// Taille typique : 2048 × 5120 × 2 bytes (bf16) = 20 MB
```

### 5. Stream de Tokens

```rust
// Stream séparé sur la même connexion QUIC (multiplexing)
// Format NDJSON compatible OpenAI streaming
// {"id":"chatcmpl-X","choices":[{"delta":{"content":"Le "}}]}
```

### 6. Fermeture et Métriques

```rust
connection.close(0u32.into(), b"done");

// Métriques publiées sur Holochain
call_zome("inference-mesh", "publish_metrics", InferenceMetrics {
    request_id,
    total_latency_ms,
    tokens_per_second,
    nodes_used: 2,
    success: true,
    ..
}).await?;
```

---

## Compression des Activations

| Modèle | Séq. | Taille brute (bf16) | Zstd-1 | Gain | Latence compression |
|---|---|---|---|---|---|
| Gemma4-31B | 512 | 5 MB | 2.1 MB | 58% | ~3ms |
| Gemma4-31B | 2048 | 20 MB | 7.8 MB | 61% | ~12ms |
| Gemma4-26B-MoE | 2048 | 16 MB | 6.2 MB | 61% | ~9ms |
| Gemma4-E4B | 512 | 1.4 MB | 0.6 MB | 57% | ~1ms |

**Décision automatique** : compression activée si bande passante estimée < 1 Gbps.

---

## NAT Traversal

iroh-net gère le NAT traversal via **hole punching** :

```
1. Les deux nœuds annoncent leurs IP publiques dans le DHT Holochain
2. iroh-net tente connexion UDP directe (hole punching)
   → succès dans ~85% des cas
3. Si échec : relay QUIC iroh (open source, opérés par la communauté)
   → performance dégradée mais fonctionnel
```

**Ports** : éphémères UDP par session — pas de port fixe en firewall (sauf relay forcé).

---

## Gestion des Échecs

### Scénarios

| Scénario | Détection | Action |
|---|---|---|
| B inaccessible (QUIC timeout) | 5s timeout connexion | Holochain → nœud alternatif |
| Attestation B expirée | Vérif DHT pré-connexion | Exclusion de ce nœud, sélection alternative |
| mTLS : clé pair incorrecte | Erreur TLS au handshake | Connexion refusée + warrant potentiel |
| Stream coupé (transfert partiel) | EOF inattendu | Abandon requête + métriques failure |
| Session token expiré (> 30s) | Rejet côté B | Re-négocier via Holochain |
| B OOM pendant calcul | Signal Holochain NodeError | B publie heartbeat load=1.0 → exclu futures |
| Réseau lent (> 10s) | Stream timeout | Abandon + log métriques |

```rust
const QUIC_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const QUIC_STREAM_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_ACTIVATION_SIZE: usize = 512 * 1024 * 1024;  // 512 MB max

match timeout(QUIC_CONNECT_TIMEOUT, connect_quic_mtls(...)).await {
    Ok(Ok(conn)) => conn,
    _ => {
        report_node_failure(offer.node_id, "quic_connect_failed").await?;
        let new_node = call_zome("agent-registry", "get_fallback_node", model_id).await?;
        connect_quic_mtls(...new_node...).await?
    }
}
```

---

## Sécurité du Canal QUIC

```
Chiffrement      : TLS 1.3 obligatoire — activations chiffrées en transit
Auth mutuelle    : ed25519 Holochain des deux côtés — plus de certificats auto-signés
Token session    : 32 bytes aléatoires via Holochain — empêche connexions non sollicitées
Pas de stockage  : activations jamais écrites sur disque — pas de fuite post-inférence
```

Ce que voit un observateur réseau :
- Connexion QUIC chiffrée entre deux IP
- Pas de contenu lisible (TLS 1.3)
- Pas d'identification du modèle ou du prompt
- Taille du transfert visible uniquement

---

## Configuration Daemon Local

```toml
# ~/.config/ainonymous/config.toml

[network]
holochain_port = 8888
holochain_app_port = 8889

[quic]
bind_addr = "0.0.0.0:0"           # port éphémère automatique
relay_fallback = true              # relay si NAT symétrique
max_session_duration_seconds = 120
mtls_strict = true                 # TOUJOURS true — ne jamais désactiver

[quic.compression]
mode = "auto"                      # "auto" | "zstd" | "none"
threshold_gbps = 1.0               # compresser si bande < 1 Gbps

[quic.limits]
max_activation_size_mb = 512
max_concurrent_sessions = 4

[inference]
llama_server_port = 9337
llama_server_host = "127.0.0.1"
api_port = 9337
metrics_port = 9338                # endpoint Prometheus

[audit]
enabled = true
interval_hours = 6
auto_warrant = true
```
