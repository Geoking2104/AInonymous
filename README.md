# AInonymous

> Inference LLM décentralisée et anonyme — architecture **HybridNode** : Holochain 0.6.1 (overlay DHT agent-centrique) + QUIC/mTLS ed25519 (data plane) + SD-WAN (underlay). Souveraineté agent-centrique, zéro serveur central, zéro token.

[![License: Apache 2.0](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Holochain](https://img.shields.io/badge/Holochain-0.6.1-purple)](https://holochain.org)
[![Rust](https://img.shields.io/badge/rust-stable-orange)](https://rustup.rs)

> ⚠️ **Projet expérimental (juillet 2026).** Avant toute évaluation technique ou déploiement, lire [`DISCLAIMER.md`](DISCLAIMER.md) — statut réel des fonctionnalités, ce qui est vérifié vs. ce qui reste architecture cible.

---

## Concept

AInonymous est un réseau d'inférence distribué où chaque participant contribue et consomme de la puissance de calcul sans serveur central, sans compte, sans traçabilité. Il adapte le principe **mesh-llm** (pooling P2P de ressources GPU/CPU pour exécuter des LLMs ouverts) via une architecture **HybridNode** en trois couches :

| Couche | Technologie | Rôle |
|--------|------------|------|
| **Overlay** | Holochain 0.6.1 + iroh | DHT, identité ed25519, coordination, warrants |
| **Data plane** | QUIC/mTLS ed25519 | Transfert d'activations tensorielles, token streams |
| **Underlay** | SD-WAN | Topology-aware routing, QoS DSCP 46, SLA enforcement |

---

## Ce qui différencie AInonymous sur le marché

Le paysage de l'inférence LLM distribuée en 2026 se divise en deux familles, et AInonymous ne rentre dans aucune des deux telles qu'elles existent aujourd'hui.

**Famille 1 — les mesh communautaires (Petals, Exo Labs, Kalavai, SharedLLM)** : pas de blockchain, mais un modèle de confiance qui repose soit sur un swarm public ouvert à l'abus (Petals — le projet est aujourd'hui en maintenance, son propre indicateur de santé réseau public est en panne), soit sur un LAN fermé (Exo, pas de couche réseau distribuée à proprement parler), soit sur un coordinateur central qui possède un registre de confiance et un secret partagé (Kalavai, SharedLLM). Aucun de ces projets ne modélise l'identité des nœuds de façon auto-certifiante ni ne publie de preuve cryptographique de comportement invalide consultable par tous les pairs.

**Famille 2 — les marchés du calcul décentralisés sur blockchain (Bittensor, Gensyn, Ritual, io.net)** : confiance assurée par consensus on-chain, staking, preuves ZK ou TEE — avec un token natif à détenir, des frais réseau, et une volatilité de marché qui n'a rien à voir avec l'objectif d'usage (faire tourner un modèle). C'est un marché du calcul, pas un outil d'inférence anonyme.

AInonymous se positionne différemment sur trois axes vérifiables dans le code de ce dépôt :

| Axe | Ce que font les autres | Ce que fait AInonymous |
|---|---|---|
| **Identité et confiance** | Secret partagé/coordinateur central (mesh communautaires) *ou* token + consensus on-chain (DePIN blockchain) | Identité ed25519 auto-certifiante par nœud (aucun tiers), `Warrant` signé publié dans le DHT Holochain en cas de comportement invalide, expiration et réhabilitation automatiques — sans jamais dépendre d'un opérateur de registre central ni d'un token |
| **Aucune financiarisation** | Bittensor/Gensyn/Ritual/io.net exigent un wallet et un token natif (coté, volatil) pour participer | Aucun token, aucun frais réseau, aucun compte — la clé ed25519 est générée localement au premier lancement |
| **Conscience du réseau physique (WAN/QoS)** | Aucun des projets étudiés (mesh communautaire ou DePIN) n'intègre le réseau WAN sous-jacent | Couche **HybridNode** : SLA par lien (latence/bande passante/jitter), marquage QoS DSCP 46 pour le trafic d'inférence, scoring géographique Haversine, failover multi-sites — pensé pour des déploiements d'entreprise sur SD-WAN existant (Cisco vEdge, VMware VeloCloud, Fortinet) autant que pour un réseau public |

**Sur le plan technique pur**, AInonymous partage un choix avec SharedLLM (le seul projet identifié qui fait le même pari) : backend **llama.cpp / GGUF** plutôt qu'un stack PyTorch custom (Petals) — accès immédiat à tous les formats de quantification et architectures déjà supportés par llama.cpp, sans maintenir un zoo de modèles parallèle. AInonymous va plus loin sur un point précis et vérifié dans ce dépôt (`patches/llama-cpp-pipeline-split/`, `docs/DEV_PLAN_TESTNET_2NODES.md`) : un patch natif de llama.cpp pour un pipeline-split par couche **sans passer par le mode RPC** ni par un pont Python, testé bit-exact sur une chaîne à 2 nœuds *et* à 3 nœuds (nœud du milieu compris, `layer_start>0` et `layer_end<n_layer` simultanément) — avec réduction mesurée du graphe de calcul (87→42 nœuds ggml). À titre de comparaison, SharedLLM (v0.1.0, avril 2026) documente elle-même que seuls ses modèles de test triviaux sont vérifiés de bout en bout, les modèles plus gros étant bloqués par un bug amont non résolu dans le RPC de llama.cpp.

**Ce que ce comparatif ne dit pas** (honnêteté avant tout, cf. `DISCLAIMER.md`) : AInonymous est expérimental. Le support GPU et MoE pour le pipeline-split natif ne sont pas encore faits (`ROADMAP.md`, Palier G), l'intégration Holochain n'est pas encore mTLS-stricte de bout en bout (Palier H), et rien ici n'a l'ancienneté d'usage réel de Petals. Les axes ci-dessus sont des différences d'architecture vérifiables dans le code, pas des garanties de maturité produit équivalente.

---

## Architecture HybridNode

```
COUCHE APPLICATION      Daemon AInonymous | Agents | API REST OpenAI-compat
COUCHE OVERLAY          Holochain : identité ed25519, DHT, Warrants, Blackboard
PLAN DE DONNÉES         QUIC/mTLS ed25519 : activations, tokens, embeddings
COUCHE UNDERLAY         SD-WAN : routage WAN, QoS, failover, tunnels chiffrés
```

Principe dual-canal : Holochain transporte uniquement le plan de contrôle (découverte, capacités, métriques, warrants) — jamais le volume de données. Les activations tensorielles et les tokens circulent en direct entre nœuds via QUIC/mTLS, avec authentification mutuelle ed25519 à chaque connexion (pas de secret partagé, pas de CA tierce). Détail complet : [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) et [`docs/HYBRIDNODE.md`](docs/HYBRIDNODE.md).

---

## Mode privé : réseau fermé par Membrane Proofs

Le mode par défaut d'AInonymous est un mesh public et anonyme : n'importe quel nœud peut rejoindre le DHT sans autorisation. À l'opposé, HybridNode permet un mode **privé** : un réseau fermé où l'admission est conditionnée à une preuve cryptographique signée par un administrateur du réseau — pensé pour un consortium d'entreprise multi-sites, un groupement de recherche ou tout déploiement où le contrôle d'accès prime sur l'ouverture publique.

**Comment ça marche** : `MembraneProofConfig` (`Base64` ou fichier) porte la preuve d'admission dans la configuration du daemon et s'injecte automatiquement dans les appels de zome (`call_zome_with_proof`). Côté HybridNode, la feature Cargo `private-network` active un contrôle d'admission dans le zome d'intégrité : un nœud sans preuve est rejeté à l'entrée. `HolochainConfig::bootstrap_mode` permet de pointer vers un bootstrap privé plutôt que le réseau public par défaut.

**Statut réel (pas d'enjolivement — cf. [`DISCLAIMER.md`](DISCLAIMER.md))** :

| Composant | Statut |
|---|---|
| `MembraneProofConfig` + injection automatique dans les appels de zome | ✅ codé et fonctionnel |
| Feature `private-network` + admission gate dans le zome d'intégrité | ✅ `genesis_self_check` désérialise la preuve (`PrivateNetworkProof`) et vérifie sa signature ed25519 via `hdi::prelude::verify_signature` contre la clé d'administrateur réseau lue dans les propriétés de la DNA (`dna.yaml` → `network_admin_pubkey`, baked into le hash de la DNA) — un nœud dont la preuve est absente, mal signée, ou adressée à une autre clé d'agent est rejeté |
| `install_app_with_membrane_proof` (installation d'une hApp avec preuve, côté conducteur) | ❌ non fonctionnel actuellement — l'API `holochain_client` a changé de forme depuis l'écriture initiale ; la fonction est volontairement stubbée (erreur explicite) plutôt que de deviner une implémentation non vérifiée |
| Expiration / anti-rejeu de la preuve (`issued_at`) | ❌ le champ existe dans `PrivateNetworkProof` mais n'est pas encore vérifié — `genesis_self_check` n'a pas d'accès horloge vérifié dans HDI 0.7.1, disclosed plutôt que deviné |
| Configuration de la clé admin dans `dna.yaml` | 🟡 fonctionnelle mais peu ergonomique — tableau brut de 36 octets, pas encore le format lisible `uhCAk...` |
| Proof-of-work à l'admission, liste blanche d'agents de confiance | ❌ pas implémenté — présents uniquement comme architecture cible dans `docs/ARCHITECTURE.md`, pas dans le code |

Le mode privé a donc désormais une vérification cryptographique réelle de l'admission (signature ed25519 contre la clé d'administrateur réseau), mais reste incomplet pour un déploiement production : pas d'expiration de preuve, configuration de clé peu ergonomique, et le chemin d'installation `install_app_with_membrane_proof` côté conducteur est toujours stubbé.

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
- Pipeline-split natif llama.cpp vérifié bit-exact (2 et 3 nœuds, voir tableau comparatif ci-dessus)
- Restent : MoE, GPU sur le pipeline natif, decoding spéculatif, quantification du KV-cache

**Packaging & déploiement** : conteneurs OCI-compliant pour `ainonymous-daemon` et `hybridnode-daemon` (`docker/`, `docker-compose.yml`) — voir [`docs/OCI_RUNTIME_SPEC_COMPLIANCE.md`](docs/OCI_RUNTIME_SPEC_COMPLIANCE.md) pour le détail de conformité vis-à-vis d'[opencontainers/runtime-spec](https://github.com/opencontainers/runtime-spec).

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

### Via Docker

```bash
docker compose up --build
```

Voir [`docs/OCI_RUNTIME_SPEC_COMPLIANCE.md`](docs/OCI_RUNTIME_SPEC_COMPLIANCE.md) pour la configuration runtime (utilisateur non-root, capacités Linux réduites, rootfs en lecture seule).

---

## Documentation

- [`DISCLAIMER.md`](DISCLAIMER.md) — statut réel du projet, à lire avant tout déploiement
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — architecture technique complète
- [`docs/HYBRIDNODE.md`](docs/HYBRIDNODE.md) — spécification SD-WAN + Holochain + QUIC/mTLS
- [`docs/PALIER_F.md`](docs/PALIER_F.md) — résumé complet de Palier F
- [`docs/NODE_SCORING.md`](docs/NODE_SCORING.md) — système de scoring des nœuds
- [`docs/OCI_RUNTIME_SPEC_COMPLIANCE.md`](docs/OCI_RUNTIME_SPEC_COMPLIANCE.md) — packaging conteneur et conformité OCI
- `zomes/warrants/README.md` — documentation du zome Warrants
- `site/ainonymous.html` — site web autonome (FR/EN)

---

## Licence

Apache 2.0
