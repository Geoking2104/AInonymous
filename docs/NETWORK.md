# Network and Data Plane

## Separation of concerns

Holochain 0.7/Iroh carries control-plane records and signals. A separate QUIC/TLS 1.3 connection carries model and inference data. SD-WAN, when configured, contributes reachability and SLA information but does not replace either cryptographic identity layer.

## Session flow

1. A node creates or loads a 32-byte Ed25519 QUIC key.
2. It publishes the hexadecimal public key in an agent-authored capability entry.
3. A requester discovers a candidate and asks the inference-mesh zome to negotiate a session.
4. The zome confirms that the supplied key belongs to the caller's latest authored capabilities.
5. The peer receives an endpoint/session offer over the authenticated control plane.
6. The QUIC client connects and verifies that the TLS certificate contains the expected Ed25519 key.
7. Framed data is transferred directly. Invalid identity, framing or integrity closes the session.

```text
Holochain agent A                  Holochain agent B
      | capabilities(key A)              |
      |---- authenticated zome call ---->|
      |<--- endpoint + expected key B ----|
      |                                   |
      +======== QUIC/TLS 1.3 =============+
             key A <-> pinned key B
```

## Addressing and NAT

Holochain's own Iroh relay configuration is controlled by the conductor. The direct data-plane endpoint is negotiated separately. A relay fallback flag exists for the application path, but operators must test the actual relay/NAT behavior for their deployment; Holochain discovery success does not prove that large direct transfers will succeed.

## SD-WAN input

The scheduler can consider site locality, latency, bandwidth and link health. The default strategy prefers local candidates, then remote candidates that meet configured SLA thresholds. Controller data is advisory and can be stale or malicious; identity and authorization checks remain mandatory.

## Data protection

- TLS 1.3 protects data in transit and pins the advertised peer key.
- The protocol does not provide traffic-flow confidentiality.
- Endpoints must apply frame/transfer size limits and backpressure.
- Sensitive payloads should not be logged or published to the DHT.
- At-rest handling is delegated to the inference runtime and host controls.

## Failure behavior

Identity or TLS mismatch fails closed. Connection loss aborts the current transfer unless the application protocol explicitly supports resumable chunks. Scheduler retry must choose a newly authenticated peer and must not replay non-idempotent generation without a request identifier.

## Operational checks

- Verify the capability key matches the peer certificate.
- Test direct, relay and blocked-path cases.
- Simulate conductor restart while a data-plane transfer is active.
- Verify bounded memory under oversized frames and slow consumers.
- Confirm DSCP markings are permitted and preserved by the network; they are optimization hints, not security controls.
