# Architecture Technique — AInonymous

> Mapping exhaustif mesh-llm (anarchai.org) → implémentation Holochain

---

## 1. Vue d'ensemble : Couches du système

```
┌─────────────────────────────────────────────────────────────────┐
│  COUCHE APPLICATION                                             │
│  Goose Agent | CLI ainonymous | API REST OpenAI-compat          │
├─────────────────────────────────────────────────────────────────┤
│  COUCHE COORDINATION (hApp ainonymous-core)                     │
│  DNA inference-mesh | DNA agent-registry | DNA blackboard       │
├────────────────────────────────┬────────────────────────────────┤
│  TRANSPORT CONTRÔLE            │  TRANSPORT DONNÉES             │
│  Holochain / kitsune2 / iroh   │  QUIC mTLS inter-nœuds         │
│  (signaling, routing, état)    │  (activations tensorielles)    │
├────────────────────────────────┴────────────────────────────────┤
│  COUCHE RÉSEAU P2P                                              │
│  DHT Holochain (iroh) | Source Chains | Warrants | Attestation  │
├─────────────────────────────────────────────────────────────────┤
│  COUCHE CALCUL                                                  │
│  llama.cpp | llama-server :9337 | GGUF models (Gemma 4, etc.)  │
└─────────────────────────────────────────────────────────────────┘

Principe dual-canal :
  • Holochain gère TOUT le plan de contrôle (qui fait quoi, état, métriques, Blackboard)
  • QUIC mTLS direct gère le plan de données (activations tensorielles, tokens en stream)
  → Holochain n'est jamais le goulot d'étranglement pour le volume de données
```

---

## 2. Identité et Clés

### 2.1 Modèle d'identité

Chaque nœud AInonymous est identifié par une paire de clés **ed25519** gérée par Holochain :

```
AgentPubKey (ed25519, 32 bytes)
  └── Identité permanente du nœud dans tous les DHTs
  └── Réutilisée comme clé TLS pour les connexions QUIC (iroh-net)
  └── Signe toutes les entrées de la source chain

AgentSecretKey (ed25519, 32 bytes — jamais exposée)
  └── Stockée dans le keystore Holochain (lair-keystore)
  └── Dérivation des sous-clés de session QUIC via HKDF-SHA256
  └── Jamais transmise sur le réseau
```

**Propriétés de l'identité :**
- **Persistante** : la clé ne change pas même si le nœud redémarre ou change d'IP
- **Auto-certifiante** : l'identité est la clé publique elle-même — pas de CA tiers
- **Liée à l'historique** : la source chain est signée par cette clé — une identité compromise est détectable
- **Pseudonyme** : l'AgentPubKey n'est pas liée à une identité réelle par défaut

### 2.2 Bootstrap et Découverte Initiale

#### Réseau public (mode par défaut)

```
1. Nouveau nœud génère sa paire ed25519 via lair-keystore
2. Se connecte au bootstrap server Holochain public (bootstrap.holo.host)
   → Obtient la liste des pairs actifs pour cette hApp (DNA hash)
3. Rejoint le DHT en téléchargeant les entrées de ses voisins
4. Publie ses NodeCapabilities dans le DHT
```

#### Bootstrap privé (mode consortium / entreprise)

Pour les déploiements privés, AInonymous supporte un bootstrap totalement isolé :

```toml
# ~/.config/ainonymous/config.toml
[network]
bootstrap_mode = "private"
bootstrap_urls = [
  "https://bootstrap.internal.example.com:8888",
]
# Liste blanche de clés publiques autorisées
trusted_agents = [
  "uhCAkXxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
  "uhCAkYyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy",
]
```

```rust
// Membrane proof pour bootstrap privé
// La preuve est vérifiée par genesis_self_check sur tous les pairs
#[derive(Serialize, Deserialize, Debug)]
pub struct PrivateNetworkProof {
    pub network_id: String,        // identifiant du réseau privé
    pub issued_at: Timestamp,      // timestamp d'émission
    pub issued_to: AgentPubKey,    // clé du nouvel agent
    pub signature: Vec<u8>,        // signé par l'admin du réseau
    pub admin_pubkey: AgentPubKey, // clé publique de l'admin
}

#[hdk_extern]
pub fn genesis_self_check(data: GenesisSelfCheckData) -> ExternResult<ValidateCallbackResult> {
    #[cfg(feature = "private-network")]
    {
        let proof: PrivateNetworkProof = data.membrane_proof
            .ok_or(wasm_error!(WasmErrorInner::Guest("Membrane proof requise".into())))?
            .try_into()?;

        let admin_keys = get_authorized_admin_keys();
        if !admin_keys.contains(&proof.admin_pubkey) {
            return Ok(ValidateCallbackResult::Invalid("Admin inconnu".into()));
        }
        verify_signature(proof.admin_pubkey.clone(), proof.signature.clone(), &proof)?;
        if proof.issued_to != data.agent_key {
            return Ok(ValidateCallbackResult::Invalid("Proof émise pour un autre agent".into()));
        }
        // Expiration 24h
        let age = sys_time()?.checked_sub(proof.issued_at)
            .ok_or(wasm_error!(WasmErrorInner::Guest("overflow".into())))?;
        if age > Duration::from_secs(86400) {
            return Ok(ValidateCallbackResult::Invalid("Membrane proof expirée".into()));
        }
        Ok(ValidateCallbackResult::Valid)
    }
    #[cfg(not(feature = "private-network"))]
    {
        Ok(ValidateCallbackResult::Valid)
    }
}
```

---

## 3. Mapping mesh-llm → Holochain

### 3.1 Découverte de pairs

**mesh-llm (anarchai.org)**
- Publication sur relais Nostr (`--auto` avec scoring régional + VRAM)
- Sonde de santé avant adhésion
- Scoring : correspondance régionale, VRAM disponible, latence

**AInonymous (Holochain)**
- Découverte via le DHT Holochain — aucun relais tiers requis
- Chaque nœud publie une **entrée `NodeCapabilities`** dans sa source chain
- Le DHT propage ces entrées aux pairs — découverte passive et continue
- Structure de l'entrée :

```rust
#[hdk_entry_helper]
pub struct NodeCapabilities {
    pub agent_pub_key: AgentPubKey,
    pub vram_gb: f32,
    pub ram_gb: f32,
    pub gpu_vendor: String,
    pub supported_backends: Vec<String>, // ["metal", "cuda", "vulkan", "cpu"]
    pub loaded_models: Vec<ModelRef>,
    pub max_concurrent_requests: u8,
    pub region_hint: Option<String>,
    pub availability_score: u8,          // 0-100
}

#[hdk_entry_helper]
pub struct ModelRef {
    pub model_id: String,
    pub model_hash: Vec<u8>,             // SHA256 du fichier GGUF
    pub layer_range: Option<(u32, u32)>,
    pub expert_ids: Option<Vec<u32>>,    // pour MoE
}
```

### 3.2 Transport et Communication — Dual-Canal

**Règle fondamentale :**
> Holochain transporte le **plan de contrôle**. QUIC mTLS direct transporte le **plan de données**.

```
Plan de Contrôle (Holochain / iroh)              Plan de Données (QUIC mTLS direct)
─────────────────────────────────────────         ──────────────────────────────────
• Découverte de pairs                             • Activations tensorielles
• Annonce de capacités (NodeCapabilities)         • Tokens en streaming
• Plan d'exécution (layer assignments)            • Draft tokens (spéculatif)
• Heartbeats et monitoring                        • Embeddings d'entrée
• Métriques post-inférence                        • Logits de sortie
• Blackboard (agents)                             • Chunks KV-cache (futur)
• Validation et réputation
```

**Séquence d'une requête pipeline-split :**

```
Nœud A (coordinateur + couches 0-23)
  │
  │ 1. call_remote() Holochain → Nœud B
  │    "inference-mesh::negotiate_quic_session"
  │    Payload: {request_id, layer_range: 24-47, quic_port: 54321}
  │
  │◄── Nœud B répond: {quic_addr, session_token, mtls_cert_hash}
  │
  │ 2. Connexion QUIC mTLS directe A → B (iroh-net, NAT traversal)
  │    Authentification mutuelle ed25519 : les deux nœuds se vérifient mutuellement
  │    Stream: activations couches 0-23 (tenseurs float16, ~120MB)
  │
  │◄── Stream QUIC B → A: activations couches 24-47 + logits finaux
  │
  │ 3. call_remote() Holochain → "inference-mesh::publish_metrics"
```

### 3.3 Distribution de modèles et Pipeline-Splitting

```
Requête: gemma4-31b (48 couches totales, 3 nœuds disponibles)

Nœud A (24GB VRAM) ──► couches 0-23  (tronc + embedding)
Nœud B (20GB VRAM) ──► couches 24-40
Nœud C (12GB VRAM) ──► couches 41-47 (+ lm_head)

Coordination : DNA inference-mesh, zome "coordinator"
  - query_available_nodes() → liste depuis DHT
  - compute_layer_assignment(nodes, model) → plan d'exécution
  - execute_pipeline(plan, prompt) → scatter/gather entre nœuds
```

**Pour Gemma 4 MoE (gemma4-26b-moe) :**

```
Nœud A : tronc complet + experts 0-15 (critiques : tous les nœuds)
Nœud B : tronc complet + experts 16-30
Nœud C : tronc complet + experts critiques seulement (standby)

Experts "critiques" = top-10% fréquence d'activation (statistiques pré-calculées)
```

### 3.4 Blackboard — Collaboration d'Agents

```rust
#[hdk_entry_helper]
pub struct BlackboardEntry {
    pub prefix: BlackboardPrefix,    // STATUS | FINDING | QUESTION | TIP | DONE
    pub content: String,              // max 4096 chars, PII-stripped
    pub tags: Vec<String>,
    pub ttl_hours: u32,               // 48h par défaut
    pub author: AgentPubKey,
    pub timestamp: Timestamp,
}
```

### 3.5 Rééquilibrage et Failover

- Monitoring via heartbeats Holochain (toutes les 30s)
- Nœud mort → lien `NodeStatus::Offline` publié dans le DHT
- Promotion standby : `claim_model_slot()` avec preuve de capacité
- Délai cible : < 30 secondes (vs 60s mesh-llm)

### 3.6 Décodage Spéculatif

- Nœud **draft** : Gemma 4-E4B (5GB VRAM, rapide)
- Nœud **verify** : Gemma 4-31B ou 26B-MoE
- Coordination via `submit_draft_tokens()` zome call
- Gain mesuré : +38% de débit sur génération de code

---

## 4. Attestation des Nœuds

### 4.1 Preuve de capacité

Avant d'être sélectionné pour servir des requêtes, un nœud doit fournir une **attestation vérifiable** de ses capacités réelles :

```rust
#[hdk_entry_helper]
pub struct NodeAttestation {
    pub agent: AgentPubKey,
    pub timestamp: Timestamp,
    pub hardware_fingerprint: HardwareFingerprint,
    pub benchmark_results: BenchmarkResults,
    pub attestation_signature: Vec<u8>,  // signé par la clé ed25519 du nœud
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HardwareFingerprint {
    pub gpu_uuid: Option<String>,     // UUID GPU si disponible (NVIDIA/AMD)
    pub metal_device_id: Option<u64>, // Apple Silicon device ID
    pub vram_total_bytes: u64,
    pub ram_total_bytes: u64,
    pub cpu_cores: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BenchmarkResults {
    pub model_id: String,
    pub tokens_per_second: f32,       // mesuré sur prompt de référence
    pub ttft_ms: u32,                 // time-to-first-token
    pub benchmark_prompt_hash: Vec<u8>, // SHA256 du prompt de benchmark (standardisé)
    pub measured_at: Timestamp,
}
```

### 4.2 Vérification croisée des attestations

```
1. Nœud A publie son NodeAttestation dans sa source chain
2. Le coordinateur (Nœud requérant) vérifie :
   - Signature ed25519 valide (key = AgentPubKey du nœud)
   - Benchmark récent (< 24h)
   - Résultats cohérents avec capacités annoncées
3. Si incohérence détectée → warrant publié
```

---

## 5. Validation des Modèles

### 5.1 Manifeste de modèle

```rust
#[hdk_entry_helper]
pub struct ModelManifest {
    pub model_id: String,
    pub model_hash: Vec<u8>,             // SHA256 du fichier GGUF (32 bytes)
    pub huggingface_repo: Option<String>,
    pub expected_size_bytes: u64,
    pub quantization: String,
    pub architecture: String,            // "llama", "gemma", "qwen3"
    pub context_length: u32,
    pub layer_count: u32,
    pub hidden_size: u32,
    pub published_by: AgentPubKey,
    pub verified_by: Vec<AgentPubKey>,   // pairs ayant validé ce hash
}

#[hdk_entry_helper]
pub struct ModelClaim {
    pub manifest_hash: ActionHash,
    pub node: AgentPubKey,
    pub verified_locally: bool,          // le nœud a vérifié le hash localement
    pub layer_range: Option<(u32, u32)>,
    pub expert_ids: Option<Vec<u32>>,
    pub timestamp: Timestamp,
}
```

### 5.2 Processus de validation

```
1. Nœud publie ModelManifest avec hash SHA256 du GGUF
2. ≥ 2 autres nœuds vérifient le hash indépendamment
   → Publient chacun une ModelVerification{manifest_hash, verified: true}
3. Quand ≥ 2 vérifications valides → modèle "certifié" dans le DHT
4. En cas de discordance → warrant publié, nœud mis en quarantaine
```

---

## 6. Observabilité

### 6.1 Pipeline de métriques

```
Nœud local
  ├── Métriques temps-réel (en mémoire) : tokens/s, TTFT, load GPU, VRAM libre
  ├── Métriques DHT (publiées toutes les 5 min) : InferenceMetrics aggregées
  └── Traces distribuées (OpenTelemetry, optionnel) : spans par requête → OTLP
```

### 6.2 Snapshot observabilité DHT

```rust
#[hdk_entry_helper]
pub struct NodeObservabilitySnapshot {
    pub agent: AgentPubKey,
    pub timestamp: Timestamp,
    pub period_minutes: u32,
    pub requests_served: u32,
    pub requests_failed: u32,
    pub avg_latency_ms: u32,
    pub p95_latency_ms: u32,
    pub avg_tokens_per_second: f32,
    pub total_tokens_generated: u64,
    pub uptime_percent: f32,
    pub avg_gpu_utilization: f32,
    pub avg_vram_used_gb: f32,
    pub active_models: Vec<String>,
}
```

### 6.3 Endpoint Prometheus local

```
GET http://localhost:9338/metrics

ainonymous_requests_total{model="gemma4-31b",status="success"} 1243
ainonymous_latency_seconds{quantile="0.95"} 2.341
ainonymous_tokens_per_second{model="gemma4-31b"} 34.7
ainonymous_vram_used_bytes{device="gpu0"} 18253611008
```

### 6.4 Traçabilité distribuée

Chaque requête porte un `request_id` (UUID v4) propagé sur tous les nœuds du pipeline. Les spans OpenTelemetry sont transmis dans les headers QUIC et dans les `call_remote()` Holochain.

---

## 7. Anti-Sybil et Sécurité du Réseau

### 7.1 Menaces et mitigations

| Menace | Description | Mitigation |
|---|---|---|
| Sybil | N faux nœuds pour dominer le DHT | PoW léger à l'admission + réputation temporelle |
| Free-rider | Consomme sans contribuer | Score contribution < seuil → exclusion douce |
| Model spoofing | Annonce modèle non détenu | ModelManifest + vérification croisée pairs |
| Inference poisoning | Réponses altérées | NofMQuorum + vérification croisée |
| Eclipse attack | Isoler un nœud de ses vrais pairs | Bootstrap multi-sources, rotation DHT |
| Prompt injection Blackboard | Entrée malveillante influence agents | Validation PII + longueur dans integrity zome |

### 7.2 Warrants Holochain

```
Pair A observe entrée invalide de Nœud B
  → Génère Warrant{accused: B, evidence: ActionHash, rule: "hash mismatch", sig: A}
  → Publie dans le DHT → visible par tous
  → Conséquence : B exclu des plans d'exécution, score de réputation dégradé
  → B peut publier une réfutation signée pour contester
```

### 7.3 Proof of Work à l'admission (mode privé optionnel)

```rust
pub struct AdmissionPoW {
    pub agent_key: AgentPubKey,
    pub nonce: u64,
    pub difficulty: u8,    // nombre de zéros en tête du SHA256
    pub hash: Vec<u8>,     // SHA256(agent_key || nonce)
}
```

### 7.4 Score de réputation

```
Score = (success_rate × 0.4) + (speed_score × 0.3) + (age_bonus × 0.2) - (warrant_penalty × 0.1)
```

---

## 8. Redondance d'Inférence

Quatre modes disponibles selon le niveau de confiance requis :

### 8.1 PrimaryShadow

```
Primary + Shadow exécutent en parallèle
Si Primary OK → résultat retourné, Shadow annulé
Si Primary échoue → basculement instantané vers Shadow
Coût : 2× VRAM
```

### 8.2 HotStandby

```
Primary exécute, Standby en attente
Si Primary échoue → Standby démarre depuis zéro
Coût : 1× VRAM supplémentaire (en veille)
```

### 8.3 NofMQuorum

```
N nœuds exécutent indépendamment
Résultat retourné quand M nœuds s'accordent (ex: 2/3)
Détecte les nœuds malveillants ou défaillants par divergence
Idéal pour requêtes à haute valeur
```

```rust
pub struct NofMQuorumPlan {
    pub nodes: Vec<AgentPubKey>,
    pub quorum: usize,             // M = seuil de consensus
    pub agreement_metric: AgreementMetric,
}

pub enum AgreementMetric {
    ExactTokenMatch,
    TopKOverlap { k: usize },
    SemanticSimilarity { threshold: f32 },
}
```

### 8.4 SpeculativeVerify (Décodage Spéculatif)

```
Draft  (Gemma 4-E4B)  → propose K tokens rapidement
Verify (Gemma 4-31B)  → valide/rejette en une passe
Gain mesuré           : +38% de débit sur génération de code
```

### 8.5 Sélection automatique du mode

```rust
pub enum RedundancyLevel {
    None,          // Solo — maximum throughput
    Failover,      // PrimaryShadow — haute disponibilité
    HighIntegrity, // NofMQuorum 2/3 — anti-poisoning
    Throughput,    // SpeculativeVerify — maximum débit
}
```

---

## 9. Sécurité et Réputation

### Modèle de confiance

```
Membrane Proofs  → Contrôle d'accès (whitelist, invitation, PoW)
Source Chain     → Historique immuable par nœud
Validation Zome  → Règles déterministes vérifiées par tous les pairs
Warrants         → Preuve cryptographique de comportement invalide → éjection
```

---

## 10. Flux d'Inférence Complet (Dual-Canal)

```
1. CLIENT → POST /v1/chat/completions {"model":"gemma4-31b", "stream":true}
2. PROXY LOCAL :9337 → conducteur Holochain

━━━━━━━━━━━━━ PLAN DE CONTRÔLE (Holochain) ━━━━━━━━━━━━━

3. zome "router" → query_available_nodes(gemma4-31b)
   └── sélectionne Nœud A (couches 0-23), Nœud B (24-47)

4. call_remote(B, "negotiate_quic_session")
   └── B ouvre listener QUIC mTLS sur port éphémère
   └── retourne {quic_addr, session_token}

━━━━━━━━━━━━━ PLAN DE DONNÉES (QUIC mTLS) ━━━━━━━━━━━━━━

5. A → llama-server local → forward pass couches 0-23
   └── activations [seq_len, 5120] float16 (~120MB pour seq=1024)

6. A ──QUIC mTLS──► B → forward pass couches 24-47 → logits

7. B ──QUIC mTLS──► A → tokens / SSE stream → CLIENT

━━━━━━━━━━━━━ POST-INFÉRENCE (Holochain) ━━━━━━━━━━━━━━━

8. A : publish_metrics() → DHT {request_id, latency_ms, tokens/s}
9. (optionnel) blackboard_post("STATUS: gemma4-31b 340ms 37tok/s")
```

**Volumes typiques (Gemma 4-31B, seq=2048) :**

| Segment | Taille | Canal |
|---|---|---|
| Prompt tokenisé | < 1 KB | Holochain |
| Activations couche 0→24 | ~240 MB | QUIC |
| Activations couche 24→48 | ~240 MB | QUIC |
| Logits finaux (vocab 256K) | ~1 MB | QUIC |
| Métriques post-inférence | < 1 KB | Holochain |

---

## 11. Stratégie de Migration

### 11.1 Migration Holochain 0.4.x → 0.6.1

Holochain 0.6.1 est la version cible (HDK 0.6.1, HDI 0.7.x). Changements clés :
- **Transport** : iroh remplace kitsune2 comme défaut en 0.6.1 (0-RTT, meilleure performance)
- **Warrants** : API stabilisée en 0.6
- **Données DHT** : format d'entrée inchangé — migration transparente

```
Phase 1 — Nœuds pilotes (semaines 1-2)
  • 10% du réseau migre vers 0.6.1
  • Validation : warrants OK, DHT sync correcte

Phase 2 — Déploiement progressif (semaines 3-6)
  • 50% migre — nœuds 0.4 et 0.6 coexistent temporairement
  • Les nœuds 0.4 restent fonctionnels mais reçoivent moins de nouvelles requêtes

Phase 3 — Finalisation (semaine 7+)
  • 100% migration — nœuds 0.4 exclus du routage
```

### 11.2 Versioning des entrées

```rust
#[hdk_entry_helper]
pub struct NodeCapabilities {
    pub schema_version: u8,  // incrémenté à chaque breaking change
    // ...
}
```

### 11.3 Rollback

```bash
ainonymous daemon stop
cargo install ainonymous-daemon --version 0.4.x
ainonymous daemon start
# Les données DHT restent intactes — resync depuis les pairs
```

---

## 12. Processus d'Audit Holochain

### 12.1 Audit des nœuds

```bash
# Auditer un nœud spécifique
ainonymous audit node uhCAkXxx...
# ✅ 1243 entrées valides
# ⚠️  2 warrants reçus
# 📊 34.7 tok/s, 99.1% uptime (30 jours)
```

### 12.2 Audit des modèles

```bash
ainonymous audit model gemma4-31b
# Hash de référence : sha256:abc123...
# Nœuds concordants : 47/48 (97.9%)
# ⚠️  Nœud uhCAkYyy... hash différent → warrant publié automatiquement
```

### 12.3 Cycle de vie d'un warrant

```
DÉTECTION → GÉNÉRATION (signé par pair détectant) → PUBLICATION (DHT)
  → PROPAGATION (vérification par tous les pairs)
  → EFFET (exclusion du routage, score dégradé)
  → CONTESTATION optionnelle (réfutation signée)
  → EXPIRATION (30 jours par défaut) → réhabilitation progressive
```

### 12.4 Audit automatique

```toml
[audit]
enabled = true
interval_hours = 6
auto_warrant = true           # publier automatiquement les warrants détectés
model_hash_check = true       # vérifier les hashes annoncés par les pairs
reputation_threshold = 0.5    # exclure les nœuds avec score < 0.5
```

---

## 13. Stack Technologique

| Composant | Technologie | Version cible |
|---|---|---|
| Runtime P2P | Holochain | **0.6.1** |
| Langage zomes | Rust + HDK | HDK 0.6.1 / HDI 0.7.x |
| Inférence locale | llama.cpp | latest |
| Format modèles | GGUF | v3 |
| LLM principal | Gemma 4 | 31B / 26B-MoE |
| Agent orchestration | Goose (Block) | 1.x |
| Protocole agent | MCP (Anthropic) | spec mars 2025 |
| Transport P2P | iroh (via Holochain 0.6) | 0.6.1 |
| Transport données | QUIC mTLS (iroh-net) | avec Holochain 0.6 |
| API client | OpenAI-compatible | v1 |
| Observabilité | OpenTelemetry | OTLP |
| Métriques locales | Prometheus-compatible | :9338/metrics |
| Langage CLI | Rust | 1.80+ |
| Interface web | React + TypeScript | — |
