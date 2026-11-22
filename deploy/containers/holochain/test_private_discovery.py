import base64
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import unittest

import yaml


HERE = Path(__file__).resolve().parent
BASE = HERE / "conductor.yaml"
BOOTSTRAP = "https://bootstrap.pilot.example:8443"
RELAY = "https://relay.pilot.example"


def render(bootstrap=BOOTSTRAP, relay=RELAY, *, settings=None, private_dir=None):
    env = {name: value for name, value in os.environ.items()
           if name not in ("AINON_BOOTSTRAP_URL", "AINON_RELAY_URL",
                           "AINON_BOOTSTRAP_AUTH_FILE", "AINON_RELAY_AUTH_FILE")}
    if bootstrap is not None:
        env["AINON_BOOTSTRAP_URL"] = bootstrap
    if relay is not None:
        env["AINON_RELAY_URL"] = relay
    env.update(settings or {})
    cmd = [sys.executable, HERE / "private_discovery.py", BASE]
    if private_dir is not None:
        cmd.extend(["--private-file", private_dir])
    return subprocess.run(cmd, env=env, capture_output=True, text=True, check=False)


class PrivateDiscoveryTests(unittest.TestCase):
    def test_file_credentials_are_independent_and_preserve_exact_bytes(self):
        material = {"bootstrap": b"bootstrap-fixture\x00\xfb\xff\n", "relay": b"\xfb\xff"}
        with tempfile.TemporaryDirectory() as tmp:
            for name, data in material.items():
                (Path(tmp) / name).write_bytes(data)
            for services in [(), ("bootstrap",), ("relay",), ("bootstrap", "relay")]:
                with self.subTest(services=services):
                    settings = {}
                    for name in services:
                        settings[f"AINON_{name.upper()}_AUTH_FILE"] = str(Path(tmp) / name)
                    result = render(settings=settings, private_dir=tmp)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    path = Path(result.stdout.strip())
                    self.assertEqual(path.parent, Path(tmp))
                    self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
                    network = yaml.safe_load(path.read_text())["network"]
                    for name, data in material.items():
                        field = f"base64_auth_material_{name}"
                        if name in services:
                            self.assertEqual(base64.b64decode(network[field], validate=True), data)
                            self.assertNotIn(network[field], result.stdout + result.stderr)
                        else:
                            self.assertNotIn(field, network)

    def test_invalid_secret_never_generates_partial_configuration(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp)
            bootstrap = folder / "bootstrap"
            bootstrap.write_bytes(b"synthetic-bootstrap-secret")
            empty = folder / "empty"
            empty.touch()
            for source in ["", str(folder), str(folder / "missing"), str(empty)]:
                with self.subTest(source=source):
                    settings = {"AINON_BOOTSTRAP_AUTH_FILE": str(bootstrap),
                                "AINON_RELAY_AUTH_FILE": source}
                    result = render(settings=settings, private_dir=tmp)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual(result.stdout, "")
                    self.assertIn("AINON_RELAY_AUTH_FILE", result.stderr)
                    self.assertNotIn("synthetic-bootstrap-secret", result.stderr)
                    self.assertFalse(list(folder.glob("ainonymous-conductor.*")))

    def test_unavailable_private_directory_never_falls_back_to_tmpdir(self):
        with tempfile.TemporaryDirectory() as tmp:
            secret = Path(tmp) / "bootstrap"
            secret.write_bytes(b"synthetic-bootstrap-secret")
            result = render(settings={"TMPDIR": tmp, "AINON_BOOTSTRAP_AUTH_FILE": str(secret)},
                            private_dir=Path(tmp) / "unavailable")
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(result.stdout, "")
            self.assertFalse(list(Path(tmp).glob("ainonymous-conductor.*")))

    def test_credentials_cannot_be_rendered_to_stdout(self):
        with tempfile.TemporaryDirectory() as tmp:
            secret = Path(tmp) / "relay"
            secret.write_bytes(b"relay-test-credential")
            result = render(settings={"AINON_RELAY_AUTH_FILE": str(secret)})
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(result.stdout, "")
            self.assertIn("--private-file", result.stderr)
            self.assertNotIn("relay-test-credential", result.stderr)

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
