# Two-Node Testnet

## Purpose

The two-node testnet verifies local orchestration and the experimental pipeline-split path. It is not a production benchmark and does not establish Internet-scale resilience.

## Prerequisites

- native Rust workspace built successfully;
- Holochain 0.7.0 when exercising real conductor discovery;
- Bash and Python 3;
- a model/runtime compatible with the selected test backend.

## Mock pipeline test

```bash
cargo build --workspace
BIN="$PWD/target/debug" \
  TOTAL_LAYERS=18 \
  bash scripts/testnet/run_testnet_2_mock.sh
```

The mock path validates control flow and framing only. It does not validate numerical equivalence of a real distributed model.

## Runtime-backed test

```bash
BIN="$PWD/target/debug" \
  TOTAL_LAYERS=18 \
  MODEL=google/gemma-3-1b-it \
  SPLIT=9 \
  bash scripts/testnet/run_testnet_2.sh
```

Review the script parameters before running; model download, GPU memory and backend behavior vary by environment.

## Holochain requirements

Use a fresh 0.7 data root and the 0.7 conductor configuration shape. Do not reuse 0.6 databases or bundles. Both peers must install the same newly packed hApps and use the same network seeds and relay/bootstrap settings.

## Acceptance checks

- both processes remain healthy;
- node capabilities include a valid 64-character QUIC public key;
- negotiation provenance matches the announced key;
- a wrong peer key is rejected;
- transfer output matches the selected mock or runtime expectation;
- stopping one peer fails the request cleanly without unbounded retry;
- logs contain no private key, membrane proof, app token or prompt body.
