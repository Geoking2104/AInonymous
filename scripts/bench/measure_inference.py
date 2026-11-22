"""Measure blocking inference through one or more local daemon ports."""

import argparse
import hashlib
import http.client
import json
import math
import statistics
import sys
import time
from datetime import datetime, timezone
from pathlib import Path


PROMPT = "Reply with the English word for the number after six."
METRICS = ("client_latency_ms", "coordinator_ttft_ms", "end_to_end_output_tokens_per_s")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def measured_count(value):
    if value is None:
        return None
    if type(value) is not int or value < 0:
        raise ValueError("invalid_measurement")
    return value


def measure(port, mode, body, timeout):
    conn = http.client.HTTPConnection("127.0.0.1", port, timeout=timeout)
    start = time.perf_counter()
    try:
        conn.request("POST", "/mesh/infer", body, {"Content-Type": "application/json"})
        response = conn.getresponse()
        if response.status != 200:
            return {"error": f"http_{response.status}"}
        raw = response.read()
        elapsed = time.perf_counter() - start
    except (OSError, http.client.HTTPException) as exc:
        return {"error": type(exc).__name__}
    finally:
        conn.close()

    expected_mode = "pipeline_split" if mode == "pipeline_split" else "solo"
    try:
        result = json.loads(raw)
        if not isinstance(result, dict):
            return {"error": "invalid_response"}
        if result.get("execution_mode") != expected_mode:
            return {"error": "unexpected_execution_mode"}
        nodes = result["node_ids"]
        if not isinstance(nodes, list) or not all(isinstance(n, str) and n for n in nodes):
            return {"error": "invalid_nodes"}
        if expected_mode == "pipeline_split" and len(set(nodes)) < 2:
            return {"error": "expected_multiple_nodes"}
        if expected_mode == "solo" and len(nodes) != 1:
            return {"error": "expected_one_node"}
        usage = result["usage"]
        if not isinstance(usage, dict):
            return {"error": "invalid_usage"}
        prompt_tokens = measured_count(usage["prompt_tokens"])
        output_tokens = measured_count(usage["completion_tokens"])
        ttft = measured_count(result["coordinator_ttft_ms"])
        content = result["content"]
        if not isinstance(content, str):
            return {"error": "invalid_content"}
        if expected_mode == "pipeline_split" and (not prompt_tokens or output_tokens is None or ttft is None):
            return {"error": "missing_pipeline_measurement"}
    except (UnicodeError, KeyError, ValueError, TypeError):
        return {"error": "invalid_or_legacy_response"}

    return {
        "node_ids": nodes,
        "output_sha256": digest(content.encode()),
        "prompt_tokens": prompt_tokens,
        "completion_tokens": output_tokens,
        "client_latency_ms": elapsed * 1000,
        "coordinator_ttft_ms": ttft,
        "end_to_end_output_tokens_per_s": None if output_tokens is None else output_tokens / elapsed,
    }


def summarize(samples):
    summary = {}
    for key in METRICS:
        values = sorted(s[key] for s in samples if s.get(key) is not None)
        summary[key] = {
            "n": len(values),
            "p50": statistics.median(values) if values else None,
            "p95": values[math.ceil(0.95 * len(values)) - 1] if values else None,
        }
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, action="append", required=True, help="repeat for replicas")
    parser.add_argument("--mode", choices=("solo", "replicas", "pipeline_split"), required=True)
    parser.add_argument("--model", required=True)
    parser.add_argument("--environment", type=Path, required=True, help="operator-supplied JSON manifest")
    parser.add_argument("--runs", type=int, default=10)
    parser.add_argument("--max-tokens", type=int, default=16)
    parser.add_argument("--timeout", type=int, default=120, help="socket timeout in seconds")
    args = parser.parse_args()
    if args.runs < len(args.port) or args.max_tokens < 1 or args.timeout < 1:
        parser.error("provide at least one run per port and positive token and timeout limits")
    if len(set(args.port)) != len(args.port) or any(not 1 <= port <= 65535 for port in args.port):
        parser.error("ports must be distinct and in the range 1–65535")
    if (args.mode == "replicas") != (len(args.port) > 1):
        parser.error("replicas needs multiple ports; solo and pipeline_split need one")
    try:
        environment = json.loads(args.environment.read_text(encoding="utf-8"))
        required = ("revision", "hosts", "network", "model_digest", "tokenizer_digest")
        if not isinstance(environment, dict) or any(not environment.get(k) for k in required):
            raise ValueError("expected " + ", ".join(required))
    except (OSError, ValueError) as exc:
        parser.error(f"invalid environment manifest: {exc}")

    body = json.dumps({
        "model_id": args.model,
        "messages": [{"role": "user", "content": PROMPT}],
        "max_tokens": args.max_tokens,
    }).encode()
    started = datetime.now(timezone.utc).isoformat()
    samples = []
    for i in range(args.runs):
        port = args.port[i % len(args.port)]
        request_start = time.perf_counter()
        sample = measure(port, args.mode, body, args.timeout)
        if "error" in sample:
            sample["failed_after_ms"] = (time.perf_counter() - request_start) * 1000
        samples.append({"run": i + 1, "port": port, **sample})
    failures = sum("error" in s for s in samples)
    report = {
        "schema_version": 1, "started_utc": started, "mode": args.mode, "model": args.model,
        "operator_environment": environment, "request_sha256": digest(body),
        "max_tokens": args.max_tokens, "timeout_s": args.timeout, "concurrency": 1,
        "samples": samples, "failed_requests": failures, "summary": summarize(samples),
    }
    print(json.dumps(report, indent=2))
    return int(failures > 0)


if __name__ == "__main__":
    sys.exit(main())
