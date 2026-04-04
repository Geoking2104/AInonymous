# AInonymous

> Inference LLM décentralisée et anonyme sur infrastructure Holochain — principe mesh-llm, souveraineté agent-centrique.

---

## Concept

AInonymous est un réseau d'inférence distribué où chaque participant contribue et consomme de la puissance de calcul sans serveur central, sans compte, sans traçabilité. Il adapte le principe **mesh-llm** (pooling P2P de ressources GPU/CPU pour exécuter des LLMs ouverts) en remplaçant l'infrastructure réseau QUIC/Nostr par **Holochain** — un runtime P2P agent-centrique, cryptographiquement souverain.

Les agents IA tournant dans le réseau sont orchestrés par **Goose** (Block/Open Source) et propulsés en priorité par **Gemma 4** (Google, Apache 2.0) avec support complet des formats GGUF.

---

## Pourquoi Holochain et pas Nostr/QUIC ?

| Besoin | mesh-llm (anarchai.org) | AInonymous (Holochain) |
|---|---|---|
| Découverte de pairs | Relais Nostr publics | DHT Holochain (aucun relais tiers) |
| Transport | QUIC + RPC | WebRTC + conducteur Holochain |
| État distribué | Gossip éphémère | Source chain immuable + DHT validé |
| Identité | Anonyme non-vérifiée | Clé cryptographique ed25519 souveraine |
| Blackboard agents | Gossip 48h | DHT persistant + entrées chainées |
| Validation pairs | Aucune | Zomes de validation déterministes |
| Réputation nœuds | Score VRAM ad-hoc | Warrants cryptographiques + membrane proofs |

---

## Architecture en un clin d'œil

```
┌─────────────────────────────────────────────────────────┐
│                   CLIENT / AGENT                        │
│  ┌─────────────┐  ┌──────────────┐  ┌───────────────┐  │
│  │    Goose    │  │  CLI ainon.  │  │  Web UI / API │  │
│  │  (agent IA) │  │  (terminal)  │  │  (OpenAI compat)│ │
│  └──────┬──────┘  └──────┬───────┘  └───────┬───────┘  │
│         └────────────────┴──────────────────┘          │
│                          │                              │
│               ┌──────────▼──────────┐                  │
│               │  Conducteur Holochain│                  │
│               │  (runtime local)     │                  │
│               └──────────┬──────────┘                  │
└──────────────────────────┼──────────────────────────────┘
                           │ WebRTC / QUIC
        ┌──────────────────┼──────────────────┐
        ▼                  ▼                  ▼
 ┌─────────────┐   ┌─────────────┐   ┌─────────────┐
 │  Nœud GPU A  │   │  Nœud GPU B  │   │  Nœud CPU C  │
 │  Gemma4-31B  │   │  Gemma4-26B  │   │  Gemma4-E4B  │
 │  (couches 0-24)│  │ (couches 24-48)│  │ (spéculatif) │
 │  [zome:infer]│   │  [zome:infer]│   │  [zome:draft]│
 └──────┬───────┘   └──────┬───────┘   └──────┬───────┘
        └──────────────────┴──────────────────┘
                     DHT Holochain
              (routing table + état partagé)
```

---

## Composants Principaux

### 1. hApp `ainonymous-core`
La hApp Holochain centrale. Contient :
- **DNA `inference-mesh`** : coordination de l'inférence distribuée
- **DNA `agent-registry`** : registre des nœuds, capacités, disponibilité
- **DNA `blackboard`** : collaboration d'agents décentralisée

### 2. Moteur d'inférence
- Binaire **llama.cpp** pour l'exécution locale GGUF
- Modèles prioritaires : **Gemma 4** (E2B, E4B, 26B-A4B MoE, 31B)
- Pipeline-splitting par couches entre nœuds (layer sharding)
- Décodage spéculatif : nœud draft (Gemma4-E4B) + nœud verify

### 3. Agent d'orchestration : Goose
- Framework agent open-source (Block, Apache 2.0)
- Intégration MCP native → accès aux zomes Holochain via serveur MCP
- Multi-LLM configurable (Gemma 4 local en priorité, fallback cloud)
- Commande : `ainonymous goose` démarre Goose pointant sur le mesh local

### 4. API OpenAI-compatible
- Endpoint local `localhost:9337/v1`
- Routage des requêtes vers le mesh via le conducteur Holochain
- Champ `model` utilisé pour le routage (ex: `gemma4-31b`, `gemma4-moe`)

---

## Installation rapide

```bash
# macOS / Linux
curl -fsSL https://ainonymous.network/install.sh | sh

# Rejoindre le mesh public
ainonymous --auto

# Rejoindre avec un modèle spécifique
ainonymous --model gemma4-26b-moe

# Lancer Goose en mode mesh
ainonymous goose

# Tester l'API locale
curl http://localhost:9337/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"gemma4-31b","messages":[{"role":"user","content":"Bonjour"}]}'
```

---

## Modèles Supportés (Gemma 4 prioritaire)

| Modèle | VRAM | Architecture | Usage |
|---|---|---|---|
| `gemma4-e2b` | ~3 GB | Dense edge | Nœuds légers, draft spéculatif |
| `gemma4-e4b` | ~5 GB | Dense edge | Nœuds légers, inférence solo |
| `gemma4-26b-moe` | ~18 GB | MoE (4B actifs) | Sharding par experts |
| `gemma4-31b` | ~20 GB | Dense | Pipeline-splitting couches |
| `qwen3-32b` | ~20 GB | Dense | Alternatif haute qualité |
| `llama-3.3-70b` | ~43 GB | Dense | Multi-nœuds requis |

---

## Statut du Projet

- [ ] Spécification technique (ce document)
- [ ] Zomes Holochain — MVP inference-mesh
- [ ] Intégration llama.cpp + pipeline-splitting
- [ ] MCP server pour Goose
- [ ] API proxy OpenAI-compatible
- [ ] Blackboard Holochain (agents collaboration)
- [ ] Testnet public
- [ ] UI dashboard

---

## Licence

Apache 2.0 — aligné avec Holochain (Apache 2.0), Goose (Apache 2.0), Gemma 4 (Apache 2.0).
