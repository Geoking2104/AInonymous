# PyTorch Integration with AInonymous HybridNode

## Overview

AInonymous exposes an **OpenAI-compatible REST API** on each HybridNode, making PyTorch integration straightforward. The `ainonymous-torch` package adds three higher-level capabilities:

1. **REST client** — chat, stream, embed (no PyTorch required, pure Python)
2. **Pipeline splitting** — cut an `nn.Module` at any layer, offload the tail to a remote HybridNode peer via QUIC/mTLS
3. **Model export** — convert HF checkpoints to GGUF + publish `ModelManifest` to the Holochain DHT

---

## Security properties carried through PyTorch integration

All properties of the HybridNode stack apply when using `ainonymous-torch`:

| Property | How it applies |
|---|---|
| **mTLS ed25519** | Every QUIC connection for activation transfer authenticates both peers with the Holochain `AgentPubKey` as TLS cert. No anonymous connections. |
| **Node Attestation** | `InferenceOptions(require_attestation=True)` restricts routing to peers with a valid `NodeAttestation` entry in the DHT. |
| **Warrant exclusion** | The scheduler auto-excludes any peer with an active `Warrant`. Clients see `node.has_warrant` in `list_nodes()`. |
| **SHA-256 model integrity** | `export_to_gguf()` computes and stores SHA-256. `verify_gguf()` must pass before a node serves the model. Cross-peer verification (≥ 2 peers confirm hash) is enforced by the `ModelClaim` coordinator zome. |
| **No pickle** | The activation wire format (`pack_activation` / `unpack_activation`) uses a hand-crafted binary header + raw `memoryview`. Pickle is never used — arbitrary code execution via tensor deserialization is impossible. |

---

## Installation

```bash
# Minimal (no transformers)
pip install ainonymous-torch

# Full (HF models + async)
pip install "ainonymous-torch[all]"

# From source
git clone https://github.com/Geoking2104/AInonymous
pip install -e "AInonymous/examples/pytorch[all]"
```

---

## REST client

```python
from ainonymous_torch import AInonymousClient, InferenceOptions

client = AInonymousClient(
    base_url="http://localhost:9337",  # Local HybridNode daemon
    model="gemma4-31b",
)

# Simple chat
reply = client.chat("What is pipeline splitting?")

# With routing constraints
opts = InferenceOptions(
    max_latency_ms=20,          # Intra-site only (< 20ms SLA)
    redundancy_mode="n_of_m_quorum",  # 2-of-3 consensus
    require_attestation=True,   # Only attested peers
)
reply = client.chat("Explain mTLS", options=opts)

# Streaming
for token in client.stream_chat("Describe Holochain warrants:"):
    print(token, end="", flush=True)

# Embeddings
vecs = client.embed(["sentence A", "sentence B"])
```

---

## Pipeline splitting

Split a Hugging Face model at layer `k`; layers `[k, N)` run on a remote HybridNode peer:

```python
import torch
from transformers import AutoModelForCausalLM
from ainonymous_torch import PipelineSplit, LayerRange

model = AutoModelForCausalLM.from_pretrained("google/gemma-2-9b-it")
split_at = 18  # out of 36 layers

pipe = PipelineSplit(
    local_layers=LayerRange(model.model.layers[:split_at], 0, split_at),
    remote_url="http://192.168.1.42:9337",      # Node B (HybridNode peer)
    remote_layers=LayerRange(start=split_at, end=36),
    transfer_dtype=torch.float16,               # 50% bandwidth saving vs float32
)

# Forward pass — local layers run here, rest via QUIC
hidden = model.model.embed_tokens(input_ids)
hidden = pipe(hidden)
logits = model.lm_head(model.model.norm(hidden))
```

### How it works under the hood

```
Node A                           Node B (HybridNode peer)
──────────────────────────       ────────────────────────
embed_tokens(input_ids)
layers[0..18] forward
pack_activation(hidden)          unpack_activation(payload)
  ── QUIC stream (mTLS) ──▶      layers[18..36] forward
                                 pack_activation(result)
unpack_activation(result) ◀──
model.norm + lm_head
```

The QUIC connection reuses the peer's ed25519 `AgentPubKey` as the TLS certificate (verified by `PeerKeyVerifier`). The activation payload uses the [safe binary wire format](#activation-wire-format).

---

## Activation wire format

```
Offset  Len  Field
──────  ───  ─────────────────────────────────────────
0       4    Magic: 0xA1A0C1C0 (big-endian uint32)
4       1    dtype_id (0=float32 1=float16 2=bfloat16 3=int8 4=int32 5=int64)
5       1    ndim (number of dimensions, 1–8)
6       ndim×8  shape (each dim as little-endian int64)
6+ndim×8  N   raw tensor bytes (contiguous, C-order)
```

- **No pickle** — safe to receive from untrusted peers
- **No compression** — QUIC handles compression internally; raw bytes maximize GPU DMA throughput
- **bfloat16 preferred** — same dynamic range as float32, 2× smaller, native on modern GPU/TPU

```python
from ainonymous_torch import pack_activation, unpack_activation
import torch

t = torch.randn(2, 512, 4096, dtype=torch.bfloat16)
wire = pack_activation(t)       # → bytes (8 MB)
restored = unpack_activation(wire)   # → same tensor
```

---

## Model export to GGUF

AInonymous uses **llama.cpp** for local inference. Export any HF checkpoint:

```python
from ainonymous_torch.model_export import export_to_gguf, verify_gguf

manifest = export_to_gguf(
    model_id="google/gemma-2-9b-it",
    output_dir="~/.ainonymous/models",
    quantization="Q4_K_M",        # ~4 bit, best quality/size ratio
    llama_cpp_dir="~/llama.cpp",
)

print(manifest.sha256)  # Publish to Holochain DHT
ok = verify_gguf(manifest.gguf_path, manifest.sha256)
assert ok, "Hash mismatch — model tampered!"
```

### Publish to Holochain DHT

```bash
# Start the HybridNode daemon
hybridnode --config ainonymous.hybridnode.yaml

# Export + publish in one command
python examples/04_export_to_gguf.py \
    --model google/gemma-2-9b-it \
    --quant Q4_K_M \
    --publish
```

The daemon calls `publish_manifest()` in the Holochain coordinator zome, creating a `ModelManifest` entry in the DHT. Remote peers can then call `claim_model()` and `verify_node_attestation()` to confirm integrity before routing inference requests.

---

## DistributedInferenceModule

Transparent `nn.Module` wrapper — tries the network, falls back to local:

```python
from ainonymous_torch import DistributedInferenceModule

class MyModel(nn.Module):
    def __init__(self):
        super().__init__()
        self.layers = nn.Sequential(...)

    def forward(self, x):
        return self.layers(x)

model = DistributedInferenceModule(
    MyModel(),
    ainonymous_url="http://localhost:9337",
    fallback_to_local=True,  # Graceful degradation
)

out = model(x)   # Runs remote if available, local otherwise
print(model.stats)  # {"remote_calls": 42, "local_fallbacks": 1, "errors": 0}
```

---

## Testing

```bash
pytest examples/pytorch/tests/ -v

# With coverage
pytest examples/pytorch/tests/ --cov=ainonymous_torch --cov-report=term-missing
```

Tests cover:
- Round-trip for all 6 supported dtypes
- Wire format magic bytes and size invariants
- Corruption detection (bad magic, truncated payload)
- Chunking identity (`ActivationTransfer.split` → `merge`)
- Bandwidth assertions (float16 is exactly 2× smaller than float32)

---

## Reference

| Class / Function | Module | Description |
|---|---|---|
| `AInonymousClient` | `client` | REST client: `chat()`, `stream_chat()`, `embed()`, `list_nodes()`, `health()` |
| `InferenceOptions` | `client` | Routing constraints: `max_latency_ms`, `redundancy_mode`, `require_attestation` |
| `NodeStatus` | `client` | Peer info: `site_id`, `reputation`, `vram_mb`, `held_models`, `has_warrant` |
| `pack_activation(t)` | `activation_transfer` | Tensor → bytes (safe binary) |
| `unpack_activation(b)` | `activation_transfer` | bytes → Tensor |
| `ActivationTransfer` | `activation_transfer` | Chunked transfer with stats |
| `PipelineSplit` | `pipeline` | Split `nn.Module` at layer boundary; tail via QUIC |
| `LayerRange` | `pipeline` | Layer slice descriptor |
| `MultiNodePipeline` | `pipeline` | N-way pipeline across multiple peers |
| `DistributedInferenceModule` | `distributed_inference` | Transparent wrapper with local fallback |
| `export_to_gguf()` | `model_export` | HF → GGUF → ModelManifest |
| `verify_gguf()` | `model_export` | SHA-256 integrity check |
| `ModelManifest` | `model_export` | Dataclass matching Holochain entry |

See also: [HYBRIDNODE.md](HYBRIDNODE.md) · [ARCHITECTURE.md](ARCHITECTURE.md) · [HOLOCHAIN_ZOMES.md](HOLOCHAIN_ZOMES.md)
