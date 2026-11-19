# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
the project intends to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

No release has been tagged yet. `v0.1.0-alpha` is the planned first tag: the
release-verification tooling exists (`spike-release`, 21 tests), but the Phase-0
gates G1–G8 are not all scored, so the tag waits for the Phase-0 report and the
go/no-go decision.

## [Unreleased]

### Fixed

- Proxy ↔ daemon wiring: the proxy now reaches the daemon HTTP API through
  `AINON_DAEMON_URL` (default `http://127.0.0.1:8890`) instead of port 8889 (the
  Holochain app WebSocket); `AINON_HOLOCHAIN_APP_PORT` is still honoured as the
  daemon port, with a deprecation warning.
- `/v1/ainonymous/mesh/nodes`: the proxy deserializes the daemon response as the
  shared `ainonymous_types::NodeSummary` (it expected `NodeCapabilities` and
  always answered 503) and forwards `model_id`.
- Daemon `POST /mesh/metrics` publishes the received `InferenceMetrics` through
  the `inference-mesh::publish_metrics` zome (it was a no-op); the proxy sends
  the measured metrics after each successful completion.
- `/v1/models`: `nodes_available` is the real number of nodes returned by the
  daemon for each model (0 when the daemon is unreachable) instead of a fixed 1.
- `ainonymous start` launches `ainonymous-daemon` and `ainonymous-proxy`, waits
  for each `/health`, and fails fast if a process exits (it used to start only the
  daemon and wait for the proxy port).
- `scripts/testnet/daemon_t51.toml` matches the current `DaemonConfig` schema
  (it no longer loaded); a unit test parses the testnet configs.

### Added
- CH3/S4 measurement campaign kit: `scripts/campaign/` cross-compiles `quic-spike` for Android (aarch64, API 24+), prepares a Termux worker over adb, orchestrates repeated two-real-machines measurements per link (usb/wifi) with fresh 30-s session offers, aggregates results into `results/ch3/` in the established format, and is covered by seven stdlib unit tests run in a dedicated CI job (`Campaign scripts`). Protocol: `docs/campaign/PROTOCOL.md`.


- Website: interactive setup guide in the “Rejoins le réseau / Join the network”
  section of `site/index.html` and `site/index.en.html` (`site/setup-guide.js`,
  `site/setup-guide.css`): a client-side scripted assistant with an optional
  local OpenAI-compatible mode; text version in `docs/JOIN_NETWORK.md`.
- Website: “État du mesh / Mesh status” section in `site/ainonymous.html`.
- `.github/workflows/pages.yml`: publishes `site/` to GitHub Pages on pushes to
  `main` (requires the Pages source to be switched to GitHub Actions).
- README: mesh integration status, website link and join-the-network quick start.

- `pytorch-adapter` CI job: the adapter test suite in `examples/pytorch` now runs
  in a GitHub workflow instead of only on developer machines.
- `CONTRIBUTING.md`, including the review policy for large pull requests.
- Regression test proving that a refused mTLS client cannot consume a one-time
  session token (`crates/ainonymous-quic/tests/mtls_handshake.rs`) and the same
  scenario at spike level (`spike-quic/tests/two_nodes.rs`).
- Reproducible bundle build: `deploy/containers/repro/Dockerfile` rebuilds the
  pinned DNA epoch byte for byte on any host with Docker by mirroring the
  reference runner layout, and `.github/workflows/repro-bundles.yml` verifies it
  against `deploy/holochain/dna-hashes.json` on every change under `dnas/**` and
  weekly.
- `docs/DNA_HASHES.md`: measured reproducibility boundary for the pinned DNA
  epoch, and the recipe that removes it. The hashes reproduce only in the reference build environment; Windows
  builds with Rust 1.91 and 1.95 both yield different DNA hashes, so peers must
  install CI/release bundles rather than a local rebuild.
- SG1 admission gate: `scripts/sg1/run_sg1.py` starts two real Holochain 0.7
  conductors, builds the private DNA variant and the app bundle, and asserts that
  a valid `PrivateNetworkProof` is admitted while a missing proof, a proof for
  another agent, another network id or another admin key are refused. Run in CI
  by the `SG1 two-conductor admission` job.
- `tools/private-network-proof`: generates and offline-verifies the
  `PrivateNetworkProof` bytes the integrity zome checks at genesis, filling the
  gap left by the documentation ("generate a proof for the target agent and
  network ID") with no tool.

### Changed

- Website copy aligned with the repository: no “public network / no signup”
  claim, no unmeasured speed-up claim, real repository URLs and install commands
  in `site/index*.html`.
- Housekeeping: the 21 leftover remote branches from earlier phases were deleted
  after checking each tip — either an ancestor of `main`, or squash-landed in
  `1824be9` (the Holochain 0.7 hardening stack) or `8a0d0e5` (site polish).
  `gh-pages` is kept deliberately: it is what GitHub Pages serves.
- CI: Clippy now runs with `-D warnings` on all targets (`--all-targets`) for the
  whole workspace; the lenient `-A dead_code -A unused` allowance is gone. The
  seven warnings it surfaced are fixed (three `print_literal` in the CLI, a
  redundant `use hex;`, `too_many_arguments` via a `PipelineInference` struct,
  `manual_is_multiple_of`, and one `type_complexity` via a type alias).

- `README.md`: `crates/ainonymous-types` added to the repository layout; the
  Phase-0 spike program recorded as merged (PR #38) instead of a draft PR; the
  `spike-quic` test count refreshed.

### Fixed

- PyTorch adapter: bfloat16 activations now round-trip bit-exactly. NumPy has no
  bfloat16 scalar type, so the raw 16-bit words are transported; `MAGIC` is
  exported, and `merge([])` raises `ValueError` instead of failing inside
  `torch.cat`.
- PyTorch adapter test suite: it had never run in CI and was stale against the
  implemented wire format (it expected a big-endian magic and 64-bit shape
  fields); the tests now assert the format the code actually writes.
- `docs/PYTORCH_INTEGRATION.md` and `examples/pytorch/README.md`: the documented
  wire format is aligned with the implementation (little-endian magic, uint32
  shape dimensions, bfloat16 as raw 16-bit words).
- QUIC data plane: the one-time session token is now consumed only **after** the
  client transport key is verified. The listener previously removed the token
  from the registry before the mTLS client-key check, so an interceptor could
  burn a token with a connection that was then refused — a session-level denial
  of service that prevented the legitimate peer from using its own token.
  Reported by spike S4 and tracked as DR10 in the Phase-0 review.

### Security

- `docs/SECURITY.md`: token consumption ordering is recorded as an enforced
  invariant, backed by a regression test.
