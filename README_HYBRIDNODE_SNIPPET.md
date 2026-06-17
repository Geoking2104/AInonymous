## HybridNode — Distributed Inference Architecture

AInonymous uses a **HybridNode** architecture that combines three layers into a locality-aware inference scheduler:

| Layer | Technology | Role |
|-------|-----------|------|
| Overlay | **Holochain 0.6.1** | DHT, identity (AgentPubKey ed25519), coordination |
| Data plane | **QUIC/mTLS** | Tensor activation transfers, token streams |
| Underlay | **SD-WAN** | Topology-aware routing, SLA enforcement, QoS |

### Quick Start

```bash
# Install from source
cargo install --path crates/hybridnode-daemon --features mock-sdwan

# Initialize a new project config
bash scripts/hybridnode/init_project.sh myproject

# Edit the config, then run
hybridnode --config hybridnode/configs/myproject.hybridnode.yaml
```

### Architecture Docs

- [`docs/HYBRIDNODE.md`](docs/HYBRIDNODE.md) — Concepts, use cases, SD-WAN policy
- [`docs/HYBRIDNODE_ARCHITECTURE.md`](docs/HYBRIDNODE_ARCHITECTURE.md) — Component tree, request flow, security
- [`docs/HYBRIDNODE_CARGO_PATCH.md`](docs/HYBRIDNODE_CARGO_PATCH.md) — Workspace integration guide
- [`hybridnode/`](hybridnode/) — Configs, policies, schemas, specs

### Key Security Properties

- **mTLS strict** — ed25519 AgentPubKey reused as QUIC certificate; `PeerKeyVerifier` enforces mutual auth
- **Model attestation** — SHA-256 verified locally + cross-peer confirmation (≥2 peers)
- **Warrant system** — Holochain DHT cryptographic proof of misbehavior; auto-excludes from scheduling
- **Private bootstrap** — `PrivateNetworkProof` membrane for closed consortium deployments
