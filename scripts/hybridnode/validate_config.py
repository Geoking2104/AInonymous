#!/usr/bin/env python3
"""
validate_config.py — Validate a HybridNode YAML configuration file
against the hybridnode.schema.json JSON Schema.

Usage:
    python scripts/hybridnode/validate_config.py <config.yaml>
    python scripts/hybridnode/validate_config.py hybridnode/configs/ainonymous.hybridnode.yaml
"""

import argparse
import sys
import json
import pathlib
from urllib.parse import urlparse

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


def _semantic_errors(config: dict, production: bool) -> list[str]:
    """Cross-field checks JSON Schema cannot express clearly."""
    if not isinstance(config, dict):
        return ["configuration root must be a mapping"]
    errors: list[str] = []
    quic = config.get("quic", {})
    sdwan = config.get("sdwan", {})
    security = config.get("security", {})
    holochain = config.get("holochain", {})

    if holochain.get("version") != "0.7.0":
        errors.append("holochain.version must be exactly 0.7.0")
    if holochain.get("admin_port") == holochain.get("app_port"):
        errors.append("holochain.admin_port and holochain.app_port must be different")
    conductor_url = holochain.get("conductor_url", "")
    try:
        parsed_conductor_url = urlparse(conductor_url)
        if parsed_conductor_url.scheme != "ws" or parsed_conductor_url.hostname not in {
            "127.0.0.1",
            "localhost",
            "::1",
        }:
            errors.append(
                "holochain.conductor_url must be an unencrypted loopback WebSocket endpoint"
            )
    except (TypeError, ValueError):
        errors.append("holochain.conductor_url must be a valid loopback WebSocket URL")

    if quic.get("mtls_strict") is not True:
        errors.append("quic.mtls_strict must be true")
    if sdwan.get("tls_verify") is False:
        errors.append("sdwan.tls_verify must not be false")
    if security.get("private_network", False) != (
        holochain.get("bootstrap_mode", "public") == "private"
    ):
        errors.append(
            "security.private_network and holochain.bootstrap_mode must describe the same trust mode"
        )
    if config.get("mode") == "sdwan-only" and security.get("private_network", False):
        errors.append("sdwan-only mode cannot claim Holochain private-network admission")
    if holochain.get("bootstrap_mode") == "private" and not holochain.get("bootstrap_url"):
        errors.append("private networks require holochain.bootstrap_url")

    serialized = json.dumps(config)
    if "<" in serialized or ">" in serialized:
        errors.append("configuration still contains template placeholders")

    if production:
        for label, value in (
            ("holochain.bootstrap_url", holochain.get("bootstrap_url", "")),
            ("sdwan.api_url", sdwan.get("api_url", "")),
        ):
            hostname = urlparse(value).hostname or ""
            if value and hostname.endswith((".example", ".invalid")):
                errors.append(f"production profile must replace the reserved {label} example")
        if sdwan.get("provider") == "mock":
            errors.append("production profile cannot use sdwan.provider=mock")
        if config.get("observability", {}).get("prometheus_addr", "").startswith("0.0.0.0"):
            errors.append(
                "production metrics must bind to a private address or be protected by an authenticated proxy"
            )
        if security.get("private_network") is not True:
            errors.append(
                "the current public-network anti-Sybil controls are not production-ready; use private_network"
            )
    return errors


def validate(config_path: str, production: bool = False) -> bool:
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

    validator = jsonschema.Draft202012Validator(
        schema, format_checker=jsonschema.FormatChecker()
    )
    errors = sorted(validator.iter_errors(config), key=lambda e: list(e.path))

    semantic_errors = _semantic_errors(config, production)
    errors.extend(semantic_errors)

    if not errors:
        print(f"OK: {config_path} - valid")
        _check_security_warnings(config)
        return True

    print(f"ERROR: {config_path} - {len(errors)} error(s):")
    for error in errors:
        if isinstance(error, str):
            print(f"  [semantic] {error}")
        else:
            path = " -> ".join(str(p) for p in error.path) or "(root)"
            print(f"  [{path}] {error.message}")
    return False


def _check_security_warnings(config: dict) -> None:
    """Warn on known risky configurations."""
    quic = config.get("quic", {})
    if not quic.get("mtls_strict", True):
        print("  WARNING: quic.mtls_strict is false - mTLS verification disabled!")

    sdwan = config.get("sdwan", {})
    if not sdwan.get("tls_verify", True):
        print("  WARNING: sdwan.tls_verify is false - SD-WAN API TLS not verified!")

    security = config.get("security", {})
    if security.get("pow_difficulty", 0) == 0 and not security.get("private_network", False):
        print("  NOTE: public-network anti-Sybil admission is not implemented")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("configs", nargs="+")
    parser.add_argument(
        "--production",
        action="store_true",
        help="also enforce the current production security baseline",
    )
    args = parser.parse_args()
    results = [validate(path, production=args.production) for path in args.configs]
    sys.exit(0 if all(results) else 1)
