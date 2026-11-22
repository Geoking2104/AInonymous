# Measuring inference on the private mesh

Run this Python 3 benchmark against an already running daemon built with the inference measurement fields. It sends the same synthetic chat request on every run. The JSON report contains per-request observations, output text digests and summaries. It contains no prompts or response text. The environment object is copied into the report, so use shareable hardware labels.

## Prepare the environment record

Create an `environment.json` for each topology. Record the actual daemon revision, physical host and accelerator inventory (model, memory, OS and runtime build), network type, model digest and tokenizer digest. Record the quantization, chat template and sampling settings when comparing outputs. These are operator-supplied facts. The runner checks the execution mode and node identities returned by the daemon; proving physical placement requires checking the inventory on each host.

Example manifest shape:

```json
{
  "revision": "<daemon commit>",
  "hosts": [{"id": "A", "cpu": "<model>", "gpu": "<model/VRAM>", "ram": "<size>", "os": "<version>"}],
  "network": "<loopback, LAN or approved inter-site link>",
  "model_digest": "<weights SHA-256>",
  "tokenizer_digest": "<tokenizer SHA-256>",
  "runtime_settings": "<engine build, quantization, template and sampling settings>"
}
```

## Collect comparable runs

Use loopback daemon ports directly or forward them over the approved private connection. For replicas, point the two forwarded ports at independent whole-model deployments. For pipeline mode, use the coordinator port. Keep the manifest, prompt, model, token budget and number of samples comparable. Record cold-start runs separately if the model load state differs.

```bash
python3 scripts/bench/measure_inference.py --model MODEL --mode solo \
  --port 18890 --environment single.json --runs 20 > single-runs.json
python3 scripts/bench/measure_inference.py --model MODEL --mode replicas \
  --port 18890 --port 28890 --environment replicas.json --runs 20 > replica-runs.json
python3 scripts/bench/measure_inference.py --model MODEL --mode pipeline_split \
  --port 18890 --environment split.json --runs 20 > split-runs.json
```

| Observation | Interpretation |
| --- | --- |
| Client latency | Wall time from the start of the local HTTP request through reception of the complete response. Includes tunnel time where applicable. |
| Coordinator TTFT | For split inference, elapsed milliseconds from just before model tokenization to receipt of the first generated token at the coordinator. Excludes plan selection and client delivery. The blocking solo backend returns null. |
| End-to-end output tokens/s | Measured output tokens divided by client wall time, including tokenization and transport. The daemon's legacy word-count fallback is excluded. A backend that omits usage yields null. |
| Replicas | Sequential round-robin requests. These samples measure latency per request at concurrency one. A saturated throughput comparison requires a separate concurrent load test. |
| Distribution | Each metric reports its own sample count, median and nearest-rank p95. HTTP failures, missing measurement contracts and topology mismatches remain in the raw report and cause exit status 1. |
| Output digest | Allows exact text equality checks across runs. Differing sampling settings can produce differing hashes. Numerical equivalence requires the separate logits comparison. |

Use the [two-node testnet](../../docs/TESTNET_2NODES.md) for local validation. A hardware qualification report also needs host placement evidence, peak memory, network conditions and a timed worker-loss drill. Measurements from a mock server exercise the protocol only.

Run the bench regression tests with `python3 -m unittest discover -s scripts/bench -v`.
