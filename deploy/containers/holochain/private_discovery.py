"""Render the conductor config for the Compose private-discovery mode."""

import ipaddress
import os
from pathlib import Path
import re
import sys

import yaml


# Holochain 0.7.0 NetworkConfig::default endpoints and the legacy testnet host.
PUBLIC_DISCOVERY_HOSTS = {
    "dev-test-bootstrap2.holochain.org",
    "use1-1.relay.n0.iroh-canary.iroh.link",
    "bootstrap.holo.host",
}


def endpoint(name):
    """Accept an explicit HTTPS origin with a DNS name, IPv4, or bracketed IPv6."""
    value = os.environ.get(name, "")
    error = ValueError(f"{name} must be an operator HTTPS origin (optionally with a port)")
    match = re.fullmatch(r"https://(\[[0-9a-fA-F:.]+\]|[A-Za-z0-9.-]+)(?::([0-9]{1,5}))?/?", value)
    if not match:
        raise error
    host, port = match.groups()
    if port is not None and not 1 <= int(port) <= 65535:
        raise error
    if host.startswith("["):
        try:
            ipaddress.IPv6Address(host[1:-1])
        except ValueError:
            raise error from None
    else:
        dns = host.removesuffix(".").lower()
        label = r"[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?"
        if len(dns) > 253 or not all(re.fullmatch(label, part) for part in dns.split(".")):
            raise error
        # URL parsers can interpret numeric hostnames as abbreviated IPv4.
        if re.fullmatch(r"[0-9]+|0x[0-9a-f]*", dns.rsplit(".", 1)[-1]):
            try:
                ipaddress.IPv4Address(dns)
            except ValueError:
                raise error from None
        if dns in PUBLIC_DISCOVERY_HOSTS:
            raise ValueError(f"{name} points to a public reference service")
    return value


def render(base, output):
    network = {
        "bootstrap_url": endpoint("AINON_BOOTSTRAP_URL"),
        "relay_url": endpoint("AINON_RELAY_URL"),
        "advanced": None,
    }
    config = yaml.safe_load(base.read_text(encoding="utf-8"))
    config["network"] = network
    yaml.safe_dump(config, output, sort_keys=False)


if __name__ == "__main__":
    try:
        render(Path(sys.argv[1]), sys.stdout)
    except ValueError as exc:
        sys.exit(str(exc))
