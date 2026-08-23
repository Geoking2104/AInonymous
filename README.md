# AInonymous

AInonymous is an experimental, agent-centric control plane for distributed LLM inference. It combines Holochain for authenticated coordination, a direct QUIC/mTLS data plane for activations and model traffic, and an optional SD-WAN adapter for topology and SLA-aware scheduling.

> **Status:** alpha research software. The repository now targets Holochain **0.7.0** exclusively. It is suitable for development and controlled private pilots, not for anonymous public production workloads. The project does not currently provide zero-knowledge inference, traffic-flow anonymity, or a complete public-network Sybil defense.

## Architecture

| Plane | Implementation | Purpose |
| --- | --- | --- |
| Control | Holochain 0.7.0 over Iroh | Agent identity, capabilities, scheduling coordination, audit records, membrane admission |
| Data | QUIC + TLS 1.3/mTLS | Direct peer-to-peer inference traffic and activation transfer |
| Underlay | SD-WAN adapter | Optional topology, QoS and link-health input |
| Runtime | Rust daemons + llama.cpp/PyTorch adapters | Local execution and pipeline orchestration |

Holochain and QUIC deliberately use different Ed25519 keys. A node publishes its QUIC public key in a capability entry authored by its Holochain agent. Session negotiation verifies that the supplied transport key matches the record authored by the caller. Private-network admission is enforced by DNA properties and a network-bound membrane proof during genesis.

See [Architecture](docs/ARCHITECTURE.md), [Security](docs/SECURITY.md), and [Holochain 0.7 migration](docs/HOLOCHAIN_0_7_MIGRATION.md) for the design and its boundaries.

## Version matrix

| Component | Pinned version |
| --- | --- |
| Holochain conductor / `hc` | 0.7.0 |
| Rust `holochain_client` | 0.9.0 |
| HDK | 0.7.0 |
| HDI | 0.8.0 |
| Lair keystore | 0.7.1 |

Holochain 0.7 is not database-compatible with 0.6. Create a new conductor data root and reinstall the hApps. The migrated manifests use new network seeds, so every peer must install the newly packed DNAs.

## Repository layout

```text
crates/
  ainonymous-daemon/      Holochain/QUIC orchestration daemon
  ainonymous-quic/        direct data plane and peer-key verification
  ainonymous-proxy/       HTTP compatibility proxy
  ainonymous-cli/         operator CLI
  ainonymous-mcp/         MCP integration
  hybridnode-core/        reusable config, identity, topology and scheduler layer
  hybridnode-daemon/      HybridNode identity/topology startup scaffold
dnas/
  ainonymous-core/        inference-mesh, agent-registry and blackboard hApp
  hybridnode/             HybridNode membership/audit hApp
hybridnode/               configuration schema, examples and policies
scripts/                  build, validation and two-node testnet tooling
docs/                     maintained architecture and operating documentation
```

## Prerequisites

- Rust stable (the workspace declares Rust 1.80 as its minimum)
- `wasm32-unknown-unknown`
- Holochain 0.7.0 and `hc` 0.7.0
- Lair keystore 0.7.1
- Python 3 with `PyYAML` and `jsonschema` for configuration validation
- Bash for packaging and testnet scripts (WSL or Git Bash on Windows)

Install the Rust target and confirm that the Holochain tools are the pinned versions:

```bash
rustup target add wasm32-unknown-unknown
hc --version
holochain --version
```

On x86_64 Linux or WSL2, `bash scripts/setup_holochain_wsl.sh` installs the pinned release assets after verifying their published SHA-256 digests.

Do not package this repository with an older `hc`; manifest validation and bundle formats are version-sensitive.

## Build and test

```bash
# Native crates
cargo check --workspace
cargo test --workspace

# Both Holochain zome workspaces
cargo build --manifest-path dnas/ainonymous-core/Cargo.toml \
  --release --target wasm32-unknown-unknown
cargo build --manifest-path dnas/hybridnode/Cargo.toml \
  --release --target wasm32-unknown-unknown

# Validate reference configurations
python scripts/hybridnode/validate_config.py \
  hybridnode/configs/ainonymous.hybridnode.yaml \
  hybridnode/configs/generic-project.hybridnode.yaml

# Package both hApps with hc 0.7.0
bash scripts/build-happ.sh release
```

For a private deployment, also run validation with `--production`. The reference development configuration intentionally uses a mock SD-WAN adapter and is not a production profile.

## Running with a conductor

1. Start a Holochain 0.7.0 conductor using a fresh data root and loopback-only admin WebSocket.
2. Pack and install `dnas/ainonymous-core/ainonymous-core.happ` and `dnas/hybridnode/hybridnode.happ`.
3. Configure distinct loopback admin and app ports plus the installed app ID.
4. For a private DNA, generate a `PrivateNetworkProof` for the target agent and network ID, then supply it in the role settings when installing the app.
5. Start the daemon. Conductor connection failures are fatal; the runtime no longer silently falls back to static discovery.

The exact commands and conductor configuration are in [Holochain build and operations](docs/HOLOCHAIN_BUILD.md).

## Reuse in another project

Generate a project-specific configuration and validate it:

```bash
bash scripts/hybridnode/init_project.sh my-project
python scripts/hybridnode/validate_config.py my-project/hybridnode.yaml
```

Integration requires more than copying YAML: install the HybridNode hApp, connect through a loopback admin interface, announce the node's QUIC public key through the authenticated agent registry, and pin that key before opening a data-plane session. Follow [HybridNode integration](HYBRIDNODE_APPLY.md).

## Security defaults

- Conductor is the default discovery backend; explicit connection failures fail closed.
- The admin WebSocket is required to be loopback-only.
- QUIC strict peer verification cannot be disabled by accepted configuration.
- SD-WAN controller TLS verification cannot be disabled by accepted configuration.
- Private-network mode requires a matching private bootstrap mode and bootstrap URL.
- Membrane proofs are installation/genesis credentials, never ordinary zome-call fields.
- Transport private keys are stored in the OS keyring when the secure-keyring feature is enabled; Holochain agent keys remain in Lair.

The public-network mode remains experimental because its advertised proof-of-work admission is not implemented. See [Security](docs/SECURITY.md) before exposing any service.

## Documentation

Start with the [documentation index](docs/README.md). The most important documents are:

- [Architecture](docs/ARCHITECTURE.md)
- [Security and threat model](docs/SECURITY.md)
- [Holochain 0.7 migration](docs/HOLOCHAIN_0_7_MIGRATION.md)
- [Holochain build and operations](docs/HOLOCHAIN_BUILD.md)
- [API reference](docs/API_SPEC.md)
- [Network/data plane](docs/NETWORK.md)
- [Two-node testnet](docs/TESTNET_2NODES.md)
- [Original architecture review](docs/ZK_AIS_GATENYM_ARCHITECTURE_REVIEW.md)

## License

Apache-2.0. See [LICENSE](LICENSE) and [DISCLAIMER](DISCLAIMER.md).
