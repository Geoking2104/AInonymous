# HybridNode — Cargo Patch et Dépendances

> Instructions pour intégrer `hybridnode-core` dans le workspace AInonymous ou un projet externe.

---

## Workspace AInonymous

Le workspace `Cargo.toml` racine doit déclarer les nouveaux crates :

```toml
# Cargo.toml (racine)
[workspace]
members = [
    # crates existants
    "crates/ainonymous-cli",
    "crates/ainonymous-daemon",
    "crates/ainonymous-mcp",
    "crates/ainonymous-proxy",
    "crates/ainonymous-quic",
    "crates/ainonymous-types",
    # nouveaux crates HybridNode
    "crates/hybridnode-core",
    "crates/hybridnode-daemon",
    # dnas Holochain
    "dnas/ainonymous-core/dnas/agent-registry/zomes/coordinator",
    "dnas/ainonymous-core/dnas/agent-registry/zomes/integrity",
    "dnas/ainonymous-core/dnas/blackboard/zomes/coordinator",
    "dnas/ainonymous-core/dnas/blackboard/zomes/integrity",
    "dnas/ainonymous-core/dnas/inference-mesh/zomes/coordinator",
    "dnas/ainonymous-core/dnas/inference-mesh/zomes/integrity",
    # nouveaux dnas HybridNode
    "dnas/hybridnode/dnas/hybridnode-core/zomes/coordinator",
    "dnas/hybridnode/dnas/hybridnode-core/zomes/integrity",
]
resolver = "2"

[workspace.dependencies]
# Holochain
hdk = "=0.6.1"
hdi = "=0.7.1"
# Sérialisation
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
# Async
tokio = { version = "1", features = ["full"] }
anyhow = "1"
thiserror = "1"
# Réseau
iroh-net = "0.23"
quinn = "0.11"
# Crypto
ed25519-dalek = { version = "2", features = ["rand_core"] }
sha2 = "0.10"
# HTTP (pour l'API SD-WAN)
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }
# Observabilité
opentelemetry = "0.22"
prometheus-client = "0.22"
# Config
config = "0.14"
```

## Dépendances de hybridnode-core

```toml
# crates/hybridnode-core/Cargo.toml
[package]
name = "hybridnode-core"
version = "0.1.0"
edition = "2021"

[dependencies]
serde.workspace = true
serde_json.workspace = true
serde_yaml.workspace = true
tokio.workspace = true
anyhow.workspace = true
thiserror.workspace = true
reqwest.workspace = true
ed25519-dalek.workspace = true
sha2.workspace = true
prometheus-client.workspace = true

[features]
default = []
vmanage = []          # Cisco vManage REST API
velocloud = []        # VMware VeloCloud REST API
mock-sdwan = []       # implémentation mock pour tests
```

## Dépendances de hybridnode-daemon

```toml
# crates/hybridnode-daemon/Cargo.toml
[package]
name = "hybridnode-daemon"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "hybridnode-daemon"
path = "src/main.rs"

[dependencies]
hybridnode-core = { path = "../hybridnode-core" }
serde.workspace = true
serde_yaml.workspace = true
tokio.workspace = true
anyhow.workspace = true
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
clap = { version = "4", features = ["derive"] }
```

## Dépendances des zomes Holochain HybridNode

```toml
# dnas/hybridnode/dnas/hybridnode-core/zomes/integrity/Cargo.toml
[package]
name = "hybridnode-integrity"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
hdi.workspace = true
serde.workspace = true
```

```toml
# dnas/hybridnode/dnas/hybridnode-core/zomes/coordinator/Cargo.toml
[package]
name = "hybridnode-coordinator"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
hdk.workspace = true
serde.workspace = true
hybridnode-integrity = { path = "../integrity" }
```

## Notes de compatibilité

- Rust stable 1.80+ requis
- Holochain 0.6.1 requis (iroh comme transport par défaut)
- Le crate `hybridnode-core` est `no_std`-compatible dans les zomes Holochain (sans les features `sdwan`)
- Le trait `SdwanProvider` est async — nécessite `tokio` en dehors des zomes
