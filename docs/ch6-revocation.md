# CH6: signed mesh removal and live enforcement

Implements the first enforcement slice for #53 using the existing
`agent-registry` DNA and QUIC data plane. No `relay-market` DNA, warrant redesign,
or genesis-proof format change is introduced.

## Contract

- `Revocation { network_id, agent_id, scope, reason, issued_at_ms, signature }` is
  public and append-only. `scope` is currently exactly `mesh` (per-model scopes
  and un-revocation are deliberately unsupported). A valid admin signature and
  an action authored by that same Holochain admin are both required. Admin
  self-removal, mutable revocations, deleted revocations/index links, malformed
  target IDs, and cross-network records are rejected by integrity validation.
- `revoke_agent` writes and indexes the entry. `list_revocations` returns signed
  entries, so the daemon independently verifies them rather than trusting a
  control-plane JSON assertion. A local zome signal is emitted; **this slice
  uses one-second polling, not a remote signal subscription**, for propagation.
- Capability announcements also publish an authenticated transport binding:
  Holochain action author -> independent Ed25519 TLS key. The daemon retains
  historical bindings, rejects unknown/ambiguous keys, and denies the agent
  behind a rotated key. A revoked agent cannot change its TLS key to resume.
- The local gate is consulted before inbound application-session acceptance and
  outbound connection setup, and again atomically when tracking the connection
  after handshake. An in-flight handshake cannot slip through an applied removal.
- Applying a removal closes tracked connections with application code `0xC6`
  and the reason. QUIC carries that reason to the other endpoint. Listener
  admission rejects a fresh token from the same revoked identity as well as an
  old offer. Neither a refreshed empty snapshot nor a daemon restart restores it.
- Signed removals are atomically persisted and fsynced before acknowledgement.
  Invalid cache contents fail startup. Write failure invalidates freshness and
  terminates tracked sessions. Freshness requires a successful complete poll;
  admission fails closed after three seconds without one, and a separate
  100 ms watchdog closes existing sessions. This detects failed refreshes,
  **not a DHT peer silently omitting an as-yet-unseen entry**.
- The daemon selects connection closure against the whole worker job future,
  closes downstream immediately, sends `/cancel`, and clears the request cache.
  Failed jobs retain their reason in logs and `GET /jobs/{request_id}` on the
  pipeline service. `/prefill` and `/decode` run in FastAPI's thread pool behind
  a model lock (preserving serial inference), so cancellation can be received
  while a forward is running. Cancellation is checked between layers and before
  any cache commit, and the same cancelled job ID cannot resume.

## Signing format

One shared Rust crate is used by the native verifier, offline signer, and WASM
integrity zome. The signature covers:

1. ASCII `AInonymous/Revocation/v1` followed by one zero byte;
2. network ID, agent ID, scope, reason in that order, each UTF-8 byte sequence
   preceded by its unsigned 32-bit **big-endian byte length**;
3. unsigned 64-bit **big-endian** `issued_at_ms`.

The signature itself is excluded. Ed25519 strict verification is used.
`issued_at_ms` is an audit timestamp, not an expiry or an ordering permission:
removals take effect on receipt and are monotonic, including delayed entries.
This versioned format is independent of admission-proof MessagePack encoding.

## Provisioning and migration

This is an **opt-in integrity change / new DNA hash (new registry epoch)**.
Existing deployed DNA hashes cannot be upgraded in place by swapping WASM.
Keep existing entry/link type indexes stable, rebuild the bundle, provision all
participating nodes with the same properties, and re-announce their capabilities.
Retain signed removal history across any planned migration. Do not operate mixed
guarded/legacy nodes and claim a mesh-wide guarantee.

Set `agent-registry` DNA `integrity.properties` in the private variant before
packing (use the same mesh admin as admission):

```yaml
network_id: "YOUR-MESH-ID"
network_admin_pubkey: [32, 1, 2] # illustrative ONLY: replace with exactly 32 public-key bytes
```

Unlike the existing SG1 HybridNode properties, which use a raw 36-byte Holochain
key, this field uses exactly the **32 Ed25519 public-key bytes**, without the
four DHT-location bytes. The admin Holochain cell must be authored by this key;
the ordinary node's independently generated QUIC identity is not the mesh admin.

On every daemon, configure matching trust and a unique cache file in an existing,
private writable directory:

```toml
[revocation]
network_id = "YOUR-MESH-ID"
admin_pubkey = "64_HEX_CHARACTERS_OF_THE_MESH_ADMIN_PUBLIC_KEY"
cache_path = "/var/lib/ainonymous/revocations.json"
```

The conductor backend is required. With this section absent, legacy behavior is
preserved and **CH6 is not enabled**. With it present, initial snapshot retrieval
must succeed within three seconds or startup fails; there is no fallback to
static/unguarded admission. This can require retrying startup while peers finish
announcing capabilities.

Sign offline using a protected file containing the admin's **32 raw seed bytes**
(the secret is never passed as a command argument or printed):

```bash
cargo run --locked -p ainonymous-revocation --example sign_revocation -- \
  /secure/mesh-admin.seed YOUR-MESH-ID uhCAk_TARGET_AGENT_KEY 'membership removed' revocation.json
curl --fail-with-body -H 'Content-Type: application/json' \
  --data-binary @revocation.json http://127.0.0.1:8890/mesh/revocations
```

Run publication through the admin node's authenticated Holochain cell.
The loopback endpoint verifies/persists locally first, then calls `revoke_agent`.
HTTP 201 confirms publication; HTTP 502 explicitly means local denial is in force
but DHT publication failed and must be retried. No admin private key is loaded
by the daemon. Every peer independently verifies the same public entry.

## Validation

```bash
cargo test --locked -p ainonymous-revocation -p ainonymous-quic -- --nocapture
cargo clippy --locked -p ainonymous-revocation -p ainonymous-quic --all-targets -- -D warnings
cargo check --locked --manifest-path dnas/ainonymous-core/Cargo.toml \
  -p agent-registry-coordinator --target wasm32-unknown-unknown
python3 -m unittest discover -s scripts/tests -v
python3 scripts/sg1/run_sg1.py
```

The loopback test measures **local application -> both QUIC sides closed and test
job future stopped**, using real quinn/rustls endpoints and only one-sided
removal delivery. It also verifies server-side refusal of a fresh token, local
re-admission refusal, restart persistence, key rotation, forged signatures,
unknown keys, empty refresh, unavailable snapshots, and persistence failure.
The Python test cancels a running forward substitute and checks that it cannot
commit cache or resume, while an unrelated job still runs. CI runs it without
requiring torch/GPU packages.

## Remaining acceptance gates (issue stays open)

1. **CH3 / T-08:** measure actual `revoke_agent` publication -> peer application
   -> acknowledged pipeline cancellation on two real machines/conductors, with
   connected gossip and a loaded model. Loopback timing is not a DHT propagation
   guarantee. Record p50/p95/max, both endpoint reasons, and job status. Use a
   synchronized wall clock or a single observer with monotonic measurements.
2. **Hard GPU deadline:** cooperative cancellation cannot interrupt a running
   GPU kernel. A <5 s *physical compute stop* guarantee requires either a measured
   upper bound on each layer/forward or an isolated worker process that can be
   terminated/restarted. This is a genuine remaining execution-policy choice;
   do not kill a shared model process as an implicit side effect of revoking one
   member. The current slice stops scheduling work and prevents cache commits.
3. **Genesis vs runtime re-admission:** SG1 uses HybridNode genesis proofs and
   does not exercise gossip. Its proof format and integrity code are unchanged.
   Holochain genesis validation cannot read a live DHT deny list. A historical
   valid proof can still install a cell; the guarded session gate then refuses
   service for the revoked agent. If “refuse re-admission” must instead mean
   refuse *installation*, choose a new membership epoch/new proof policy; that
   would be a separate change to SG1 and should not be claimed here.
4. **Durable revocation distribution:** polling cannot prove completeness of an
   eventually consistent DHT response. Bound propagation on CH3 before asserting
   <5 s, or add an authenticated direct push/ack channel if that bound fails.
   Persisted deny state and cancellation tombstones are local trusted state;
   deleting them or deliberately operating legacy nodes is outside this slice.
