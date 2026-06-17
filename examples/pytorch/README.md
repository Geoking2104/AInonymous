# `ainonymous-torch` — PyTorch × AInonymous HybridNode

Python package for integrating PyTorch models with the **AInonymous HybridNode** distributed inference network.

## What this package does

| Capability | Module |
|---|---|
| Chat / stream / embed via REST | `ainonymous_torch.client` |
| Binary wire format for tensor transfer over QUIC | `ainonymous_torch.activation_transfer` |
| Model pipeline splitting across nodes | `ainonymous_torch.pipeline` |
| Transparent `nn.Module` wrapper (local ↔ remote fallback) | `ainonymous_torch.distributed_inference` |
| PyTorch → GGUF + SHA-256 manifest for attestation | `ainonymous_torch.model_export` |

## Install

```bash
pip install ainonymous-torch
# or from source:
pip install -e "examples/pytorch[all]"
```

## Quick start

```python
from ainonymous_torch import AInonymousClient

client = AInonymousClient("http://localhost:9337", model="gemma4-31b")
print(client.chat("Hello from AInonymous!"))
```

## Examples

| File | What it shows |
|---|---|
| `examples/01_basic_chat.py` | REST chat, streaming, embeddings, routing options |
| `examples/02_activation_transfer.py` | Binary tensor serialization, chunking, pipeline sim |
| `examples/03_pipeline_split.py` | Split a Gemma model between two HybridNodes |
| `examples/04_export_to_gguf.py` | HF → GGUF → SHA-256 → Holochain ModelManifest |

## Architecture

```
Your code
  └── PipelineSplit / DistributedInferenceModule (nn.Module)
        ├── Local layers  →  GPU/CPU
        └── Remote layers →  QUIC/mTLS →  HybridNode peer
                                 │
                    Holochain DHT (attestation + scheduling)
                                 │
                            SD-WAN underlay
```

### Activation wire format

Safe binary protocol — no pickle, no arbitrary code execution:

```
[4 bytes magic 0xA1A0C1C0] [1 byte dtype_id] [1 byte ndim]
[ndim × 8 bytes shape]     [raw tensor bytes]
```

Supports: `float32`, `float16`, `bfloat16`, `int8`, `int32`, `int64`.

## Run tests

```bash
pytest examples/pytorch/tests/ -v
```

## Requirements

- Python ≥ 3.10
- PyTorch ≥ 2.3.0
- AInonymous HybridNode daemon running (`hybridnode --config ainonymous.hybridnode.yaml`)

For GGUF export: clone [llama.cpp](https://github.com/ggerganov/llama.cpp) and set `LLAMA_CPP_DIR`.
