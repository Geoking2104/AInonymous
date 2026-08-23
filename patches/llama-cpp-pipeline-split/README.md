# llama.cpp Pipeline-Split Prototype

This directory contains an experimental llama.cpp patch and a small native server used to test layer-range execution across multiple processes. It is a research prototype, not a maintained upstream integration.

## What was validated

- early exit after a configured layer range;
- injection of a hidden state into a non-first stage;
- a three-stage chain with a true middle stage;
- bit-exact output for the tiny deterministic test model used during development;
- reduced graph reservation for stages that stop before the final layer.

These checks do not prove numerical equivalence for every architecture, quantization, batch shape or modern llama.cpp revision. The patch targets a specific upstream snapshot and may require manual rebasing.

## Build

Prerequisites: Git, CMake and a C++17 compiler.

```bash
make build-native-pipeline-server
```

Optional variables:

```bash
make build-native-pipeline-server \
  WORKDIR=/opt/llama.cpp \
  OUT_BIN=/opt/bin/pipeline_server \
  JOBS=8
```

The helper clones the expected llama.cpp revision, applies `0001-pipeline-split-poc.patch` and builds the prototype. Review the script and pinned revision before use.

## Local protocol test

Use the tiny deterministic GGUF generator and the C++ test binaries in this directory. The native server transfers hidden states as float32; the Python prototype may use float16. Do not mix the two encodings without an explicit negotiated wire format.

The repository's two-node runner can select the native backend:

```bash
BIN="$PWD/target/debug" \
BACKEND=native \
TOTAL_LAYERS=2 \
NATIVE_BIN=/path/to/pipeline_server \
NATIVE_MODEL_GGUF=/path/to/model.gguf \
bash scripts/testnet/run_testnet_2.sh
```

## Limitations

- not upstreamed and not continuously rebased against llama.cpp;
- no stable cross-version wire protocol;
- no heterogeneous dtype negotiation;
- no production backpressure, retry or resumable transfer contract;
- models with architecture-specific recurrent/state behavior need separate validation;
- chains longer than the tested three-stage topology are not covered by the current evidence.

Treat this directory as a reproducible experiment. The supported AInonymous control-plane security model is documented separately in [Network and data plane](../../docs/NETWORK.md).
