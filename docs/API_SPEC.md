# API Reference

## Stability

The APIs are experimental and may change before a tagged release. Local HTTP endpoints are intended for loopback use. Holochain zome calls are the authoritative control-plane interface; QUIC is the authenticated data plane.

## Local HTTP surface

The following routes support the local inference path. The [proxy router](../crates/ainonymous-proxy/src/router.rs) accepts client requests. The [daemon router](../crates/ainonymous-daemon/src/router.rs) serves the proxy. The quick-start proxy listens on loopback port 9337. Its daemon URL defaults to `http://127.0.0.1:8890` and can be set with `AINON_DAEMON_URL`. The t51 configuration uses daemon port 18890.

| Binary | Method | Route | Purpose |
| --- | --- | --- | --- |
| Both | GET | `/health` | Process liveness |
| Proxy | GET | `/v1/models` | Model metadata and observed node counts |
| Proxy | POST | `/v1/chat/completions` | Submit an OpenAI-compatible chat request |
| Proxy | GET | `/v1/ainonymous/mesh/nodes` | Read node summaries, optionally filtered by `model_id` |
| Daemon | GET | `/mesh/nodes` | Return the discovered nodes to the proxy |
| Daemon | POST | `/mesh/plan` | Resolve the plan for a model |
| Daemon | POST | `/mesh/infer` | Execute a supported plan and return the result |
| Daemon | POST | `/mesh/metrics` | Accept an `InferenceMetrics` record for publication through the inference-mesh zome |

Keep these interfaces local. The daemon also exposes management operations that require an isolated management boundary.

## Holochain roles

### `agent-registry`

`announce_capabilities` writes a capability entry authored by the caller. The payload includes compute backends, GPU/VRAM information, loaded model IDs, supported inference types, availability and the 32-byte QUIC public key encoded as 64 hexadecimal characters.

`get_node` and discovery functions return capability summaries. The authored transport key is used by inference-mesh negotiation to bind the direct session to Holochain provenance.

### `inference-mesh`

`negotiate_quic_session` accepts the requester's transport public key and session parameters. It derives the caller from `call_info().provenance`; the caller cannot select an arbitrary agent identity. Negotiation rejects a missing, malformed or non-matching announced key.

Control-plane signals may advertise session offers, but a receiver must still perform strict TLS key verification before accepting data.

### `blackboard`

Blackboard functions publish and query shared coordination entries. They are not suitable for prompts, raw activations, model weights or other large/private payloads.

### `hybridnode-core`

The reusable hApp publishes membership, capability and audit-related entries. Its integrity zome enforces author consistency and private-network genesis admission according to DNA properties.

## Error handling

Clients must treat these as terminal for the current operation:

- conductor or app WebSocket connection failure;
- invalid app token or missing provisioned cell;
- invalid membrane proof;
- transport key/provenance mismatch;
- TLS peer verification failure;
- malformed frame or transfer integrity failure.

Retry only idempotent discovery reads automatically. Installation, plan creation and inference submission require application-level idempotency keys before transparent retry is safe.

## Serialization

Zome payloads use Holochain's serialized-bytes representation. HTTP uses JSON. QUIC frames use the codec in `ainonymous-quic`; peers must enforce maximum frame and transfer sizes before allocating buffers.
