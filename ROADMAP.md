# Roadmap

## Enterprise pilot

The [enterprise specification](docs/ENTERPRISE_PILOT_SPEC.md) maps the proposed private inference and learning service to CH3, CH6 and CH7. Follow its delivery sequence for the first customer pilot.

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

The seven product workstreams that turn the current demonstrations into something
usable — Windows installer, Android SDK, real multi-node measurement, GPU/phone
measurements, stealth transport mode, real-time revocation, and the external
audit + pilot kit — are defined with measurable acceptance criteria, dependencies
and sequencing in [`docs/chantiers-v1.md`](docs/chantiers-v1.md) (French).

## Next security gates

1. [SG1](scripts/sg1/README.md) checks membership proofs and mutual discovery
   between two Holochain 0.7 conductors through a test-owned loopback bootstrap
   and relay. The deployment gate requires two physical hosts, operator-managed
   TLS, successful data exchange, and evidence that public fallback is blocked
   when the private service is unavailable.
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
