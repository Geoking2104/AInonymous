# Enterprise inference and governed learning

**Functional and technical specification · Proposed for review · 22 November 2026**

## Product decision

Give an enterprise control over three decisions for every inference request: which knowledge a person may retrieve, which machines may process it, and which corrections may change future answers. The first delivery is a read-only assistant for internal procedures. Two teams with different access rights ask the same question and receive answers grounded in the sources each team may read. A procedure owner can correct the source, validate the new answer and withdraw it.

One customer, one approved document source and two customer-administered physical machines form the pilot. “Tokenization” covers the LLM input tokenizer, prompt and generated tokens, intermediate activations and generation caches. The pilot uses synthetic procedures until the customer approves its data classes and the isolation tests pass. Public compute, autonomous tool execution and cross-company model training are outside this release.

## Engineering baseline

The baseline is main commit `960ac99586543ee8303e9218b5153da870e37701`. The [architecture](ARCHITECTURE.md), [current API](API_SPEC.md) and [security model](SECURITY.md) describe the implementation. Requirements below describe the target.

| Existing evidence | Dependency for the enterprise pilot |
| --- | --- |
| [SG1](../scripts/sg1/README.md) exercises network-bound admission with two Holochain conductors. The QUIC transport key is bound to the authenticated agent. | Use operator-controlled bootstrap and relay services. Add request-level authorization, expiring execution rights and revocation of active work. |
| [Pipeline experiments](../spike-pipeline/README.md) record equivalence in a two-process test. [QUIC experiments](../spike-quic/README.md) document loopback results. | CH3 requires physical hosts and an end-to-end inference measurement. [PR 61](https://github.com/Geoking2104/AInonymous/pull/61) concerns transport measurement tooling. Record inference results independently of transport throughput. |
| The local daemon supports solo and pipeline plan types. The proxy accepts OpenAI-compatible chat requests. | Introduce a controlled enterprise entry point and check the execution grant at each destination. Measure actual input tokens before enforcing usage-based accounting. |
| The repository tracks [CH6](https://github.com/Geoking2104/AInonymous/issues/53) and [PR 57](https://github.com/Geoking2104/AInonymous/pull/57) for revocation. | Review and reuse the work against connected and partitioned acceptance cases below. An open change is insufficient evidence for release. |

## Users and acceptance journeys

| ID | Actor and action | Observable result |
| --- | --- | --- |
| F1 | Employee signs in and asks about a current procedure. | The answer cites accessible document versions and sections. An evidence gap yields an explicit abstention. The answer and citation endpoints apply the caller's current rights. |
| F2 | Two team members submit identical questions after the first answer warmed the caches. | Each receives only authorized content. Search results, titles, snippets, citations, cache hits and audit views preserve the same boundary. |
| F3 | Data owner approves a new procedure version or withdraws a source. | The next eligible answer cites the approved revision after publication. Revoked sources immediately become ineligible once the serving policy observes the withdrawal. The release gate measures propagation and checks restored copies. |
| F4 | Security operator removes a worker during generation. | The client sees an interrupted request with an opaque request ID. Remaining healthy workers enforce the withdrawal. A disconnected worker's grant expires within the approved offline window. |
| F5 | Procedure owner reviews feedback and promotes a correction. | Review shows source lineage, the reference answer, candidate output and evaluation results. Approval records the reviewer and deployed version. Rollback restores the previous eligible version while retaining current access restrictions. |
| F6 | SRE investigates a slow or interrupted request. | An authorized operator can trace the policy decision, worker IDs, model version, timing and outcome by request ID. Customer content stays in the customer's data plane. |
| F7 | A team exceeds its quota or requests a model outside its policy. | The gateway returns a stable reason and request ID. The scheduler sends zero prompt bytes to a worker for that rejected request. |

## Technical contract

**Request path.** Enterprise client → authenticated gateway → policy check and authorized retrieval → eligible-node selection → worker grant verification → QUIC/mTLS execution → cited answer and audit receipt. Reuse the local proxy, daemon, Holochain control plane and QUIC data plane. The gateway and retrieval, policy and evaluation components are new work. Keep the conductor, daemon management routes and zome bridge inaccessible to tenant clients. Retain the local development entry points.

| ID | Component and target behavior | Contract and failure handling |
| --- | --- | --- |
| T1 | Gateway and identity | Validate corporate OIDC issuer, audience and expiry. Derive tenant and role from trusted mappings. Authorize the requested model, source scope, region, input/output token budgets and concurrency. Reject caller-supplied identity overrides. Return 401 for missing authentication, 403 for denied operations, 429 for quota exhaustion and 503 when no authorized destination is available. |
| T2 | Policy and scheduler | The customer's policy authority signs short-lived grants bound to request ID, tenant, worker identity, role in the plan, permitted model/tokenizer digests, policy epoch and expiry. Verify at the gateway and each worker before dispatch/acceptance and on renewal. Select on load or link quality only from eligible peers. Disable static discovery, public fallback and uncontrolled outbound calls for the enterprise profile. |
| T3 | Runtime and tokenizer | Pin model weights, tokenizer vocabulary/configuration, chat template, quantization and engine build. Record their digests with the plan. Use golden fixtures for Unicode, special tokens, truncation and context limits. Count the fully templated input with the selected tokenizer. Reject incompatible artifacts before processing content. Use measured input/output usage or mark it unknown. |
| T4 | Data placement and memory | Scope documents, embeddings, retrieval indexes and KV/answer caches by customer, access scope and version. Filter before retrieval and recheck immediately before disclosure. Carry document ID, source version, ACL revision and retention class through every derivative. Keep prompts, tokens, raw vectors, KV caches and weights off the DHT. Encrypt retained data and backups under customer-scoped keys. |
| T5 | Revocation and recovery | Publish an authenticated withdrawal with a monotonic epoch. Persist the highest accepted epoch and reject stale grants, including after restart. Check freshness before each session and while generating. Bound offline execution by grant lifetime and clock uncertainty. If trustworthy time or revocation freshness is unavailable, deny renewal. Rotate keys and re-establish authorization before a recovered node serves requests. |
| T6 | Stream lifecycle and accounting | Track accepted, running, completed, denied, cancelled and interrupted outcomes. Emit one terminal outcome per request. Scope idempotency keys to tenant and request digest. Reuse with different input returns 409. Record sequence and cumulative usage for streamed output. Retry only within fresh authorization. If a safe checkpoint is unavailable, terminate explicitly and require a new generation. |
| T7 | Audit and supply chain | Keep tenant-local receipts with opaque request ID, decision reason, policy epoch, eligible execution identities, artifact digests, timings and outcome. Restrict and time-limit access. Reconcile charges against a deduplicated usage ledger. Verify release signatures and pinned digests, retain a software bill of materials and exercise restoration and rollback. Replicated coordination records contain only the minimal approved metadata. |

**Sensitive execution boundary.** A machine processing a prompt or activation can inspect its input. Approve every stage, including the embedding server, tokenizer, split workers, plugins and backup destinations. Isolate workers and restrict egress to the approved topology. During a partition, a disconnected honest worker can enforce an expiring grant locally. A compromised host can retain data already received. Record that residual risk and keep restricted workloads on customer-controlled machines. The pilot makes no anonymity, confidential-GPU or zero-knowledge claim.

**Failure visible to users.** A response interrupted after streaming has started carries its terminal status through the stream or request-status record. An HTTP success header alone cannot mark it complete. Recheck source rights before serving a cached response or issuing a fresh citation. Retain withdrawal tombstones independently of index backups. Reapply them before bringing a restored index or rolled-back model online.

## Learning and change authority

| Stage | Input and decision owner | Promotion and rollback |
| --- | --- | --- |
| L1 · Approved knowledge | The source owner reviews a correction and authorizes retention of the feedback. A reviewer links it to a source revision. | Reindex in isolation, compare answers on a frozen evaluation set, publish a signed version and retain its predecessor. A deleted source stays ineligible across rollback. |
| L2 · Routing | SRE uses content-free timing and load measurements to rank already eligible workers. | Compare against deterministic routing in observation mode, then use limited traffic. Revert on a latency or availability regression. Policy eligibility remains authoritative. |
| L3 · Customer-specific model adaptation | After L1 and L2 pass, the customer explicitly approves a dataset, purpose, retention period and isolated training environment. | Separate training and evaluation sets. Record provenance, test disclosure and memorization, sign the adapter, and demonstrate withdrawal or rebuild before serving it. Federation requires a separate architecture and customer decision. |

At least 100 pilot questions must cover both access groups, revised and withdrawn documents, unanswerable questions and representative languages. Product and the source owner freeze the reference set and the minimum quality gain before evaluating a candidate. Score source-supported correctness, valid citations, correct abstentions and disclosure by cohort. Report counts and uncertainty. A disclosure or a critical access regression blocks promotion; a small or inconclusive quality change stays in evaluation.

## Qualification and evidence

Each run records commit, topology, model/tokenizer digests, policy version, sampling settings, hardware, raw measurements, owner and pass/fail decision. Place sanitized reports under version control. Store customer-specific traces inside the customer's boundary.

| Test | Covers | Evidence required for acceptance |
| --- | --- | --- |
| A1 · Isolation and denial | F1–F3, F7; T1–T4 | Two teams plus a second synthetic tenant, cold and warm caches, stale ACLs, forbidden model, missing identity and exhausted quota. Zero unauthorized markers in answer, citation, metadata or cache across the fixed matrix. Trace refused dispatches. |
| A2 · Physical inference | F1, F6; T2–T4 | On two physical hosts run single-node, replicated service and layer-split modes against identical fixtures on LAN, then an approved inter-site link. Compare outputs under a predeclared numerical tolerance. Report input size, concurrency, time to first token p50/p95, generated tokens/s per stream, aggregate throughput, peak memory and cost per million tokens. |
| A3 · Revocation under load | F4; T2, T5, T6 | From publication of a signed withdrawal to the last observed generation event, record maximum delay over repeated trials. CH6 target is under 5 s on a healthy connected network. Under partition test immediately after grant issuance, measure offline lifetime plus allowed clock error and reject a replay after restart. Set both bounds before admission of pilot data. |
| A4 · Failure and recovery | F4, F6, F7; T5–T7 | Lose a worker mid-stream, restart the conductor and repeat submission with the same key. Verify one terminal record, zero duplicate usage and zero public fallback. Restore to a clean host, replay withdrawals and deletions, and verify isolation before reopening. |
| A5 · Governed improvement | F1, F3, F5; T3, T4, T7; L1–L2 | Demonstrate one approved correction, one rejected candidate, one rollback and one withdrawal surviving rollback. Compare observed routing with the fixed baseline. Confirm the same destination constraints at every stage. |
| A6 · Operational readiness | F6; T1–T7 | Independent reviewer examines delivered artifacts and threat assumptions. Retest critical findings, rotate credentials and rehearse the incident and recovery runbooks. Sign a release decision scoped to one customer, data classification, version and topology. |

Proposed operational objectives for customer review are 99.5% monthly availability for eligible requests, four-hour service restoration and at most one hour of lost approved knowledge updates. Measure availability as completed eligible requests divided by eligible requests received, including infrastructure failures. Report deliberate policy denials separately. Set latency and cost budgets from A2. Exercise recovery of the withdrawal ledger separately. A successful restore must never re-enable a revoked grant or deleted source. These targets are planning assumptions.

## Delivery sequence

| Window | Accountable owner | Deliverable and exit decision |
| --- | --- | --- |
| Days 1–10 | Product owner and security lead | Select the source owner, identity provider, two hosts and authorized data classes. Sign the placement matrix, evaluation design, lease bounds and retention schedule. |
| Days 11–30 | Runtime and platform leads | Establish private bootstrap/relay. Capture the first physical-host baseline. At day 30 select replicas when the model fits and evidence favors them. If splitting fails qualification, continue with qualified replicas and track the split case separately. |
| Weeks 5–6 | Runtime lead | Complete A2 and the first failure drill. Publish the supported topology, observed operating envelope and open defects against [CH3](https://github.com/Geoking2104/AInonymous/issues/50). |
| Weeks 7–12 | Security and platform leads | Deliver T1, T2, T5 and quota enforcement. Pass A3 and dispatch-denial checks. Review CH6 evidence with the customer security owner. |
| Months 4–5 | Knowledge owner and data lead | Onboard one approved source, publish the two-role assistant, pass A1 and exercise restoration of withdrawn sources. |
| Month 6 | Applied AI lead and business owner | Deliver L1 and L2 with A5. Require signed approval to activate any change affecting answers. |
| Months 7–9 | SRE and customer security owner | Close A4/A6 and [CH7](https://github.com/Geoking2104/AInonymous/issues/54). Demonstrate 30 days of operation at the declared load. Customer and product owners record the production decision and residual risks. |

Schedule assumes three engineering contributors across runtime, platform/security and applied AI/data, a customer source owner, two physical hosts and access to an independent reviewer. Windows and Android delivery, address-hiding transport and vendor SD-WAN adapters remain tracked in the [existing workstreams](chantiers-v1.md). After qualification, a federation proposal must define separate organizational keys, consent for every shared artifact, withdrawal of a participant and continuity of each local service.
