# SG1 — two-conductor admission test

`run_sg1.py` starts **two real Holochain 0.7 conductors** from scratch and checks
the HybridNode private-network admission model end to end:

| Assertion | Expected |
| --- | --- |
| valid proof, node A and node B | admitted (app installed and listed) |
| no membrane proof | refused |
| proof issued to another agent | refused |
| proof issued for another network id | refused |
| proof signed by another admin key | refused |
| fresh agent with a valid proof, after the refusals | admitted |
| each admitted conductor queries its agent store with `--local-network` | a remote record matches the other conductor's private DNA and advertised address on the test relay |

Everything is built from the repository sources: the `hybridnode` zomes, a
private DNA variant whose properties carry the admin key and network id, the app
bundle, and the `private-network-proof` tool that signs the proof the way the
integrity zome verifies it at genesis.

## Requirements

- `hc` and `holochain` 0.7.0 on `PATH` (or `HC_BIN` / `HOLOCHAIN_BIN`).
- For discovery, the combined `kitsune2-bootstrap-srv-iroh-relay` binary from the [same release](https://github.com/holochain/holochain/releases/tag/holochain-0.7.0) on `PATH`, or its path in `BOOTSTRAP_RELAY_BIN`. The [CI job](../../.github/workflows/ci.yml) pins the Linux x86-64 checksum.
- Rust 1.91 with the `wasm32-unknown-unknown` target.
- Python 3.10+.

## Run

```bash
python3 scripts/sg1/run_sg1.py                    # admission assertions only
python3 scripts/sg1/run_sg1.py --local-network    # same mode as CI
python3 scripts/sg1/run_sg1.py --local-network --keep
```

The discovery run starts the combined service on dynamically allocated loopback
ports. It configures both conductors with that bootstrap and relay URL before
starting either conductor. After enabling the two apps, the harness polls both
agent stores for up to 90 seconds. In `hc` 0.7 the remote record omits the agent
key value, so the test compares its DNA and relay address with the other
conductor's local record for the known admitted key. Both directions must pass.

Exit code 0 means all enabled assertions passed. Use `--keep` to inspect
`bootstrap-relay.log` and the `conductor.log` in each node directory. The
retained directory contains synthetic keystores and proof material.

## Scope and limits

This covers admission and peer discovery on one host. The generated
test configurations permit HTTP for the loopback relay. The enterprise
deployment still needs an operator-managed TLS endpoint on two physical hosts,
an application data-exchange test, and evidence of zero public fallback when
the private service is unavailable. Proof expiry and active revocation have
separate acceptance criteria in the [pilot specification](../../docs/ENTERPRISE_PILOT_SPEC.md).
