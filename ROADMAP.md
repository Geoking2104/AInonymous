# Roadmap

## Completed in the Holochain 0.7 migration

- Pin one Holochain 0.7 compatibility family across native clients and zomes.
- Migrate integrity callbacks, link queries, app installation and manifests.
- Make conductor discovery fail closed.
- Install membrane proofs through role settings at genesis.
- Enforce private admission from DNA properties on every build.
- Bind QUIC public keys to Holochain call provenance.
- Validate loopback administration, strict mTLS and SD-WAN TLS settings.
- Build both zome workspaces in CI.
- Replace obsolete 0.6 and speculative documentation.

## Workstreams (v1)

The six product workstreams that turn the current demonstrations into something
usable — Windows installer, Android SDK, real multi-node measurement, GPU/phone
measurements, stealth transport mode, real-time revocation, and the external
audit + pilot kit — are defined with measurable acceptance criteria, dependencies
and sequencing in [`docs/chantiers-v1.md`](docs/chantiers-v1.md) (French).

## Next security gates

1. End-to-end tests with two real 0.7 conductors and membrane-proof installation: **done**
   (`scripts/sg1/run_sg1.py`, CI job `SG1 two-conductor admission` — admission
   accepted for a valid proof and refused without one, for another agent, another
   network id or another admin key). Still open for this gate: private
   bootstrap/relay services, so the two conductors gossip through operator-
   controlled infrastructure instead of the public defaults.
2. Add negative tests for cross-network proof replay, wrong transport keys and stale/replaced capability entries.
3. Define a trusted membership revocation design rather than relying on genesis-time expiry.
4. Add bounded transfer/backpressure fuzzing and adversarial frame tests.
5. Add authenticated authorization and rate limits for any non-loopback HTTP deployment.
6. Commission an independent review before a public pilot.

## Product/runtime gates

1. Validate native multi-node inference for supported models with numerical equivalence tests.
2. Implement and test production SD-WAN controller adapters.
3. Add resumable/idempotent job semantics and scheduler recovery.
4. Define resource accounting, quotas and model trust/sandbox policy.
5. Publish reproducible release artifacts and an operator upgrade/rollback runbook.

Claims of anonymity or zero-knowledge inference remain out of scope until a concrete protocol, threat model and implementation are reviewed.
