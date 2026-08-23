# Security and Threat Model

## Security posture

This repository is alpha software for controlled development and private pilots. It has not received an independent security audit. Do not use it for secrets, regulated data or adversarial public workloads without additional review, isolation and operational controls.

## Protected assets

- Holochain agent signing keys in Lair
- QUIC transport private keys in the OS keyring when enabled
- inference prompts, activations and generated tokens in transit
- capability, plan and audit integrity
- private-network membership credentials
- SD-WAN controller credentials supplied through environment variables

## Trust boundaries

The local operator trusts the machine, Lair process and conductor. Holochain peers are authenticated agents but are not assumed honest. QUIC peers are trusted only after their transport key is bound to the Holochain provenance. The SD-WAN controller is a topology oracle, not an authorization oracle.

## Enforced invariants

| Invariant | Enforcement |
| --- | --- |
| Holochain version is 0.7.0 | Rust config validation and JSON Schema |
| Conductor admin endpoint is local | loopback URL validation; reference conductor binds loopback |
| Admin and app ports differ | Rust and semantic configuration validation |
| Explicit conductor failure cannot downgrade discovery | conductor backend returns an error; static mode must be selected explicitly |
| QUIC peer verification is strict | configuration rejects `mtls_strict: false` |
| Holochain caller owns the announced transport key | provenance lookup and constant-format key comparison in session negotiation |
| Discovery indexes cannot be written on behalf of another agent | integrity validation binds agent links, targets and entry authors |
| SD-WAN TLS verification stays enabled | configuration rejects `tls_verify: false` |
| Private admission is not compile-time optional | genesis validation always reads DNA properties |
| A membrane proof is scoped to an agent and network | signed agent key plus `network_id` checked at genesis |
| Membrane proof is installed at genesis | admin API role settings; no zome-call injection |

## Known limitations

### Public network admission

The configuration includes a proof-of-work difficulty field, but the public-network anti-Sybil mechanism is not implemented. Public mode is development-only.

### Anonymity and zero knowledge

Pseudonymous agent keys do not provide network anonymity. Iroh relays, QUIC endpoints, timing and transfer sizes can reveal metadata. No mixnet, onion routing, differential privacy or zero-knowledge inference protocol is implemented.

### Membrane proof lifetime

Genesis cannot safely trust the joining machine's local wall clock. Proof expiry is therefore an issuance/installation control, not a complete on-DHT revocation mechanism. Rotate network IDs and rebuild/reinstall the DNA when hard membership revocation is required.

### Application authorization

The Holochain admin interface is powerful and must remain on loopback or an equivalently isolated management namespace. App authentication tokens should be short-lived and never logged. Local HTTP endpoints should remain loopback-only or be placed behind authenticated access control.

### Data-plane authorization

Key pinning authenticates the selected peer but does not by itself implement per-model authorization, quotas, billing, prompt confidentiality at rest, or malicious-model sandboxing. Apply policy before scheduling and isolate inference runtimes.

### External adapters

Mock SD-WAN and pipeline adapters are not production controls. Vendor controller integrations, revocation workflows and native distributed inference paths require integration tests against the actual deployment.

### Self-reported hardware

Node attestations and benchmark values are agent-authored claims, not TPM/TEE-backed measurements. Their Holochain authorship is validated, but the hardware facts are not independently proven. Do not use them as the sole authorization or billing signal.

## Deployment baseline

1. Use private-network DNA properties with a unique, high-entropy network ID.
2. Operate private bootstrap and relay infrastructure reachable only through the intended network.
3. Keep conductor admin and local HTTP interfaces on loopback.
4. Store Holochain keys in Lair and enable the native keyring feature for transport keys.
5. Run `validate_config.py --production` and reject any warning or placeholder.
6. Pin exact Holochain artifacts and verify checksums in the release pipeline.
7. Separate inference workers from the management plane with OS/container isolation.
8. Collect audit logs without prompts, tokens, auth tokens or private keys.
9. Exercise relay loss, conductor restart, peer churn and partial transfer failure before launch.

## Reporting

Do not publish live credentials or exploit details in a public issue. Contact the repository owner privately before coordinated disclosure.
