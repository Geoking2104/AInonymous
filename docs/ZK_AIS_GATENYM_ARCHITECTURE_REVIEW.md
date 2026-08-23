# ZK-AIS / Gatenym Architecture Review

## Executive conclusion

The original proposal had a useful separation between Holochain coordination and a direct high-volume transport, but it overstated anonymity, treated membrane proofs as ordinary call metadata, left the Holochain-to-QUIC identity binding implicit, mixed incompatible Holochain generations and described several aspirational components as implemented.

The repository has now adopted the defensible core of that design on Holochain 0.7.0 and removed or corrected the unsafe assumptions.

## Findings and disposition

| Severity | Finding | Disposition |
| --- | --- | --- |
| Critical | QUIC identity was not cryptographically bound to Holochain provenance | Fixed: capabilities contain the transport key and negotiation checks the caller's authored value |
| High | Private admission could be compiled out | Fixed: DNA properties select admission mode; validation is always compiled |
| High | Membrane proof was injected into ordinary zome calls | Fixed: proof is supplied in provisioned role settings during install/genesis |
| High | Explicit conductor failure silently downgraded to static peers | Fixed: conductor backend fails closed |
| High | 0.6-era code/configuration was inconsistent and outdated | Fixed: conductor/client/HDK/HDI/manifests migrated to one 0.7 family |
| High | Proposal implied anonymity and zero knowledge without a protocol | Documentation corrected; capability is not claimed |
| Medium | Proofs could be replayed across private networks | Fixed: signed proof includes and validates `network_id` |
| Medium | Admin and application WebSocket boundaries were unclear | Config requires distinct ports and loopback administration |
| Medium | Runtime YAML advertised a network seed it did not apply | Fixed: network seed is owned by hApp/DNA manifests |
| Medium | Public anti-Sybil proof of work was presented as available | Explicitly marked unimplemented/development-only |
| High | Agent discovery links could be poisoned by another author | Fixed: integrity validation binds agent bases and target authors |
| Medium | SD-WAN/mock runtime maturity was overstated | Documented as optional/experimental with production validation gates |

## Residual risk

The resulting architecture is substantially more coherent, but it is not yet the “most secure” possible system in an absolute sense. Security depends on deployment isolation, private bootstrap/relay infrastructure, Lair/keyring integrity, runtime sandboxing and operational controls. Membership revocation, trusted proof lifetime, hostile-input fuzzing, real-conductor end-to-end CI and independent audit remain open gates.

## Recommendation

Use the current design for controlled private pilots only. Preserve the two-plane boundary, provenance-to-transport-key binding and fail-closed conductor behavior in all integrations. Do not expose public endpoints or claim anonymity/ZK properties until those features exist and are independently reviewed.

Current operational detail is maintained in [Architecture](ARCHITECTURE.md), [Security](SECURITY.md) and [Holochain 0.7 migration](HOLOCHAIN_0_7_MIGRATION.md).
