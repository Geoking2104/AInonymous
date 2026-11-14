# AInonymous

AInonymous is an experimental, agent-centric control plane for distributed LLM inference. It combines Holochain for authenticated coordination, a direct QUIC/mTLS data plane for activations and model traffic, and an optional SD-WAN adapter for topology and SLA-aware scheduling.

> **Status:** alpha research software. The repository now targets Holochain **0.7.0** exclusively. It is suitable for development and controlled private pilots, not for anonymous public production workloads. The project does not currently provide zero-knowledge inference, traffic-flow anonymity, or a complete public-network Sybil defense.

**Website:** <https://geoking2104.github.io/AInonymous/> — includes an interactive setup guide ([`#rejoindre`](https://geoking2104.github.io/AInonymous/#rejoindre), English page [`index.en.html#join`](https://geoking2104.github.io/AInonymous/index.en.html#join)). Text version: [Rejoindre le réseau](docs/JOIN_NETWORK.md) (French).

## Architecture

| Plane | Implementation | Purpose |
| --- | --- | --- |
| Control | Holochain 0.7.0 over Iroh | Agent identity, capabilities, scheduling coordination, audit records, membrane admission |
| Data | QUIC + TLS 1.3/mTLS | Direct peer-to-peer inference traffic and activation transfer |
| Underlay | SD-WAN adapter | Optional topology, QoS and link-health input |
| Runtime | Rust daemons + llama.cpp/PyTorch adapters | Local execution and pipeline orchestration |

Holochain and QUIC deliberately use different Ed25519 keys. A node publishes its QUIC public key in a capability entry authored by its Holochain agent. Session negotiation verifies that the supplied transport key matches the record authored by the caller. Private-network admission is enforced by DNA properties and a network-bound membrane proof during genesis.

See [Architecture](docs/ARCHITECTURE.md), [Security](docs/SECURITY.md), and [Holochain 0.7 migration](docs/HOLOCHAIN_0_7_MIGRATION.md) for the design and its boundaries.

> **Non-technical readers:** [Pourquoi AInonymous ?](docs/PITCH_FR.md) (French) explains the
> problem, who it is for, and what is—and is not—working today, without jargon.

## What is verified today

Every claim in this repository is expected to be backed by something runnable. The
current evidence:

| Capability | Evidence |
| --- | --- |
| Private admission, two real conductors | [`scripts/sg1/run_sg1.py`](scripts/sg1/README.md) starts two Holochain 0.7 conductors and asserts that a signed `PrivateNetworkProof` is admitted, while a missing proof, a proof for another agent, another network id or another admin key are refused (CI job `SG1 two-conductor admission`) |
| DNA epoch reproducibility | [`deploy/containers/repro/Dockerfile`](deploy/containers/repro/Dockerfile) rebuilds the pinned bundles byte for byte and checks them against the registry (CI job `Reproducible bundles`) |
| Data-plane authentication | `crates/ainonymous-quic/tests/mtls_handshake.rs`, including the regression proving a refused client cannot burn a one-time session token |
| Shared type contracts | `crates/ainonymous-types/tests/types_contract.rs` (activation wire format, warrants, node scoring, OpenAI request shapes) |
| Build hygiene | CI runs `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings`, the PyTorch adapter suite, the hApp build with pinned hashes and CodeQL on every push and pull request; the SG1 job on every push and pull request; the reproducibility job on changes under `dnas/**` and weekly |

Not verified, and deliberately not claimed: multi-node inference on real
hardware, phone/GPU measurements, anonymous operation, revocation, quotas, and an
independent audit. See [Security](docs/SECURITY.md) and [ROADMAP](ROADMAP.md).

## Mesh integration status

Holochain coordinates (identity, admission, discovery, execution plan); QUIC/mTLS
carries activations directly between peers. The building blocks are in place and
tested, but distributed inference has **not yet been measured across several real
machines**.

| Priority | Still to do | Tracking |
| --- | --- | --- |
| P1 | Measure distributed inference on two or more physical machines (LAN, then Tailscale/VPN): TTFT, tok/s, node loss — gates G4/G7/G8 | [CH3 #50](https://github.com/Geoking2104/AInonymous/issues/50), [S4 #30](https://github.com/Geoking2104/AInonymous/issues/30) |
| P1 | Private bootstrap/relay operated by the mesh administrator (reference configs use Holochain's public defaults) | ROADMAP security gate 1 |
| P1 | Proxy ↔ daemon wiring: the proxy's default daemon port (`AINON_HOLOCHAIN_APP_PORT`, 8889) is the Holochain app WebSocket, not the daemon API (8890); `/mesh/nodes` format mismatch; `/mesh/metrics` not published; fixed node count in `/v1/models` | `crates/ainonymous-proxy`, `crates/ainonymous-daemon/src/router.rs` |
| P2 | Real-time revocation | [PR #57](https://github.com/Geoking2104/AInonymous/pull/57) (draft), [CH6 #53](https://github.com/Geoking2104/AInonymous/issues/53) |
| P2 | Remaining negative tests (cross-network replay, wrong transport key, stale capabilities) and frame/backpressure fuzzing; `ExpertShard` (MoE) plans computed but not executed by the daemon | [#32](https://github.com/Geoking2104/AInonymous/issues/32), ROADMAP |
| P3 | GPU/phone measurements, stealth mode, production SD-WAN adapters, non-loopback authorization and quotas, external audit | [Chantiers v1](docs/chantiers-v1.md) |

## Join the network (quick start)

There is no online account: a node's identity is a key generated locally. A
**private mesh** additionally requires a membrane proof issued by the network
administrator. Public mode is development-only.

```bash
git clone https://github.com/Geoking2104/AInonymous.git
cd AInonymous
rustup target add wasm32-unknown-unknown
bash scripts/setup_holochain_wsl.sh            # x86_64 Linux/WSL2; Windows: scripts/setup_holochain_windows.ps1
cargo build --workspace
bash scripts/build-happ.sh release

# Disposable local node (test keystore, admin port 65000)
holochain -c scripts/testnet/conductor_t51.yaml
hc client call --port 65000 new-agent
hc client call --port 65000 install-app --app-id ainonymous \
  --agent-key <uhCAk...> dnas/ainonymous-core/ainonymous-core.happ
hc client call --port 65000 enable-app ainonymous
hc client call --port 65000 add-app-ws 65001 --allowed-origins "*" --installed-app-id ainonymous
AINON_CONFIG=scripts/testnet/daemon_t51.toml ./target/debug/ainonymous-daemon
AINON_HOLOCHAIN_APP_PORT=18890 ./target/debug/ainonymous-proxy   # OpenAI-compatible API on 127.0.0.1:9337
```

Run the conductor, daemon and proxy in separate terminals. The persistent Docker
stack, joining or operating a private mesh, ports and troubleshooting are covered in
[Rejoindre le réseau](docs/JOIN_NETWORK.md) and, interactively, by the
[setup guide on the website](https://geoking2104.github.io/AInonymous/#rejoindre).

## Version matrix

| Component | Pinned version |
| --- | --- |
| Holochain conductor / `hc` | 0.7.0 |
| Rust `holochain_client` | 0.9.0 |
| HDK | 0.7.0 |
| HDI | 0.8.0 |
| Lair keystore | 0.7.1 |

Holochain 0.7 is not database-compatible with 0.6. Create a new conductor data root and reinstall the hApps. The current DNA epoch uses `ainonymous-core-hc07-v3-20260823` and `ainonymous-hybridnode-hc07-v3-20260823`; every peer must install bundles built from this epoch.

The four canonical effective hashes are pinned in the hApp manifests and listed in the [DNA hash registry](docs/DNA_HASHES.md).

## Repository layout

```text
crates/
  ainonymous-daemon/      Holochain/QUIC orchestration daemon
  ainonymous-quic/        direct data plane and peer-key verification
  ainonymous-proxy/       HTTP compatibility proxy
  ainonymous-cli/         operator CLI
  ainonymous-mcp/         MCP integration
  ainonymous-types/       shared types (inference, node, API, errors, warrants)
  hybridnode-core/        reusable config, identity, topology and scheduler layer
  hybridnode-daemon/      HybridNode identity/topology startup scaffold
dnas/
  ainonymous-core/        inference-mesh, agent-registry and blackboard hApp
  hybridnode/             HybridNode membership/audit hApp
hybridnode/               configuration schema, examples and policies
tools/
  dna-hashes/             prints and checks effective DNA hashes of packed bundles
  private-network-proof/  generates and verifies HybridNode admission proofs
scripts/                  build, validation, SG1 harness and testnet tooling
deploy/containers/        conductor and daemons images, plus the reproducible bundle builder
docs/                     maintained architecture and operating documentation
site/                     static website (GitHub Pages), including the interactive setup guide
```

## Prerequisites

- Rust 1.91 or newer (`holochain_zome_types` 0.7.0 uses an API stabilized in Rust 1.91)
- `wasm32-unknown-unknown`
- Holochain 0.7.0 and `hc` 0.7.0
- Lair keystore 0.7.1
- Python 3 with `PyYAML` and `jsonschema` for configuration validation
- Bash for packaging and testnet scripts (WSL or Git Bash on Windows)

Install the Rust target and confirm that the Holochain tools are the pinned versions:

```bash
rustup target add wasm32-unknown-unknown
hc --version
holochain --version
```

On x86_64 Linux or WSL2, `bash scripts/setup_holochain_wsl.sh` installs the pinned release assets after verifying their published SHA-256 digests.
On x86_64 Windows, `powershell -ExecutionPolicy Bypass -File scripts/setup_holochain_windows.ps1`
does the same from the official `x86_64-pc-windows-msvc` assets, verifies the pinned
SHA-256 digests, checks the `hc client call` surface CI relies on, and leaves any older
installation untouched.

Do not package this repository with an older `hc`; manifest validation and bundle formats are version-sensitive.

## Build and test

```bash
# Native crates
cargo check --workspace
cargo test --workspace

# Both Holochain zome workspaces
cargo build --manifest-path dnas/ainonymous-core/Cargo.toml \
  --release --target wasm32-unknown-unknown
cargo build --manifest-path dnas/hybridnode/Cargo.toml \
  --release --target wasm32-unknown-unknown

# Validate reference configurations
python scripts/hybridnode/validate_config.py \
  hybridnode/configs/ainonymous.hybridnode.yaml \
  hybridnode/configs/generic-project.hybridnode.yaml

# Package both hApps with hc 0.7.0
bash scripts/build-happ.sh release
```

Print the effective hashes from the packed Holochain 0.7 bundles:

```bash
cargo run --locked -p dna-hashes -- --check deploy/holochain/dna-hashes.json \
  dnas/ainonymous-core/dnas/inference-mesh/workdir/inference-mesh.dna \
  dnas/ainonymous-core/dnas/agent-registry/workdir/agent-registry.dna \
  dnas/ainonymous-core/dnas/blackboard/workdir/blackboard.dna \
  dnas/hybridnode/dnas/hybridnode-core/workdir/hybridnode-core.dna
```

Verify private admission end to end with two real conductors (requires `hc` and
`holochain` 0.7.0 on `PATH`):

```bash
python3 scripts/sg1/run_sg1.py
```

For a private deployment, also run validation with `--production`. The reference development configuration intentionally uses a mock SD-WAN adapter and is not a production profile.

## Reprovision the container stack

The Compose stack runs one Holochain 0.7.0 conductor and both daemons in a shared, isolated loopback namespace. The admin and app WebSockets are not published to the host. Official Holochain Linux artifacts are SHA-256 pinned in the conductor image, Lair runs in-process with a persistent encrypted keystore, and the conductor volume is versioned for DNA epoch v3.

Reprovisioning intentionally deletes only the `ainonymous-hc07-v3` Compose project's named volumes, rebuilds both hApps, and installs fresh cells:

```powershell
.\scripts\reprovision-containers.ps1 -ConfirmReset -HcBin C:\path\to\hc.exe
```

```bash
HC_BIN=/path/to/hc ./scripts/reprovision-containers.sh --confirm-reset
```

Both commands require `hc 0.7.0` and refuse any other version. Docker is not required for normal native development. See [Container deployment](deploy/containers/README.md) for ports, secrets, lifecycle and production boundaries.

## Running with a conductor

1. Start a Holochain 0.7.0 conductor using a fresh data root and a loopback-only admin WebSocket.
2. Pack and install `dnas/ainonymous-core/ainonymous-core.happ` and `dnas/hybridnode/hybridnode.happ`.
3. Configure app ID `ainonymous` on port 8889 and app ID `hybridnode` on port 8891; admin remains on port 8888.
4. For a private DNA, generate a `PrivateNetworkProof` for the target agent and network ID
   with `cargo run -p private-network-proof -- proof --network-id ... --agent-key ... --seed-hex ...`,
   then supply it in the role settings as a YAML integer sequence when installing the app
   ([details](scripts/sg1/README.md)).
5. Start the daemon. Conductor connection failures are fatal; the runtime no longer silently falls back to static discovery.

The exact commands and conductor configuration are in [Holochain build and operations](docs/HOLOCHAIN_BUILD.md).

## Reuse in another project

Generate a project-specific configuration and validate it:

```bash
bash scripts/hybridnode/init_project.sh my-project
python scripts/hybridnode/validate_config.py my-project/hybridnode.yaml
```

Integration requires more than copying YAML: install the HybridNode hApp, connect through a loopback admin interface, announce the node's QUIC public key through the authenticated agent registry, and pin that key before opening a data-plane session. Follow [HybridNode integration](HYBRIDNODE_APPLY.md).

## Security defaults

- Conductor is the default discovery backend; explicit connection failures fail closed.
- The admin WebSocket is required to be loopback-only.
- QUIC strict peer verification cannot be disabled by accepted configuration.
- SD-WAN controller TLS verification cannot be disabled by accepted configuration.
- Private-network mode requires a matching private bootstrap mode and bootstrap URL.
- Membrane proofs are installation/genesis credentials, never ordinary zome-call fields.
- Transport private keys are stored in the OS keyring when the secure-keyring feature is enabled; Holochain agent keys remain in Lair.

The public-network mode remains experimental because its advertised proof-of-work admission is not implemented. See [Security](docs/SECURITY.md) before exposing any service.

## AInonymous Relay — Phase 0 spikes

AInonymous Relay is a planned GPU compute-relay module for local LLMs (Windows and Android nodes orchestrated through the Holochain mesh). Before any build commitment, Phase 0 validates its riskiest assumptions with throwaway spikes. Requirements and plan (French):

- [Cahier des charges — AInonymous Relay](docs/cahier-des-charges-relay-gpu.md)
- [Plan d'exécution — Phase 0 « Spikes »](docs/plan-phase-0-relay-gpu.md)
- Tracking: [milestone “Phase 0 — Spikes Relay GPU”](https://github.com/Geoking2104/AInonymous/milestone/1); box environment in [docs/phase-0-env.md](docs/phase-0-env.md)

Spikes are standalone and not production code. Status reflects only results actually measured so far (Linux x86_64 box, no GPU, no phone):

| Folder | Spike | Current status |
| --- | --- | --- |
| [`spike-android/`](spike-android/README.md) | S1 — Tauri + embedded Holochain 0.7.0 on Android | aarch64 APK built with the NDK (no Nix): 5,662,875 B minimal, 39,049,571 B with Holochain (< 200 MB gate). On-device runtime not yet measured (needs a phone). |
| [`spike-android-gpu/`](spike-android-gpu/README.md) | S2 — llama.cpp GPU inference on Android | Build/bench scripts and protocol only; no measurements yet (needs a phone). |
| [`spike-windows/`](spike-windows/README.md) | S3 — Windows GPU detection, llama.cpp build, installer | PowerShell scripts only; not run yet (needs a Windows GPU PC). |
| [`spike-pipeline/`](spike-pipeline/README.md) | S4 — pipeline-parallel equivalence | Split vs single-node: max abs logit diff 0, relative L2 0; two-process chain produced 16 tokens identical to single node. TTFT/tok/s on real nodes pending. |
| [`spike-quic/`](spike-quic/README.md) | S4 — QUIC activation transport | 7 tests pass. Loopback only (not LAN-representative): setup 1.27–1.61 ms, RTT p50 0.040–0.049 ms, 100 MiB at 3039–3217 Mb/s. LAN/Tailscale pending. |
| [`spike-release/`](spike-release/README.md) | S5 — signed release manifest and APK verification | 21 tests pass; tampered APK and replayed manifest are rejected. |

The Phase-0 spike program landed in [#38](https://github.com/Geoking2104/AInonymous/pull/38) (merged 2026-10-27); on-device measurements remain open. The core-platform roadmap remains in [ROADMAP.md](ROADMAP.md).

## Documentation

Start with the [documentation index](docs/README.md). The most important documents are:

- [Architecture](docs/ARCHITECTURE.md)
- [Security and threat model](docs/SECURITY.md)
- [Holochain 0.7 migration](docs/HOLOCHAIN_0_7_MIGRATION.md)
- [Holochain build and operations](docs/HOLOCHAIN_BUILD.md)
- [API reference](docs/API_SPEC.md)
- [Network/data plane](docs/NETWORK.md)
- [Two-node testnet](docs/TESTNET_2NODES.md)
- [Rejoindre le réseau — setup guide (French)](docs/JOIN_NETWORK.md)
- [Original architecture review](docs/ZK_AIS_GATENYM_ARCHITECTURE_REVIEW.md)
- [Pourquoi AInonymous ? (French, non-technical)](docs/PITCH_FR.md)
- [SG1 two-conductor admission test](scripts/sg1/README.md)

## License

Apache-2.0. See [LICENSE](LICENSE) and [DISCLAIMER](DISCLAIMER.md).
