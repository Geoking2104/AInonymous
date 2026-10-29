# DNA Hash Registry

DNA epoch `v3-20260823` is the canonical Holochain 0.7.0 deployment epoch for this repository. These values were produced on the Linux GitHub Actions runner by `hc` 0.7.0 and independently decoded by the repository's `dna-hashes` utility using `holochain_types` 0.7.0.

| DNA | Network seed | Effective hash |
| --- | --- | --- |
| inference-mesh | `ainonymous-core-hc07-v3-20260823` | `uhC0kYFyQtLQaaKs--WrpAQdNh44E0SXDX8Kk409Tan7Us4QLU5Q_` |
| agent-registry | `ainonymous-core-hc07-v3-20260823` | `uhC0kF5tmjLHIMwemHJ7efEd-HlZxl2P77_LJoQM_PYmeYkGft9Ap` |
| blackboard | `ainonymous-core-hc07-v3-20260823` | `uhC0k7wOBmtXg_cY2UwORA_Q5yG37Wmc551T582wgG1nlFWezcZuB` |
| hybridnode-core | `ainonymous-hybridnode-hc07-v3-20260823` | `uhC0k-sq0wS1opMFIrNzZKb2r94Uynr84xdUWg0Y9ix3WJc9zDLNw` |

The machine-readable source is [`deploy/holochain/dna-hashes.json`](../deploy/holochain/dna-hashes.json). Each hApp role also pins its `installed_hash`, so installation fails instead of silently joining a network produced by different WASM, properties or modifiers.

## Verification

After packaging with `hc` 0.7.0, run:

```bash
cargo run --locked -p dna-hashes -- --check deploy/holochain/dna-hashes.json \
  dnas/ainonymous-core/dnas/inference-mesh/workdir/inference-mesh.dna \
  dnas/ainonymous-core/dnas/agent-registry/workdir/agent-registry.dna \
  dnas/ainonymous-core/dnas/blackboard/workdir/blackboard.dna \
  dnas/hybridnode/dnas/hybridnode-core/workdir/hybridnode-core.dna
```

## Reproducibility boundary (measured 2026-10-29)

The pinned values are reproducible **only** in the reference build environment
(ubuntu-latest x86_64, Rust 1.91, `hc` 0.7.0). Rebuilding the same sources on
another host or with another `rustc` produces different WASM bytes, and therefore
a different DNA hash:

| Build environment | `inference-mesh` DNA hash | Matches the epoch |
| --- | --- | --- |
| Reference (CI, ubuntu x86_64, Rust 1.91) | `uhC0kYFyQtLQaaKs--WrpAQdNh44E0SXDX8Kk409Tan7Us4QLU5Q_` | yes (pinned) |
| Windows 11, Rust 1.91, `hc` 0.7.0 (msvc) | `uhC0kzs5P3jR0D2MKCjueITX4GDLgUd7v6yCmqUb_PVnEc09nrEKF` | no |
| Windows 11, Rust 1.95, `hc` 0.7.0 (msvc) | `uhC0kPZgq4_efY2J6_ZwSZU3AOuuvso7jhowLelod0ClwX7OcKpPK` | no |

Consequences:

- peers join the network with the bundles produced by CI and the release
  pipeline, **not** with a locally rebuilt hApp: `hc install-app` refuses a DNA
  whose hash differs from the role's `installed_hash`, so a local rebuild
  effectively defines a new epoch;
- `scripts/build-happ.sh` remains the development path; on a non-reference host,
  expect the `dna-hashes --check` step to report a mismatch instead of treating
  it as a regression;
- `hc app pack` does **not** validate `installed_hash` against the packed DNA —
  the `dna-hashes --check` invocation above is the actual gate.

### Reproducing the epoch

Two independent builds on the same machine are bit-identical, so the compiler is
deterministic; what changes the bytes is the *environment*. rustc embeds the
absolute source paths of registry dependencies (for example
`/home/runner/.cargo/registry/src/index.crates.io-.../serde-1.0.x/src/de.rs`) in
panic locations, so `CARGO_HOME`, `RUSTUP_HOME`, the workspace path and the host
all influence the WASM. Measured on 2026-10-29, `inference-mesh` hash:

| Build environment | Hash |
| --- | --- |
| Reference runner layout (ubuntu, `HOME=/home/runner`, `CARGO_HOME=/home/runner/.cargo`, workspace `/home/runner/work/AInonymous/AInonymous`, Rust 1.91.1) | `uhC0kYFyQtLQaaKs--WrpAQdNh44E0SXDX8Kk409Tan7Us4QLU5Q_` (pinned) |
| Same sources, Debian container, reference workspace path, but `CARGO_HOME=/usr/local/cargo` | `uhC0kidndeCTdYPBoLVdylWL5TGo2VIykd140eFTd3J6ixYNvHIIq` |
| Debian container mirroring the runner layout | `uhC0kYFyQtLQaaKs--WrpAQdNh44E0SXDX8Kk409Tan7Us4QLU5Q_` (pinned) |
| Windows 11, Rust 1.91.1, `--remap-path-prefix` on the workspace | `uhC0kzs5P3jR0D2MKCjueITX4GDLgUd7v6yCmqUb_PVnEc09nrEKF` |
| Windows 11, Rust 1.95 | `uhC0kPZgq4_efY2J6_ZwSZU3AOuuvso7jhowLelod0ClwX7OcKpPK` |

The supported way to rebuild the epoch is therefore the pinned image, which
mirrors the runner layout instead of normalising paths away:

```bash
docker build -f deploy/containers/repro/Dockerfile -t ainonymous-repro .
docker run --rm ainonymous-repro   # exits non-zero unless the hashes match the registry
```

[`repro-bundles.yml`](../.github/workflows/repro-bundles.yml) runs exactly this
on pull requests that touch `dnas/**` and weekly, so the claim stays verifiable
rather than aspirational.

Any intentional change to an integrity zome, DNA properties, seed or manifest modifiers requires a new epoch. Rebuild with the pinned CLI, record all new hashes together, update the hApp `installed_hash` values, use a new conductor-volume suffix and reinstall every peer from fresh state.
