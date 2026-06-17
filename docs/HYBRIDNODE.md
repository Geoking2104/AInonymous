# HybridNode — Spécification Technique

> Architecture commune : SD-WAN (underlay) + Holochain (overlay) + QUIC/mTLS (plan de données)

---

## 1. Concept

**HybridNode** est un mode de déploiement commun à tous les projets du portfolio (AInonymous, et futurs projets). Il n'est pas un remplacement d'Holochain ou du SD-WAN : c'est le collage entre les deux.

```
┌─────────────────────────────────────────────────────────────────────┐
│  COUCHE APPLICATION                                                  │
│  AInonymous daemon | Agents Goose | API REST OpenAI-compat           │
├─────────────────────────────────────────────────────────────────────┤
│  COUCHE OVERLAY (Holochain)                                          │
│  Identité ed25519 | DHT | Validation | Audit | Warrants | Blackboard │
├──────────────────────────────┬──────────────────────────────────────┤
│  PLAN DE DONNÉES (QUIC/mTLS) │  PLAN DE CONTRÔLE (Holochain DHT)   │
│  Activations tensorielles    │  Découverte, coordination, métriques  │
│  Tokens, Embeddings, Logits  │  Heartbeats, Warrants, Attestation    │
├──────────────────────────────┴──────────────────────────────────────┤
│  COUCHE UNDERLAY (SD-WAN)                                            │
│  Routage WAN | QoS | Failover | Tunnels chiffrés | Politiques sites  │
├─────────────────────────────────────────────────────────────────────┤
│  RÉSEAU PHYSIQUE                                                      │
│  MPLS | Internet | 4G/5G | fibres dédiées                            │
└─────────────────────────────────────────────────────────────────────┘
```

**Principe de séparation des responsabilités :**

| Couche | Responsabilité | Technologie |
|---|---|---|
| SD-WAN (underlay) | Connectivité WAN, QoS, failover, tunnels | Cisco vEdge / VeloCloud / Fortinet / open-source |
| Holochain (overlay) | Identité, coordination, validation, audit | Holochain 0.6.1 + iroh |
| QUIC/mTLS (data plane) | Flux lourds directs entre nœuds | iroh-net, quinn |
| HybridNode (scheduler) | Décisions de routage tenant compte des 3 couches | `hybridnode-core` (Rust) |

---

## 2. Spécifications Techniques

### 2.1 SD-WAN Underlay

Le SD-WAN fournit les primitives de transport que HybridNode exploite :

```
Primitives SD-WAN exposées à HybridNode :
  • get_site_topology()     → carte des sites et liens disponibles
  • get_link_sla()          → latence, bande passante, jitter, perte paquets par lien
  • set_traffic_policy()    → classifier/prioriser les flux QUIC d'inférence
  • get_peer_reachability() → quels nœuds sont joignables depuis ce site
  • get_local_site_id()     → identifiant du site SD-WAN local
```

**SLA requis pour l'inférence distribuée :**

| Métrique | Intra-site | Inter-sites (même région) | Inter-régions |
|---|---|---|---|
| Latence max (p95) | < 2ms | < 20ms | < 80ms |
| Bande passante min | > 10 Gbps | > 1 Gbps | > 100 Mbps |
| Jitter max | < 0.5ms | < 5ms | < 20ms |
| Perte paquets max | < 0.01% | < 0.1% | < 0.5% |

### 2.2 Scheduler Locality-Aware

Le scheduler HybridNode sélectionne les nœuds en tenant compte simultanément de la topologie SD-WAN et des capacités Holochain :

```rust
pub struct SchedulingContext {
    pub request: InferenceRequest,
    pub available_nodes: Vec<NodeInfo>,      // depuis Holochain DHT
    pub site_topology: SiteTopology,          // depuis SD-WAN API
    pub link_sla: HashMap<(SiteId, SiteId), LinkSla>, // depuis SD-WAN API
    pub local_site: SiteId,
}

pub enum SchedulingStrategy {
    /// Préférer nœuds sur le même site SD-WAN (latence minimale)
    LocalFirst,
    /// Équilibrer entre latence et capacité de calcul
    BalancedQos,
    /// Maximiser le débit quel que soit le site
    MaxThroughput,
    /// Respecter un budget de latence strict
    LatencyBudget { max_ms: u32 },
}

pub fn schedule(ctx: &SchedulingContext, strategy: SchedulingStrategy) -> ExecutionPlan {
    match strategy {
        SchedulingStrategy::LocalFirst => schedule_local_first(ctx),
        SchedulingStrategy::BalancedQos => schedule_balanced(ctx),
        SchedulingStrategy::MaxThroughput => schedule_max_throughput(ctx),
        SchedulingStrategy::LatencyBudget { max_ms } => schedule_latency_budget(ctx, max_ms),
    }
}

fn schedule_local_first(ctx: &SchedulingContext) -> ExecutionPlan {
    // 1. Grouper les nœuds par site SD-WAN
    let by_site = group_by_site(&ctx.available_nodes, &ctx.site_topology);
    
    // 2. Prioriser le site local
    let local_nodes = by_site.get(&ctx.local_site).cloned().unwrap_or_default();
    
    // 3. Si le site local a assez de VRAM → plan local complet
    if total_vram(&local_nodes) >= model_vram_requirement(&ctx.request.model_id) {
        return build_pipeline_plan(&local_nodes, &ctx.request.model_id);
    }
    
    // 4. Compléter avec des nœuds voisins (meilleur SLA d'abord)
    let mut sorted_sites = sort_sites_by_sla(&by_site, &ctx.link_sla, &ctx.local_site);
    for (site_id, nodes) in sorted_sites {
        // Ajouter nœuds jusqu'à VRAM suffisante
        // Vérifier que le lien SD-WAN respecte le SLA activation-transfer
    }
    // ...
    ExecutionPlan::default()
}
```

### 2.3 Gestion des Modèles

Pour un déploiement multi-sites, les modèles doivent être répliqués intelligemment :

```
Stratégie de réplication des modèles GGUF :

Site A (hub, 80 Gbps inter-sites) :
  • Modèles lourds complets : gemma4-31b, qwen3-72b
  • Modèles MoE : gemma4-26b-moe (tronc + tous experts)

Site B (spoke, 1 Gbps vers hub) :
  • Modèles légers : gemma4-e4b, gemma4-e2b, qwen3-8b
  • Modèles lourds : couches 0-24 seulement (pipeline-split avec hub)

Site C (edge, 100 Mbps vers hub) :
  • Modèles légers uniquement : gemma4-e4b
  • Pas de pipeline-split (bande passante insuffisante pour les activations)

Règle : activation_size_mb / bandwidth_mbps < latency_budget_ms / 1000
→ Gemma4-31B (240MB activations) sur lien 100 Mbps → 19.2s → inacceptable
→ Gemma4-31B sur lien 1 Gbps → 1.9s → borderline (acceptable pour non-stream)
→ Gemma4-E4B (30MB activations) sur lien 100 Mbps → 2.4s → acceptable
```

### 2.4 Politiques SD-WAN pour l'inférence

Les flux QUIC d'activation doivent être traités prioritairement par le SD-WAN :

```yaml
# Politique SD-WAN pour trafic AInonymous/HybridNode
traffic-policy:
  name: ainonymous-inference
  classifier:
    - match:
        protocol: udp
        port-range: 49152-65535        # ports éphémères QUIC
        dscp: 46                        # Expedited Forwarding (marquage par ainonymous-daemon)
      action:
        queue: priority-high
        bandwidth-guarantee: 500mbps
        latency-target: 20ms
    - match:
        protocol: tcp
        destination-port: 8888-8889    # Holochain conductor + app port
      action:
        queue: standard
        bandwidth-guarantee: 10mbps
    - match:
        protocol: tcp
        destination-port: 9337-9338    # llama-server + métriques
      action:
        queue: standard
        bandwidth-guarantee: 5mbps
```

### 2.5 Bootstrap et Découverte dans un Contexte SD-WAN

```
Sans HybridNode (internet public) :
  Nœud A → bootstrap.holo.host → liste des pairs → DHT

Avec HybridNode (SD-WAN privé) :
  Nœud A → bootstrap.internal.sdwan:8888 → liste des pairs connus sur le SD-WAN
  → DHT privé restreint aux sites SD-WAN autorisés
  → Membrane proof = PrivateNetworkProof signée par l'admin du réseau SD-WAN
```

---

## 3. Architecture d'Ensemble — Illustration

```
                    ┌─────────────────────────────────────────┐
                    │          RÉSEAU SD-WAN (underlay)        │
                    │                                          │
   ┌────────────┐   │  ┌─────────┐   Lien WAN   ┌─────────┐  │  ┌────────────┐
   │  Site HQ   │   │  │ vEdge A │◄────────────►│ vEdge B │  │  │  Site Edge │
   │            │   │  └────┬────┘   1 Gbps     └────┬────┘  │  │            │
   │ ┌────────┐ │   │       │        < 20ms           │       │  │ ┌────────┐ │
   │ │ Node-1 │ │◄──┼───────┘                         └───────┼─►│ │ Node-3 │ │
   │ │ 24GB   │ │   │                                         │  │ │  8GB   │ │
   │ └────────┘ │   │                                         │  │ └────────┘ │
   │ ┌────────┐ │   └─────────────────────────────────────────┘  │            │
   │ │ Node-2 │ │                                                  └────────────┘
   │ │ 20GB   │ │
   │ └────────┘ │
   └────────────┘
         │                    OVERLAY HOLOCHAIN
         │         ┌──────────────────────────────────────┐
         └─────────►  DHT privé (bootstrap SD-WAN interne) │
                   │  Identité ed25519 | Warrants | Audit   │
                   │  NodeCapabilities | Heartbeats | DHT   │
                   └────────────────────┬─────────────────┘
                                        │
                              QUIC/mTLS DATA PLANE
                   ┌──────────────────────────────────────┐
                   │  Node-1 ◄══ 240MB activations ══► Node-2  │
                   │  (couches 0-23)              (couches 24-47)│
                   │  Chiffrement mTLS ed25519 end-to-end        │
                   └──────────────────────────────────────┘
```

---

## 4. Cas d'Usage

### 4.1 Inférence intra-site (latence optimale)

```
Requête → Node-1 (site HQ)
Scheduler : VRAM locale suffisante → plan solo ou pipeline intra-site
→ Pas de trafic WAN
→ Latence < 5ms pour les activations QUIC
```

### 4.2 Pipeline-split inter-sites (haute capacité)

```
Requête gemma4-31b → Node-1 (site HQ, 24GB) + Node-3 (edge, 8GB)
Scheduler : vérifie SLA lien HQ↔Edge (1 Gbps, < 20ms)
→ Activations 240MB via QUIC sur tunnel SD-WAN
→ Temps transfert : ~2s (acceptable pour batch, borderline pour stream)
→ Fallback : si SLA dégradé → routage vers Node-2 intra-site
```

### 4.3 Failover automatique

```
Node-2 tombe (OOM, crash)
→ Heartbeat Holochain manquant → NodeStatus::Offline dans DHT
→ Scheduler SD-WAN : lien vers Node-2 marqué dégradé
→ Plan d'exécution recalculé : Node-1 + Node-3 (inter-sites)
→ SLA vérifié avant basculement
→ Délai total : < 30s
```

---

## 5. Intégration avec les Projets Existants

HybridNode est conçu comme une **couche optionnelle** activable par feature flag :

```toml
# Cargo.toml du daemon projet
[features]
default = []
hybridnode = ["hybridnode-core", "hybridnode-daemon"]
```

```bash
# Sans HybridNode (mode standard internet)
cargo run --bin ainonymous-daemon

# Avec HybridNode (mode SD-WAN)
cargo run --bin ainonymous-daemon --features hybridnode
```

Voir `HYBRIDNODE_APPLY.md` pour les étapes d'intégration dans un projet existant.
