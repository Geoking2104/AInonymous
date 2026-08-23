## HybridNode

This project uses HybridNode's three-plane model: Holochain 0.7.0/Iroh for authenticated coordination, QUIC/TLS 1.3 for direct data transfer, and an optional SD-WAN adapter for topology and SLA input. Holochain agent keys stay in Lair; a separate QUIC Ed25519 key is published in an agent-authored capability record and verified during session negotiation.

HybridNode is alpha software. Private deployments require network-bound membrane proofs at hApp installation. Public-network Sybil resistance, zero-knowledge inference and network anonymity are not implemented.

See [integration instructions](HYBRIDNODE_APPLY.md) and the [security model](docs/SECURITY.md).
