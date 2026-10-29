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

Everything is built from the repository sources: the `hybridnode` zomes, a
private DNA variant whose properties carry the admin key and network id, the app
bundle, and the `private-network-proof` tool that signs the proof the way the
integrity zome verifies it at genesis.

## Requirements

- `hc` and `holochain` 0.7.0 on `PATH` (or `HC_BIN` / `HOLOCHAIN_BIN`);
- Rust 1.91 with the `wasm32-unknown-unknown` target;
- Python 3.10+.

## Run

```bash
python3 scripts/sg1/run_sg1.py            # temporary work dir, removed afterwards
python3 scripts/sg1/run_sg1.py --keep     # keep artefacts and conductor logs
```

Exit code 0 means every assertion passed. The CI job `SG1 two-conductor
admission` runs exactly this on every push to `main` and on pull requests.

## Scope and limits

This gate covers admission: `genesis_self_check` accepts a signed proof for the
right agent and network, and rejects every other case. It does **not** yet run
private bootstrap/relay services, so the two conductors do not gossip to each
other here; that half of ROADMAP security gate 1 remains open, as does proof
expiry and revocation.
