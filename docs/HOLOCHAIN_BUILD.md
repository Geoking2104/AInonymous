# Build & exécution Holochain (zomes → .happ → conducteur)

Statut : les zomes sont alignés sur **Holochain 0.6.1** (`hdk 0.6` / `hdi 0.7`) et
**compilent en WASM** (6 `.wasm` : 3 integrity + 3 coordinator). Reste à packager
le `.happ` et à le faire tourner dans un conducteur — ces étapes exigent la CLI
`hc`, qui nécessite `libsodium`/`sqlite` (non installables dans tous les
environnements). Voici comment procéder sur une machine de dev.

## Prérequis (CLI `hc`)

La CLI Holochain `hc` doit correspondre à **Holochain 0.6**. Deux options.

### Option A — Nix (recommandé, `hc` prêt à l'emploi)

```bash
# Dev shell Holochain 0.6 (fournit hc, holochain, lair-keystore)
nix develop github:holochain/holochain#holonix --override-input versions \
  'github:holochain/holochain?dir=versions/0_6'
```

### Option B — cargo install (Linux, avec libs système)

```bash
sudo apt install -y libsodium-dev libsqlite3-dev pkg-config build-essential
rustup target add wasm32-unknown-unknown
cargo install holochain_cli --version 0.6.1     # binaire `hc`
# (optionnel, pour exécuter un conducteur : holochain + lair-keystore 0.6)
```

> Note : la compilation WASM des zomes dépend de
> `dnas/ainonymous-core/.cargo/config.toml`, qui sélectionne le backend
> `getrandom="custom"` requis pour `wasm32-unknown-unknown` sous Holochain 0.6.

## 1) Packager le hApp

```bash
make build-happ
# équivaut à :
#   cd dnas/ainonymous-core
#   cargo build --release --target wasm32-unknown-unknown
#   hc dna pack dnas/inference-mesh/workdir
#   hc dna pack dnas/agent-registry/workdir
#   hc dna pack dnas/blackboard/workdir
#   hc app pack .
```

Sortie attendue : `dnas/ainonymous-core/ainonymous-core.happ`.

## 2) Lancer un conducteur de test (sandbox)

```bash
# Démarre un conducteur éphémère avec le hApp installé
hc sandbox generate dnas/ainonymous-core/ainonymous-core.happ --run
# Note le port de l'app websocket affiché (ex: ws://127.0.0.1:<port>)
```

Pour deux agents (mesh local), lancer deux sandboxes sur des ports distincts.

## 3) Brancher le daemon sur le conducteur (à implémenter)

Aujourd'hui `crates/ainonymous-daemon/src/holochain.rs` utilise un **pont REST
factice** (`zome_call` POST vers le daemon lui-même) et le plan de contrôle
**bootstrap statique** (cf. `peers`/`pipeline_stages` dans la config). Pour le
mode Holochain réel :

1. Ajouter la dépendance `holochain_client` (≈ 0.8, à aligner sur le conducteur 0.6)
   au crate `ainonymous-daemon`.
2. Dans `holochain.rs`, remplacer `zome_call` par un appel
   `AppWebsocket::call_zome(...)` :
   - connexion au port app websocket du conducteur (cf. `holochain_conductor_url`/
     port app dans `DaemonConfig`),
   - authentification de l'app interface (token),
   - signature des zome calls via le lair-keystore de l'agent.
3. Conserver le **trait de plan de contrôle** : l'implémentation Holochain se
   branche derrière la même API que le bootstrap statique (`negotiate_quic_session`,
   `get_execution_plan`, `get_available_nodes`, blackboard…), de sorte que le
   testnet loopback (mock) et le mode Holochain réel restent interchangeables.
4. Câbler le signal `QuicListenerSignal` (émis par le zome `inference-mesh`) vers
   `SessionRegistry::register` du listener QUIC (le pont REST actuel le fait déjà
   pour le bootstrap statique).

## Matrice de versions (référence)

| Composant | Version |
|---|---|
| Holochain (conducteur) | 0.6.1 |
| HDK (coordinator) | 0.6 |
| HDI (integrity) | 0.7 |
| holochain_serialized_bytes | 0.0.57 |
| holochain_client (daemon, à ajouter) | ~0.8 |

## Dépannage

| Symptôme | Cause / fix |
|---|---|
| `getrandom ... wasm32 not supported` | `.cargo/config.toml` manquant (backend custom) |
| `cannot find holochain_serialized_bytes` | dépendance directe absente dans un zome |
| `hc: command not found` | CLI non installée (cf. Option A/B) |
| `libsodium-sys` build échoue | installer `libsodium-dev` ou utiliser Nix |
