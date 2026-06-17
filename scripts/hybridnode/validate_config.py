#!/usr/bin/env python3
"""
validate_config.py — Validate a HybridNode YAML configuration file
against the hybridnode.schema.json JSON Schema.

Usage:
    python scripts/hybridnode/validate_config.py <config.yaml>
    python scripts/hybridnode/validate_config.py hybridnode/configs/ainonymous.hybridnode.yaml
"""

import sys
import json
import pathlib

try:
    import yaml
except ImportError:
    print("ERROR: PyYAML not installed. Run: pip install pyyaml jsonschema")
    sys.exit(1)

try:
    import jsonschema
except ImportError:
    print("ERROR: jsonschema not installed. Run: pip install pyyaml jsonschema")
    sys.exit(1)


SCHEMA_PATH = pathlib.Path(__file__).parent.parent.parent / "hybridnode" / "schemas" / "hybridnode.schema.json"


def validate(config_path: str) -> bool:
    config_file = pathlib.Path(config_path)
    if not config_file.exists():
        print(f"ERROR: Config file not found: {config_path}")
        return False

    if not SCHEMA_PATH.exists():
        print(f"ERROR: Schema not found at {SCHEMA_PATH}")
        return False

    with open(config_file) as f:
        config = yaml.safe_load(f)

    with open(SCHEMA_PATH) as f:
        schema = json.load(f)

    validator = jsonschema.Draft202012Validator(schema)
    errors = sorted(validator.iter_errors(config), key=lambda e: list(e.path))

    if not errors:
        print(f"✓ {config_path} — valid")
        _check_security_warnings(config)
        return True

    print(f"✗ {config_path} — {len(errors)} error(s):")
    for error in errors:
        path = " → ".join(str(p) for p in error.path) or "(root)"
        print(f"  [{path}] {error.message}")
    return False


def _check_security_warnings(config: dict) -> None:
    """Warn on known risky configurations."""
    quic = config.get("quic", {})
    if not quic.get("mtls_strict", True):
        print("  ⚠ WARNING: quic.mtls_strict is false — mTLS verification disabled!")

    sdwan = config.get("sdwan", {})
    if not sdwan.get("tls_verify", True):
        print("  ⚠ WARNING: sdwan.tls_verify is false — SD-WAN API TLS not verified!")

    security = config.get("security", {})
    if security.get("pow_difficulty", 0) == 0 and not security.get("private_network", False):
        print("  ⚠ NOTE: pow_difficulty=0 on public network — no anti-Sybil PoW at admission")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <config.yaml> [<config2.yaml> ...]")
        sys.exit(1)

    results = [validate(path) for path in sys.argv[1:]]
    sys.exit(0 if all(results) else 1)
