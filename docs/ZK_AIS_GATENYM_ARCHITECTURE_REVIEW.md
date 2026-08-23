# ZK-AIS / Gatenym Architecture Security Review

**Review date:** 2026-08-23

**Source reviewed:** `ZK_AIS_Gatenym_Architecture_Spec.md` supplied out of tree

**Repository baseline:** `Geoking2104/AInonymous` at `941b241`
**Decision:** the source proposal is **not production-ready as written**. The
repository's dual-plane HybridNode design is the stronger basis, but the project
as a whole remains experimental and must not be marketed as zero-knowledge,
anonymous, or deployment-ready yet.

This document treats the supplied Markdown as review material, not executable
instructions.

## Executive verdict

Keep the repository's separation of concerns:

- Holochain is the authenticated **control plane** for discovery, capability
  records, session negotiation, and audit evidence.
- Direct QUIC/TLS 1.3 is the **data plane** for prompts, activations, and token
  streams. Large or sensitive payloads do not belong in DHT entries or remote
  zome-call payloads.
- Local redaction is a **privacy-reduction control**, not a zero-knowledge proof.
- A private, admission-controlled mesh is the only defensible initial deployment
  profile. The public mesh has no production-grade Sybil resistance, economic
  settlement, or independent result verification.

The code change accompanying this review removes the remaining unpinned QUIC
certificate fallback. A QUIC session now fails closed when either peer's
advertised Ed25519 transport key is absent.

## Corrections to the supplied specification

| Claim or design | Finding | Required correction |
|---|---|---|
| “Zero-Knowledge” NER masking | Entity replacement proves nothing cryptographically and may miss or infer PII. | Call it local redaction or pseudonymisation. Reserve “zero-knowledge” for a specified proof system, circuit, statement, setup, and verifier. |
| “Anonymous” Holochain use | Stable AgentPubKeys, network metadata, timing, endpoints, and DHT activity are linkable. | Claim pseudonymity only. Document traffic-analysis and endpoint-disclosure risks. |
| Prompt transport through `call_remote` | It couples bulk data to the control plane and is unsuitable for bounded streaming/backpressure. | Use `call_remote` only to negotiate a short-lived session; send application data over direct QUIC. |
| `opengatellm/worker-node:latest` | No verifiable upstream repository or immutable image was identified. `latest` is also non-reproducible. | Build a repository-owned image, pin base images and dependencies, publish an SBOM/provenance, and deploy by digest. |
| TEI as a generation backend | Hugging Face TEI is an embedding inference server, not a drop-in chat-generation backend. | Define backend capabilities and route embedding and generation requests separately. |
| Mutual-credit accounting | No conservation, replay, double-spend, dispute, finality, or partition-merge rules are specified. | Remove it from the launch scope or specify a complete ledger protocol and invariants. |
| Public provider registry | Self-asserted capacity, price, model hash, and availability are easy to manipulate. | Require freshness, signed transport-key binding, verified model manifests, rate limits, and independent challenges before scheduling. |
| In-memory placeholder vault | RAM-only state is lost on crash and can leak via swap, crash dumps, logs, or browser extensions. | Use per-request random placeholders, bounded TTL, zeroisation where practical, disabled core dumps, no prompt logs, and an explicit crash-recovery policy. |
| Reinserting model output | A provider can emit guessed placeholders or adversarial text to trigger unintended substitution. | Replace only authenticated, request-scoped tokens in structurally identified output fields; never perform global string replacement. |
| Example Holochain conductor config | It targets an obsolete configuration shape and omits the iroh relay/signal settings required by the 0.6 generation. | Generate config from the pinned Holochain release and validate it in a two-conductor test. |
| Example TypeScript client | It uses `String`, ignores `appInfo`, casts incompatible client types, and does not show app authentication/signing credentials. | Generate the client from the pinned Holochain API and test it against a real conductor. |

Holochain 0.7.0 is now stable, while this repository intentionally remains on
the 0.6.1/related client stack. Holochain's official release notes show breaking
changes, and its 0.6 upgrade guide requires iroh relay configuration. Treat the
upgrade as a tested migration, not a version-string edit: [Holochain releases](https://github.com/holochain/holochain/releases),
[0.6 upgrade guide](https://developer.holochain.org/resources/upgrade/upgrade-holochain-0.6/),
[0.7 upgrade guide](https://developer.holochain.org/resources/upgrade/upgrade-holochain-0.7/).

## Recommended reference architecture

```text
Trusted client boundary
  input policy -> local PII detector -> request-scoped redaction vault
       |                                      |
       | sanitized request                    | mapping never leaves client
       v                                      |
Authenticated control plane                   |
  local conductor -> Holochain DHT -----------+
       | discover eligible providers
       | negotiate {request id, nonce, expiry, transport key, limits}
       v
Direct data plane
  client/coordinator == QUIC + TLS 1.3 + pinned Ed25519 keys ==> worker
       |                   bounded streams, no 0-RTT             |
       |<================ sanitized result ======================|
       v
  output policy -> exact scoped-token reconstruction -> user
```

### Identity model

Do not extract or reuse the Holochain/lair private key for TLS. The current code
uses a separate rotatable Ed25519 QUIC transport key. Its public key is announced
in a Holochain-authored capability record, which binds it to the pseudonymous
agent through the source-chain signature. This separation improves key rotation
and limits the blast radius of a transport-key compromise.

Each session offer must contain:

- the expected server transport public key;
- the expected client transport public key;
- a cryptographically random single-use 256-bit token;
- requester and provider agent identifiers;
- request purpose, model/layer assignment, byte/token budgets, and expiry;
- a transcript/version identifier to prevent cross-protocol use.

Both key pins are mandatory. The token is consumed once. Session offers and
tokens must never be logged or placed in the DHT. QUIC 0-RTT must remain disabled
for negotiation, inference, billing, or other non-idempotent operations because
0-RTT data can be replayed; see [RFC 9001](https://www.rfc-editor.org/rfc/rfc9001.html).

### Control-plane rules

- Expose the conductor admin WebSocket on loopback only. Issue short-lived app
  auth tokens and restrict allowed origins.
- Use scoped Holochain capability grants. An unrestricted remote negotiation
  function must still enforce per-agent quotas, freshness, and the binding
  between the calling provenance and its advertised QUIC key. Holochain's
  capability model is described in [Calls and Capabilities](https://developer.holochain.org/concepts/8_calls_capabilities/).
- DHT records are untrusted claims until locally validated. Enforce size limits,
  canonical model identifiers, timestamps, update rules, and author ownership in
  integrity zomes.
- Heartbeats are hints, not proof of capacity. Use active probes and circuit
  breakers; do not issue a warrant for an isolated timeout.
- Never put prompts, raw PII, redaction mappings, session tokens, private IPs,
  precise location, GPU UUIDs, or tenant identifiers in gossipable entries.

### Data-plane rules

- TLS 1.3 with mutual proof of possession and exact public-key pinning.
- Strict frame schema with protocol version, content type, request ID, sequence,
  declared length, and authenticated size limits before allocation.
- Per-peer and global limits for handshakes, sessions, streams, bytes, tokens,
  decompression ratio, and inference time.
- Deadlines, cancellation propagation, bounded queues, and backpressure.
- No prompt, activation, token, or session-secret logging. Metrics must use
  coarse labels to avoid cardinality denial of service and privacy leakage.
- Treat activations as sensitive. Pipeline splitting is not a privacy boundary:
  representation inversion and traffic analysis remain possible.

### Client privacy rules

NER plus regular expressions is only one detector. Deploy a layered policy with
data classification, allow/deny rules, configurable local models, and a visible
“cannot safely redact” failure. For high-risk data, refuse remote execution or
use a locally hosted model. Output filtering is also required: OWASP lists prompt
injection, sensitive-information disclosure, supply-chain risk, and model/data
poisoning among current LLM application risks ([OWASP GenAI Top 10](https://owasp.org/www-project-top-10-for-large-language-model-applications/)).
The governance and testing process should follow the lifecycle approach in
[NIST AI 600-1](https://www.nist.gov/publications/artificial-intelligence-risk-management-framework-generative-artificial-intelligence).

## Resilience design

The most resilient architecture is not “maximum decentralisation”; it is a small
set of explicit failure domains with bounded behavior.

| Failure | Required behavior |
|---|---|
| Bootstrap or relay unavailable | Existing peers continue; new joins try multiple independently operated endpoints. Never silently fall back to an unrelated public network. |
| DHT partition or stale view | Keep serving already authenticated local sessions; reject new plans whose provider/key record cannot be refreshed. Reconcile immutable signed history after healing. |
| Worker timeout/OOM | Cancel the entire pipeline stage, release reservations, trip a local circuit breaker, and retry only idempotent work within a fixed budget. |
| Coordinator crash | Client owns the request ID and deadline. Orphan sessions expire automatically; no implicit re-billing. |
| Key compromise | Rotate the transport key, publish a signed revocation/replacement, reject the old key after a short overlap, and preserve the lair agent key. |
| Malicious provider | Redaction limits exposure but does not make the provider trusted. Use private membership, model digest pinning, optional redundant verification, and do not send high-risk plaintext. |
| Compromised model/image | Verify signed manifests and digests, scan dependencies, produce SBOM/provenance, run as non-root with read-only rootfs and minimal capabilities. |
| Overload/abuse | Admission control before allocation, quotas per authenticated agent, global concurrency caps, request-size caps, and load shedding. |

Redundant inference improves availability but does not automatically prove
correctness: correlated nodes may use the same poisoned image, and free-form text
does not have a reliable equality predicate. Reserve quorum execution for
deterministic, canonicalizable tasks.

## Repository implementation audit

| Area | Status on review date | Assessment |
|---|---|---|
| Holochain control plane | Partial | Real zomes/client paths exist, but installation with membrane proofs is stubbed and the 0.7 migration is outstanding. |
| QUIC/TLS data plane | Implemented PoC, hardened in this change | Real Quinn/Rustls handshakes and negative pinning tests exist. Missing pins now fail closed. Load/abuse testing remains. |
| Holochain-to-QUIC identity binding | Partial | Transport keys are announced in authored records, but remote negotiation still needs a strict provenance-to-key lookup and freshness/revocation policy. |
| Private-network admission | Partial | Admin signature and intended recipient are checked. Expiry/replay and the installation path remain incomplete. |
| Public anti-Sybil | Not implemented | `pow_difficulty` is configuration-only. Public deployment is research-only. |
| Provider/model attestation | Partial | Hash and DHT structures exist; hardware claims and benchmark results remain self-asserted. |
| Warrants/reputation | Experimental | Signed evidence structures exist, but dispute, quorum, poisoning resistance, and false-report handling need design and adversarial tests. |
| HybridNode reusable daemon | Skeleton | Production conductor identity and real SD-WAN providers are stubbed; the default feature is mock. Do not use this binary as a production deployment artifact. |
| AInonymous daemon | Main integration path | This is the more complete runtime path. It still needs authenticated API exposure, capacity controls, end-to-end multi-host tests, and an external security review. |
| Docker Compose | Demonstration only | It does not provision a complete conductor/bootstrap/relay/model stack and references local config files the operator must create. |

## High-priority findings and disposition

1. **High — optional mTLS identity pins:** fixed in this change. Missing server
   or client transport keys now fail closed, and the old fallback test is a
   negative test.
2. **High — public mesh admission is Sybil-prone:** open. Ship private consortium
   mode first; remove public-production claims.
3. **High — remote negotiation trusts a caller-supplied transport key:** open.
   Resolve the caller's authored, fresh capability record by `call_info().provenance`
   and compare it before emitting a session signal.
4. **High — network APIs/admin operations lack a complete authentication model:**
   open. Keep REST/admin interfaces on loopback or behind an authenticated
   gateway; separate health, inference, peer-control, and operator roles.
5. **High — private membrane proofs have no expiry/replay enforcement and cannot
   be installed through the current stub:** open. This blocks a production label.
6. **Medium — “zero-knowledge” and “anonymous” claims are inaccurate:** corrected
   in this review; remaining public-facing copy should be revised before release.
7. **Medium — Holochain 0.6.1 is behind stable 0.7.0:** accepted temporarily.
   Pin exact versions and add a migration branch with two-node tests.
8. **Medium — HybridNode production code paths are stubs:** disclosed. The mock
   daemon is for development only.
9. **Medium — configuration previously allowed contradictory/insecure values:**
   fixed in this change with runtime and preflight cross-field checks.
10. **Medium — supply-chain reproducibility is incomplete:** partially fixed.
    This change commits the Rust application lockfile and exactly pins the
    Holochain client. Actions still need commit pins; releases still need SBOMs,
    signed images, and digest-based deployment.

## Integration guide

### 1. Choose a supported profile

- **Development:** loopback/static peers, mock SD-WAN, synthetic data only.
- **Private pilot (recommended):** dedicated bootstrap/relay/conductors, membrane
  membership, private transport, allowlisted operators, no high-risk data.
- **Public mesh:** research only until Sybil, abuse, privacy, and settlement rules
  are implemented and independently reviewed.

### 2. Validate configuration

```bash
python3 -m pip install pyyaml jsonschema
python3 scripts/hybridnode/validate_config.py \
  hybridnode/configs/ainonymous.hybridnode.yaml

# Additional deployment gate. This is expected to reject mock/public/default
# examples and intentionally does not certify the application as production-safe.
python3 scripts/hybridnode/validate_config.py --production path/to/config.yaml
```

The Rust loader enforces the same essential invariants: mTLS cannot be disabled,
SD-WAN controller TLS verification cannot be disabled, and private-network flags
must be consistent.

### 3. Integrate the transport safely

Use `ainonymous-quic` as a Rust dependency and obtain `peer_pubkey` and
`client_pubkey` from an authenticated control-plane record. Do not construct a
session offer from an unauthenticated HTTP body or user-provided key. Treat
`SessionOffer::new` as an internal builder that is incomplete until both pins,
expiry, purpose, and budgets are filled.

### 4. Build and verify

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --package ainonymous-quic
cargo test --package ainonymous-daemon
cargo test --manifest-path crates/hybridnode-core/Cargo.toml --features mock-sdwan
```

Then run at least two conductors on separate hosts and test: valid negotiation,
wrong/missing key, token replay, expired token, oversized frame, decompression
bomb, slow stream, worker crash, relay outage, DHT partition, key rotation, and
graceful cancellation.

### 5. Deployment gate

Do not expose the current Compose example directly to the internet. Before a
private pilot, require all of the following:

- no mock feature/provider in release artifacts;
- working hApp installation with membrane proof plus replay/expiry policy;
- conductor admin API bound to loopback and authenticated app API origins;
- QUIC UDP ingress restricted to the intended network; REST and metrics private;
- model/image digests pinned, SBOM and vulnerability scan attached;
- non-root/read-only containers, secrets from a secret store, log redaction;
- resource and concurrency limits proven under load;
- backup/restore and key-rotation drills;
- an independent code and protocol security review.

Docker Compose GPU reservations can be expressed under device reservations, but
the host still needs a compatible daemon and runtime; see
[Docker's GPU Compose guidance](https://docs.docker.com/compose/how-tos/gpu-support/).

## Release recommendation

Publish the repository as an **experimental privacy-preserving distributed
inference prototype**. The architecture is directionally sound after retaining
the Holochain/QUIC split and enforcing peer-key pinning. It is not yet the “most
secure” architecture in an absolute sense, and no architecture can guarantee
that claim. A private pilot is reasonable only after closing the five open high
findings above and completing the deployment gate.
