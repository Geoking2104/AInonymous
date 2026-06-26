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

Les agents IA sont orchestrés par **Goose** (Block/Open Source) et propulsés en priorité par **Gemma 4** (Google, Apache 2.0) avec support complet GGUF.

---

## Pourquoi Holochain et pas Nostr/QUIC pur ?

| Besoin | mesh-llm (anarchai.org) | AInonymous (HybridNode) |
|---|---|---|
| Découverte de pairs | Relais Nostr publics | DHT Holochain iroh (aucun relais tiers) |
| Transport activations | QUIC | QUIC/mTLS ed25519 — `PeerKeyVerifier` strict |
| État distribué | Gossip éphémère | Source chain immuable + DHT validé |
| Identité | Anonyme non-vérifiée | AgentPubKey ed25519 = DHT + TLS cert + signer |
| Blackboard agents | Gossip 48h | DHT persistant + entrées chainées |
| Réputation nœuds | Score VRAM ad-hoc | Warrants cryptographiques Holochain |
| Réseau privé | Non | `PrivateNetworkProof` membrane proof |
| Topologie réseau | Non | SD-WAN SLA-aware scheduler |

---

## Architecture en un clin d'œil

```
┌─────────────────────────────────────────────────────────────┐
│                   CLIENT / AGENT                            │
│  ┌──────────┐  ┌─────────────┐  ┌──────────────────────┐  │
│  │  Goose   │  │ CLI ainon.  │  │  API OpenAI-compat   │  │
│  │ (agent)  │  │ (terminal)  │  │  localhost:9337/v1   │  │
│  └────┬─────┘  └──────┬──────┘  └──────────┬───────────┘  │
│       └───────────────┴──────────────────────┘             │
│                        │                                    │
│          ┌─────────────▼──────────────┐                    │
│          │     HybridNode Scheduler   │                    │
│          │  (SD-WAN topo + DHT caps)  │                    │
│          └──────┬──────────┬──────────┘                    │
│                 │          │                                │
│    ┌────────────▼──┐  ┌────▼──────────────┐               │
│    │Holochain 0.6.1│  │  QUIC/mTLS plane  │               │
│    │  (iroh DHT)   │  │  ed25519 PeerKey  │               │
│    └───────────────┘  └───────────────────┘               │
└─────────────────────────────┼───────────────────────────────┘
                              │ SD-WAN fabric (DSCP 46)
           ┌──────────────────┼──────────────────┐
           ▼                  ▼                  ▼
  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
  │  Nœud GPU A  │  │  Nœud GPU B  │  │  Nœud CPU C  │
  │  Gemma4-31B  │  │  Gemma4-26B  │  │  Gemma4-E4B  │
  │  couches 0-24│  │ couches 24-48│  │  spéculatif  │
  │ [attestation]│  │ [ModelClaim] │  │  [warrant ✓] │
  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘
         └─────────────────┴─────────────────┘
                   DHT Holochain + SD-WAN underlay
```

---

## Stack technique

| Composant | Version | Notes |
|-----------|---------|-------|
| Holochain | **0.6.1** | iroh transport, HDK 0.6.1, warrants API stable |
| QUIC | quinn 0.11 | mTLS strict — `PeerKeyVerifier` ed25519 |
| Identité | ed25519-dalek 2 | AgentPubKey = DHT + TLS cert + signer |
| Prometheus | :9338/metrics | `ainonymous_*` + `hybridnode_*` métriques |
| OpenTelemetry | 0.22 | Traces OTLP optionnelles |
| llama.cpp | latest | Format GGUF, pipeline-splitting par couches |
| SD-WAN | vManage / mock | API REST, DSCP 46 EF pour inférence |

---

## Composants Principaux

### 1. HybridNode (`crates/hybridnode-core`)
Couche d'architecture réutilisable combinant les trois plans :
- **`scheduler`** — routage locality-aware : local-first → inter-site si SLA OK
- **`sdwan`** — abstraction provider (vManage, VeloCloud, mock)
- **`topology`** — modèle `NodeTopology` : `LinkSla` + `PeerCapabilities`
- **`identity`** — chargement AgentPubKey depuis lair-keystore
- **`observability`** — endpoint Prometheus :9338

### 2. hApp `ainonymous-core`
La hApp Holochain centrale. DNAs :
- **`inference-mesh`** : coordination de l'inférence distribuée
- **`attestation`** : NodeAttestation, ModelManifest, ModelClaim, Warrant, WarrantRefutation
- **`agent-registry`** : capacités nœuds, disponibilité
- **`blackboard`** : collaboration d'agents décentralisée

### 3. Sécurité — mTLS + Warrants
- **mTLS QUIC strict** : `PeerKeyVerifier` — ed25519 AgentPubKey réutilisée comme certificat TLS, vérification mutuelle obligatoire
- **Attestation nœuds** : `NodeAttestation` signé ed25519, vérifié par les pairs avant connexion
- **Validation modèles** : SHA-256 GGUF + confirmation ≥ 2 pairs (`ModelManifest` + `ModelClaim`)
- **Warrants** : preuve cryptographique DHT de comportement invalide, exclusion automatique du scheduling
- **Bootstrap privé** : `PrivateNetworkProof` membrane proof pour consortiums fermés

### 4. Moteur d'inférence
- Binaire **llama.cpp** pour l'exécution locale GGUF
- Modèles prioritaires : **Gemma 4** (E2B, E4B, 26B-A4B MoE, 31B)
- Pipeline-splitting par couches entre nœuds (layer sharding)
- Redondance : PrimaryShadow, HotStandby, NofM Quorum (2/3), SpeculativeVerify

### 5. Agent d'orchestration : Goose
- Framework agent open-source (Block, Apache 2.0)
- Intégration MCP native → accès aux zomes Holochain
- Multi-LLM configurable (Gemma 4 local en priorité, fallback cloud)

### 6. API OpenAI-compatible
- Endpoint local `localhost:9337/v1`
- Routage via HybridNode Scheduler → mesh Holochain
- Champ `model` pour le routage (ex : `gemma4-31b`, `gemma4-moe`)

---

## Installation rapide

```bash
# macOS / Linux
curl -fsSL https://ainonymous.network/install.sh | sh

# Rejoindre le mesh public
ainonymous --auto

# Ou démarrer le daemon HybridNode directement
cargo install --path crates/hybridnode-daemon --features mock-sdwan
hybridnode --config hybridnode/configs/ainonymous.hybridnode.yaml

# Initialiser un nouveau projet avec HybridNode
bash scripts/hybridnode/init_project.sh mon-projet

# Tester l'API locale
curl http://localhost:9337/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"gemma4-31b","messages":[{"role":"user","content":"Bonjour"}]}'
```

---

## Modèles Supportés (Gemma 4 prioritaire)

| Modèle | VRAM | Architecture | Usage HybridNode |
|---|---|---|---|
| `gemma4-e2b` | ~3 GB | Dense edge | Nœuds légers, draft spéculatif |
| `gemma4-e4b` | ~5 GB | Dense edge | Nœuds légers, inférence solo |
| `gemma4-26b-moe` | ~18 GB | MoE (4B actifs) | Sharding par experts inter-sites |
| `gemma4-31b` | ~20 GB | Dense | Pipeline-splitting couches, NofM quorum |
| `qwen3-32b` | ~20 GB | Dense | Alternatif haute qualité |
| `llama-3.3-70b` | ~43 GB | Dense | Multi-nœuds requis, SpeculativeVerify |

---

## Documentation

| Document | Contenu |
|----------|---------|
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | Architecture complète : identité, mTLS, attestation, observabilité, anti-Sybil, redondance, migration |
| [`docs/NETWORK.md`](docs/NETWORK.md) | QUIC/mTLS — `PeerKeyVerifier`, `connect_quic_mtls()`, `verify_node_before_connect()` |
| [`docs/HOLOCHAIN_ZOMES.md`](docs/HOLOCHAIN_ZOMES.md) | Zomes Holochain — DNA `attestation` complet : entrées, validations, API coordinator |
| [`docs/HYBRIDNODE.md`](docs/HYBRIDNODE.md) | HybridNode — concepts, cas d'usage, politique SD-WAN |
| [`docs/HYBRIDNODE_ARCHITECTURE.md`](docs/HYBRIDNODE_ARCHITECTURE.md) | Composants Rust, flux de requête, sécurité double-chiffrement |
| [`docs/HYBRIDNODE_CARGO_PATCH.md`](docs/HYBRIDNODE_CARGO_PATCH.md) | Intégration workspace Cargo |
| [`HYBRIDNODE_APPLY.md`](HYBRIDNODE_APPLY.md) | Guide d'intégration pas-à-pas dans un nouveau projet |
| [`hybridnode/`](hybridnode/) | Configs, policies, schémas, specs YAML |

---

## Statut du Projet

- [x] Spécification technique (architecture complète)
- [x] Architecture sécurité — mTLS QUIC, attestation nœuds, warrants, anti-Sybil
- [x] HybridNode — crate Rust `hybridnode-core` + daemon + DNA Holochain
- [x] DNA `attestation` — zomes integrity + coordinator (Holochain 0.6.1)
- [x] Politique SD-WAN + configs + schéma JSON + CI GitHub Actions
- [x] Workspace Rust compile — `cargo build --workspace` vert (0 warning), 4 binaires (daemon, proxy, cli, mcp)
- [x] API proxy OpenAI-compatible — `/v1/chat/completions` (solo + routage `pipeline_split`)
- [x] MCP server pour Goose — stdio JSON-RPC (outils mesh + blackboard)
- [x] Pipeline-splitting — inférence distribuée, topologie chaîne : coordinateur prefill+decode, relais worker, session QUIC réutilisée, purge KV-cache (cf. [`docs/ADR_001_coordinator_decode_loop.md`](docs/ADR_001_coordinator_decode_loop.md))
- [x] Harnais testnet 2 nœuds (loopback) — `make testnet-2`, validé en runtime via mock (cf. [`docs/TESTNET_2NODES.md`](docs/TESTNET_2NODES.md))
- [ ] Validation bout-en-bout avec un **vrai modèle** (torch/transformers)
- [ ] Intégration Holochain réelle — zomes WASM buildés + `AppWebsocket` (remplace le bootstrap statique)
- [ ] mTLS QUIC réel — vérification ed25519 du pair (actuellement skip-verify + token de session)
- [ ] Intégration llama.cpp (chemin solo GGUF) + détection GPU NVIDIA/AMD
- [ ] Tests d'intégration + CI Rust
- [ ] Testnet public
- [ ] UI dashboard

---

## Licence

Apache 2.0 — aligné avec Holochain (Apache 2.0), Goose (Apache 2.0), Gemma 4 (Apache 2.0).


---

## Site Web / Website

### ✦ Fichier unique tout-en-un

**[→ Voir le site en ligne](https://htmlpreview.github.io/?https://github.com/Geoking2104/AInonymous/blob/main/site/ainonymous.html)**

> `site/ainonymous.html` — fichier HTML autonome contenant les 4 pages (Landing FR/EN + Enterprise FR/EN) avec navigation JS intégrée, sélecteur de langue et tous les liens fonctionnels. Aucune dépendance externe.

---

### Pages individuelles

Les fichiers HTML ci-dessous sont autonomes (aucune dépendance externe) — ouvrez-les directement dans un navigateur ou déployez-les sur n'importe quel hébergeur statique.

| Fichier | Langue | Description |
|---|---|---|
| `site/ainonymous.html` | 🇫🇷🇬🇧 FR + EN | **Tout-en-un** — Landing + Enterprise avec switcher FR/EN |
| `site/landing.html` | 🇫🇷 Français | Page d'accueil grand public |
| `site/landing-en.html` | 🇬🇧 English | Public landing page |
| `site/enterprise.html` | 🇫🇷 Français | Page entreprise — architecture & business case |
| `site/enterprise-en.html` | 🇬🇧 English | Enterprise page — architecture & business case |

> Chaque page contient un sélecteur de langue **FR \| EN** dans la navigation.

---

<details>
<summary><strong>📄 site/landing.html</strong> — Page d'accueil (FR)</summary>

```html
<!DOCTYPE html>
<html lang="fr">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>AInonymous — L'IA qui vous appartient</title>
  <style>
    :root {
      --bg: #0a0a0f;
      --bg2: #12121a;
      --bg3: #1a1a28;
      --border: rgba(255,255,255,0.08);
      --border2: rgba(255,255,255,0.14);
      --text: #f0eff8;
      --muted: #8887a0;
      --accent: #7c6deb;
      --accent2: #5dd8a8;
      --accent3: #eb6d7c;
      --accent-glow: rgba(124,109,235,0.18);
    }
    *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
    html { scroll-behavior: smooth; }
    body {
      font-family: 'Segoe UI', system-ui, -apple-system, sans-serif;
      background: var(--bg); color: var(--text);
      line-height: 1.7; -webkit-font-smoothing: antialiased;
    }
    a { color: inherit; text-decoration: none; }

    /* NAV */
    nav {
      position: sticky; top: 0; z-index: 100;
      display: flex; align-items: center; justify-content: space-between;
      padding: 14px 40px;
      background: rgba(10,10,15,0.9);
      backdrop-filter: blur(12px);
      border-bottom: 1px solid var(--border);
    }
    .nav-logo { font-size: 18px; font-weight: 700; letter-spacing: -0.5px; }
    .nav-logo span { color: var(--accent); }
    .nav-links { display: flex; gap: 28px; font-size: 14px; color: var(--muted); align-items: center; }
    .nav-links a:hover { color: var(--text); }
    .lang-switch {
      display: flex; align-items: center; gap: 2px;
      font-size: 12px; font-weight: 700;
      background: var(--bg3); border: 1px solid var(--border2);
      border-radius: 7px; overflow: hidden;
    }
    .lang-switch a {
      padding: 4px 9px; color: var(--muted); transition: background .15s, color .15s;
    }
    .lang-switch a.active {
      background: var(--accent); color: #fff;
    }
    .lang-switch a:hover:not(.active) { color: var(--text); }
    .nav-cta {
      background: var(--accent); color: #fff;
      padding: 8px 18px; border-radius: 8px;
      font-size: 14px; font-weight: 600; transition: opacity .15s;
    }
    .nav-cta:hover { opacity: .85; }

    /* LAYOUT */
    .section { padding: 96px 40px; max-width: 1080px; margin: 0 auto; }
    .eyebrow {
      font-size: 12px; font-weight: 700; letter-spacing: 2px;
      text-transform: uppercase; color: var(--accent2); margin-bottom: 14px;
    }
    h2 {
      font-size: clamp(28px, 4vw, 44px); font-weight: 800;
      letter-spacing: -1.5px; line-height: 1.15; margin-bottom: 16px;
    }
    .section-sub {
      font-size: 18px; color: var(--muted); max-width: 560px;
      line-height: 1.6; margin-bottom: 56px;
    }

    /* HERO */
    .hero {
      text-align: center;
      padding: 120px 40px 96px; max-width: 860px; margin: 0 auto;
    }
    .hero-badge {
      display: inline-flex; align-items: center; gap: 8px;
      font-size: 13px; color: var(--accent2); font-weight: 500;
      background: rgba(93,216,168,0.1); border: 1px solid rgba(93,216,168,0.3);
      padding: 5px 14px; border-radius: 20px; margin-bottom: 32px;
    }
    .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--accent2); animation: pulse 2s infinite; }
    @keyframes pulse { 0%,100%{opacity:1}50%{opacity:.3} }
    h1 {
      font-size: clamp(40px, 7vw, 72px); font-weight: 800;
      line-height: 1.08; letter-spacing: -2.5px; margin-bottom: 24px;
    }
    .a1 { color: var(--accent); }
    .a2 { color: var(--accent2); }
    .hero-sub {
      font-size: clamp(17px, 2.2vw, 21px); color: var(--muted);
      max-width: 600px; margin: 0 auto 44px; line-height: 1.6;
    }
    .hero-actions { display: flex; gap: 14px; justify-content: center; flex-wrap: wrap; }
    .btn-p {
      background: var(--accent); color: #fff;
      padding: 14px 28px; border-radius: 10px; font-weight: 700; font-size: 15px;
      display: inline-flex; align-items: center; gap: 8px;
      transition: transform .15s, opacity .15s;
    }
    .btn-p:hover { transform: translateY(-1px); opacity: .9; }
    .btn-g {
      border: 1px solid var(--border2); color: var(--muted);
      padding: 14px 28px; border-radius: 10px; font-weight: 600; font-size: 15px;
      display: inline-flex; align-items: center; gap: 8px;
      transition: border-color .15s, color .15s;
    }
    .btn-g:hover { border-color: var(--accent); color: var(--text); }

    /* STATS BAR */
    .stats-bar {
      display: flex; justify-content: center; gap: 60px; flex-wrap: wrap;
      padding: 40px; background: var(--bg2);
      border-top: 1px solid var(--border); border-bottom: 1px solid var(--border);
    }
    .stat { text-align: center; }
    .stat-num { font-size: 32px; font-weight: 800; letter-spacing: -1px; color: var(--accent); }
    .stat-lbl { font-size: 13px; color: var(--muted); margin-top: 2px; }

    /* PROBLEM CARDS */
    .prob-grid {
      display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
      gap: 18px; margin-top: 48px;
    }
    .prob-card {
      background: var(--bg2); border: 1px solid rgba(235,109,124,0.25);
      border-radius: 14px; padding: 28px 22px;
    }
    .prob-icon { font-size: 26px; margin-bottom: 12px; }
    .prob-card h3 { font-size: 15px; font-weight: 700; margin-bottom: 7px; }
    .prob-card p { font-size: 13px; color: var(--muted); line-height: 1.6; }

    /* MESH VISUAL */
    .mesh-wrap {
      position: relative; height: 260px;
      display: flex; align-items: center; justify-content: center;
      margin: 56px 0;
    }
    .mesh-svg { position: absolute; inset: 0; width: 100%; height: 100%; }
    .mesh-node {
      position: absolute; border-radius: 50%;
      display: flex; flex-direction: column; align-items: center;
      justify-content: center; text-align: center;
      font-size: 11px; font-weight: 600; border: 1px solid; line-height: 1.3;
    }
    .n-you {
      width: 80px; height: 80px;
      background: rgba(124,109,235,0.2); border-color: var(--accent); color: var(--accent);
      left: 50%; top: 50%; transform: translate(-50%,-50%); z-index: 2; font-size: 13px;
    }
    .n-peer {
      width: 56px; height: 56px;
      background: rgba(93,216,168,0.1); border-color: var(--accent2); color: var(--accent2);
    }
    .p1{left:13%;top:18%} .p2{right:15%;top:14%} .p3{left:6%;top:54%}
    .p4{right:10%;top:50%} .p5{left:26%;bottom:8%} .p6{right:25%;bottom:6%}

    /* STEPS */
    .steps { display: flex; flex-direction: column; }
    .step {
      display: grid; grid-template-columns: 72px 1fr; gap: 24px;
      padding: 36px 0; border-bottom: 1px solid var(--border);
    }
    .step:last-child { border-bottom: none; }
    .step-num {
      width: 50px; height: 50px; border-radius: 13px; flex-shrink: 0;
      background: var(--accent-glow); border: 1px solid rgba(124,109,235,0.4);
      display: flex; align-items: center; justify-content: center;
      font-size: 20px; font-weight: 800; color: var(--accent);
    }
    .step h3 { font-size: 19px; font-weight: 700; margin-bottom: 7px; }
    .step p { font-size: 14px; color: var(--muted); line-height: 1.7; }
    .tag {
      display: inline-block; margin-top: 10px; font-size: 12px;
      padding: 3px 10px; border-radius: 6px; font-weight: 600;
    }
    .t-p { background: rgba(124,109,235,0.15); color: #a89cf5; }
    .t-t { background: rgba(93,216,168,0.15); color: #5dd8a8; }
    .t-c { background: rgba(235,109,124,0.15); color: #eb8090; }

    /* FEATURES */
    .feat-grid {
      display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
      gap: 18px;
    }
    .feat-card {
      background: var(--bg2); border: 1px solid var(--border);
      border-radius: 16px; padding: 30px 26px; transition: border-color .2s;
    }
    .feat-card:hover { border-color: var(--border2); }
    .feat-icon {
      width: 42px; height: 42px; border-radius: 11px; margin-bottom: 16px;
      display: flex; align-items: center; justify-content: center; font-size: 19px;
    }
    .fi-p { background: rgba(124,109,235,0.15); }
    .fi-t { background: rgba(93,216,168,0.15); }
    .fi-c { background: rgba(235,109,124,0.15); }
    .fi-a { background: rgba(250,199,117,0.15); }
    .feat-card h3 { font-size: 16px; font-weight: 700; margin-bottom: 8px; }
    .feat-card p { font-size: 13px; color: var(--muted); line-height: 1.65; }

    /* VS TABLE */
    .vs-wrap {
      background: var(--bg2); padding: 80px 40px;
      border-top: 1px solid var(--border); border-bottom: 1px solid var(--border);
    }
    .vs-inner { max-width: 900px; margin: 0 auto; }
    table { width: 100%; border-collapse: collapse; margin-top: 48px; font-size: 14px; }
    th {
      text-align: left; padding: 12px 18px; font-weight: 700;
      border-bottom: 2px solid var(--border2); color: var(--muted);
    }
    th.hl { color: var(--accent); }
    td { padding: 12px 18px; border-bottom: 1px solid var(--border); }
    tr:last-child td { border-bottom: none; }
    .ok { color: var(--accent2); font-weight: 600; }
    .no { color: var(--accent3); }
    .col { background: rgba(124,109,235,0.06); }

    /* MODELS */
    .models-grid {
      display: grid; grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));
      gap: 14px; margin-top: 48px;
    }
    .model-card {
      background: var(--bg2); border: 1px solid var(--border);
      border-radius: 12px; padding: 20px 18px;
    }
    .model-name { font-size: 14px; font-weight: 700; margin-bottom: 3px; }
    .model-meta { font-size: 12px; color: var(--muted); margin-bottom: 12px; }
    .bar-bg { background: var(--bg3); border-radius: 4px; height: 5px; }
    .bar { height: 5px; border-radius: 4px; background: var(--accent2); }
    .bar-p { background: #a89cf5; }
    .bar-c { background: #f5c4b3; }
    .model-use { font-size: 12px; color: var(--muted); margin-top: 6px; }

    /* INSTALL */
    .install-box {
      background: var(--bg3); border: 1px solid var(--border2);
      border-radius: 14px; padding: 32px;
    }
    pre {
      font-family: 'JetBrains Mono','Fira Code',monospace;
      font-size: 13px; line-height: 2; color: var(--accent2);
      overflow-x: auto; white-space: pre;
    }
    .comment { color: rgba(136,135,160,0.45); }

    /* FOOTER */
    footer {
      padding: 48px 40px; border-top: 1px solid var(--border);
      text-align: center; color: var(--muted); font-size: 14px;
    }
    .f-logo { font-size: 20px; font-weight: 800; margin-bottom: 10px; }
    .f-logo span { color: var(--accent); }
    footer a { color: var(--muted); margin: 0 12px; }
    footer a:hover { color: var(--text); }

    /* BUSINESS CASE */
    .biz-banner {
      background: linear-gradient(135deg, #0e0e1a 0%, #13102a 100%);
      border-top: 1px solid rgba(124,109,235,0.25);
      border-bottom: 1px solid rgba(124,109,235,0.25);
      padding: 80px 40px;
    }
    .biz-inner { max-width: 1080px; margin: 0 auto; }
    .biz-top {
      display: grid; grid-template-columns: 1fr 1fr; gap: 60px;
      align-items: start; margin-bottom: 56px;
    }
    .biz-kpi-grid {
      display: grid; grid-template-columns: 1fr 1fr; gap: 14px;
    }
    .biz-kpi {
      background: rgba(124,109,235,0.08); border: 1px solid rgba(124,109,235,0.2);
      border-radius: 12px; padding: 20px 18px;
    }
    .biz-kpi-num {
      font-size: 30px; font-weight: 800; color: var(--accent);
      letter-spacing: -1px; margin-bottom: 4px;
    }
    .biz-kpi-lbl { font-size: 12px; color: var(--muted); line-height: 1.4; }
    .biz-adv-grid {
      display: grid; grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
      gap: 16px; margin-bottom: 48px;
    }
    .biz-adv {
      background: var(--bg2); border: 1px solid var(--border);
      border-radius: 14px; padding: 26px 22px;
      transition: border-color .2s;
    }
    .biz-adv:hover { border-color: rgba(124,109,235,0.4); }
    .biz-adv-icon { font-size: 22px; margin-bottom: 12px; }
    .biz-adv h3 { font-size: 15px; font-weight: 700; margin-bottom: 6px; }
    .biz-adv p { font-size: 13px; color: var(--muted); line-height: 1.6; }
    .biz-cta-box {
      background: rgba(124,109,235,0.07); border: 1px solid rgba(124,109,235,0.3);
      border-radius: 16px; padding: 36px 32px;
      display: flex; align-items: center; justify-content: space-between;
      gap: 24px; flex-wrap: wrap;
    }
    .biz-cta-box h3 { font-size: 22px; font-weight: 800; letter-spacing: -0.5px; margin-bottom: 6px; }
    .biz-cta-box p { font-size: 14px; color: var(--muted); max-width: 500px; }
    .btn-enterprise {
      background: var(--accent); color: #fff; white-space: nowrap;
      padding: 14px 28px; border-radius: 10px; font-weight: 700; font-size: 15px;
      display: inline-flex; align-items: center; gap: 8px; flex-shrink: 0;
      transition: transform .15s, opacity .15s;
    }
    .btn-enterprise:hover { transform: translateY(-1px); opacity: .9; }

    /* CLOSED LOOP DIAGRAM */
    .loop-diagram {
      display: flex; align-items: center; justify-content: center;
      gap: 0; margin: 40px 0 16px; flex-wrap: wrap;
    }
    .loop-node {
      background: var(--bg2); border: 1px solid rgba(124,109,235,0.3);
      border-radius: 12px; padding: 14px 18px; text-align: center;
      min-width: 120px;
    }
    .loop-node-icon { font-size: 22px; margin-bottom: 6px; }
    .loop-node-lbl { font-size: 11px; font-weight: 700; color: var(--text); }
    .loop-node-sub { font-size: 10px; color: var(--muted); margin-top: 2px; }
    .loop-arrow {
      font-size: 18px; color: rgba(124,109,235,0.5); padding: 0 10px;
      flex-shrink: 0;
    }
    .loop-shield {
      display: inline-flex; align-items: center; gap: 6px;
      font-size: 11px; font-weight: 600; color: var(--accent2);
      background: rgba(93,216,168,0.08); border: 1px solid rgba(93,216,168,0.2);
      padding: 4px 12px; border-radius: 20px; margin-top: 16px;
    }

    @media (max-width: 680px) {
      nav { padding: 12px 20px; }
      .nav-links { display: none; }
      .section { padding: 60px 20px; }
      .stats-bar { gap: 30px; padding: 28px 20px; }
      .vs-wrap { padding: 60px 20px; }
      .step { grid-template-columns: 56px 1fr; gap: 16px; }
      .install-box { padding: 20px 16px; }
      .biz-banner { padding: 60px 20px; }
      .biz-top { grid-template-columns: 1fr; gap: 36px; }
      .biz-cta-box { flex-direction: column; }
      .loop-diagram { gap: 8px; }
    }
  </style>
</head>
<body>

<nav>
  <div class="nav-logo">AI<span>n</span>onymous</div>
  <div class="nav-links">
    <a href="#comment">Comment ça marche</a>
    <a href="#pourquoi">Pourquoi nous</a>
    <a href="#installer">Installer</a>
    <a href="https://github.com/Geoking2104/AInonymous">GitHub</a>
    <a href="enterprise.html" style="color: var(--accent2); font-weight: 600;">Entreprises ✦</a>
    <div class="lang-switch">
      <a href="landing.html" class="active">FR</a>
      <a href="landing-en.html">EN</a>
    </div>
  </div>
  <a href="https://github.com/Geoking2104/AInonymous" class="nav-cta">Démarrer →</a>
</nav>

<!-- HERO -->
<div class="hero">
  <div class="hero-badge"><div class="dot"></div>Open source · Apache 2.0 · Bêta publique</div>
  <h1>L'IA puissante,<br><span class="a1">sans serveur.</span><br><span class="a2">Sans compte.</span></h1>
  <p class="hero-sub">
    AInonymous est un réseau pair-à-pair où vos voisins font tourner l'IA pour vous, et vous pour eux — sans aucune entreprise au milieu.
  </p>
  <div class="hero-actions">
    <a href="#installer" class="btn-p">⚡ Rejoindre le réseau</a>
    <a href="#comment" class="btn-g">Voir comment ça marche</a>
  </div>
</div>

<!-- STATS -->
<div class="stats-bar">
  <div class="stat"><div class="stat-num">0</div><div class="stat-lbl">Serveur central</div></div>
  <div class="stat"><div class="stat-num">0€</div><div class="stat-lbl">Abonnement</div></div>
  <div class="stat"><div class="stat-num">0</div><div class="stat-lbl">Données collectées</div></div>
  <div class="stat"><div class="stat-num">100%</div><div class="stat-lbl">Open source</div></div>
</div>

<!-- PROBLEM -->
<div class="section">
  <div class="eyebrow">Le problème</div>
  <h2>L'IA actuelle vous surveille.<br>Et vous le payez cher.</h2>
  <p class="section-sub">ChatGPT, Claude, Gemini — ces services sont excellents, mais ils passent par des serveurs centralisés qui enregistrent vos conversations et vous facturent chaque mois.</p>
  <div class="prob-grid">
    <div class="prob-card">
      <div class="prob-icon">👁️</div>
      <h3>Vos conversations sont lues</h3>
      <p>Tout ce que vous tapez est stocké et analysé pour entraîner les prochains modèles.</p>
    </div>
    <div class="prob-card">
      <div class="prob-icon">🏦</div>
      <h3>Un abonnement à vie</h3>
      <p>20€/mois en moyenne. Si vous arrêtez de payer, vous perdez l'accès. Pour toujours.</p>
    </div>
    <div class="prob-card">
      <div class="prob-icon">🔒</div>
      <h3>Un seul point de défaillance</h3>
      <p>Si OpenAI tombe, vous tombez aussi. Si les conditions changent, vous n'avez rien à dire.</p>
    </div>
    <div class="prob-card">
      <div class="prob-icon">🌍</div>
      <h3>Hors de portée pour beaucoup</h3>
      <p>Dans de nombreux pays, 20€/mois représente un salaire hebdomadaire entier.</p>
    </div>
  </div>
</div>

<!-- HOW IT WORKS -->
<div class="section" id="comment" style="padding-top: 0">
  <div class="eyebrow">La solution</div>
  <h2>Un réseau IA comme BitTorrent,<br>mais pour votre cerveau.</h2>
  <p class="section-sub">Imaginez un village où chacun prête un peu de sa puissance de calcul aux autres. En échange, quand vous avez besoin d'IA, le village travaille pour vous.</p>

  <div class="mesh-wrap">
    <svg class="mesh-svg" viewBox="0 0 700 260" preserveAspectRatio="none">
      <line x1="350" y1="130" x2="105" y2="55"  stroke="rgba(124,109,235,0.3)" stroke-width="1.5" stroke-dasharray="6,4"/>
      <line x1="350" y1="130" x2="595" y2="46"  stroke="rgba(124,109,235,0.3)" stroke-width="1.5" stroke-dasharray="6,4"/>
      <line x1="350" y1="130" x2="68"  y2="160" stroke="rgba(93,216,168,0.22)" stroke-width="1"   stroke-dasharray="6,4"/>
      <line x1="350" y1="130" x2="632" y2="155" stroke="rgba(93,216,168,0.22)" stroke-width="1"   stroke-dasharray="6,4"/>
      <line x1="350" y1="130" x2="204" y2="238" stroke="rgba(93,216,168,0.22)" stroke-width="1"   stroke-dasharray="6,4"/>
      <line x1="350" y1="130" x2="500" y2="240" stroke="rgba(93,216,168,0.22)" stroke-width="1"   stroke-dasharray="6,4"/>
    </svg>
    <div class="mesh-node n-you">⚡<br>Vous</div>
    <div class="mesh-node n-peer p1">🖥️<br>GPU A</div>
    <div class="mesh-node n-peer p2">💻<br>GPU B</div>
    <div class="mesh-node n-peer p3">🖥️<br>CPU C</div>
    <div class="mesh-node n-peer p4">💻<br>GPU D</div>
    <div class="mesh-node n-peer p5">🖥️<br>CPU E</div>
    <div class="mesh-node n-peer p6">💻<br>GPU F</div>
  </div>

  <div class="steps">
    <div class="step">
      <div class="step-num">1</div>
      <div>
        <h3>Vous rejoignez le réseau en 30 secondes</h3>
        <p>Une seule commande installe AInonymous. Votre machine reçoit une identité cryptographique unique — pas de nom, pas d'e-mail, pas de mot de passe. Vous êtes souverain.</p>
        <span class="tag t-p">Holochain · clé ed25519</span>
      </div>
    </div>
    <div class="step">
      <div class="step-num">2</div>
      <div>
        <h3>Votre machine contribue, les autres aussi</h3>
        <p>Quand vous n'utilisez pas votre ordinateur, il prête sa puissance de calcul aux autres membres. En échange, vous accumulez du crédit de calcul. Aucun argent ne s'échange — c'est du troc de GPU.</p>
        <span class="tag t-t">Mesh P2P · DHT Holochain</span>
      </div>
    </div>
    <div class="step">
      <div class="step-num">3</div>
      <div>
        <h3>Vous utilisez l'IA comme si c'était local</h3>
        <p>Envoyez votre question. Le réseau la distribue entre plusieurs machines — chacune traite une partie du modèle IA. Vous recevez la réponse en temps réel. Aucun serveur central n'a vu votre question.</p>
        <span class="tag t-c">Pipeline-splitting · Gemma 4</span>
      </div>
    </div>
  </div>
</div>

<!-- FEATURES -->
<div class="section" id="pourquoi" style="padding-top: 0">
  <div class="eyebrow">Pourquoi AInonymous</div>
  <h2>Tout ce qu'on vous a retiré,<br>on vous le rend.</h2>
  <p class="section-sub">Pas un concurrent de ChatGPT — une alternative fondamentalement différente dans sa philosophie.</p>
  <div class="feat-grid">
    <div class="feat-card">
      <div class="feat-icon fi-p">🔐</div>
      <h3>Confidentialité absolue</h3>
      <p>Vos questions ne quittent jamais le réseau chiffré. Aucune entreprise, aucun gouvernement ne peut lire votre conversation. Pas de logs, pas de cookies, pas de profil.</p>
    </div>
    <div class="feat-card">
      <div class="feat-icon fi-t">⚡</div>
      <h3>Gratuit par construction</h3>
      <p>Il n'y a personne à payer. Pas d'actionnaires, pas d'infrastructure cloud à financer. Vous échangez de la puissance de calcul, pas de l'argent.</p>
    </div>
    <div class="feat-card">
      <div class="feat-icon fi-c">🌐</div>
      <h3>Indestructible</h3>
      <p>Il n'y a pas de bouton «off». Aucune entreprise ne peut fermer le réseau, censurer un utilisateur ou modifier les règles du jeu du jour au lendemain.</p>
    </div>
    <div class="feat-card">
      <div class="feat-icon fi-a">🤖</div>
      <h3>Les meilleurs modèles ouverts</h3>
      <p>Gemma 4 (Google), Qwen3, Llama 3.3 — les modèles utilisés rivalisent avec GPT-4 sur de nombreuses tâches, et ils sont 100% open source.</p>
    </div>
    <div class="feat-card">
      <div class="feat-icon fi-p">🔌</div>
      <h3>Compatible avec vos outils</h3>
      <p>L'API locale est compatible OpenAI. Branchez n'importe quel outil qui parle à ChatGPT — il parlera automatiquement à AInonymous, sans modification.</p>
    </div>
    <div class="feat-card">
      <div class="feat-icon fi-t">🌍</div>
      <h3>Accessible partout</h3>
      <p>Pas de carte de crédit, pas de compte, pas de frontières. Si vous avez un ordinateur et Internet, vous accédez à l'IA la plus puissante du moment.</p>
    </div>
  </div>
</div>

<!-- VS TABLE -->
<div class="vs-wrap">
  <div class="vs-inner">
    <div class="eyebrow">Comparaison</div>
    <h2>AInonymous vs. les alternatives</h2>
    <table>
      <thead>
        <tr>
          <th>Critère</th>
          <th>ChatGPT / Claude</th>
          <th>Ollama (local)</th>
          <th class="hl">AInonymous ✦</th>
        </tr>
      </thead>
      <tbody>
        <tr><td>Prix</td><td>20€/mois</td><td>Gratuit</td><td class="col ok">Gratuit</td></tr>
        <tr><td>Confidentialité</td><td><span class="no">✗ Données envoyées</span></td><td><span class="ok">✓ 100% local</span></td><td class="col ok">✓ Chiffré P2P</td></tr>
        <tr><td>GPU requis</td><td>Non (cloud)</td><td>Oui (≥8 GB VRAM)</td><td class="col ok">Non (mutualisé)</td></tr>
        <tr><td>Résistance censure</td><td><span class="no">✗ Centralisé</span></td><td><span class="ok">✓ Local</span></td><td class="col ok">✓ Décentralisé</td></tr>
        <tr><td>Qualité modèle</td><td>Très haute</td><td>Haute (selon GPU)```

</details>

<details>
<summary><strong>📄 site/landing-en.html</strong> — Landing page (EN)</summary>

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>AInonymous — The AI that belongs to you</title>
  <style>
    :root {
      --bg: #0a0a0f;
      --bg2: #12121a;
      --bg3: #1a1a28;
      --border: rgba(255,255,255,0.08);
      --border2: rgba(255,255,255,0.14);
      --text: #f0eff8;
      --muted: #8887a0;
      --accent: #7c6deb;
      --accent2: #5dd8a8;
      --accent3: #eb6d7c;
      --accent-glow: rgba(124,109,235,0.18);
    }
    *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
    html { scroll-behavior: smooth; }
    body {
      font-family: 'Segoe UI', system-ui, -apple-system, sans-serif;
      background: var(--bg); color: var(--text);
      line-height: 1.7; -webkit-font-smoothing: antialiased;
    }
    a { color: inherit; text-decoration: none; }

    /* NAV */
    nav {
      position: sticky; top: 0; z-index: 100;
      display: flex; align-items: center; justify-content: space-between;
      padding: 14px 40px;
      background: rgba(10,10,15,0.9);
      backdrop-filter: blur(12px);
      border-bottom: 1px solid var(--border);
    }
    .nav-logo { font-size: 18px; font-weight: 700; letter-spacing: -0.5px; }
    .nav-logo span { color: var(--accent); }
    .nav-links { display: flex; gap: 28px; font-size: 14px; color: var(--muted); align-items: center; }
    .nav-links a:hover { color: var(--text); }
    .nav-cta {
      background: var(--accent); color: #fff;
      padding: 8px 18px; border-radius: 8px;
      font-size: 14px; font-weight: 600; transition: opacity .15s;
    }
    .nav-cta:hover { opacity: .85; }
    .lang-switch {
      display: flex; align-items: center; gap: 2px;
      font-size: 12px; font-weight: 700;
      background: var(--bg3); border: 1px solid var(--border2);
      border-radius: 7px; overflow: hidden;
    }
    .lang-switch a {
      padding: 4px 9px; color: var(--muted); transition: background .15s, color .15s;
    }
    .lang-switch a.active {
      background: var(--accent); color: #fff;
    }
    .lang-switch a:hover:not(.active) { color: var(--text); }

    /* LAYOUT */
    .section { padding: 96px 40px; max-width: 1080px; margin: 0 auto; }
    .eyebrow {
      font-size: 12px; font-weight: 700; letter-spacing: 2px;
      text-transform: uppercase; color: var(--accent2); margin-bottom: 14px;
    }
    h2 {
      font-size: clamp(28px, 4vw, 44px); font-weight: 800;
      letter-spacing: -1.5px; line-height: 1.15; margin-bottom: 16px;
    }
    .section-sub {
      font-size: 18px; color: var(--muted); max-width: 560px;
      line-height: 1.6; margin-bottom: 56px;
    }

    /* HERO */
    .hero {
      text-align: center;
      padding: 120px 40px 96px; max-width: 860px; margin: 0 auto;
    }
    .hero-badge {
      display: inline-flex; align-items: center; gap: 8px;
      font-size: 13px; color: var(--accent2); font-weight: 500;
      background: rgba(93,216,168,0.1); border: 1px solid rgba(93,216,168,0.3);
      padding: 5px 14px; border-radius: 20px; margin-bottom: 32px;
    }
    .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--accent2); animation: pulse 2s infinite; }
    @keyframes pulse { 0%,100%{opacity:1}50%{opacity:.3} }
    h1 {
      font-size: clamp(40px, 7vw, 72px); font-weight: 800;
      line-height: 1.08; letter-spacing: -2.5px; margin-bottom: 24px;
    }
    .a1 { color: var(--accent); }
    .a2 { color: var(--accent2); }
    .hero-sub {
      font-size: clamp(17px, 2.2vw, 21px); color: var(--muted);
      max-width: 600px; margin: 0 auto 44px; line-height: 1.6;
    }
    .hero-actions { display: flex; gap: 14px; justify-content: center; flex-wrap: wrap; }
    .btn-p {
      background: var(--accent); color: #fff;
      padding: 14px 28px; border-radius: 10px; font-weight: 700; font-size: 15px;
      display: inline-flex; align-items: center; gap: 8px;
      transition: transform .15s, opacity .15s;
    }
    .btn-p:hover { transform: translateY(-1px); opacity: .9; }
    .btn-g {
      border: 1px solid var(--border2); color: var(--muted);
      padding: 14px 28px; border-radius: 10px; font-weight: 600; font-size: 15px;
      display: inline-flex; align-items: center; gap: 8px;
      transition: border-color .15s, color .15s;
    }
    .btn-g:hover { border-color: var(--accent); color: var(--text); }

    /* STATS BAR */
    .stats-bar {
      display: flex; justify-content: center; gap: 60px; flex-wrap: wrap;
      padding: 40px; background: var(--bg2);
      border-top: 1px solid var(--border); border-bottom: 1px solid var(--border);
    }
    .stat { text-align: center; }
    .stat-num { font-size: 32px; font-weight: 800; letter-spacing: -1px; color: var(--accent); }
    .stat-lbl { font-size: 13px; color: var(--muted); margin-top: 2px; }

    /* PROBLEM CARDS */
    .prob-grid {
      display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
      gap: 18px; margin-top: 48px;
    }
    .prob-card {
      background: var(--bg2); border: 1px solid rgba(235,109,124,0.25);
      border-radius: 14px; padding: 28px 22px;
    }
    .prob-icon { font-size: 26px; margin-bottom: 12px; }
    .prob-card h3 { font-size: 15px; font-weight: 700; margin-bottom: 7px; }
    .prob-card p { font-size: 13px; color: var(--muted); line-height: 1.6; }

    /* MESH VISUAL */
    .mesh-wrap {
      position: relative; height: 260px;
      display: flex; align-items: center; justify-content: center;
      margin: 56px 0;
    }
    .mesh-svg { position: absolute; inset: 0; width: 100%; height: 100%; }
    .mesh-node {
      position: absolute; border-radius: 50%;
      display: flex; flex-direction: column; align-items: center;
      justify-content: center; text-align: center;
      font-size: 11px; font-weight: 600; border: 1px solid; line-height: 1.3;
    }
    .n-you {
      width: 80px; height: 80px;
      background: rgba(124,109,235,0.2); border-color: var(--accent); color: var(--accent);
      left: 50%; top: 50%; transform: translate(-50%,-50%); z-index: 2; font-size: 13px;
    }
    .n-peer {
      width: 56px; height: 56px;
      background: rgba(93,216,168,0.1); border-color: var(--accent2); color: var(--accent2);
    }
    .p1{left:13%;top:18%} .p2{right:15%;top:14%} .p3{left:6%;top:54%}
    .p4{right:10%;top:50%} .p5{left:26%;bottom:8%} .p6{right:25%;bottom:6%}

    /* STEPS */
    .steps { display: flex; flex-direction: column; }
    .step {
      display: grid; grid-template-columns: 72px 1fr; gap: 24px;
      padding: 36px 0; border-bottom: 1px solid var(--border);
    }
    .step:last-child { border-bottom: none; }
    .step-num {
      width: 50px; height: 50px; border-radius: 13px; flex-shrink: 0;
      background: var(--accent-glow); border: 1px solid rgba(124,109,235,0.4);
      display: flex; align-items: center; justify-content: center;
      font-size: 20px; font-weight: 800; color: var(--accent);
    }
    .step h3 { font-size: 19px; font-weight: 700; margin-bottom: 7px; }
    .step p { font-size: 14px; color: var(--muted); line-height: 1.7; }
    .tag {
      display: inline-block; margin-top: 10px; font-size: 12px;
      padding: 3px 10px; border-radius: 6px; font-weight: 600;
    }
    .t-p { background: rgba(124,109,235,0.15); color: #a89cf5; }
    .t-t { background: rgba(93,216,168,0.15); color: #5dd8a8; }
    .t-c { background: rgba(235,109,124,0.15); color: #eb8090; }

    /* FEATURES */
    .feat-grid {
      display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
      gap: 18px;
    }
    .feat-card {
      background: var(--bg2); border: 1px solid var(--border);
      border-radius: 16px; padding: 30px 26px; transition: border-color .2s;
    }
    .feat-card:hover { border-color: var(--border2); }
    .feat-icon {
      width: 42px; height: 42px; border-radius: 11px; margin-bottom: 16px;
      display: flex; align-items: center; justify-content: center; font-size: 19px;
    }
    .fi-p { background: rgba(124,109,235,0.15); }
    .fi-t { background: rgba(93,216,168,0.15); }
    .fi-c { background: rgba(235,109,124,0.15); }
    .fi-a { background: rgba(250,199,117,0.15); }
    .feat-card h3 { font-size: 16px; font-weight: 700; margin-bottom: 8px; }
    .feat-card p { font-size: 13px; color: var(--muted); line-height: 1.65; }

    /* VS TABLE */
    .vs-wrap {
      background: var(--bg2); padding: 80px 40px;
      border-top: 1px solid var(--border); border-bottom: 1px solid var(--border);
    }
    .vs-inner { max-width: 900px; margin: 0 auto; }
    table { width: 100%; border-collapse: collapse; margin-top: 48px; font-size: 14px; }
    th {
      text-align: left; padding: 12px 18px; font-weight: 700;
      border-bottom: 2px solid var(--border2); color: var(--muted);
    }
    th.hl { color: var(--accent); }
    td { padding: 12px 18px; border-bottom: 1px solid var(--border); }
    tr:last-child td { border-bottom: none; }
    .ok { color: var(--accent2); font-weight: 600; }
    .no { color: var(--accent3); }
    .col { background: rgba(124,109,235,0.06); }

    /* MODELS */
    .models-grid {
      display: grid; grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));
      gap: 14px; margin-top: 48px;
    }
    .model-card {
      background: var(--bg2); border: 1px solid var(--border);
      border-radius: 12px; padding: 20px 18px;
    }
    .model-name { font-size: 14px; font-weight: 700; margin-bottom: 3px; }
    .model-meta { font-size: 12px; color: var(--muted); margin-bottom: 12px; }
    .bar-bg { background: var(--bg3); border-radius: 4px; height: 5px; }
    .bar { height: 5px; border-radius: 4px; background: var(--accent2); }
    .bar-p { background: #a89cf5; }
    .bar-c { background: #f5c4b3; }
    .model-use { font-size: 12px; color: var(--muted); margin-top: 6px; }

    /* INSTALL */
    .install-box {
      background: var(--bg3); border: 1px solid var(--border2);
      border-radius: 14px; padding: 32px;
    }
    pre {
      font-family: 'JetBrains Mono','Fira Code',monospace;
      font-size: 13px; line-height: 2; color: var(--accent2);
      overflow-x: auto; white-space: pre;
    }
    .comment { color: rgba(136,135,160,0.45); }

    /* FOOTER */
    footer {
      padding: 48px 40px; border-top: 1px solid var(--border);
      text-align: center; color: var(--muted); font-size: 14px;
    }
    .f-logo { font-size: 20px; font-weight: 800; margin-bottom: 10px; }
    .f-logo span { color: var(--accent); }
    footer a { color: var(--muted); margin: 0 12px; }
    footer a:hover { color: var(--text); }

    /* BUSINESS CASE */
    .biz-banner {
      background: linear-gradient(135deg, #0e0e1a 0%, #13102a 100%);
      border-top: 1px solid rgba(124,109,235,0.25);
      border-bottom: 1px solid rgba(124,109,235,0.25);
      padding: 80px 40px;
    }
    .biz-inner { max-width: 1080px; margin: 0 auto; }
    .biz-top {
      display: grid; grid-template-columns: 1fr 1fr; gap: 60px;
      align-items: start; margin-bottom: 56px;
    }
    .biz-kpi-grid {
      display: grid; grid-template-columns: 1fr 1fr; gap: 14px;
    }
    .biz-kpi {
      background: rgba(124,109,235,0.08); border: 1px solid rgba(124,109,235,0.2);
      border-radius: 12px; padding: 20px 18px;
    }
    .biz-kpi-num {
      font-size: 30px; font-weight: 800; color: var(--accent);
      letter-spacing: -1px; margin-bottom: 4px;
    }
    .biz-kpi-lbl { font-size: 12px; color: var(--muted); line-height: 1.4; }
    .biz-adv-grid {
      display: grid; grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
      gap: 16px; margin-bottom: 48px;
    }
    .biz-adv {
      background: var(--bg2); border: 1px solid var(--border);
      border-radius: 14px; padding: 26px 22px;
      transition: border-color .2s;
    }
    .biz-adv:hover { border-color: rgba(124,109,235,0.4); }
    .biz-adv-icon { font-size: 22px; margin-bottom: 12px; }
    .biz-adv h3 { font-size: 15px; font-weight: 700; margin-bottom: 6px; }
    .biz-adv p { font-size: 13px; color: var(--muted); line-height: 1.6; }
    .biz-cta-box {
      background: rgba(124,109,235,0.07); border: 1px solid rgba(124,109,235,0.3);
      border-radius: 16px; padding: 36px 32px;
      display: flex; align-items: center; justify-content: space-between;
      gap: 24px; flex-wrap: wrap;
    }
    .biz-cta-box h3 { font-size: 22px; font-weight: 800; letter-spacing: -0.5px; margin-bottom: 6px; }
    .biz-cta-box p { font-size: 14px; color: var(--muted); max-width: 500px; }
    .btn-enterprise {
      background: var(--accent); color: #fff; white-space: nowrap;
      padding: 14px 28px; border-radius: 10px; font-weight: 700; font-size: 15px;
      display: inline-flex; align-items: center; gap: 8px; flex-shrink: 0;
      transition: transform .15s, opacity .15s;
    }
    .btn-enterprise:hover { transform: translateY(-1px); opacity: .9; }

    /* CLOSED LOOP DIAGRAM */
    .loop-diagram {
      display: flex; align-items: center; justify-content: center;
      gap: 0; margin: 40px 0 16px; flex-wrap: wrap;
    }
    .loop-node {
      background: var(--bg2); border: 1px solid rgba(124,109,235,0.3);
      border-radius: 12px; padding: 14px 18px; text-align: center;
      min-width: 120px;
    }
    .loop-node-icon { font-size: 22px; margin-bottom: 6px; }
    .loop-node-lbl { font-size: 11px; font-weight: 700; color: var(--text); }
    .loop-node-sub { font-size: 10px; color: var(--muted); margin-top: 2px; }
    .loop-arrow {
      font-size: 18px; color: rgba(124,109,235,0.5); padding: 0 10px;
      flex-shrink: 0;
    }
    .loop-shield {
      display: inline-flex; align-items: center; gap: 6px;
      font-size: 11px; font-weight: 600; color: var(--accent2);
      background: rgba(93,216,168,0.08); border: 1px solid rgba(93,216,168,0.2);
      padding: 4px 12px; border-radius: 20px; margin-top: 16px;
    }

    @media (max-width: 680px) {
      nav { padding: 12px 20px; }
      .nav-links { display: none; }
      .section { padding: 60px 20px; }
      .stats-bar { gap: 30px; padding: 28px 20px; }
      .vs-wrap { padding: 60px 20px; }
      .step { grid-template-columns: 56px 1fr; gap: 16px; }
      .install-box { padding: 20px 16px; }
      .biz-banner { padding: 60px 20px; }
      .biz-top { grid-template-columns: 1fr; gap: 36px; }
      .biz-cta-box { flex-direction: column; }
      .loop-diagram { gap: 8px; }
    }
  </style>
</head>
<body>

<nav>
  <div class="nav-logo">AI<span>n</span>onymous</div>
  <div class="nav-links">
    <a href="#how-it-works">How it works</a>
    <a href="#why-us">Why us</a>
    <a href="#install">Install</a>
    <a href="https://github.com/Geoking2104/AInonymous">GitHub</a>
    <a href="enterprise-en.html" style="color: var(--accent2); font-weight: 600;">Enterprise ✦</a>
    <div class="lang-switch">
      <a href="landing.html">FR</a>
      <a href="landing-en.html" class="active">EN</a>
    </div>
  </div>
  <a href="https://github.com/Geoking2104/AInonymous" class="nav-cta">Get started →</a>
</nav>

<!-- HERO -->
<div class="hero">
  <div class="hero-badge"><div class="dot"></div>Open source · Apache 2.0 · Public Beta</div>
  <h1>Powerful AI,<br><span class="a1">no server.</span><br><span class="a2">No account.</span></h1>
  <p class="hero-sub">
    AInonymous is a peer-to-peer network where your neighbors run AI for you, and you for them — with no company in the middle.
  </p>
  <div class="hero-actions">
    <a href="#install" class="btn-p">⚡ Join the network</a>
    <a href="#how-it-works" class="btn-g">See how it works</a>
  </div>
</div>

<!-- STATS -->
<div class="stats-bar">
  <div class="stat"><div class="stat-num">0</div><div class="stat-lbl">Central server</div></div>
  <div class="stat"><div class="stat-num">$0</div><div class="stat-lbl">Subscription</div></div>
  <div class="stat"><div class="stat-num">0</div><div class="stat-lbl">Data collected</div></div>
  <div class="stat"><div class="stat-num">100%</div><div class="stat-lbl">Open source</div></div>
</div>

<!-- PROBLEM -->
<div class="section">
  <div class="eyebrow">The problem</div>
  <h2>Today's AI is watching you.<br>And charging you for it.</h2>
  <p class="section-sub">ChatGPT, Claude, Gemini — excellent services, but they run through centralized servers that log your conversations and bill you every month.</p>
  <div class="prob-grid">
    <div class="prob-card">
      <div class="prob-icon">👁️</div>
      <h3>Your conversations are read</h3>
      <p>Everything you type is stored and analyzed to train future models.</p>
    </div>
    <div class="prob-card">
      <div class="prob-icon">🏦</div>
      <h3>A lifelong subscription</h3>
      <p>~$20/month on average. Stop paying and you lose access. Forever.</p>
    </div>
    <div class="prob-card">
      <div class="prob-icon">🔒</div>
      <h3>A single point of failure</h3>
      <p>If OpenAI goes down, you go down too. If terms change, you have no say.</p>
    </div>
    <div class="prob-card">
      <div class="prob-icon">🌍</div>
      <h3>Out of reach for many</h3>
      <p>In many countries, $20/month represents an entire week's wages.</p>
    </div>
  </div>
</div>

<!-- HOW IT WORKS -->
<div class="section" id="how-it-works" style="padding-top: 0">
  <div class="eyebrow">The solution</div>
  <h2>An AI network like BitTorrent,<br>but for your brain.</h2>
  <p class="section-sub">Imagine a village where everyone lends a little compute power to their neighbors. In return, when you need AI, the village works for you.</p>

  <div class="mesh-wrap">
    <svg class="mesh-svg" viewBox="0 0 700 260" preserveAspectRatio="none">
      <line x1="350" y1="130" x2="105" y2="55"  stroke="rgba(124,109,235,0.3)" stroke-width="1.5" stroke-dasharray="6,4"/>
      <line x1="350" y1="130" x2="595" y2="46"  stroke="rgba(124,109,235,0.3)" stroke-width="1.5" stroke-dasharray="6,4"/>
      <line x1="350" y1="130" x2="68"  y2="160" stroke="rgba(93,216,168,0.22)" stroke-width="1"   stroke-dasharray="6,4"/>
      <line x1="350" y1="130" x2="632" y2="155" stroke="rgba(93,216,168,0.22)" stroke-width="1"   stroke-dasharray="6,4"/>
      <line x1="350" y1="130" x2="204" y2="238" stroke="rgba(93,216,168,0.22)" stroke-width="1"   stroke-dasharray="6,4"/>
      <line x1="350" y1="130" x2="500" y2="240" stroke="rgba(93,216,168,0.22)" stroke-width="1"   stroke-dasharray="6,4"/>
    </svg>
    <div class="mesh-node n-you">⚡<br>You</div>
    <div class="mesh-node n-peer p1">🖥️<br>GPU A</div>
    <div class="mesh-node n-peer p2">💻<br>GPU B</div>
    <div class="mesh-node n-peer p3">🖥️<br>CPU C</div>
    <div class="mesh-node n-peer p4">💻<br>GPU D</div>
    <div class="mesh-node n-peer p5">🖥️<br>CPU E</div>
    <div class="mesh-node n-peer p6">💻<br>GPU F</div>
  </div>

  <div class="steps">
    <div class="step">
      <div class="step-num">1</div>
      <div>
        <h3>Join the network in 30 seconds</h3>
        <p>A single command installs AInonymous. Your machine receives a unique cryptographic identity — no name, no email, no password. You are sovereign.</p>
        <span class="tag t-p">Holochain · ed25519 key</span>
      </div>
    </div>
    <div class="step">
      <div class="step-num">2</div>
      <div>
        <h3>Your machine contributes — and so do others</h3>
        <p>When you're not using your computer, it lends compute power to other members. In return, you accumulate compute credits. No money changes hands — it's GPU bartering.</p>
        <span class="tag t-t">P2P Mesh · Holochain DHT</span>
      </div>
    </div>
    <div class="step">
      <div class="step-num">3</div>
      <div>
        <h3>Use AI as if it were running locally</h3>
        <p>Send your question. The network distributes it across multiple machines — each handling a slice of the AI model. You get the response in real time. No central server ever saw your query.</p>
        <span class="tag t-c">Pipeline-splitting · Gemma 4</span>
      </div>
    </div>
  </div>
</div>

<!-- FEATURES -->
<div class="section" id="why-us" style="padding-top: 0">
  <div class="eyebrow">Why AInonymous</div>
  <h2>Everything that was taken from you,<br>given back.</h2>
  <p class="section-sub">Not a ChatGPT competitor — a fundamentally different alternative in its philosophy.</p>
  <div class="feat-grid">
    <div class="feat-card">
      <div class="feat-icon fi-p">🔐</div>
      <h3>Absolute privacy</h3>
      <p>Your questions never leave the encrypted network. No company, no government can read your conversation. No logs, no cookies, no profile.</p>
    </div>
    <div class="feat-card">
      <div class="feat-icon fi-t">⚡</div>
      <h3>Free by design</h3>
      <p>There's no one to pay. No shareholders, no cloud infrastructure to fund. You trade compute power, not money.</p>
    </div>
    <div class="feat-card">
      <div class="feat-icon fi-c">🌐</div>
      <h3>Unstoppable</h3>
      <p>There is no "off switch". No company can shut down the network, censor a user, or change the rules overnight.</p>
    </div>
    <div class="feat-card">
      <div class="feat-icon fi-a">🤖</div>
      <h3>The best open models</h3>
      <p>Gemma 4 (Google), Qwen3, Llama 3.3 — the models we use rival GPT-4 on many tasks, and they're 100% open source.</p>
    </div>
    <div class="feat-card">
      <div class="feat-icon fi-p">🔌</div>
      <h3>Works with your tools</h3>
      <p>The local API is OpenAI-compatible. Plug in any tool that talks to ChatGPT — it will talk to AInonymous automatically, with no modifications.</p>
    </div>
    <div class="feat-card">
      <div class="feat-icon fi-t">🌍</div>
      <h3>Accessible everywhere</h3>
      <p>No credit card, no account, no borders. If you have a computer and internet access, you can use the most powerful AI available today.</p>
    </div>
  </div>
</div>

<!-- VS TABLE -->
<div class="vs-wrap">
  <div class="vs-inner">
    <div class="eyebrow">Comparison</div>
    <h2>AInonymous vs. the alternatives</h2>
    <table>
      <thead>
        <tr>
          <th>Criteria</th>
          <th>ChatGPT / Claude</th>
          <th>Ollama (local)</th>
          <th class="hl">AInonymous ✦</th>
        </tr>
      </thead>
      <tbody>
        <tr><td>Price</td><td>~$20/month</td><td>Free</td><td class="col ok">Free</td></tr>
        <tr><td>Privacy</td><td><span class="no">✗ Data sent to servers</span></td><td><span class="ok">✓ 100% local</span></td><td class="col ok">✓ P2P encrypted</td></tr>
        <tr><td>GPU required</td><td>No (cloud)</td><td>Yes (≥8 GB VRAM)</td><td class="col ok">No (pooled)</td></tr>
        <tr><td>Censorship resistance</td><td><span class="no">✗ Centralized</span></td><td><span class="ok">✓ Local</span></td><td class="col ok">✓ Decentralized</td></tr>
        <tr><td>Model quality</td><td>Very high</td><td>High (GPU dependent)</td><td class="col ok">High (Gemma 4)</td></tr>
        <tr><td>Account required</td><td><span class="no">✗ Mandatory</span></td><td><span class="ok">✓ None</span></td><td class="col ok">✓ None</td></tr>
        <tr><td>OpenAI API compatible</td><td>Yes (paid)</td><td>Yes</td><td class="col ok">Yes (localhost)</td></tr>
      </tbody>
    </table>
  </div>
</div>

<!-- MODELS -->
<div class="section">
  <div class="eyebrow">Available models</div>
  <h2>State-of-the-art models,<br>completely free.</h2>
  <p class="section-sub">AInonymous prioritizes Google's Gemma 4 — performant, compact, Apache 2.0. Each model is automatically distributed based on available compute in the network.</p>
  <div class="models-grid">
    <div class="model-card">
      <div class="model-name">Gemma 4 · E2B</div>
      <div class="model-meta">~3 GB · Edge</div>
      <div class="bar-bg"><div class="bar" style="width:28%"></div></div>
      <div class="model-use">Light nodes · draft</div>
    </div>
    <div class="model-card">
      <div class="model-name">Gemma 4 · E4B</div>
      <div class="model-meta">~5 GB · Edge</div>
      <div class="bar-bg"><div class="bar" style="width:42%"></div></div>
      <div class="model-use">Speed + quality</div>
    </div>
    <div class="model-card">
      <div class="model-name">Gemma 4 · 26B MoE</div>
      <div class="model-meta">~18 GB · Multi-node</div>
      <div class="bar-bg"><div class="bar" style="width:70%"></div></div>
      <div class="model-use">Complex tasks</div>
    </div>
    <div class="model-card">
      <div class="model-name">Gemma 4 · 31B</div>
      <div class="model-meta">~20 GB · Flagship</div>
      <div class="bar-bg"><div class="bar" style="width:82%"></div></div>
      <div class="model-use">Best quality</div>
    </div>
    <div class="model-card">
      <div class="model-name">Qwen3 · 32B</div>
      <div class="model-meta">~20 GB · Code</div>
      <div class="bar-bg"><div class="bar bar-p" style="width:80%"></div></div>
      <div class="model-use">Code · Reasoning</div>
    </div>
    <div class="model-card">
      <div class="model-name">Llama 3.3 · 70B</div>
      <div class="model-meta">~43 GB · Multi-node</div>
      <div class="bar-bg"><div class="bar bar-c" style="width:94%"></div></div>
      <div class="model-use">Maximum power</div>
    </div>
  </div>
</div>

<!-- BUSINESS CASE -->
<div class="biz-banner" id="enterprise">
  <div class="biz-inner">
    <div class="eyebrow">For enterprises</div>
    <h2 style="margin-bottom: 14px">Your servers are already running.<br>Make them work for you.</h2>
    <p style="font-size:17px;color:var(--muted);max-width:600px;margin-bottom:48px;line-height:1.6">
      AInonymous Enterprise creates a closed, sovereign AI network inside your infrastructure.
      Your machines share compute power to run state-of-the-art models — without a single byte leaving your network.
    </p>

    <!-- Closed loop diagram -->
    <div style="text-align:center;margin-bottom:48px">
      <p style="font-size:12px;text-transform:uppercase;letter-spacing:1.5px;color:var(--muted);font-weight:700;margin-bottom:20px">Closed P2P loop — 100% on-premises</p>
      <div class="loop-diagram">
        <div class="loop-node">
          <div class="loop-node-icon">🖥️</div>
          <div class="loop-node-lbl">Workstations</div>
          <div class="loop-node-sub">idle GPU / CPU</div>
        </div>
        <div class="loop-arrow">→</div>
        <div class="loop-node" style="border-color:rgba(124,109,235,0.6);background:rgba(124,109,235,0.08)">
          <div class="loop-node-icon">⚡</div>
          <div class="loop-node-lbl">Private mesh</div>
          <div class="loop-node-sub">internal Holochain DHT</div>
        </div>
        <div class="loop-arrow">→</div>
        <div class="loop-node">
          <div class="loop-node-icon">🤖</div>
          <div class="loop-node-lbl">AI Models</div>
          <div class="loop-node-sub">Gemma 4 · Qwen3</div>
        </div>
        <div class="loop-arrow">→</div>
        <div class="loop-node">
          <div class="loop-node-icon">👥</div>
          <div class="loop-node-lbl">Your teams</div>
          <div class="loop-node-sub">OpenAI-compat. API</div>
        </div>
        <div class="loop-arrow" style="color:rgba(93,216,168,0.6)">↺</div>
      </div>
      <div class="loop-shield">🔒 Data confined to corporate network — zero external traffic</div>
    </div>

    <!-- KPIs + description -->
    <div class="biz-top">
      <div>
        <h3 style="font-size:20px;font-weight:700;margin-bottom:12px">The compute you're already paying for sleeps at night.</h3>
        <p style="font-size:14px;color:var(--muted);line-height:1.7;margin-bottom:16px">
          A 200-person company has on average several teraflops sitting idle between 7pm and 8am. AInonymous turns them into AI inference infrastructure — without buying a single additional GPU.
        </p>
        <p style="font-size:14px;color:var(--muted);line-height:1.7">
          The bigger your organization, the more powerful your AI. Every new machine is an additional compute unit. The network grows stronger with your headcount — not with a cloud vendor's credit card.
        </p>
      </div>
      <div class="biz-kpi-grid">
        <div class="biz-kpi">
          <div class="biz-kpi-num">~70%</div>
          <div class="biz-kpi-lbl">of enterprise CPU/GPU idle outside business hours</div>
        </div>
        <div class="biz-kpi">
          <div class="biz-kpi-num">$0</div>
          <div class="biz-kpi-lbl">additional cloud AI cost using existing hardware</div>
        </div>
        <div class="biz-kpi">
          <div class="biz-kpi-num">100%</div>
          <div class="biz-kpi-lbl">of data stays within your GDPR perimeter</div>
        </div>
        <div class="biz-kpi">
          <div class="biz-kpi-num">×N</div>
          <div class="biz-kpi-lbl">power scales with every new employee</div>
        </div>
      </div>
    </div>

    <!-- Business advantages -->
    <div class="biz-adv-grid">
      <div class="biz-adv">
        <div class="biz-adv-icon">🏛️</div>
        <h3>Complete data sovereignty</h3>
        <p>GDPR, HIPAA, SOC 2 compliance guaranteed by architecture. No data leaves your infrastructure — no Data Processing Agreement needed with any third party.</p>
      </div>
      <div class="biz-adv">
        <div class="biz-adv-icon">📈</div>
        <h3>AI that grows with your fleet</h3>
        <p>Every machine added increases inference capacity. No pricing tiers, no quota negotiation — power scales linearly with your headcount.</p>
      </div>
      <div class="biz-adv">
        <div class="biz-adv-icon">🔐</div>
        <h3>Enterprise cryptographic identities</h3>
        <p>Every node is authenticated via ed25519. Holochain Membrane Proofs enable granular access control: who can submit tasks, who can run models.</p>
      </div>
      <div class="biz-adv">
        <div class="biz-adv-icon">🧠</div>
        <h3>Fine-tuning on internal data</h3>
        <p>Models can be fine-tuned on your documents, processes, and domain vocabulary — without ever leaving the network. Your AI becomes an expert in your industry.</p>
      </div>
      <div class="biz-adv">
        <div class="biz-adv-icon">🔌</div>
        <h3>Transparent integration</h3>
        <p>Local OpenAI-compatible API on <code style="font-size:12px;color:var(--accent2)">localhost:9337</code>. Your existing tools (Copilot, n8n, LangChain, Cursor) connect with no changes.</p>
      </div>
      <div class="biz-adv">
        <div class="biz-adv-icon">📋</div>
        <h3>Immutable audit trail</h3>
        <p>Every inference task is cryptographically chained in the Holochain source chain. Full traceability for compliance and internal audits.</p>
      </div>
    </div>

    <!-- CTA -->
    <div class="biz-cta-box">
      <div>
        <h3>Private AI network for your enterprise</h3>
        <p>Explore the technical architecture, use cases, and detailed business model on our dedicated enterprise deployment page.</p>
      </div>
      <a href="enterprise-en.html" class="btn-enterprise">View Enterprise page →</a>
    </div>
  </div>
</div>

<!-- INSTALL -->
<div class="section" id="install" style="padding-top: 0">
  <div class="eyebrow">Installation</div>
  <h2>Ready in 30 seconds.</h2>
  <p class="section-sub">One command. Your machine joins the network, picks a model suited to its power, and starts contributing — and receiving.</p>
  <div class="install-box">
    <pre><span class="comment"># 1. Install AInonymous</span>
curl -fsSL https://ainonymous.network/install.sh | sh

<span class="comment"># 2. Join the network automatically</span>
ainonymous --auto

<span class="comment"># 3. Or pick your model</span>
ainonymous --model gemma4-26b-moe

<span class="comment"># 4. Test in 5 seconds</span>
curl http://localhost:9337/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"gemma4-31b","messages":[{"role":"user","content":"Hello!"}]}'</pre>
  </div>
  <div style="margin-top: 24px; display: flex; gap: 14px; flex-wrap: wrap;">
    <a href="https://github.com/Geoking2104/AInonymous" class="btn-p">View on GitHub →</a>
    <a href="https://github.com/Geoking2104/AInonymous/blob/main/README.md" class="btn-g">Technical documentation</a>
  </div>
</div>

<footer>
  <div class="f-logo">AI<span>n</span>onymous</div>
  <p style="margin-bottom: 16px">Decentralized AI, for everyone, forever.</p>
  <div>
    <a href="https://github.com/Geoking2104/AInonymous">GitHub</a>
    <a href="https://github.com/Geoking2104/AInonymous/blob/main/README.md">Docs</a>
    <a href="enterprise-en.html">Enterprise</a>
    <a href="https://github.com/Geoking2104/AInonymous/blob/main/LICENSE">Apache 2.0</a>
  </div>
  <p style="margin-top: 28px; font-size: 12px; opacity: .4">Built with Holochain · Gemma 4 · Goose · llama.cpp</p>
</footer>

</body>
</html>
```

</details>

<details>
<summary><strong>📄 site/enterprise.html</strong> — Page Enterprise (FR)</summary>

```html
<!DOCTYPE html>
<html lang="fr">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>AInonymous Enterprise — IA privée, souveraine, on-premises</title>
  <style>
    :root {
      --bg: #08080e;
      --bg2: #10101a;
      --bg3: #181826;
      --border: rgba(255,255,255,0.08);
      --border2: rgba(255,255,255,0.14);
      --text: #f0eff8;
      --muted: #8887a0;
      --accent: #7c6deb;
      --accent2: #5dd8a8;
      --accent3: #eb6d7c;
      --amber: #f5c475;
    }
    *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
    html { scroll-behavior: smooth; }
    body {
      font-family: 'Segoe UI', system-ui, -apple-system, sans-serif;
      background: var(--bg); color: var(--text);
      line-height: 1.7; -webkit-font-smoothing: antialiased;
    }
    a { color: inherit; text-decoration: none; }
    code { font-family: 'JetBrains Mono','Fira Code',monospace; font-size: .9em; color: var(--accent2); }

    /* NAV */
    nav {
      position: sticky; top: 0; z-index: 100;
      display: flex; align-items: center; justify-content: space-between;
      padding: 14px 40px; background: rgba(8,8,14,0.92);
      backdrop-filter: blur(12px);
      border-bottom: 1px solid var(--border);
    }
    .nav-logo { font-size: 18px; font-weight: 700; }
    .nav-logo span { color: var(--accent); }
    .nav-badge {
      font-size: 11px; font-weight: 700; padding: 3px 10px; border-radius: 6px;
      background: rgba(124,109,235,0.15); color: var(--accent); margin-left: 10px;
      border: 1px solid rgba(124,109,235,0.3);
    }
    .nav-links { display: flex; gap: 24px; font-size: 14px; color: var(--muted); align-items: center; }
    .nav-links a:hover { color: var(--text); }
    .lang-switch {
      display: flex; align-items: center; gap: 2px;
      font-size: 12px; font-weight: 700;
      background: var(--bg3); border: 1px solid var(--border2);
      border-radius: 7px; overflow: hidden;
    }
    .lang-switch a {
      padding: 4px 9px; color: var(--muted); transition: background .15s, color .15s;
    }
    .lang-switch a.active {
      background: var(--accent); color: #fff;
    }
    .lang-switch a:hover:not(.active) { color: var(--text); }
    .nav-back { font-size: 13px; color: var(--muted); display: flex; align-items: center; gap: 6px; }
    .nav-back:hover { color: var(--text); }

    /* LAYOUT */
    .section { padding: 88px 40px; max-width: 1080px; margin: 0 auto; }
    .section-narrow { padding: 88px 40px; max-width: 820px; margin: 0 auto; }
    .full { padding: 72px 40px; }
    .full-inner { max-width: 1080px; margin: 0 auto; }
    .eyebrow {
      font-size: 11px; font-weight: 700; letter-spacing: 2px;
      text-transform: uppercase; color: var(--accent2); margin-bottom: 14px;
    }
    h2 { font-size: clamp(26px, 3.8vw, 42px); font-weight: 800; letter-spacing: -1.5px; line-height: 1.15; margin-bottom: 16px; }
    h3 { font-size: 18px; font-weight: 700; margin-bottom: 8px; }
    .section-sub { font-size: 17px; color: var(--muted); max-width: 560px; line-height: 1.6; margin-bottom: 52px; }
    .divider { border: none; border-top: 1px solid var(--border); }

    /* HERO */
    .hero {
      padding: 110px 40px 80px; text-align: center; max-width: 900px; margin: 0 auto;
    }
    .hero-badge {
      display: inline-flex; align-items: center; gap: 7px;
      font-size: 12px; font-weight: 600; color: var(--accent);
      background: rgba(124,109,235,0.1); border: 1px solid rgba(124,109,235,0.3);
      padding: 5px 14px; border-radius: 20px; margin-bottom: 28px;
    }
    .hero-badge::before { content: '🏢'; font-size: 14px; }
    h1 { font-size: clamp(36px, 6vw, 64px); font-weight: 800; letter-spacing: -2px; line-height: 1.08; margin-bottom: 22px; }
    .a1 { color: var(--accent); } .a2 { color: var(--accent2); }
    .hero-sub { font-size: clamp(16px, 2.2vw, 20px); color: var(--muted); max-width: 620px; margin: 0 auto 40px; line-height: 1.6; }
    .hero-actions { display: flex; gap: 12px; justify-content: center; flex-wrap: wrap; }
    .btn-p { background: var(--accent); color: #fff; padding: 13px 26px; border-radius: 10px; font-weight: 700; font-size: 14px; display: inline-flex; align-items: center; gap: 7px; transition: opacity .15s, transform .15s; }
    .btn-p:hover { opacity: .88; transform: translateY(-1px); }
    .btn-g { border: 1px solid var(--border2); color: var(--muted); padding: 13px 26px; border-radius: 10px; font-weight: 600; font-size: 14px; display: inline-flex; align-items: center; gap: 7px; transition: border-color .15s, color .15s; }
    .btn-g:hover { border-color: var(--accent); color: var(--text); }

    /* PROOF STRIP */
    .proof-strip {
      background: var(--bg2); border-top: 1px solid var(--border); border-bottom: 1px solid var(--border);
      padding: 32px 40px;
      display: flex; justify-content: center; gap: 48px; flex-wrap: wrap;
    }
    .proof-item { display: flex; align-items: center; gap: 10px; font-size: 13px; color: var(--muted); }
    .proof-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--accent2); flex-shrink: 0; }

    /* CLOSED LOOP VISUAL */
    .loop-section { background: var(--bg2); padding: 72px 40px; border-top: 1px solid var(--border); border-bottom: 1px solid var(--border); }
    .loop-inner { max-width: 960px; margin: 0 auto; }
    .loop-grid {
      display: grid; grid-template-columns: 1fr 1fr; gap: 56px; align-items: center; margin-top: 48px;
    }
    .loop-diagram-v {
      display: flex; flex-direction: column; gap: 0; align-items: center;
    }
    .lnode {
      background: var(--bg3); border: 1px solid rgba(124,109,235,0.25);
      border-radius: 12px; padding: 14px 20px; width: 100%; text-align: center;
      position: relative;
    }
    .lnode-icon { font-size: 20px; margin-bottom: 5px; }
    .lnode-title { font-size: 13px; font-weight: 700; }
    .lnode-sub { font-size: 11px; color: var(--muted); margin-top: 2px; }
    .lnode.accent { background: rgba(124,109,235,0.1); border-color: rgba(124,109,235,0.5); }
    .lnode.accent2 { background: rgba(93,216,168,0.08); border-color: rgba(93,216,168,0.4); }
    .larrow { text-align: center; font-size: 16px; color: rgba(124,109,235,0.4); padding: 6px 0; }
    .larrow-closed { color: rgba(93,216,168,0.5); }
    .loop-desc h3 { font-size: 20px; font-weight: 700; margin-bottom: 14px; }
    .loop-desc p { font-size: 14px; color: var(--muted); line-height: 1.7; margin-bottom: 14px; }
    .loop-bullets { list-style: none; display: flex; flex-direction: column; gap: 10px; margin-top: 20px; }
    .loop-bullets li {
      display: flex; align-items: flex-start; gap: 10px;
      font-size: 13px; color: var(--muted); line-height: 1.5;
    }
    .bullet-ok { color: var(--accent2); font-size: 14px; margin-top: 1px; flex-shrink: 0; }

    /* ARCH DIAGRAM (ASCII-style) */
    .arch-box {
      background: var(--bg3); border: 1px solid var(--border2);
      border-radius: 14px; padding: 32px; font-family: 'JetBrains Mono','Fira Code',monospace;
      font-size: 12px; line-height: 1.8; color: var(--muted); overflow-x: auto;
    }
    .arch-box .c-acc { color: var(--accent); }
    .arch-box .c-teal { color: var(--accent2); }
    .arch-box .c-amber { color: var(--amber); }
    .arch-box .c-muted { color: rgba(136,135,160,0.4); }

    /* TECH CARDS */
    .tech-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 18px; }
    .tech-card {
      background: var(--bg2); border: 1px solid var(--border);
      border-radius: 14px; padding: 28px 24px; transition: border-color .2s;
    }
    .tech-card:hover { border-color: rgba(124,109,235,0.3); }
    .tech-layer {
      font-size: 10px; font-weight: 700; letter-spacing: 1.5px; text-transform: uppercase;
      padding: 3px 9px; border-radius: 5px; display: inline-block; margin-bottom: 14px;
    }
    .layer-infra { background: rgba(124,109,235,0.12); color: var(--accent); }
    .layer-model { background: rgba(93,216,168,0.12); color: var(--accent2); }
    .layer-sec   { background: rgba(235,109,124,0.12); color: var(--accent3); }
    .layer-api   { background: rgba(245,196,117,0.12); color: var(--amber); }
    .tech-card h3 { font-size: 15px; font-weight: 700; margin-bottom: 8px; }
    .tech-card p { font-size: 13px; color: var(--muted); line-height: 1.65; }
    .tech-tag { font-size: 11px; color: var(--muted); margin-top: 12px; font-family: monospace; }

    /* FUNCTIONAL FLOW */
    .flow-steps { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 2px; margin-top: 48px; }
    .flow-step {
      background: var(--bg2); border: 1px solid var(--border);
      padding: 24px 18px; position: relative; text-align: center;
    }
    .flow-step:first-child { border-radius: 14px 0 0 14px; }
    .flow-step:last-child  { border-radius: 0 14px 14px 0; }
    .flow-step::after {
      content: '→'; position: absolute; right: -14px; top: 50%;
      transform: translateY(-50%); z-index: 1;
      font-size: 18px; color: rgba(124,109,235,0.4);
    }
    .flow-step:last-child::after { display: none; }
    .fs-num {
      width: 28px; height: 28px; border-radius: 8px; margin: 0 auto 10px;
      background: rgba(124,109,235,0.15); display: flex; align-items: center; justify-content: center;
      font-size: 12px; font-weight: 800; color: var(--accent);
    }
    .flow-step h4 { font-size: 12px; font-weight: 700; margin-bottom: 5px; }
    .flow-step p { font-size: 11px; color: var(--muted); line-height: 1.5; }

    /* SECURITY TABLE */
    .sec-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 14px; margin-top: 40px; }
    .sec-card {
      background: var(--bg2); border: 1px solid rgba(235,109,124,0.18);
      border-radius: 12px; padding: 22px 20px;
    }
    .sec-icon { font-size: 20px; margin-bottom: 10px; }
    .sec-card h3 { font-size: 14px; font-weight: 700; margin-bottom: 6px; }
    .sec-card p { font-size: 12px; color: var(--muted); line-height: 1.6; }
    .sec-badge {
      display: inline-block; margin-top: 10px; font-size: 10px; font-weight: 700;
      padding: 2px 8px; border-radius: 5px;
      background: rgba(93,216,168,0.12); color: var(--accent2);
    }

    /* BUSINESS CASE TABLE */
    .bc-table-wrap { overflow-x: auto; margin-top: 44px; }
    table { width: 100%; border-collapse: collapse; font-size: 14px; min-width: 640px; }
    th { padding: 12px 16px; font-weight: 700; border-bottom: 2px solid var(--border2); text-align: left; color: var(--muted); }
    th.hl { color: var(--accent); }
    td { padding: 12px 16px; border-bottom: 1px solid var(--border); vertical-align: top; }
    tr:last-child td { border-bottom: none; }
    .ok { color: var(--accent2); font-weight: 600; }
    .v-ok { color: var(--accent2); }
    .v-no { color: var(--accent3); }
    .col { background: rgba(124,109,235,0.05); }
    tr.section-row td { background: var(--bg3); font-weight: 700; font-size: 12px; text-transform: uppercase; letter-spacing: 1px; color: var(--muted); padding: 8px 16px; }

    /* ROI SECTION */
    .roi-strip {
      background: var(--bg2); padding: 72px 40px;
      border-top: 1px solid var(--border); border-bottom: 1px solid var(--border);
    }
    .roi-inner { max-width: 1080px; margin: 0 auto; }
    .roi-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 14px; margin-top: 44px; }
    .roi-card {
      background: var(--bg); border: 1px solid rgba(93,216,168,0.2);
      border-radius: 12px; padding: 22px 18px; text-align: center;
    }
    .roi-num { font-size: 32px; font-weight: 800; color: var(--accent2); letter-spacing: -1px; }
    .roi-label { font-size: 12px; color: var(--muted); margin-top: 4px; line-height: 1.4; }
    .roi-scenario {
      background: var(--bg); border: 1px solid var(--border);
      border-radius: 14px; padding: 28px; margin-top: 28px;
    }
    .roi-scenario h3 { font-size: 16px; font-weight: 700; margin-bottom: 16px; color: var(--accent2); }
    .roi-row {
      display: flex; justify-content: space-between; align-items: baseline;
      padding: 8px 0; border-bottom: 1px solid var(--border); font-size: 13px;
    }
    .roi-row:last-child { border-bottom: none; font-weight: 700; color: var(--accent2); font-size: 15px; }
    .roi-row span:first-child { color: var(--muted); }
    .roi-row span:last-child { font-weight: 600; }

    /* COMPLIANCE */
    .compliance-grid {
      display: flex; flex-wrap: wrap; gap: 10px; margin-top: 32px;
    }
    .comp-badge {
      background: var(--bg2); border: 1px solid var(--border2);
      border-radius: 10px; padding: 10px 16px; font-size: 13px; font-weight: 600;
      display: flex; align-items: center; gap: 7px;
    }
    .comp-check { color: var(--accent2); font-size: 14px; }

    /* DEPLOYMENT MODES */
    .deploy-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); gap: 16px; margin-top: 44px; }
    .deploy-card {
      background: var(--bg2); border: 1px solid var(--border);
      border-radius: 14px; padding: 28px 24px; position: relative; overflow: hidden;
    }
    .deploy-card.featured { border-color: rgba(124,109,235,0.45); }
    .deploy-card.featured::before {
      content: 'Recommandé'; position: absolute; top: 14px; right: 14px;
      font-size: 10px; font-weight: 700; padding: 3px 8px; border-radius: 5px;
      background: rgba(124,109,235,0.18); color: var(--accent);
    }
    .deploy-icon { font-size: 26px; margin-bottom: 14px; }
    .deploy-card h3 { font-size: 16px; font-weight: 700; margin-bottom: 8px; }
    .deploy-card p { font-size: 13px; color: var(--muted); line-height: 1.6; margin-bottom: 14px; }
    .deploy-tags { display: flex; flex-wrap: wrap; gap: 6px; }
    .dtag { font-size: 11px; padding: 2px 8px; border-radius: 5px; font-weight: 600; }
    .dtag-p { background: rgba(124,109,235,0.12); color: var(--accent); }
    .dtag-t { background: rgba(93,216,168,0.12); color: var(--accent2); }
    .dtag-a { background: rgba(245,196,117,0.12); color: var(--amber); }

    /* FINAL CTA */
    .cta-section {
      background: var(--bg2);
      padding: 80px 40px; text-align: center;
      border-top: 1px solid var(--border);
    }
    .cta-inner { max-width: 680px; margin: 0 auto; }
    .cta-section h2 { margin-bottom: 16px; }
    .cta-section p { font-size: 16px; color: var(--muted); margin-bottom: 36px; line-height: 1.6; }
    .cta-actions { display: flex; gap: 12px; justify-content: center; flex-wrap: wrap; }

    footer {
      padding: 44px 40px; border-top: 1px solid var(--border);
      text-align: center; color: var(--muted); font-size: 13px;
    }
    .f-logo { font-size: 18px; font-weight: 800; margin-bottom: 10px; }
    .f-logo span { color: var(--accent); }
    footer a { color: var(--muted); margin: 0 10px; }
    footer a:hover { color: var(--text); }

    @media (max-width: 720px) {
      nav { padding: 12px 20px; }
      .nav-links { display: none; }
      .section, .section-narrow { padding: 56px 20px; }
      .full { padding: 56px 20px; }
      .proof-strip { padding: 24px 20px; gap: 24px; }
      .loop-section { padding: 56px 20px; }
      .loop-grid { grid-template-columns: 1fr; gap: 36px; }
      .flow-step:first-child { border-radius: 14px 14px 0 0; }
      .flow-step:last-child  { border-radius: 0 0 14px 14px; }
      .flow-step::after { display: none; }
      .roi-strip { padding: 56px 20px; }
      .cta-section { padding: 56px 20px; }
    }
  </style>
</head>
<body>

<!-- NAV -->
<nav>
  <div style="display:flex;align-items:center">
    <a href="landing.html" class="nav-logo">AI<span>n</span>onymous</a>
    <span class="nav-badge">Enterprise</span>
  </div>
  <div class="nav-links">
    <a href="#closed-loop">Boucle fermée</a>
    <a href="#architecture">Architecture</a>
    <a href="#securite">Sécurité</a>
    <a href="#business">Business case</a>
    <a href="#deploiement">Déploiement</a>
    <div class="lang-switch">
      <a href="enterprise.html" class="active">FR</a>
      <a href="enterprise-en.html">EN</a>
    </div>
  </div>
  <a href="landing.html" class="nav-back">← Retour accueil</a>
</nav>

<!-- HERO -->
<div class="hero">
  <div class="hero-badge">Réseau d'IA privé — on-premises</div>
  <h1>L'IA de votre entreprise,<br><span class="a1">souveraine</span> et<br><span class="a2">auto-apprenante.</span></h1>
  <p class="hero-sub">
    Un réseau P2P fermé qui transforme la puissance de calcul dormante de vos machines en infrastructure d'IA privée — sans cloud, sans fuite de données, sans coût marginal.
  </p>
  <div class="hero-actions">
    <a href="#business" class="btn-p">📊 Voir le business case</a>
    <a href="#architecture" class="btn-g">Architecture technique →</a>
  </div>
</div>

<!-- PROOF STRIP -->
<div class="proof-strip">
  <div class="proof-item"><div class="proof-dot"></div>Zéro donnée hors périmètre</div>
  <div class="proof-item"><div class="proof-dot"></div>Compatible RGPD · HIPAA · SOC 2</div>
  <div class="proof-item"><div class="proof-dot"></div>Pas de GPU supplémentaire requis</div>
  <div class="proof-item"><div class="proof-dot"></div>API OpenAI-compatible en local</div>
  <div class="proof-item"><div class="proof-dot"></div>Apache 2.0 — aucun vendor lock-in</div>
</div>

<!-- CLOSED LOOP -->
<div class="loop-section" id="closed-loop">
  <div class="loop-inner">
    <div class="eyebrow">Boucle P2P fermée</div>
    <h2>Un réseau d'IA qui ne sort<br>jamais de chez vous.</h2>

    <div class="loop-grid">
      <!-- Diagram -->
      <div class="loop-diagram-v">
        <div class="lnode">
          <div class="lnode-icon">🖥️</div>
          <div class="lnode-title">Postes de travail & serveurs</div>
          <div class="lnode-sub">GPU/CPU idle — toutes plateformes</div>
        </div>
        <div class="larrow">↓ contribuent au pool</div>
        <div class="lnode accent">
          <div class="lnode-icon">⚡</div>
          <div class="lnode-title">Mesh privé Holochain</div>
          <div class="lnode-sub">DHT interne · WebRTC · LAN/VPN</div>
        </div>
        <div class="larrow">↓ exécute</div>
        <div class="lnode">
          <div class="lnode-icon">🧠</div>
          <div class="lnode-title">Modèles IA distribués</div>
          <div class="lnode-sub">Pipeline-splitting par couches · GGUF</div>
        </div>
        <div class="larrow">↓ répond via</div>
        <div class="lnode accent2">
          <div class="lnode-icon">🔌</div>
          <div class="lnode-title">API locale OpenAI-compat.</div>
          <div class="lnode-sub">localhost:9337 · vos outils existants</div>
        </div>
        <div class="larrow larrow-closed">↑ les interactions améliorent le modèle ↑</div>
        <div style="text-align:center;margin-top:8px">
          <span style="font-size:11px;color:var(--accent2);background:rgba(93,216,168,0.08);border:1px solid rgba(93,216,168,0.25);padding:4px 14px;border-radius:20px;font-weight:600">
            🔒 Boucle 100% fermée — aucun trafic externe
          </span>
        </div>
      </div>

      <!-- Description -->
      <div class="loop-desc">
        <h3>Pourquoi une boucle fermée change tout</h3>
        <p>Dans un déploiement public, vos données traversent des serveurs tiers — anonymisées ou non. Dans la boucle fermée AInonymous, la question d'un collaborateur ne quitte jamais votre LAN ou VPN d'entreprise.</p>
        <p>La Distributed Hash Table (DHT) Holochain tourne <strong>à l'intérieur</strong> de votre infrastructure. Les nœuds se découvrent mutuellement par votre réseau interne, et les échanges sont chiffrés de bout en bout par des clés ed25519 propres à chaque machine.</p>
        <ul class="loop-bullets">
          <li><span class="bullet-ok">✓</span>Découverte de nœuds : via le réseau d'entreprise (pas de relais externe Nostr ou public)</li>
          <li><span class="bullet-ok">✓</span>Transport : WebRTC sur LAN ou QUIC sur VPN — pas d'internet requis</li>
          <li><span class="bullet-ok">✓</span>État partagé : source chain Holochain immutable, auditée, locale</li>
          <li><span class="bullet-ok">✓</span>Identité : Membrane Proofs pour contrôler qui rejoint le réseau privé</li>
          <li><span class="bullet-ok">✓</span>Air-gap possible : fonctionne sur réseau isolé sans accès internet</li>
        </ul>
      </div>
    </div>
  </div>
</div>

<!-- ARCHITECTURE -->
<div class="section" id="architecture">
  <div class="eyebrow">Architecture technique</div>
  <h2>Cinq couches. Zéro dépendance externe.</h2>
  <p class="section-sub">Chaque composant tourne sur votre infrastructure. La cryptographie remplace la confiance envers des tiers.</p>

  <!-- ASCII arch -->
  <div class="arch-box">
<span class="c-muted">┌─────────────────────────────────────────────────────────────────────┐
│                    PÉRIMÈTRE RÉSEAU ENTREPRISE                       │
│  ════════════════════════════════════════════════════════════════   │
│                                                                      │</span>
<span class="c-amber">│  COUCHE 4 — CLIENTS & OUTILS                                        │
│  ┌────────────┐  ┌─────────────┐  ┌──────────────┐  ┌──────────┐  │
│  │  VS Code   │  │  n8n/Make   │  │  LangChain   │  │  Goose   │  │
│  │  Cursor    │  │  Automation │  │  LlamaIndex  │  │  Agent   │  │
│  └─────┬──────┘  └──────┬──────┘  └──────┬───────┘  └────┬─────┘  │
│        └───────────────────────────────────────────────────┘        │</span>
<span class="c-muted">│                              │ OpenAI-compatible API                │</span>
<span class="c-acc">│  COUCHE 3 — API PROXY LOCAL (localhost:9337)                        │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │  Route POST /v1/chat/completions → Conducteur Holochain       │  │
│  │  Champ model → sélection du modèle et routage vers le mesh   │  │
│  └───────────────────────────────┬──────────────────────────────┘  │</span>
<span class="c-muted">│                                   │ WebRTC / QUIC (LAN/VPN)         │</span>
<span class="c-teal">│  COUCHE 2 — MESH HOLOCHAIN PRIVÉ                                    │
│  ┌──────────────┐  ┌──────────────┐  ┌───────────────────────────┐ │
│  │  DNA infer.  │  │  DNA agents  │  │  DNA registry + blackboard│ │
│  │  mesh (zome) │  │  + goose MCP │  │  (capacités, warrants)    │ │
│  └──────┬───────┘  └──────┬───────┘  └───────────────────────────┘ │
│         └─────────────────┘                                         │
│                 DHT Holochain — validée, persistante, locale        │</span>
<span class="c-muted">│                                   │                                 │</span>
<span class="c-acc">│  COUCHE 1 — MOTEUR D'INFÉRENCE (llama.cpp)                         │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌────────────┐   │
│  │  Nœud A    │  │  Nœud B    │  │  Nœud C    │  │  Nœud D    │   │
│  │  Gemma4-31B│  │  Gemma4-31B│  │  Gemma4-E4B│  │  Qwen3-32B │   │
│  │  couches   │  │  couches   │  │  draft spec│  │  code tasks│   │
│  │  [0 → 15]  │  │  [16 → 31] │  │  spéculatif│  │            │   │
│  └────────────┘  └────────────┘  └────────────┘  └────────────┘   │</span>
<span class="c-muted">│                                                                      │
└─────────────────────────────────────────────────────────────────────┘</span>
  </div>

  <div class="tech-grid" style="margin-top: 32px;">
    <div class="tech-card">
      <span class="tech-layer layer-infra">Infrastructure P2P</span>
      <h3>Holochain — DHT agent-centrique</h3>
      <p>Pas de blockchain, pas de consensus global. Chaque nœud maintient sa propre source chain immuable. La DHT valide l'état partagé sans serveur central.</p>
      <div class="tech-tag">Holochain 0.4 · Rust · WebRTC · ed25519</div>
    </div>
    <div class="tech-card">
      <span class="tech-layer layer-model">Inférence distribuée</span>
      <h3>Pipeline-splitting par couches</h3>
      <p>Un modèle 31B trop grand pour une seule machine est découpé par couches et distribué sur plusieurs nœuds. Le décodage spéculatif (draft + verify) accélère l'inférence.</p>
      <div class="tech-tag">llama.cpp · GGUF · layer-sharding · spec-decode</div>
    </div>
    <div class="tech-card">
      <span class="tech-layer layer-sec">Sécurité & identité</span>
      <h3>Membrane Proofs + Warrants</h3>
      <p>Seules les machines détenant une Membrane Proof valide rejoignent le réseau privé. Les warrants cryptographiques attestent la réputation et les capacités de chaque nœud.</p>
      <div class="tech-tag">ed25519 · zomes de validation · warrants Holochain</div>
    </div>
    <div class="tech-card">
      <span class="tech-layer layer-api">Compatibilité</span>
      <h3>API OpenAI-compatible locale</h3>
      <p>Endpoint <code>localhost:9337/v1</code> reproduit l'API d'OpenAI. Copilot, LangChain, n8n, Cursor, Goose — tous fonctionnent sans modification.</p>
      <div class="tech-tag">REST · SSE streaming · OpenAI v1 spec</div>
    </div>
    <div class="tech-card">
      <span class="tech-layer layer-model">Orchestration agents</span>
      <h3>Goose — framework multi-agents</h3>
      <p>Goose (Block, Apache 2.0) orchestre des agents IA complexes via MCP natif. Ses serveurs MCP se connectent aux zomes Holochain pour accéder au blackboard partagé.</p>
      <div class="tech-tag">Goose · MCP · Rust · multi-LLM</div>
    </div>
    <div class="tech-card">
      <span class="tech-layer layer-infra">Modèles</span>
      <h3>Gemma 4 · Qwen3 · Llama 3.3</h3>
      <p>Modèles GGUF open source. Gemma 4 (Google, Apache 2.0) en priorité pour sa densité qualité/VRAM. Support complet du pipeline-splitting sur modèles denses et MoE.</p>
      <div class="tech-tag">GGUF · quantization Q4-Q8 · MoE · dense</div>
    </div>
  </div>
</div>

<!-- FUNCTIONAL FLOW -->
<div class="full" style="background: var(--bg2); border-top: 1px solid var(--border); border-bottom: 1px solid var(--border);">
  <div class="full-inner">
    <div class="eyebrow">Modèle fonctionnel</div>
    <h2>Comment une requête traverse le réseau</h2>
    <p class="section-sub" style="margin-bottom: 0">Du prompt de votre collaborateur à la réponse — sans jamais toucher l'extérieur.</p>
    <div class="flow-steps">
      <div class="flow-step">
        <div class="fs-num">1</div>
        <h4>Prompt utilisateur</h4>
        <p>L'utilisateur envoie une requête via son outil (VS Code, chat interne, n8n…)</p>
      </div>
      <div class="flow-step">
        <div class="fs-num">2</div>
        <h4>API proxy locale</h4>
        <p>Le proxy <code style="font-size:10px">:9337</code> reçoit la requête OpenAI-compat. et l'envoie au conducteur Holochain</p>
      </div>
      <div class="flow-step">
        <div class="fs-num">3</div>
        <h4>Routage DHT</h4>
        <p>Le mesh consulte le registre des nœuds pour trouver les machines disponibles selon le modèle et la charge</p>
      </div>
      <div class="flow-step">
        <div class="fs-num">4</div>
        <h4>Inférence distribuée</h4>
        <p>Les couches du modèle sont exécutées en pipeline sur les nœuds sélectionnés (layer-sharding)</p>
      </div>
      <div class="flow-step">
        <div class="fs-num">5</div>
        <h4>Streaming tokens</h4>
        <p>Les tokens générés sont streamés vers l'API en temps réel — latence identique à du local</p>
      </div>
      <div class="flow-step">
        <div class="fs-num">6</div>
        <h4>Audit chain</h4>
        <p>La tâche est chainée cryptographiquement pour traçabilité. Aucune donnée sensible stockée.</p>
      </div>
    </div>
  </div>
</div>

<!-- SECURITY -->
<div class="section" id="securite">
  <div class="eyebrow">Sécurité & conformité</div>
  <h2>Conçu pour les environnements<br>les plus exigeants.</h2>
  <p class="section-sub">Chaque couche du système est pensée pour satisfaire les exigences des DSI, DPO et RSSI.</p>

  <div class="sec-grid">
    <div class="sec-card">
      <div class="sec-icon">🔑</div>
      <h3>Identités cryptographiques</h3>
      <p>Chaque nœud est identifié par une paire de clés ed25519. Aucune authentification centralisée, aucun mot de passe — la cryptographie est le seul mécanisme d'identité.</p>
      <span class="sec-badge">ed25519 · Holochain keys</span>
    </div>
    <div class="sec-card">
      <div class="sec-icon">🚪</div>
      <h3>Contrôle d'accès au réseau</h3>
      <p>Les Membrane Proofs définissent qui peut rejoindre le réseau privé. Un ordinateur sans proof valide est invisible pour le mesh — même sur le même LAN.</p>
      <span class="sec-badge">Membrane Proofs · RBAC</span>
    </div>
    <div class="sec-card">
      <div class="sec-icon">🔒</div>
      <h3>Chiffrement de bout en bout</h3>
      <p>Tous les échanges entre nœuds sont chiffrés. Les données en transit sont protégées même sur un réseau interne compromis.</p>
      <span class="sec-badge">TLS · WebRTC DTLS · E2E</span>
    </div>
    <div class="sec-card">
      <div class="sec-icon">📋</div>
      <h3>Audit trail immuable</h3>
      <p>Chaque tâche d'inférence est ajoutée à une source chain Holochain — une structure append-only cryptographiquement liée. Idéal pour les audits réglementaires.</p>
      <span class="sec-badge">Source chain · append-only</span>
    </div>
    <div class="sec-card">
      <div class="sec-icon">🌐</div>
      <h3>Isolation réseau totale</h3>
      <p>Le réseau peut tourner sans aucun accès internet. Modes supportés : LAN interne, VPN d'entreprise, réseau air-gappé. Aucune dépendance à des relais ou DNS externes.</p>
      <span class="sec-badge">Air-gap ready · VPN · LAN</span>
    </div>
    <div class="sec-card">
      <div class="sec-icon">🔍</div>
      <h3>Validation des pairs</h3>
      <p>Les zomes de validation Holochain permettent à chaque nœud de vérifier la conformité des données des autres avant de les accepter. Pas de nœud malveillant non détecté.</p>
      <span class="sec-badge">Zome validation · warrants</span>
    </div>
  </div>

  <div style="margin-top: 40px;">
    <h3 style="margin-bottom: 20px; font-size: 16px; color: var(--muted); font-weight: 600;">Certifications et frameworks supportés</h3>
    <div class="compliance-grid">
      <div class="comp-badge"><span class="comp-check">✓</span> RGPD / GDPR</div>
      <div class="comp-badge"><span class="comp-check">✓</span> HIPAA</div>
      <div class="comp-badge"><span class="comp-check">✓</span> SOC 2 Type II</div>
      <div class="comp-badge"><span class="comp-check">✓</span> ISO 27001</div>
      <div class="comp-badge"><span class="comp-check">✓</span> NIS2</div>
      <div class="comp-badge"><span class="comp-check">✓</span> DORA (Finance)</div>
      <div class="comp-badge"><span class="comp-check">✓</span> HDS (Santé)</div>
      <div class="comp-badge"><span class="comp-check">✓</span> Réseau air-gappé</div>
    </div>
  </div>
</div>

<!-- BUSINESS CASE -->
<div class="roi-strip" id="business">
  <div class="roi-inner">
    <div class="eyebrow">Business case</div>
    <h2>Le calcul est simple.</h2>
    <p class="section-sub" style="margin-bottom: 0">Vous payez déjà pour les machines. AInonymous transforme leurs heures creuses en infrastructure IA.</p>

    <div class="roi-grid">
      <div class="roi-card">
        <div class="roi-num">~70%</div>
        <div class="roi-label">des ressources de calcul dormantes en dehors des heures ouvrées</div>
      </div>
      <div class="roi-card">
        <div class="roi-num">0€</div>
        <div class="roi-label">de GPU supplémentaire requis pour un mesh de 50+ machines</div>
      </div>
      <div class="roi-card">
        <div class="roi-num">×N</div>
        <div class="roi-label">la puissance d'inférence scale avec chaque nouveau poste</div>
      </div>
      <div class="roi-card">
        <div class="roi-num">&lt;1 an</div>
        <div class="roi-label">ROI vs. abonnements cloud IA pour une équipe de 100 personnes</div>
      </div>
    </div>

    <!-- Scenario ROI -->
    <div class="roi-scenario">
      <h3>Scénario : 200 collaborateurs, 50 postes GPU modestes (8 GB VRAM)</h3>
      <div class="roi-row">
        <span>Coût actuel — abonnements cloud IA (200 × 20€/mois)</span>
        <span>4 000 € / mois</span>
      </div>
      <div class="roi-row">
        <span>Coût AInonymous Enterprise (déploiement + support)</span>
        <span>~500–800 € / mois</span>
      </div>
      <div class="roi-row">
        <span>Économie annuelle estimée</span>
        <span>~38 000 – 42 000 €</span>
      </div>
      <div class="roi-row">
        <span>Données exposées à des tiers après migration</span>
        <span style="color: var(--accent2)">0</span>
      </div>
      <div class="roi-row">
        <span>Économie sur 3 ans (sans compter la croissance d'équipe)</span>
        <span>+120 000 €</span>
      </div>
    </div>
  </div>
</div>

<!-- COMPARATIVE TABLE -->
<div class="section">
  <div class="eyebrow">Comparaison détaillée</div>
  <h2>Enterprise vs. cloud vs. local</h2>
  <div class="bc-table-wrap">
    <table>
      <thead>
        <tr>
          <th>Critère</th>
          <th>Cloud IA (OpenAI, Azure)</th>
          <th>Ollama local (par poste)</th>
          <th class="hl">AInonymous Enterprise ✦</th>
        </tr>
      </thead>
      <tbody>
        <tr class="section-row"><td colspan="4">Souveraineté & sécurité</td></tr>
        <tr>
          <td>Données hors périmètre</td>
          <td class="v-no">✗ Oui — serveurs tiers</td>
          <td class="v-ok">✓ Non — machine locale</td>
          <td class="col v-ok">✓ Non — réseau interne</td>
        </tr>
        <tr>
          <td>Conformité RGPD by-design</td>
          <td>Partielle (DPA requis)</td>
          <td class="v-ok">✓ Totale</td>
          <td class="col v-ok">✓ Totale + auditée</td>
        </tr>
        <tr>
          <td>Contrôle d'accès granulaire</td>
          <td>IAM externe</td>
          <td class="v-no">✗ Aucun</td>
          <td class="col v-ok">✓ Membrane Proofs</td>
        </tr>
        <tr>
          <td>Audit trail cryptographique</td>
          <td class="v-no">✗ Non</td>
          <td class="v-no">✗ Non</td>
          <td class="col v-ok">✓ Source chain Holochain</td>
        </tr>
        <tr>
          <td>Mode air-gap</td>
          <td class="v-no">✗ Impossible</td>
          <td class="v-ok">✓ Oui (1 machine)</td>
          <td class="col v-ok">✓ Oui (réseau entier)</td>
        </tr>

        <tr class="section-row"><td colspan="4">Coûts & scalabilité</td></tr>
        <tr>
          <td>Coût par utilisateur/mois</td>
          <td>~20–30 €</td>
          <td>0 € (GPU requis)</td>
          <td class="col">~2–4 € (support)</td>
        </tr>
        <tr>
          <td>Scalabilité puissance IA</td>
          <td>$ (achat de quotas)</td>
          <td class="v-no">✗ Limitée au GPU local</td>
          <td class="col v-ok">✓ Scale avec la flotte</td>
        </tr>
        <tr>
          <td>GPU dédié requis</td>
          <td class="v-ok">✓ Non (cloud)</td>
          <td class="v-no">✗ Oui (par poste)</td>
          <td class="col v-ok">✓ Non (mutualisé)</td>
        </tr>

        <tr class="section-row"><td colspan="4">Capacités IA</td></tr>
        <tr>
          <td>Fine-tuning sur données internes</td>
          <td>Possible (données cloud)</td>
          <td class="v-no">✗ Complexe</td>
          <td class="col v-ok">✓ On-prem, natif</td>
        </tr>
        <tr>
          <td>Modèles &gt;30B params</td>
          <td class="v-ok">✓ Oui</td>
          <td class="v-no">✗ Nécessite 20+ GB VRAM</td>
          <td class="col v-ok">✓ Via pipeline-splitting</td>
        </tr>
        <tr>
          <td>Multi-agents / orchestration</td>
          <td>API uniquement</td>
          <td class="v-no">✗ Manuel</td>
          <td class="col v-ok">✓ Goose + blackboard DHT</td>
        </tr>
        <tr>
          <td>API OpenAI-compatible</td>
          <td class="v-ok">✓ Native</td>
          <td class="v-ok">✓ Oui</td>
          <td class="col v-ok">✓ localhost:9337</td>
        </tr>

        <tr class="section-row"><td colspan="4">Gouvernance</td></tr>
        <tr>
          <td>Vendor lock-in</td>
          <td class="v-no">✗ Fort</td>
          <td class="v-ok">✓ Aucun</td>
          <td class="col v-ok">✓ Aucun (Apache 2.0)</td>
        </tr>
        <tr>
          <td>Indépendance fournisseur</td>
          <td class="v-no">✗ Dépendance totale</td>
          <td class="v-ok">✓ Totale</td>
          <td class="col v-ok">✓ Totale + résilient</td>
        </tr>
      </tbody>
    </table>
  </div>
</div>

<!-- DEPLOYMENT MODES -->
<div class="section" id="deploiement" style="padding-top: 0">
  <div class="eyebrow">Modes de déploiement</div>
  <h2>Du pilote à l'échelle groupe.</h2>
  <p class="section-sub">Démarrez petit, scalez sans migration — l'architecture est identique à 5 ou 5 000 nœuds.</p>
  <div class="deploy-grid">
    <div class="deploy-card">
      <div class="deploy-icon">🌱</div>
      <h3>Pilote équipe</h3>
      <p>10–30 machines, réseau LAN. Idéal pour valider le cas d'usage sur une équipe tech ou R&D. Déploiement en moins d'une journée.</p>
      <div class="deploy-tags">
        <span class="dtag dtag-t">Gemma4-E4B</span>
        <span class="dtag dtag-t">LAN</span>
        <span class="dtag dtag-a">1 jour</span>
      </div>
    </div>
    <div class="deploy-card featured">
      <div class="deploy-icon">🏢</div>
      <h3>Déploiement site</h3>
      <p>50–500 machines, VPN d'entreprise. Modèles 26B–31B distribués, multi-services, API centralisée par site. Configuration recommandée pour la majorité des entreprises.</p>
      <div class="deploy-tags">
        <span class="dtag dtag-p">Gemma4-31B</span>
        <span class="dtag dtag-t">VPN · LAN</span>
        <span class="dtag dtag-a">1–2 semaines</span>
      </div>
    </div>
    <div class="deploy-card">
      <div class="deploy-icon">🏛️</div>
      <h3>Groupe multi-sites</h3>
      <p>500+ machines, multi-sites, réseau maillé entre filiales. Chaque site dispose d'un sous-réseau autonome avec fédération possible. Support Llama 3.3 70B.</p>
      <div class="deploy-tags">
        <span class="dtag dtag-p">Llama3.3-70B</span>
        <span class="dtag dtag-t">Multi-VPN</span>
        <span class="dtag dtag-a">1 mois</span>
      </div>
    </div>
    <div class="deploy-card">
      <div class="deploy-icon">🔒</div>
      <h3>Air-gap / haute sécurité</h3>
      <p>Réseau totalement isolé, sans accès internet. Pour environnements défense, santé, finance réglementée. Modèles pré-chargés, zéro dépendance externe.</p>
      <div class="deploy-tags">
        <span class="dtag dtag-p">Tous modèles</span>
        <span class="dtag dtag-t">Air-gap</span>
        <span class="dtag dtag-a">Sur mesure</span>
      </div>
    </div>
  </div>
</div>

<!-- FINAL CTA -->
<div class="cta-section">
  <div class="cta-inner">
    <div class="eyebrow" style="text-align:center">Passez à l'action</div>
    <h2>Votre infrastructure IA privée<br>commence ici.</h2>
    <p>AInonymous Enterprise est open source, Apache 2.0. Déployez, adaptez, auditez — sans demander la permission à personne.</p>
    <div class="cta-actions">
      <a href="https://github.com/Geoking2104/AInonymous" class="btn-p">Accéder au code source →</a>
      <a href="https://github.com/Geoking2104/AInonymous/blob/main/README.md" class="btn-g">Documentation technique</a>
      <a href="landing.html" class="btn-g">← Retour à l'accueil</a>
    </div>
  </div>
</div>

<footer>
  <div class="f-logo">AI<span>n</span>onymous <span style="color:var(--muted);font-size:14px;font-weight:400">Enterprise</span></div>
  <p style="margin-bottom: 14px">L'IA souveraine pour les organisations qui ne font pas de compromis.</p>
  <div>
    <a href="landing.html">Accueil</a>
    <a href="https://github.com/Geoking2104/AInonymous">GitHub</a>
    <a href="https://github.com/Geoking2104/AInonymous/blob/main/README.md">Docs</a>
    <a href="https://github.com/Geoking2104/AInonymous/blob/main/LICENSE">Apache 2.0</a>
  </div>
  <p style="margin-top: 24px; font-size: 11px; opacity: .35">Holochain · llama.cpp · Gemma 4 · Goose · Apache 2.0</p>
</footer>

</body>
</html>
```

</details>

<details>
<summary><strong>📄 site/enterprise-en.html</strong> — Enterprise page (EN)</summary>

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>AInonymous Enterprise — Private, Sovereign, On-Premises AI</title>
  <style>
    :root {
      --bg: #08080e;
      --bg2: #10101a;
      --bg3: #181826;
      --border: rgba(255,255,255,0.08);
      --border2: rgba(255,255,255,0.14);
      --text: #f0eff8;
      --muted: #8887a0;
      --accent: #7c6deb;
      --accent2: #5dd8a8;
      --accent3: #eb6d7c;
      --amber: #f5c475;
    }
    *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
    html { scroll-behavior: smooth; }
    body {
      font-family: 'Segoe UI', system-ui, -apple-system, sans-serif;
      background: var(--bg); color: var(--text);
      line-height: 1.7; -webkit-font-smoothing: antialiased;
    }
    a { color: inherit; text-decoration: none; }
    code { font-family: 'JetBrains Mono','Fira Code',monospace; font-size: .9em; color: var(--accent2); }

    /* NAV */
    nav {
      position: sticky; top: 0; z-index: 100;
      display: flex; align-items: center; justify-content: space-between;
      padding: 14px 40px; background: rgba(8,8,14,0.92);
      backdrop-filter: blur(12px);
      border-bottom: 1px solid var(--border);
    }
    .nav-logo { font-size: 18px; font-weight: 700; }
    .nav-logo span { color: var(--accent); }
    .nav-badge {
      font-size: 11px; font-weight: 700; padding: 3px 10px; border-radius: 6px;
      background: rgba(124,109,235,0.15); color: var(--accent); margin-left: 10px;
      border: 1px solid rgba(124,109,235,0.3);
    }
    .nav-links { display: flex; gap: 24px; font-size: 14px; color: var(--muted); align-items: center; }
    .nav-links a:hover { color: var(--text); }
    .nav-back { font-size: 13px; color: var(--muted); display: flex; align-items: center; gap: 6px; }
    .nav-back:hover { color: var(--text); }
    .lang-switch {
      display: flex; align-items: center; gap: 2px;
      font-size: 12px; font-weight: 700;
      background: var(--bg3); border: 1px solid var(--border2);
      border-radius: 7px; overflow: hidden;
    }
    .lang-switch a {
      padding: 4px 9px; color: var(--muted); transition: background .15s, color .15s;
    }
    .lang-switch a.active {
      background: var(--accent); color: #fff;
    }
    .lang-switch a:hover:not(.active) { color: var(--text); }

    /* LAYOUT */
    .section { padding: 88px 40px; max-width: 1080px; margin: 0 auto; }
    .section-narrow { padding: 88px 40px; max-width: 820px; margin: 0 auto; }
    .full { padding: 72px 40px; }
    .full-inner { max-width: 1080px; margin: 0 auto; }
    .eyebrow {
      font-size: 11px; font-weight: 700; letter-spacing: 2px;
      text-transform: uppercase; color: var(--accent2); margin-bottom: 14px;
    }
    h2 { font-size: clamp(26px, 3.8vw, 42px); font-weight: 800; letter-spacing: -1.5px; line-height: 1.15; margin-bottom: 16px; }
    h3 { font-size: 18px; font-weight: 700; margin-bottom: 8px; }
    .section-sub { font-size: 17px; color: var(--muted); max-width: 560px; line-height: 1.6; margin-bottom: 52px; }
    .divider { border: none; border-top: 1px solid var(--border); }

    /* HERO */
    .hero {
      padding: 110px 40px 80px; text-align: center; max-width: 900px; margin: 0 auto;
    }
    .hero-badge {
      display: inline-flex; align-items: center; gap: 7px;
      font-size: 12px; font-weight: 600; color: var(--accent);
      background: rgba(124,109,235,0.1); border: 1px solid rgba(124,109,235,0.3);
      padding: 5px 14px; border-radius: 20px; margin-bottom: 28px;
    }
    .hero-badge::before { content: '🏢'; font-size: 14px; }
    h1 { font-size: clamp(36px, 6vw, 64px); font-weight: 800; letter-spacing: -2px; line-height: 1.08; margin-bottom: 22px; }
    .a1 { color: var(--accent); } .a2 { color: var(--accent2); }
    .hero-sub { font-size: clamp(16px, 2.2vw, 20px); color: var(--muted); max-width: 620px; margin: 0 auto 40px; line-height: 1.6; }
    .hero-actions { display: flex; gap: 12px; justify-content: center; flex-wrap: wrap; }
    .btn-p { background: var(--accent); color: #fff; padding: 13px 26px; border-radius: 10px; font-weight: 700; font-size: 14px; display: inline-flex; align-items: center; gap: 7px; transition: opacity .15s, transform .15s; }
    .btn-p:hover { opacity: .88; transform: translateY(-1px); }
    .btn-g { border: 1px solid var(--border2); color: var(--muted); padding: 13px 26px; border-radius: 10px; font-weight: 600; font-size: 14px; display: inline-flex; align-items: center; gap: 7px; transition: border-color .15s, color .15s; }
    .btn-g:hover { border-color: var(--accent); color: var(--text); }

    /* PROOF STRIP */
    .proof-strip {
      background: var(--bg2); border-top: 1px solid var(--border); border-bottom: 1px solid var(--border);
      padding: 32px 40px;
      display: flex; justify-content: center; gap: 48px; flex-wrap: wrap;
    }
    .proof-item { display: flex; align-items: center; gap: 10px; font-size: 13px; color: var(--muted); }
    .proof-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--accent2); flex-shrink: 0; }

    /* CLOSED LOOP VISUAL */
    .loop-section { background: var(--bg2); padding: 72px 40px; border-top: 1px solid var(--border); border-bottom: 1px solid var(--border); }
    .loop-inner { max-width: 960px; margin: 0 auto; }
    .loop-grid {
      display: grid; grid-template-columns: 1fr 1fr; gap: 56px; align-items: center; margin-top: 48px;
    }
    .loop-diagram-v {
      display: flex; flex-direction: column; gap: 0; align-items: center;
    }
    .lnode {
      background: var(--bg3); border: 1px solid rgba(124,109,235,0.25);
      border-radius: 12px; padding: 14px 20px; width: 100%; text-align: center;
      position: relative;
    }
    .lnode-icon { font-size: 20px; margin-bottom: 5px; }
    .lnode-title { font-size: 13px; font-weight: 700; }
    .lnode-sub { font-size: 11px; color: var(--muted); margin-top: 2px; }
    .lnode.accent { background: rgba(124,109,235,0.1); border-color: rgba(124,109,235,0.5); }
    .lnode.accent2 { background: rgba(93,216,168,0.08); border-color: rgba(93,216,168,0.4); }
    .larrow { text-align: center; font-size: 16px; color: rgba(124,109,235,0.4); padding: 6px 0; }
    .larrow-closed { color: rgba(93,216,168,0.5); }
    .loop-desc h3 { font-size: 20px; font-weight: 700; margin-bottom: 14px; }
    .loop-desc p { font-size: 14px; color: var(--muted); line-height: 1.7; margin-bottom: 14px; }
    .loop-bullets { list-style: none; display: flex; flex-direction: column; gap: 10px; margin-top: 20px; }
    .loop-bullets li {
      display: flex; align-items: flex-start; gap: 10px;
      font-size: 13px; color: var(--muted); line-height: 1.5;
    }
    .bullet-ok { color: var(--accent2); font-size: 14px; margin-top: 1px; flex-shrink: 0; }

    /* ARCH DIAGRAM (ASCII-style) */
    .arch-box {
      background: var(--bg3); border: 1px solid var(--border2);
      border-radius: 14px; padding: 32px; font-family: 'JetBrains Mono','Fira Code',monospace;
      font-size: 12px; line-height: 1.8; color: var(--muted); overflow-x: auto;
    }
    .arch-box .c-acc { color: var(--accent); }
    .arch-box .c-teal { color: var(--accent2); }
    .arch-box .c-amber { color: var(--amber); }
    .arch-box .c-muted { color: rgba(136,135,160,0.4); }

    /* TECH CARDS */
    .tech-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 18px; }
    .tech-card {
      background: var(--bg2); border: 1px solid var(--border);
      border-radius: 14px; padding: 28px 24px; transition: border-color .2s;
    }
    .tech-card:hover { border-color: rgba(124,109,235,0.3); }
    .tech-layer {
      font-size: 10px; font-weight: 700; letter-spacing: 1.5px; text-transform: uppercase;
      padding: 3px 9px; border-radius: 5px; display: inline-block; margin-bottom: 14px;
    }
    .layer-infra { background: rgba(124,109,235,0.12); color: var(--accent); }
    .layer-model { background: rgba(93,216,168,0.12); color: var(--accent2); }
    .layer-sec   { background: rgba(235,109,124,0.12); color: var(--accent3); }
    .layer-api   { background: rgba(245,196,117,0.12); color: var(--amber); }
    .tech-card h3 { font-size: 15px; font-weight: 700; margin-bottom: 8px; }
    .tech-card p { font-size: 13px; color: var(--muted); line-height: 1.65; }
    .tech-tag { font-size: 11px; color: var(--muted); margin-top: 12px; font-family: monospace; }

    /* FUNCTIONAL FLOW */
    .flow-steps { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 2px; margin-top: 48px; }
    .flow-step {
      background: var(--bg2); border: 1px solid var(--border);
      padding: 24px 18px; position: relative; text-align: center;
    }
    .flow-step:first-child { border-radius: 14px 0 0 14px; }
    .flow-step:last-child  { border-radius: 0 14px 14px 0; }
    .flow-step::after {
      content: '→'; position: absolute; right: -14px; top: 50%;
      transform: translateY(-50%); z-index: 1;
      font-size: 18px; color: rgba(124,109,235,0.4);
    }
    .flow-step:last-child::after { display: none; }
    .fs-num {
      width: 28px; height: 28px; border-radius: 8px; margin: 0 auto 10px;
      background: rgba(124,109,235,0.15); display: flex; align-items: center; justify-content: center;
      font-size: 12px; font-weight: 800; color: var(--accent);
    }
    .flow-step h4 { font-size: 12px; font-weight: 700; margin-bottom: 5px; }
    .flow-step p { font-size: 11px; color: var(--muted); line-height: 1.5; }

    /* SECURITY TABLE */
    .sec-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 14px; margin-top: 40px; }
    .sec-card {
      background: var(--bg2); border: 1px solid rgba(235,109,124,0.18);
      border-radius: 12px; padding: 22px 20px;
    }
    .sec-icon { font-size: 20px; margin-bottom: 10px; }
    .sec-card h3 { font-size: 14px; font-weight: 700; margin-bottom: 6px; }
    .sec-card p { font-size: 12px; color: var(--muted); line-height: 1.6; }
    .sec-badge {
      display: inline-block; margin-top: 10px; font-size: 10px; font-weight: 700;
      padding: 2px 8px; border-radius: 5px;
      background: rgba(93,216,168,0.12); color: var(--accent2);
    }

    /* BUSINESS CASE TABLE */
    .bc-table-wrap { overflow-x: auto; margin-top: 44px; }
    table { width: 100%; border-collapse: collapse; font-size: 14px; min-width: 640px; }
    th { padding: 12px 16px; font-weight: 700; border-bottom: 2px solid var(--border2); text-align: left; color: var(--muted); }
    th.hl { color: var(--accent); }
    td { padding: 12px 16px; border-bottom: 1px solid var(--border); vertical-align: top; }
    tr:last-child td { border-bottom: none; }
    .ok { color: var(--accent2); font-weight: 600; }
    .v-ok { color: var(--accent2); }
    .v-no { color: var(--accent3); }
    .col { background: rgba(124,109,235,0.05); }
    tr.section-row td { background: var(--bg3); font-weight: 700; font-size: 12px; text-transform: uppercase; letter-spacing: 1px; color: var(--muted); padding: 8px 16px; }

    /* ROI SECTION */
    .roi-strip {
      background: var(--bg2); padding: 72px 40px;
      border-top: 1px solid var(--border); border-bottom: 1px solid var(--border);
    }
    .roi-inner { max-width: 1080px; margin: 0 auto; }
    .roi-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 14px; margin-top: 44px; }
    .roi-card {
      background: var(--bg); border: 1px solid rgba(93,216,168,0.2);
      border-radius: 12px; padding: 22px 18px; text-align: center;
    }
    .roi-num { font-size: 32px; font-weight: 800; color: var(--accent2); letter-spacing: -1px; }
    .roi-label { font-size: 12px; color: var(--muted); margin-top: 4px; line-height: 1.4; }
    .roi-scenario {
      background: var(--bg); border: 1px solid var(--border);
      border-radius: 14px; padding: 28px; margin-top: 28px;
    }
    .roi-scenario h3 { font-size: 16px; font-weight: 700; margin-bottom: 16px; color: var(--accent2); }
    .roi-row {
      display: flex; justify-content: space-between; align-items: baseline;
      padding: 8px 0; border-bottom: 1px solid var(--border); font-size: 13px;
    }
    .roi-row:last-child { border-bottom: none; font-weight: 700; color: var(--accent2); font-size: 15px; }
    .roi-row span:first-child { color: var(--muted); }
    .roi-row span:last-child { font-weight: 600; }

    /* COMPLIANCE */
    .compliance-grid {
      display: flex; flex-wrap: wrap; gap: 10px; margin-top: 32px;
    }
    .comp-badge {
      background: var(--bg2); border: 1px solid var(--border2);
      border-radius: 10px; padding: 10px 16px; font-size: 13px; font-weight: 600;
      display: flex; align-items: center; gap: 7px;
    }
    .comp-check { color: var(--accent2); font-size: 14px; }

    /* DEPLOYMENT MODES */
    .deploy-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); gap: 16px; margin-top: 44px; }
    .deploy-card {
      background: var(--bg2); border: 1px solid var(--border);
      border-radius: 14px; padding: 28px 24px; position: relative; overflow: hidden;
    }
    .deploy-card.featured { border-color: rgba(124,109,235,0.45); }
    .deploy-card.featured::before {
      content: 'Recommended'; position: absolute; top: 14px; right: 14px;
      font-size: 10px; font-weight: 700; padding: 3px 8px; border-radius: 5px;
      background: rgba(124,109,235,0.18); color: var(--accent);
    }
    .deploy-icon { font-size: 26px; margin-bottom: 14px; }
    .deploy-card h3 { font-size: 16px; font-weight: 700; margin-bottom: 8px; }
    .deploy-card p { font-size: 13px; color: var(--muted); line-height: 1.6; margin-bottom: 14px; }
    .deploy-tags { display: flex; flex-wrap: wrap; gap: 6px; }
    .dtag { font-size: 11px; padding: 2px 8px; border-radius: 5px; font-weight: 600; }
    .dtag-p { background: rgba(124,109,235,0.12); color: var(--accent); }
    .dtag-t { background: rgba(93,216,168,0.12); color: var(--accent2); }
    .dtag-a { background: rgba(245,196,117,0.12); color: var(--amber); }

    /* FINAL CTA */
    .cta-section {
      background: var(--bg2);
      padding: 80px 40px; text-align: center;
      border-top: 1px solid var(--border);
    }
    .cta-inner { max-width: 680px; margin: 0 auto; }
    .cta-section h2 { margin-bottom: 16px; }
    .cta-section p { font-size: 16px; color: var(--muted); margin-bottom: 36px; line-height: 1.6; }
    .cta-actions { display: flex; gap: 12px; justify-content: center; flex-wrap: wrap; }

    footer {
      padding: 44px 40px; border-top: 1px solid var(--border);
      text-align: center; color: var(--muted); font-size: 13px;
    }
    .f-logo { font-size: 18px; font-weight: 800; margin-bottom: 10px; }
    .f-logo span { color: var(--accent); }
    footer a { color: var(--muted); margin: 0 10px; }
    footer a:hover { color: var(--text); }

    @media (max-width: 720px) {
      nav { padding: 12px 20px; }
      .nav-links { display: none; }
      .section, .section-narrow { padding: 56px 20px; }
      .full { padding: 56px 20px; }
      .proof-strip { padding: 24px 20px; gap: 24px; }
      .loop-section { padding: 56px 20px; }
      .loop-grid { grid-template-columns: 1fr; gap: 36px; }
      .flow-step:first-child { border-radius: 14px 14px 0 0; }
      .flow-step:last-child  { border-radius: 0 0 14px 14px; }
      .flow-step::after { display: none; }
      .roi-strip { padding: 56px 20px; }
      .cta-section { padding: 56px 20px; }
    }
  </style>
</head>
<body>

<!-- NAV -->
<nav>
  <div style="display:flex;align-items:center">
    <a href="landing-en.html" class="nav-logo">AI<span>n</span>onymous</a>
    <span class="nav-badge">Enterprise</span>
  </div>
  <div class="nav-links">
    <a href="#closed-loop">Closed loop</a>
    <a href="#architecture">Architecture</a>
    <a href="#security">Security</a>
    <a href="#business">Business case</a>
    <a href="#deployment">Deployment</a>
    <div class="lang-switch">
      <a href="enterprise.html">FR</a>
      <a href="enterprise-en.html" class="active">EN</a>
    </div>
  </div>
  <a href="landing-en.html" class="nav-back">← Back to home</a>
</nav>

<!-- HERO -->
<div class="hero">
  <div class="hero-badge">Private AI network — on-premises</div>
  <h1>Your enterprise AI,<br><span class="a1">sovereign</span> and<br><span class="a2">self-improving.</span></h1>
  <p class="hero-sub">
    A closed P2P network that turns the idle compute power of your machines into private AI infrastructure — no cloud, no data leakage, no marginal cost.
  </p>
  <div class="hero-actions">
    <a href="#business" class="btn-p">📊 View business case</a>
    <a href="#architecture" class="btn-g">Technical architecture →</a>
  </div>
</div>

<!-- PROOF STRIP -->
<div class="proof-strip">
  <div class="proof-item"><div class="proof-dot"></div>Zero data outside your perimeter</div>
  <div class="proof-item"><div class="proof-dot"></div>GDPR · HIPAA · SOC 2 compliant</div>
  <div class="proof-item"><div class="proof-dot"></div>No additional GPU required</div>
  <div class="proof-item"><div class="proof-dot"></div>Local OpenAI-compatible API</div>
  <div class="proof-item"><div class="proof-dot"></div>Apache 2.0 — zero vendor lock-in</div>
</div>

<!-- CLOSED LOOP -->
<div class="loop-section" id="closed-loop">
  <div class="loop-inner">
    <div class="eyebrow">Closed P2P loop</div>
    <h2>An AI network that never<br>leaves your premises.</h2>

    <div class="loop-grid">
      <!-- Diagram -->
      <div class="loop-diagram-v">
        <div class="lnode">
          <div class="lnode-icon">🖥️</div>
          <div class="lnode-title">Workstations & servers</div>
          <div class="lnode-sub">idle GPU/CPU — all platforms</div>
        </div>
        <div class="larrow">↓ contribute to the pool</div>
        <div class="lnode accent">
          <div class="lnode-icon">⚡</div>
          <div class="lnode-title">Private Holochain mesh</div>
          <div class="lnode-sub">internal DHT · WebRTC · LAN/VPN</div>
        </div>
        <div class="larrow">↓ executes</div>
        <div class="lnode">
          <div class="lnode-icon">🧠</div>
          <div class="lnode-title">Distributed AI models</div>
          <div class="lnode-sub">layer pipeline-splitting · GGUF</div>
        </div>
        <div class="larrow">↓ responds via</div>
        <div class="lnode accent2">
          <div class="lnode-icon">🔌</div>
          <div class="lnode-title">Local OpenAI-compat. API</div>
          <div class="lnode-sub">localhost:9337 · your existing tools</div>
        </div>
        <div class="larrow larrow-closed">↑ interactions improve the model ↑</div>
        <div style="text-align:center;margin-top:8px">
          <span style="font-size:11px;color:var(--accent2);background:rgba(93,216,168,0.08);border:1px solid rgba(93,216,168,0.25);padding:4px 14px;border-radius:20px;font-weight:600">
            🔒 100% closed loop — zero external traffic
          </span>
        </div>
      </div>

      <!-- Description -->
      <div class="loop-desc">
        <h3>Why a closed loop changes everything</h3>
        <p>In a public deployment, your data travels through third-party servers — anonymized or not. In the AInonymous closed loop, an employee's query never leaves your corporate LAN or VPN.</p>
        <p>The Holochain Distributed Hash Table (DHT) runs <strong>inside</strong> your infrastructure. Nodes discover each other through your internal network, and all exchanges are end-to-end encrypted with ed25519 keys unique to each machine.</p>
        <ul class="loop-bullets">
          <li><span class="bullet-ok">✓</span>Node discovery: via the corporate network (no external Nostr or public relay)</li>
          <li><span class="bullet-ok">✓</span>Transport: WebRTC over LAN or QUIC over VPN — no internet required</li>
          <li><span class="bullet-ok">✓</span>Shared state: immutable, audited, local Holochain source chain</li>
          <li><span class="bullet-ok">✓</span>Identity: Membrane Proofs to control who joins the private network</li>
          <li><span class="bullet-ok">✓</span>Air-gap capable: works on isolated network with no internet access</li>
        </ul>
      </div>
    </div>
  </div>
</div>

<!-- ARCHITECTURE -->
<div class="section" id="architecture">
  <div class="eyebrow">Technical architecture</div>
  <h2>Five layers. Zero external dependencies.</h2>
  <p class="section-sub">Every component runs on your infrastructure. Cryptography replaces trust in third parties.</p>

  <!-- ASCII arch -->
  <div class="arch-box">
<span class="c-muted">┌─────────────────────────────────────────────────────────────────────┐
│                    ENTERPRISE NETWORK PERIMETER                      │
│  ════════════════════════════════════════════════════════════════   │
│                                                                      │</span>
<span class="c-amber">│  LAYER 4 — CLIENTS & TOOLS                                          │
│  ┌────────────┐  ┌─────────────┐  ┌──────────────┐  ┌──────────┐  │
│  │  VS Code   │  │  n8n/Make   │  │  LangChain   │  │  Goose   │  │
│  │  Cursor    │  │  Automation │  │  LlamaIndex  │  │  Agent   │  │
│  └─────┬──────┘  └──────┬──────┘  └──────┬───────┘  └────┬─────┘  │
│        └───────────────────────────────────────────────────┘        │</span>
<span class="c-muted">│                              │ OpenAI-compatible API                │</span>
<span class="c-acc">│  LAYER 3 — LOCAL API PROXY (localhost:9337)                         │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │  Route POST /v1/chat/completions → Holochain conductor        │  │
│  │  model field → model selection and routing to the mesh        │  │
│  └───────────────────────────────┬──────────────────────────────┘  │</span>
<span class="c-muted">│                                   │ WebRTC / QUIC (LAN/VPN)         │</span>
<span class="c-teal">│  LAYER 2 — PRIVATE HOLOCHAIN MESH                                   │
│  ┌──────────────┐  ┌──────────────┐  ┌───────────────────────────┐ │
│  │  inference   │  │  agents DNA  │  │  registry DNA + blackboard│ │
│  │  mesh DNA    │  │  + goose MCP │  │  (capabilities, warrants) │ │
│  └──────┬───────┘  └──────┬───────┘  └───────────────────────────┘ │
│         └─────────────────┘                                         │
│                 Holochain DHT — validated, persistent, local        │</span>
<span class="c-muted">│                                   │                                 │</span>
<span class="c-acc">│  LAYER 1 — INFERENCE ENGINE (llama.cpp)                             │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌────────────┐   │
│  │  Node A    │  │  Node B    │  │  Node C    │  │  Node D    │   │
│  │  Gemma4-31B│  │  Gemma4-31B│  │  Gemma4-E4B│  │  Qwen3-32B │   │
│  │  layers    │  │  layers    │  │  draft spec│  │  code tasks│   │
│  │  [0 → 15]  │  │  [16 → 31] │  │  specul.   │  │            │   │
│  └────────────┘  └────────────┘  └────────────┘  └────────────┘   │</span>
<span class="c-muted">│                                                                      │
└─────────────────────────────────────────────────────────────────────┘</span>
  </div>

  <div class="tech-grid" style="margin-top: 32px;">
    <div class="tech-card">
      <span class="tech-layer layer-infra">P2P Infrastructure</span>
      <h3>Holochain — agent-centric DHT</h3>
      <p>No blockchain, no global consensus. Each node maintains its own immutable source chain. The DHT validates shared state without any central server.</p>
      <div class="tech-tag">Holochain 0.4 · Rust · WebRTC · ed25519</div>
    </div>
    <div class="tech-card">
      <span class="tech-layer layer-model">Distributed inference</span>
      <h3>Layer pipeline-splitting</h3>
      <p>A 31B model too large for a single machine is split by layers and distributed across multiple nodes. Speculative decoding (draft + verify) accelerates inference.</p>
      <div class="tech-tag">llama.cpp · GGUF · layer-sharding · spec-decode</div>
    </div>
    <div class="tech-card">
      <span class="tech-layer layer-sec">Security & identity</span>
      <h3>Membrane Proofs + Warrants</h3>
      <p>Only machines holding a valid Membrane Proof can join the private network. Cryptographic warrants attest the reputation and capabilities of each node.</p>
      <div class="tech-tag">ed25519 · validation zomes · Holochain warrants</div>
    </div>
    <div class="tech-card">
      <span class="tech-layer layer-api">Compatibility</span>
      <h3>Local OpenAI-compatible API</h3>
      <p>The <code>localhost:9337/v1</code> endpoint mirrors the OpenAI API spec. Copilot, LangChain, n8n, Cursor, Goose — all work without modification.</p>
      <div class="tech-tag">REST · SSE streaming · OpenAI v1 spec</div>
    </div>
    <div class="tech-card">
      <span class="tech-layer layer-model">Agent orchestration</span>
      <h3>Goose — multi-agent framework</h3>
      <p>Goose (Block, Apache 2.0) orchestrates complex AI agents via native MCP. Its MCP servers connect to Holochain zomes to access the shared DHT blackboard.</p>
      <div class="tech-tag">Goose · MCP · Rust · multi-LLM</div>
    </div>
    <div class="tech-card">
      <span class="tech-layer layer-infra">Models</span>
      <h3>Gemma 4 · Qwen3 · Llama 3.3</h3>
      <p>Open-source GGUF models. Gemma 4 (Google, Apache 2.0) is prioritized for its quality/VRAM density. Full pipeline-splitting support for both dense and MoE architectures.</p>
      <div class="tech-tag">GGUF · quantization Q4-Q8 · MoE · dense</div>
    </div>
  </div>
</div>

<!-- FUNCTIONAL FLOW -->
<div class="full" style="background: var(--bg2); border-top: 1px solid var(--border); border-bottom: 1px solid var(--border);">
  <div class="full-inner">
    <div class="eyebrow">Functional model</div>
    <h2>How a request traverses the network</h2>
    <p class="section-sub" style="margin-bottom: 0">From your employee's prompt to the response — without ever touching the outside world.</p>
    <div class="flow-steps">
      <div class="flow-step">
        <div class="fs-num">1</div>
        <h4>User prompt</h4>
        <p>The user submits a request via their tool (VS Code, internal chat, n8n…)</p>
      </div>
      <div class="flow-step">
        <div class="fs-num">2</div>
        <h4>Local API proxy</h4>
        <p>The <code style="font-size:10px">:9337</code> proxy receives the OpenAI-compat. request and forwards it to the Holochain conductor</p>
      </div>
      <div class="flow-step">
        <div class="fs-num">3</div>
        <h4>DHT routing</h4>
        <p>The mesh consults the node registry to find available machines by model and current load</p>
      </div>
      <div class="flow-step">
        <div class="fs-num">4</div>
        <h4>Distributed inference</h4>
        <p>Model layers are executed in pipeline across selected nodes (layer-sharding)</p>
      </div>
      <div class="flow-step">
        <div class="fs-num">5</div>
        <h4>Token streaming</h4>
        <p>Generated tokens stream to the API in real time — latency identical to local execution</p>
      </div>
      <div class="flow-step">
        <div class="fs-num">6</div>
        <h4>Audit chain</h4>
        <p>The task is cryptographically chained for traceability. No sensitive data stored.</p>
      </div>
    </div>
  </div>
</div>

<!-- SECURITY -->
<div class="section" id="security">
  <div class="eyebrow">Security & compliance</div>
  <h2>Built for the most demanding<br>environments.</h2>
  <p class="section-sub">Every layer of the system is designed to satisfy the requirements of CISOs, DPOs, and security teams.</p>

  <div class="sec-grid">
    <div class="sec-card">
      <div class="sec-icon">🔑</div>
      <h3>Cryptographic identities</h3>
      <p>Every node is identified by an ed25519 key pair. No centralized authentication, no password — cryptography is the sole identity mechanism.</p>
      <span class="sec-badge">ed25519 · Holochain keys</span>
    </div>
    <div class="sec-card">
      <div class="sec-icon">🚪</div>
      <h3>Network access control</h3>
      <p>Membrane Proofs define who can join the private network. A machine without a valid proof is invisible to the mesh — even on the same LAN.</p>
      <span class="sec-badge">Membrane Proofs · RBAC</span>
    </div>
    <div class="sec-card">
      <div class="sec-icon">🔒</div>
      <h3>End-to-end encryption</h3>
      <p>All inter-node exchanges are encrypted. Data in transit is protected even on a compromised internal network.</p>
      <span class="sec-badge">TLS · WebRTC DTLS · E2E</span>
    </div>
    <div class="sec-card">
      <div class="sec-icon">📋</div>
      <h3>Immutable audit trail</h3>
      <p>Every inference task is appended to a Holochain source chain — a cryptographically linked append-only structure. Ideal for regulatory audits.</p>
      <span class="sec-badge">Source chain · append-only</span>
    </div>
    <div class="sec-card">
      <div class="sec-icon">🌐</div>
      <h3>Full network isolation</h3>
      <p>The network can run without any internet access. Supported modes: internal LAN, corporate VPN, air-gapped network. Zero dependency on external relays or DNS.</p>
      <span class="sec-badge">Air-gap ready · VPN · LAN</span>
    </div>
    <div class="sec-card">
      <div class="sec-icon">🔍</div>
      <h3>Peer validation</h3>
      <p>Holochain validation zomes allow each node to verify the conformity of other nodes' data before accepting it. No undetected malicious node.</p>
      <span class="sec-badge">Zome validation · warrants</span>
    </div>
  </div>

  <div style="margin-top: 40px;">
    <h3 style="margin-bottom: 20px; font-size: 16px; color: var(--muted); font-weight: 600;">Supported certifications & frameworks</h3>
    <div class="compliance-grid">
      <div class="comp-badge"><span class="comp-check">✓</span> GDPR</div>
      <div class="comp-badge"><span class="comp-check">✓</span> HIPAA</div>
      <div class="comp-badge"><span class="comp-check">✓</span> SOC 2 Type II</div>
      <div class="comp-badge"><span class="comp-check">✓</span> ISO 27001</div>
      <div class="comp-badge"><span class="comp-check">✓</span> NIS2</div>
      <div class="comp-badge"><span class="comp-check">✓</span> DORA (Finance)</div>
      <div class="comp-badge"><span class="comp-check">✓</span> HIPAA (Healthcare)</div>
      <div class="comp-badge"><span class="comp-check">✓</span> Air-gapped network</div>
    </div>
  </div>
</div>

<!-- BUSINESS CASE -->
<div class="roi-strip" id="business">
  <div class="roi-inner">
    <div class="eyebrow">Business case</div>
    <h2>The math is simple.</h2>
    <p class="section-sub" style="margin-bottom: 0">You're already paying for the machines. AInonymous turns their off-hours into AI infrastructure.</p>

    <div class="roi-grid">
      <div class="roi-card">
        <div class="roi-num">~70%</div>
        <div class="roi-label">of compute resources idle outside business hours</div>
      </div>
      <div class="roi-card">
        <div class="roi-num">$0</div>
        <div class="roi-label">additional GPU required for a mesh of 50+ machines</div>
      </div>
      <div class="roi-card">
        <div class="roi-num">×N</div>
        <div class="roi-label">inference power scales with every new workstation</div>
      </div>
      <div class="roi-card">
        <div class="roi-num">&lt;1 yr</div>
        <div class="roi-label">ROI vs. cloud AI subscriptions for a 100-person team</div>
      </div>
    </div>

    <!-- Scenario ROI -->
    <div class="roi-scenario">
      <h3>Scenario: 200 employees, 50 modest GPU workstations (8 GB VRAM)</h3>
      <div class="roi-row">
        <span>Current cost — cloud AI subscriptions (200 × $20/month)</span>
        <span>$4,000 / month</span>
      </div>
      <div class="roi-row">
        <span>AInonymous Enterprise cost (deployment + support)</span>
        <span>~$500–800 / month</span>
      </div>
      <div class="roi-row">
        <span>Estimated annual saving</span>
        <span>~$38,000 – $42,000</span>
      </div>
      <div class="roi-row">
        <span>Data exposed to third parties after migration</span>
        <span style="color: var(--accent2)">0</span>
      </div>
      <div class="roi-row">
        <span>3-year saving (excluding headcount growth)</span>
        <span>+$120,000</span>
      </div>
    </div>
  </div>
</div>

<!-- COMPARATIVE TABLE -->
<div class="section">
  <div class="eyebrow">Detailed comparison</div>
  <h2>Enterprise vs. cloud vs. local</h2>
  <div class="bc-table-wrap">
    <table>
      <thead>
        <tr>
          <th>Criteria</th>
          <th>Cloud AI (OpenAI, Azure)</th>
          <th>Ollama local (per device)</th>
          <th class="hl">AInonymous Enterprise ✦</th>
        </tr>
      </thead>
      <tbody>
        <tr class="section-row"><td colspan="4">Sovereignty & security</td></tr>
        <tr>
          <td>Data outside your perimeter</td>
          <td class="v-no">✗ Yes — third-party servers</td>
          <td class="v-ok">✓ No — local machine</td>
          <td class="col v-ok">✓ No — internal network</td>
        </tr>
        <tr>
          <td>GDPR compliance by design</td>
          <td>Partial (DPA required)</td>
          <td class="v-ok">✓ Full</td>
          <td class="col v-ok">✓ Full + audited</td>
        </tr>
        <tr>
          <td>Granular access control</td>
          <td>External IAM</td>
          <td class="v-no">✗ None</td>
          <td class="col v-ok">✓ Membrane Proofs</td>
        </tr>
        <tr>
          <td>Cryptographic audit trail</td>
          <td class="v-no">✗ No</td>
          <td class="v-no">✗ No</td>
          <td class="col v-ok">✓ Holochain source chain</td>
        </tr>
        <tr>
          <td>Air-gap mode</td>
          <td class="v-no">✗ Impossible</td>
          <td class="v-ok">✓ Yes (single machine)</td>
          <td class="col v-ok">✓ Yes (entire network)</td>
        </tr>

        <tr class="section-row"><td colspan="4">Cost & scalability</td></tr>
        <tr>
          <td>Cost per user/month</td>
          <td>~$20–30</td>
          <td>$0 (GPU required)</td>
          <td class="col">~$2–4 (support)</td>
        </tr>
        <tr>
          <td>AI power scalability</td>
          <td>$ (quota purchase)</td>
          <td class="v-no">✗ Limited to local GPU</td>
          <td class="col v-ok">✓ Scales with the fleet</td>
        </tr>
        <tr>
          <td>Dedicated GPU required</td>
          <td class="v-ok">✓ No (cloud)</td>
          <td class="v-no">✗ Yes (per device)</td>
          <td class="col v-ok">✓ No (pooled)</td>
        </tr>

        <tr class="section-row"><td colspan="4">AI capabilities</td></tr>
        <tr>
          <td>Fine-tuning on internal data</td>
          <td>Possible (cloud data)</td>
          <td class="v-no">✗ Complex</td>
          <td class="col v-ok">✓ On-prem, native</td>
        </tr>
        <tr>
          <td>Models &gt;30B params</td>
          <td class="v-ok">✓ Yes</td>
          <td class="v-no">✗ Requires 20+ GB VRAM</td>
          <td class="col v-ok">✓ Via pipeline-splitting</td>
        </tr>
        <tr>
          <td>Multi-agent / orchestration</td>
          <td>API only</td>
          <td class="v-no">✗ Manual</td>
          <td class="col v-ok">✓ Goose + DHT blackboard</td>
        </tr>
        <tr>
          <td>OpenAI-compatible API</td>
          <td class="v-ok">✓ Native</td>
          <td class="v-ok">✓ Yes</td>
          <td class="col v-ok">✓ localhost:9337</td>
        </tr>

        <tr class="section-row"><td colspan="4">Governance</td></tr>
        <tr>
          <td>Vendor lock-in</td>
          <td class="v-no">✗ Strong</td>
          <td class="v-ok">✓ None</td>
          <td class="col v-ok">✓ None (Apache 2.0)</td>
        </tr>
        <tr>
          <td>Vendor independence</td>
          <td class="v-no">✗ Total dependency</td>
          <td class="v-ok">✓ Full</td>
          <td class="col v-ok">✓ Full + resilient</td>
        </tr>
      </tbody>
    </table>
  </div>
</div>

<!-- DEPLOYMENT MODES -->
<div class="section" id="deployment" style="padding-top: 0">
  <div class="eyebrow">Deployment modes</div>
  <h2>From pilot to enterprise scale.</h2>
  <p class="section-sub">Start small, scale without migration — the architecture is identical at 5 or 5,000 nodes.</p>
  <div class="deploy-grid">
    <div class="deploy-card">
      <div class="deploy-icon">🌱</div>
      <h3>Team pilot</h3>
      <p>10–30 machines, LAN network. Ideal for validating the use case with a tech or R&D team. Deployment in under a day.</p>
      <div class="deploy-tags">
        <span class="dtag dtag-t">Gemma4-E4B</span>
        <span class="dtag dtag-t">LAN</span>
        <span class="dtag dtag-a">1 day</span>
      </div>
    </div>
    <div class="deploy-card featured">
      <div class="deploy-icon">🏢</div>
      <h3>Site deployment</h3>
      <p>50–500 machines, corporate VPN. 26B–31B models distributed, multi-service, centralized API per site. Recommended configuration for most enterprises.</p>
      <div class="deploy-tags">
        <span class="dtag dtag-p">Gemma4-31B</span>
        <span class="dtag dtag-t">VPN · LAN</span>
        <span class="dtag dtag-a">1–2 weeks</span>
      </div>
    </div>
    <div class="deploy-card">
      <div class="deploy-icon">🏛️</div>
      <h3>Multi-site group</h3>
      <p>500+ machines, multi-site, mesh network across subsidiaries. Each site has an autonomous sub-network with optional federation. Llama 3.3 70B support.</p>
      <div class="deploy-tags">
        <span class="dtag dtag-p">Llama3.3-70B</span>
        <span class="dtag dtag-t">Multi-VPN</span>
        <span class="dtag dtag-a">1 month</span>
      </div>
    </div>
    <div class="deploy-card">
      <div class="deploy-icon">🔒</div>
      <h3>Air-gap / high security</h3>
      <p>Completely isolated network, no internet access. For defense, healthcare, regulated finance environments. Pre-loaded models, zero external dependency.</p>
      <div class="deploy-tags">
        <span class="dtag dtag-p">All models</span>
        <span class="dtag dtag-t">Air-gap</span>
        <span class="dtag dtag-a">Custom</span>
      </div>
    </div>
  </div>
</div>

<!-- FINAL CTA -->
<div class="cta-section">
  <div class="cta-inner">
    <div class="eyebrow" style="text-align:center">Take action</div>
    <h2>Your private AI infrastructure<br>starts here.</h2>
    <p>AInonymous Enterprise is open source, Apache 2.0. Deploy, adapt, audit — without asking anyone's permission.</p>
    <div class="cta-actions">
      <a href="https://github.com/Geoking2104/AInonymous" class="btn-p">Access source code →</a>
      <a href="https://github.com/Geoking2104/AInonymous/blob/main/README.md" class="btn-g">Technical documentation</a>
      <a href="landing-en.html" class="btn-g">← Back to home</a>
    </div>
  </div>
</div>

<footer>
  <div class="f-logo">AI<span>n</span>onymous <span style="color:var(--muted);font-size:14px;font-weight:400">Enterprise</span></div>
  <p style="margin-bottom: 14px">Sovereign AI for organizations that don't compromise.</p>
  <div>
    <a href="landing-en.html">Home</a>
    <a href="https://github.com/Geoking2104/AInonymous">GitHub</a>
    <a href="https://github.com/Geoking2104/AInonymous/blob/main/README.md">Docs</a>
    <a href="https://github.com/Geoking2104/AInonymous/blob/main/LICENSE">Apache 2.0</a>
  </div>
  <p style="margin-top: 24px; font-size: 11px; opacity: .35">Holochain · llama.cpp · Gemma 4 · Goose · Apache 2.0</p>
</footer>

</body>
</html>
```

</details>
