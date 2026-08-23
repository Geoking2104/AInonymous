# Architecture

## Purpose and scope

AInonymous coordinates distributed inference without placing a central database in the control path. Holochain stores agent-authored state and validates shared records; direct QUIC connections carry high-volume inference data. SD-WAN is an optional source of topology and SLA signals, not an identity authority.

The design is intentionally split into planes so that DHT replication is never used for model weights, prompts, activations or token streams.

```text
Operator / local client
        |
        v
Local proxy and daemon (loopback HTTP)
        |
        +---- Holochain 0.7 control plane ---- Iroh/DHT peers
        |       identity, capabilities, plans, audit
        |
        +---- QUIC + TLS 1.3 data plane ------ selected peer
        |       activations and inference messages
        |
        +---- optional SD-WAN adapter -------- controller
                link health and policy input
```

## Components

### Native workspace

- `ainonymous-daemon` connects to the conductor, announces capabilities and coordinates QUIC sessions.
- `ainonymous-quic` implements framing, transfer, TLS certificates and peer public-key verification.
- `ainonymous-proxy` exposes a local compatibility API.
- `ainonymous-cli` and `ainonymous-mcp` are local operator/client integrations.
- `hybridnode-core` validates reusable configuration and combines Holochain identity, topology and scheduling input.
- `hybridnode-daemon` currently initializes validated configuration, Holochain identity, observability and topology adapters. It is a startup scaffold, not a second complete inference data plane.

### Holochain hApps

`ainonymous-core` contains three roles:

- `agent-registry`: agent-authored compute capabilities, loaded models and the QUIC public key.
- `inference-mesh`: plan negotiation and control-plane signals used to establish a direct session.
- `blackboard`: shared task/coordination entries.

`hybridnode` contains a reusable membership and audit role. Its genesis validation reads DNA properties on every build. When `private_network` is true, a valid membrane proof is mandatory and must bind the target agent to the configured `network_id`.

## Identity model

Holochain agent keys remain in Lair. QUIC uses a separate Ed25519 transport key because Holochain does not expose the agent private key for arbitrary TLS signing. The separation is safe only if the public keys are bound:

1. The daemon loads or creates the QUIC transport key.
2. The node publishes that public key in `NodeCapabilities`; the entry is authored by its Holochain agent.
3. The peer calls `negotiate_quic_session` with the same public key.
4. The zome uses `call_info().provenance` to load the caller's authored capability record.
5. A mismatch rejects negotiation before a QUIC peer is trusted.
6. The TLS verifier pins the expected Ed25519 public key.

This proves continuity between the authenticated Holochain caller and the direct transport key. It does not hide network addresses or traffic patterns.

## Admission model

Public DNA mode permits normal Holochain joining and remains suitable only for development. Private mode uses a signed `PrivateNetworkProof` supplied through `RoleSettings::Provisioned` during app installation. The proof includes the agent key and network ID. It is not sent with ordinary zome calls.

An expiry field exists in the proof format, but genesis validation cannot safely rely on an untrusted local wall clock. Operators must therefore issue short-lived installation credentials and enforce issuance/revocation policy outside the DNA until a trusted-time mechanism is designed.

## Resilience

- Agent-authored state is replicated by Holochain rather than a single central database.
- High-volume transfers bypass the DHT, limiting control-plane amplification and replication pressure.
- Direct QUIC can reconnect independently of DHT record propagation.
- Scheduler input can degrade to Holochain-only mode when no SD-WAN controller is configured.
- Conductor connection failures fail closed; static discovery is an explicit development choice rather than an automatic fallback.

Remaining resilience limits are documented in [Security](SECURITY.md): no complete congestion/backpressure proof, no production chaos suite, and several external integrations are adapters or mocks.

## Configuration authority

The hApp manifest owns DNA network seeds and DNA properties. The conductor configuration owns bootstrap and relay endpoints. HybridNode YAML records the expected local deployment and is checked before startup, but it does not dynamically rewrite an already installed DNA or conductor network configuration.

## Version boundary

All Holochain-facing code targets the 0.7.0 compatibility family. A Holochain 0.6 conductor, client, database or packed DNA is not supported. See [Holochain 0.7 migration](HOLOCHAIN_0_7_MIGRATION.md).
