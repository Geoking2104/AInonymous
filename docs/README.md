# Documentation

This directory contains the maintained documentation for the Holochain 0.7 generation of AInonymous. Older speculative milestone documents were removed during the migration because they described APIs, manifests and security properties that were no longer true. Their history remains available in Git.

## Start here

- [Architecture](ARCHITECTURE.md) — components, trust boundaries and request flow
- [Security](SECURITY.md) — threat model, enforced invariants and known gaps
- [Holochain 0.7 migration](HOLOCHAIN_0_7_MIGRATION.md) — compatibility matrix and upgrade procedure
- [Holochain build and operations](HOLOCHAIN_BUILD.md) — build, pack, install and conductor configuration
- [HybridNode integration](../HYBRIDNODE_APPLY.md) — reuse in another application
- [API reference](API_SPEC.md) — supported local HTTP and zome boundaries
- [Network data plane](NETWORK.md) — QUIC identity binding and traffic flow
- [Two-node testnet](TESTNET_2NODES.md) — local validation procedure
- [PyTorch integration](PYTORCH_INTEGRATION.md) — experimental Python adapter
- [Architecture review](ZK_AIS_GATENYM_ARCHITECTURE_REVIEW.md) — consolidated findings and disposition

## Documentation policy

The source code and executable schemas are authoritative. Documentation must distinguish implemented behavior from planned work. In particular, “anonymous”, “zero knowledge”, public-network Sybil resistance, production SD-WAN integrations and native multi-node llama.cpp execution must not be presented as complete unless backed by tests in the repository.
