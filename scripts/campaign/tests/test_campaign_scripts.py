#!/usr/bin/env python3
"""Tests des scripts de campagne CH3/S4 (sans matériel).

Exécution : python -m unittest discover -s scripts/campaign/tests -v
(exécuté par le job CI « campaign-scripts »).
"""

from __future__ import annotations

import importlib.util
import json
import pathlib
import statistics
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
SCRIPTS = HERE.parent


def load_module(name: str, path: pathlib.Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    sys.modules[name] = mod
    spec.loader.exec_module(mod)
    return mod


run_campaign = load_module("run_campaign", SCRIPTS / "run-campaign.py")
aggregate_results = load_module("aggregate_results", SCRIPTS / "aggregate-results.py")


def synthetic_run(i: int) -> dict:
    """Rapport au format imprimé par `quic-spike client` (Report du spike S4)."""
    return {
        "mode": f"ch3/usb",
        "peer": "127.0.0.1:7700",
        "session_setup_ms": 10.0 + i,
        "rtt_samples": 200,
        "rtt_p50_ms": 0.9 + 0.1 * i,
        "rtt_p95_ms": 2.0 + 0.2 * i,
        "transfer_bytes": 104857600,
        "transfer_ms": 900.0 + 50.0 * i,
        "throughput_mbit_s": 900.0 - 50.0 * i,
    }


class TestRunCampaign(unittest.TestCase):
    def test_seuils_couvrent_les_liens_connus(self):
        for link in run_campaign.KNOWN_LINKS:
            self.assertIn(link, run_campaign.THRESHOLDS,
                          f"seuil manquant pour le lien {link}")

    def test_summarize_agrege_min_mediane_max(self):
        runs = [synthetic_run(i) for i in range(3)]
        with tempfile.TemporaryDirectory() as td:
            out = pathlib.Path(td)
            s = run_campaign.summarize("usb", runs, rtt_n=200, out_dir=out,
                                       hardware="pc-test + téléphone-test")
            self.assertEqual(s["runs"], 3)
            self.assertEqual(s["rtt_samples_per_run"], 200)
            self.assertEqual(s["session_setup_ms"]["min"], 10.0)
            self.assertEqual(s["session_setup_ms"]["max"], 12.0)
            self.assertAlmostEqual(s["session_setup_ms"]["median"],
                                   statistics.median([10.0, 11.0, 12.0]))
            # runs.json écrit et rechargeable
            reloaded = json.loads((out / "runs.json").read_text(encoding="utf-8"))
            self.assertEqual(reloaded["link"], "usb")
            self.assertEqual(len(reloaded["runs_detail"]), 3)

    def test_summarize_verdict(self):
        # setup ~10 ms < 1000 ms et RTT p50 ~1 ms < 3 ms : verdict positif.
        s = run_campaign.summarize("usb", [synthetic_run(0)], rtt_n=200,
                                   out_dir=pathlib.Path(tempfile.mkdtemp()),
                                   hardware="test")
        self.assertTrue(s["verdict"]["setup_ok"])
        self.assertTrue(s["verdict"]["rtt_p50_ok"])

    def test_summarize_verdict_echoue_au_dela_du_seuil(self):
        slow = synthetic_run(0)
        slow["rtt_p50_ms"] = 99.0  # > seuil usb (3 ms)
        s = run_campaign.summarize("usb", [slow], rtt_n=200,
                                   out_dir=pathlib.Path(tempfile.mkdtemp()),
                                   hardware="test")
        self.assertFalse(s["verdict"]["rtt_p50_ok"])

    def test_offer_endpoint_lit_le_json(self):
        with tempfile.TemporaryDirectory() as td:
            p = pathlib.Path(td) / "offer.json"
            p.write_text(json.dumps({"quic_endpoint": "192.168.1.42:7700"}), encoding="utf-8")
            self.assertEqual(run_campaign.offer_endpoint(p), "192.168.1.42:7700")
            self.assertEqual(run_campaign.offer_endpoint(pathlib.Path(td) / "absent.json"), "?")


class TestAggregateResults(unittest.TestCase):
    def test_markdown_table_format(self):
        runs = [synthetic_run(i) for i in range(2)]
        s = run_campaign.summarize("usb", runs, rtt_n=200,
                                   out_dir=pathlib.Path(tempfile.mkdtemp()),
                                   hardware="test")
        table = aggregate_results.markdown_table([s])
        lines = table.splitlines()
        self.assertEqual(len(lines), 3)  # en-tête + séparateur + 1 ligne usb
        self.assertIn("| usb |", lines[2])
        self.assertIn("§6.3", lines[0])

    def test_fmt_range(self):
        out = aggregate_results.fmt_range(
            {"min": 1.0, "median": 2.5, "max": 4.0}, 3)
        self.assertEqual(out, "2.500 (1.000–4.000)")


if __name__ == "__main__":
    unittest.main()
