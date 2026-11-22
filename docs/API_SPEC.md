# API Reference

## Stability

The APIs are experimental and may change before a tagged release. Local HTTP endpoints are intended for loopback use. Holochain zome calls are the authoritative control-plane interface; QUIC is the authenticated data plane.

## Local HTTP surface

The proxy and daemons expose a subset of these routes depending on the selected binary and features:

| Method | Route | Purpose |
| --- | --- | --- |
| `GET` | `/health` | process health |
| `GET` | `/metrics` | Prometheus metrics when enabled |
| `GET` | `/v1/models` | locally visible model metadata |
| `POST` | `/v1/chat/completions` | OpenAI-compatible local inference request |
| `GET` | `/mesh/nodes` | discovered node summary |
| `POST` | `/mesh/plan` | request or calculate an inference plan |

Callers must inspect the actual router in the selected binary; the repository does not claim one consolidated, remotely hardened public API. Do not expose these endpoints directly to the Internet.

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

### Streamed chat completions

For `POST /v1/chat/completions` with `stream: true`, the proxy sends server-sent events. Retain the request ID in the first chat chunk. A completed generation has a final choice with a finish reason, followed by the `[DONE]` marker. The solo path preserves the model server's finish reason when supplied.

If inference fails after streaming starts, the final data event has the following shape and the connection closes immediately:

```json
{"id":"chatcmpl-ainon-example","error":{"type":"upstream_error","code":"STREAM_INTERRUPTED","message":"Model stream ended before completion"}}
```

An error event or an unexpected disconnect leaves any text already delivered incomplete. Clients should inspect the terminal event even when the initial HTTP status was 200. Pipeline inference still produces its text events once the coordinator returns the complete result.

## Serialization

Zome payloads use Holochain's serialized-bytes representation. HTTP uses JSON. QUIC frames use the codec in `ainonymous-quic`; peers must enforce maximum frame and transfer sizes before allocating buffers.
