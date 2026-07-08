# AInonymous

> Inference LLM décentralisée et anonyme — architecture **HybridNode** : Holochain 0.6.1 (overlay DHT) + QUIC/mTLS ed25519 (data plane) + SD-WAN (underlay). Souveraineté agent-centrique, zéro serveur central.

[![License: Apache 2.0](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Holochain](https://img.shields.io/badge/Holochain-0.6.1-purple)](https://holochain.org)
[![Rust](https://img.shields.io/badge/rust-stable-orange)](https://rustup.rs)

---

## Concept

AInonymous est un réseau d'inférence distribué où chaque participant contribue et consomme de la puissance de calcul sans serveur central, sans compte, sans traçabilité. Il adapte le principe **mesh-llm** (pooling P2P de ressources GPU/CPU pour exécuter des LLMs ouverts) via une architecture **HybridNode** en trois couches :

| Couche | Technologie | Rôle |
|--------|------------|------|
| **Overlay** | Holochain 0.6.1 + iroh | DHT, identité ed25519, coordination, warrants |
| **Data plane** | QUIC/mTLS ed25519 | Transfert d'activations tensorielles, token streams |
| **Underlay** | SD-WAN | Topology-aware routing, QoS DSCP 46, SLA enforcement |

---

## Statut du Projet (Juillet 2026)

**Palier F — Intégration Holochain + Warrants** : Largement terminé

- Membrane Proofs pour réseaux privés
- Zome `warrants` complet (émission, vérification Ed25519ctx, liens, cleanup)
- `NodeCapabilities` avec estimation VRAM réaliste
- Scoring intelligent des nœuds (VRAM + charge + géolocalisation via Haversine)
- Découverte P2P dynamique + cache
- Sécurité renforcée (`zeroize`, Domain Separation, validation stricte)
- Optimisations QUIC (compression zstd, quantification INT8 SIMD avec `wide`)

**Palier G — Moteur d'Inférence Réel (llama.cpp)** : En cours

- `LlamaManager` robuste (GPU detection, VRAM estimation, auto-réduction `n_gpu_layers`, `mlock`, KV-cache q8_0)
- `layer_range` support skeleton dans `LlamaManager`
- Préparation du pipeline distribué réel

---

## Installation rapide

```bash
# macOS / Linux
git clone https://github.com/Geoking2104/AInonymous.git
cd AInonymous

# Build
cargo build --workspace --release

# Lancer le daemon
./target/release/ainonymous-daemon
```

---

## Documentation

- `docs/PALIER_F.md` — Résumé complet de Palier F
- `zomes/warrants/README.md` — Documentation du zome Warrants
- `docs/NODE_SCORING.md` — Système de scoring des nœuds
- `site/ainonymous.html` — Site web autonome (FR/EN)

---

## Licence

Apache 2.0
