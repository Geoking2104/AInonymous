# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
the project intends to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

No release has been tagged yet. `v0.1.0-alpha` is the planned first tag: the
release-verification tooling exists (`spike-release`, 21 tests), but the Phase-0
gates G1–G8 are not all scored, so the tag waits for the Phase-0 report and the
go/no-go decision.

## [Unreleased]

### Added

- `pytorch-adapter` CI job: the adapter test suite in `examples/pytorch` now runs
  in a GitHub workflow instead of only on developer machines.
- `CONTRIBUTING.md`, including the review policy for large pull requests.
- Regression test proving that a refused mTLS client cannot consume a one-time
  session token (`crates/ainonymous-quic/tests/mtls_handshake.rs`) and the same
  scenario at spike level (`spike-quic/tests/two_nodes.rs`).

### Changed

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
