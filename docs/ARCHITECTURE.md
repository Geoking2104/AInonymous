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
│  Holochain / kitsune2          │  QUIC direct inter-nœuds       │
│  (signaling, routing, état)    │  (activations tensorielles)    │
├────────────────────────────────┴────────────────────────────────┤
│  COUCHE RÉSEAU P2P                                              │
│  DHT Holochain (kitsune2) | Source Chains | Validation          │
├─────────────────────────────────────────────────────────────────┤
│  COUCHE CALCUL                                                  │
│  llama.cpp | llama-server :9337 | GGUF models (Gemma 4, etc.)  │
└─────────────────────────────────────────────────────────────────┘

Principe dual-canal :
  • Holochain gère TOUT le plan de contrôle (qui fait quoi, état, métriques, Blackboard)
  • QUIC direct gère le plan de données (activations tensorielles volumineuses, tokens en stream)
  → Holochain n'est jamais le goulot d'étranglement pour le volume de données
```

---

## 2. Mapping mesh-llm → Holochain

### 2.1 Découverte de pairs

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
    pub gpu_vendor: String,          // "apple", "nvidia", "amd", "cpu"
    pub supported_backends: Vec<String>, // ["metal", "cuda", "vulkan", "cpu"]
    pub loaded_models: Vec<ModelRef>,
    pub max_concurrent_requests: u8,
    pub region_hint: Option<String>, // "eu-west", "us-east", etc.
    pub availability_score: u8,      // 0-100, mis à jour périodiquement
}

#[hdk_entry_helper]
pub struct ModelRef {
    pub model_id: String,        // "gemma4-31b", "gemma4-26b-moe"
    pub model_hash: Vec<u8>,     // SHA256 du fichier GGUF
    pub layer_range: Option<(u32, u32)>, // couches gérées par ce nœud
    pub expert_ids: Option<Vec<u32>>,    // pour MoE : experts hébergés
}
```

### 2.2 Transport et Communication — Dual-Canal

**Règle fondamentale :**
> Holochain transporte le **plan de contrôle**. QUIC direct transporte le **plan de données**.

Les activations tensorielles entre couches peuvent dépasser 500 MB par requête sur des modèles comme Gemma 4-31B — les faire transiter par le DHT Holochain serait impraticable. On sépare donc strictement les deux flux :

```
Plan de Contrôle (Holochain / kitsune2)          Plan de Données (QUIC direct)
─────────────────────────────────────────         ──────────────────────────────
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
  │◄── Nœud B répond: {quic_addr: "203.0.113.42:54321", session_token: "..."}
  │
  │ 2. Connexion QUIC directe A → B (iroh-net, NAT traversal)
  │    Stream: activations couches 0-23 (tenseurs float16, ~120MB)
  │
  │◄── Stream QUIC B → A: activations couches 24-47 + logits finaux
  │
  │ 3. call_remote() Holochain → "inference-mesh::publish_metrics"
  │    Métriques de la requête → DHT
```

**Avantages du dual-canal :**
- Holochain ne transporte jamais de blobs > quelques KB
- QUIC sur iroh-net gère le NAT traversal et la chiffrement de bout en bout
- Les activations ne passent jamais par le DHT → pas de fuite de données d'inférence
- Failover : si QUIC direct échoue, repli sur un autre nœud via Holochain (pas de retry QUIC)

### 2.3 Distribution de modèles et Pipeline-Splitting

**mesh-llm**
- Modèles denses : pipeline-splitting par couches entre nœuds
- MoE (Mixtral, GLM, Qwen3) : sharding par experts (tronc entier + sous-ensemble experts)
- Experts critiques répliqués partout ; autres distribués uniquement
- Chargement zéro-transfert : lecture depuis fichiers GGUF locaux

**AInonymous**

Même principe, coordonné via Holochain :

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

**Pour Gemma 4 MoE (gemma4-26b-moe, 26B params, ~4B actifs)**

```
Architecture MoE Gemma 4 :
  - 30 blocs Transformer
  - Chaque bloc : 1 expert dense (toujours actif) + N experts sparse
  - Routeur sélectionne K experts par token

Distribution AInonymous :
  Nœud A : tronc complet + experts 0-15 (critiques : tous les nœuds)
  Nœud B : tronc complet + experts 16-30
  Nœud C : tronc complet + experts critiques seulement (standby)

  Experts "critiques" = top-10% fréquence d'activation (statistiques pre-calculées)
```

### 2.4 Blackboard — Collaboration d'Agents

**mesh-llm**
- Gossip éphémère (48h de persistance)
- Conventions de préfixes : `STATUS:`, `FINDING:`, `QUESTION:`, `TIP:`, `DONE:`
- Nettoyage automatique PII (chemins, clés, secrets)
- Propagation limitée aux nœuds du mesh

**AInonymous (DNA `blackboard`)**
- Entrées DHT persistantes (pas éphémères) avec TTL configurable
- Même système de préfixes, validé au niveau zome
- Recherche textuelle locale rapide (index BTreeMap en mémoire)
- Nettoyage PII : validation déterministe dans l'integrity zome
- Propagation contrôlée par membrane proof (même hApp uniquement)

```rust
#[hdk_entry_helper]
pub struct BlackboardEntry {
    pub prefix: BlackboardPrefix,   // STATUS | FINDING | QUESTION | TIP | DONE
    pub content: String,             // max 4096 chars, PII-stripped
    pub tags: Vec<String>,           // pour la recherche
    pub ttl_hours: u32,              // 48h par défaut, configurable
    pub author: AgentPubKey,         // anonymisé si privacy_mode
    pub timestamp: Timestamp,
}

pub enum BlackboardPrefix {
    Status,
    Finding,
    Question,
    Tip,
    Done,
    Custom(String),
}
```

### 2.5 Rééquilibrage et Failover

**mesh-llm**
- Nœuds standby se promeuvent si modèle non servi ou surchargé
- Hôtes morts remplacés en 60 secondes
- Évolutivité passive : sync gossip GPU

**AInonymous**
- Monitoring de disponibilité via **liens Holochain** (heartbeat toutes les 30s)
- Nœud mort détecté : lien `NodeStatus::Offline` publié dans le DHT
- Promotion standby : `claim_model_slot()` zome call avec preuve de capacité
- Délai cible : < 30 secondes (vs 60s mesh-llm)

```rust
// Heartbeat : publié par chaque nœud toutes les 30s
pub fn publish_heartbeat(input: HeartbeatInput) -> ExternResult<()> {
    let heartbeat = NodeHeartbeat {
        agent: agent_info()?.agent_latest_pubkey,
        timestamp: sys_time()?,
        current_load: input.current_load,    // 0.0 - 1.0
        available_slots: input.available_slots,
    };
    create_entry(EntryTypes::NodeHeartbeat(heartbeat))?;
    Ok(())
}
```

### 2.6 Décodage Spéculatif

**mesh-llm**
- Modèle brouillon local propose des tokens
- Vérification en une passe regroupée par le modèle principal
- Gain mesuré : +38% de débit sur génération de code

**AInonymous**
- Nœud **draft** : Gemma 4-E4B (5GB VRAM, rapide)
- Nœud **verify** : Gemma 4-31B ou 26B-MoE (puissant)
- Coordination via `submit_draft_tokens()` zome call
- Le nœud verify accepte/rejette les tokens proposés en batch

---

## 3. Sécurité et Réputation

### Modèle de confiance

Contrairement à mesh-llm qui n'a pas de système de réputation formalisé, AInonymous exploite les primitives cryptographiques Holochain :

```
Membrane Proofs
  └── Contrôle d'accès au réseau (whitelist de clés, invitation)

Source Chain
  └── Historique immuable de chaque nœud (pas de révision possible)

Validation Zome
  └── Règles déterministes vérifiées par tous les pairs

Warrants
  └── Preuve cryptographique de comportement invalide
  └── Éjection automatique des nœuds malveillants
```

### Règles de validation clés

```rust
// Integrity Zome : inference-mesh
pub fn validate(op: Op) -> ExternResult<ValidateCallbackResult> {
    match op.flattened::<EntryTypes, LinkTypes>()? {
        FlatOp::StoreEntry(store_entry) => match store_entry {
            OpEntry::CreateEntry { app_entry, .. } => match app_entry {
                EntryTypes::InferenceRequest(req) => {
                    // Vérifier format de la requête
                    validate_inference_request(req)
                },
                EntryTypes::NodeCapabilities(caps) => {
                    // VRAM déclarée doit être > 0 et < limite physique raisonnable
                    if caps.vram_gb <= 0.0 || caps.vram_gb > 512.0 {
                        return Ok(ValidateCallbackResult::Invalid(
                            "VRAM hors plage valide".into()
                        ));
                    }
                    Ok(ValidateCallbackResult::Valid)
                },
                _ => Ok(ValidateCallbackResult::Valid),
            },
        },
        _ => Ok(ValidateCallbackResult::Valid),
    }
}
```

---

## 4. Flux d'Inférence Complet (Dual-Canal)

```
1. CLIENT → POST /v1/chat/completions {"model":"gemma4-31b", "stream":true}

2. PROXY LOCAL :9337
   └── parse → appelle conducteur Holochain

━━━━━━━━━━━━━ PLAN DE CONTRÔLE (Holochain) ━━━━━━━━━━━━━

3. zome "router" → query_available_nodes(gemma4-31b)
   └── DHT lookup NodeCapabilities + heartbeats récents
   └── sélectionne Nœud A (couches 0-23), Nœud B (24-47)
   └── compute_execution_plan() → ExecutionPlan

4. call_remote(nœud_B, "negotiate_quic_session")
   └── Nœud B ouvre listener QUIC sur port éphémère
   └── retourne {quic_addr, session_token, layer_range: 24-47}

━━━━━━━━━━━━━ PLAN DE DONNÉES (QUIC direct) ━━━━━━━━━━━━━

5. Nœud A → llama-server local
   └── tokenize prompt + embedding
   └── forward pass couches 0-23
   └── activations shape [seq_len, 5120] float16 (~120MB pour seq_len=1024)

6. Nœud A ──QUIC──► Nœud B
   └── stream binaire : activations couches 0-23
   └── Nœud B reçoit → forward pass couches 24-47
   └── logits finaux

7. Nœud B ──QUIC──► Nœud A
   └── stream tokens décodés (si non-streaming) ou
   └── stream SSE tokens au fil de la génération (si streaming)

8. Nœud A → Proxy → CLIENT (SSE stream)

━━━━━━━━━━━━━ POST-INFÉRENCE (Holochain) ━━━━━━━━━━━━━━━

9. Nœud A : publish_metrics() → DHT
   └── {request_id, latency_ms, tokens/s, nodes_used}

10. (Optionnel agent) : blackboard_post("STATUS: gemma4-31b 340ms 37tok/s")
```

**Volumes de données typiques (Gemma 4-31B, seq_len=2048) :**

| Segment | Taille estimée | Canal |
|---|---|---|
| Prompt tokenisé | < 1 KB | Holochain (contrôle) |
| Activations couche 0→24 | ~240 MB | QUIC direct |
| Activations couche 24→48 | ~240 MB | QUIC direct |
| Logits finaux (vocab 256K) | ~1 MB | QUIC direct |
| Tokens générés (stream) | < 10 KB | QUIC direct |
| Métriques post-inférence | < 1 KB | Holochain (DHT) |

---

## 5. Stack Technologique

| Composant | Technologie | Version cible |
|---|---|---|
| Runtime P2P | Holochain | 0.4.x |
| Langage zomes | Rust + HDK | Rust 1.80+ |
| Inférence locale | llama.cpp | latest |
| Format modèles | GGUF | v3 |
| LLM principal | Gemma 4 | 31B / 26B-MoE |
| Agent orchestration | Goose (Block) | 1.x |
| Protocole agent | MCP (Anthropic) | spec mars 2025 |
| Transport P2P | kitsune2 / iroh-net | avec Holochain 0.4 |
| API client | OpenAI-compatible | v1 |
| Langage CLI | Rust | 1.80+ |
| Interface web | React + TypeScript | - |
