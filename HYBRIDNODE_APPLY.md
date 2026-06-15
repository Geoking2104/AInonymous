# Appliquer HybridNode à un nouveau projet

Ce document décrit comment intégrer l'architecture HybridNode dans un projet existant ou nouveau.

## Prérequis

- Rust stable (≥ 1.78)
- Holochain conductor 0.6.1 (avec `lair-keystore`)
- Python 3.10+ (pour les scripts de validation)
- Accès à un contrôleur SD-WAN (ou feature `mock-sdwan` pour le dev)

## 1. Ajouter hybridnode-core comme dépendance

Dans votre `Cargo.toml` workspace (voir [`docs/HYBRIDNODE_CARGO_PATCH.md`](docs/HYBRIDNODE_CARGO_PATCH.md)) :

```toml
[workspace.dependencies]
hybridnode-core = { path = "crates/hybridnode-core", features = ["mock-sdwan"] }
```

Dans votre crate applicatif :

```toml
[dependencies]
hybridnode-core = { workspace = true }
```

## 2. Créer votre configuration

```bash
bash scripts/hybridnode/init_project.sh <mon-projet>
# → crée hybridnode/configs/mon-projet.hybridnode.yaml
```

Editez le fichier créé et renseignez :
- `holochain.conductor_url` — WebSocket du conductor Holochain
- `holochain.bootstrap_url` — URL du serveur bootstrap privé ou public
- `sdwan.api_url` — URL du contrôleur SD-WAN (laisser vide pour mock)
- Variables d'environnement : `SDWAN_API_TOKEN`, `SDWAN_SITE_ID`

## 3. Intégrer le HybridNode dans votre code Rust

```rust
use hybridnode_core::HybridNode;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let node = HybridNode::from_config("hybridnode/configs/mon-projet.hybridnode.yaml").await?;
    node.run().await
}
```

Ou utiliser les composants individuellement :

```rust
use hybridnode_core::{
    config::load_config,
    scheduler::{schedule, SchedulingContext, SchedulingStrategy},
    sdwan::connect as sdwan_connect,
    topology::NodeTopology,
};

let config = load_config("mon-projet.hybridnode.yaml")?;
let sdwan = sdwan_connect(&config.sdwan).await?;
// ... construire NodeTopology depuis Holochain DHT + SD-WAN ...
let decision = schedule(&SchedulingContext {
    model_name: "llama3-8b-q4".to_string(),
    activation_size_mb: 45.0,
    latency_budget_ms: 20.0,
    strategy: SchedulingStrategy::LocalFirst,
    topology,
});
```

## 4. Déployer le DNA Holochain

```bash
# Compiler les zomes (nécessite target wasm32-unknown-unknown)
rustup target add wasm32-unknown-unknown
cargo build --manifest-path dnas/hybridnode/dnas/hybridnode-core/zomes/integrity/Cargo.toml \
    --target wasm32-unknown-unknown --release
cargo build --manifest-path dnas/hybridnode/dnas/hybridnode-core/zomes/coordinator/Cargo.toml \
    --target wasm32-unknown-unknown --release

# Packager la DNA avec hc
hc dna pack dnas/hybridnode/dnas/hybridnode-core/workdir
hc app pack dnas/hybridnode
```

## 5. Politique SD-WAN

Importez `hybridnode/policies/sdwan-policy.yaml` dans votre contrôleur SD-WAN pour activer :
- DSCP 46 (EF) pour le trafic QUIC d'inférence
- Préférence MPLS/WAN privé pour les activations inter-sites
- Garantie de bande passante 40% pour la classe `inference-data`

## 6. Validation CI

Le workflow `.github/workflows/hybridnode-validate.yml` vérifie automatiquement :
- Configs YAML valides contre le schema JSON
- `mtls_strict` jamais désactivé
- Politique model-validation cohérente
- Clippy + tests unitaires sur `hybridnode-core`

## Réseaux privés

Pour un déploiement en consortium fermé :

```bash
bash scripts/hybridnode/init_project.sh mon-projet --private-network
```

Cela active `security.private_network: true`, ce qui déclenche `genesis_self_check` dans le zome d'intégrité — chaque nœud doit présenter un `PrivateNetworkProof` signé pour rejoindre le DHT.

## Support

- Architecture complète : [`docs/HYBRIDNODE_ARCHITECTURE.md`](docs/HYBRIDNODE_ARCHITECTURE.md)
- Spec complète : [`hybridnode/specs/hybridnode.yaml`](hybridnode/specs/hybridnode.yaml)
- Schéma JSON : [`hybridnode/schemas/hybridnode.schema.json`](hybridnode/schemas/hybridnode.schema.json)
