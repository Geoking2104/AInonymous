import os
from pathlib import Path
import subprocess
import sys
import unittest

import yaml


HERE = Path(__file__).resolve().parent
BASE = HERE / "conductor.yaml"
BOOTSTRAP = "https://bootstrap.pilot.example:8443"
RELAY = "https://relay.pilot.example"


def render(bootstrap=BOOTSTRAP, relay=RELAY):
    env = {name: value for name, value in os.environ.items()
           if name not in ("AINON_BOOTSTRAP_URL", "AINON_RELAY_URL")}
    if bootstrap is not None:
        env["AINON_BOOTSTRAP_URL"] = bootstrap
    if relay is not None:
        env["AINON_RELAY_URL"] = relay
    return subprocess.run([sys.executable, HERE / "private_discovery.py", BASE],
                          env=env, capture_output=True, text=True, check=False)


class PrivateDiscoveryTests(unittest.TestCase):
    def test_preserves_base_settings_and_selects_both_endpoints(self):
        base = yaml.safe_load(BASE.read_text())
        for bootstrap, relay in [(BOOTSTRAP, RELAY), (RELAY, RELAY),
                                 ("https://[fd00::1]:8443/", "https://192.0.2.3")]:
            with self.subTest(bootstrap=bootstrap, relay=relay):
                result = render(bootstrap, relay)
                self.assertEqual(result.returncode, 0, result.stderr)
                config = yaml.safe_load(result.stdout)
                self.assertEqual(config.pop("network"), {
                    "bootstrap_url": bootstrap, "relay_url": relay, "advanced": None,
                })
                self.assertEqual(config, base)

    def test_missing_or_empty_endpoint_never_emits_a_config(self):
        for value in (None, ""):
            for endpoints in [(value, RELAY), (BOOTSTRAP, value)]:
                with self.subTest(endpoints=endpoints):
                    result = render(*endpoints)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual(result.stdout, "")

    def test_rejects_invalid_origins_and_public_reference_endpoints(self):
        invalid = [
            "http://relay.pilot.example", "https://", "https://host:65536",
            "https://host:0", "https://a..b", "https://[bad:ipv6]", "https://0x7f000001",
            "https://256.0.0.1", "https://127.1",
            "https://name:password@host", "https://host?token=secret",
            "https://host/#fragment", "https://host/non-root", "https://host\nrelay_url: public",
            "https://DEV-TEST-BOOTSTRAP2.HOLOCHAIN.ORG.:443/",
            "https://use1-1.relay.n0.iroh-canary.iroh.link./", "https://bootstrap.holo.host",
        ]
        for candidate in invalid:
            for endpoints in [(candidate, RELAY), (BOOTSTRAP, candidate)]:
                with self.subTest(endpoints=endpoints):
                    result = render(*endpoints)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual(result.stdout, "")
                    self.assertNotIn(candidate, result.stderr)
