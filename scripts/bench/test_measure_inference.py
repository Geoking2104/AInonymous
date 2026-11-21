import json
import subprocess
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

from measure_inference import measure, summarize


class BenchTests(unittest.TestCase):
    def setUp(self):
        self.result = dict(content="private fixture answer", execution_mode="pipeline_split",
                           node_ids=["A", "B"], usage=dict(prompt_tokens=10, completion_tokens=4),
                           token_count=99, coordinator_ttft_ms=0)
        self.status = 200
        case = self

        class Handler(BaseHTTPRequestHandler):
            def do_POST(self):
                case.path = self.path
                self.rfile.read(int(self.headers["Content-Length"]))
                self.send_response(case.status)
                self.end_headers()
                self.wfile.write(json.dumps(case.result).encode())

            def log_message(self, *args):
                pass

        self.server = HTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        thread.start()
        self.addCleanup(self.server.server_close)
        self.addCleanup(self.server.shutdown)
        self.port = self.server.server_port

    def test_pipeline_cli(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = Path(tmp) / "env.json"
            manifest.write_text(json.dumps(dict(revision="test", hosts=["A", "B"], network="fixture",
                                                model_digest="fixture", tokenizer_digest="fixture")))
            run = subprocess.run([sys.executable, str(Path(__file__).with_name("measure_inference.py")),
                                  "--port", str(self.port), "--mode", "pipeline_split", "--model", "fixture",
                                  "--runs", "2", "--environment", str(manifest)], capture_output=True, check=True)
        report = json.loads(run.stdout)
        self.assertEqual(self.path, "/mesh/infer")
        self.assertNotIn(b"private fixture answer", run.stdout)
        self.assertEqual(report["samples"][1]["completion_tokens"], 4)
        self.assertEqual(report["summary"]["coordinator_ttft_ms"], dict(n=2, p50=0, p95=0))

    def test_unknown_solo_usage_never_uses_legacy_estimate(self):
        self.result.update(execution_mode="solo", node_ids=["A"], coordinator_ttft_ms=None,
                           usage=dict(prompt_tokens=None, completion_tokens=None))
        sample = measure(self.port, "solo", b"{}", 5)
        self.assertIsNone(sample["end_to_end_output_tokens_per_s"])
        self.assertEqual(summarize([sample])["coordinator_ttft_ms"]["n"], 0)

    def test_rejects_wrong_topology_old_daemon_and_http_failure(self):
        self.assertIn("error", measure(self.port, "solo", b"{}", 5))
        del self.result["usage"]
        self.assertIn("error", measure(self.port, "pipeline_split", b"{}", 5))
        self.status = 503
        self.assertEqual(measure(self.port, "pipeline_split", b"{}", 5), {"error": "http_503"})

    def test_cli_writes_failed_observations_and_exits_nonzero(self):
        self.status = 503
        with tempfile.TemporaryDirectory() as tmp:
            manifest = Path(tmp) / "env.json"
            manifest.write_text(json.dumps(dict(revision="test", hosts=["A"], network="fixture",
                                                model_digest="fixture", tokenizer_digest="fixture")))
            run = subprocess.run([sys.executable, str(Path(__file__).with_name("measure_inference.py")),
                                  "--port", str(self.port), "--mode", "solo", "--model", "fixture",
                                  "--runs", "1", "--environment", str(manifest)], capture_output=True)
        self.assertEqual(run.returncode, 1)
        report = json.loads(run.stdout)
        self.assertEqual(report["failed_requests"], 1)
        self.assertGreaterEqual(report["samples"][0]["failed_after_ms"], 0)
        self.assertEqual(report["summary"]["client_latency_ms"]["n"], 0)

    def test_summary_tracks_only_measured_values(self):
        samples = [{"client_latency_ms": i} for i in range(1, 21)] + [{"error": "timeout"}]
        self.assertEqual(summarize(samples)["client_latency_ms"], dict(n=20, p50=10.5, p95=19))
