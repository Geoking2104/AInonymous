# Container Deployment

This deployment is the supported reproducible local topology for Holochain 0.7.0 and DNA epoch v3. It is appropriate for development and controlled pilots after replacing the mock adapters. It is not a turnkey anonymous public service.

## Topology

The `holochain` service owns the network namespace. `ainonymous-daemon` and `hybridnode-daemon` join that namespace, so all three processes can use loopback while Holochain ports remain inaccessible from the host and Docker bridge.

| Endpoint | Container port | Host exposure |
| --- | ---: | --- |
| Holochain admin WebSocket | 8888/TCP | none |
| AInonymous app WebSocket | 8889/TCP | none |
| HybridNode app WebSocket | 8891/TCP | none |
| AInonymous local API | 8890/TCP via internal proxy 8892 | `127.0.0.1:8890` by default |
| AInonymous QUIC | 9000/UDP | all host interfaces by default |
| HybridNode metrics | 9338/TCP | none; query from the shared namespace |

The conductor image downloads official Holochain and `hc` 0.7.0 Linux release assets and verifies their pinned SHA-256 digests. It runs as UID/GID 10000 with a read-only root filesystem, no Linux capabilities and `no-new-privileges`. The data root and in-process Lair keystore live in `holochain-data-hc07-v3`. The AInonymous daemon keeps its writable runtime state in `ainonymous-runtime-hc07-v3` and model payloads in the nested `ainonymous-models` volume; the rest of its filesystem remains read-only. A small `socat` listener bridges host-loopback port 8890 to the daemon's internal loopback socket; no Holochain socket is bridged. The reference config inherits Holochain's public bootstrap/relay defaults; replace the complete `network` block with authenticated operator-controlled services for a private multi-host deployment.

## Prerequisites

- Docker Engine with Compose v2
- Rust 1.91 or newer
- the `wasm32-unknown-unknown` Rust target
- `hc` 0.7.0 available locally or selected through `HC_BIN`/`-HcBin`
- OpenSSL for the Bash workflow

Verify the CLI before changing state:

```bash
/path/to/hc --version
docker compose version
```

## Reprovision

Reprovisioning rebuilds all zomes and hApps, prints the effective DNA hashes using the pinned Rust libraries, validates Compose, removes the exact `ainonymous-hc07-v3` project and its named volumes, rebuilds images without cache, and starts a fresh stack.

PowerShell:

```powershell
.\scripts\reprovision-containers.ps1 `
  -ConfirmReset `
  -HcBin C:\absolute\path\to\hc.exe
```

Bash:

```bash
HC_BIN=/absolute/path/to/hc \
  ./scripts/reprovision-containers.sh --confirm-reset
```

The confirmation option is mandatory. The reset is destructive for data in the versioned Compose volumes. Export application-level data and back up the Lair data and password through an approved secret manager before resetting any non-disposable deployment.

The scripts create `deploy/secrets/holochain_keystore_password` with random bytes when it is absent. The file is ignored by Git. Do not replace it with a committed password, and do not rotate it independently of the corresponding Lair state.

## First-start provisioning

The conductor entrypoint waits for the loopback admin API, then installs and enables:

| App ID | Bundle | Restricted app interface |
| --- | --- | ---: |
| `ainonymous` | `dnas/ainonymous-core/ainonymous-core.happ` | 8889 |
| `hybridnode` | `dnas/hybridnode/hybridnode.happ` | 8891 |

Provisioning completes only after both apps and interfaces are created. A versioned marker in the conductor volume controls restart idempotency. The daemons start only after the conductor health check observes that marker.

## Operations

```bash
docker compose --project-name ainonymous-hc07-v3 ps
docker compose --project-name ainonymous-hc07-v3 logs --follow holochain
docker compose --project-name ainonymous-hc07-v3 stop
docker compose --project-name ainonymous-hc07-v3 start
```

Changing a DNA seed, integrity WASM, DNA properties or origin metadata changes its hash. Treat that as a new deployment epoch: change the explicit seed and volume suffix, rebuild every peer's hApp, and install into fresh conductor state. Never reuse an old marker or conductor database after a hash rotation.

## Production boundaries

Before a controlled production pilot:

1. Replace the mock SD-WAN adapter and public HybridNode profile with the intended provider and private-network DNA properties.
2. Operate authenticated private bootstrap/relay services and configure the conductor accordingly.
3. Enable and integrate the QUIC `secure-keyring` feature with a real container secret service; the reference image does not provide persistent transport-key custody.
4. Move the Lair password to a managed secret store and use encrypted, tested volume backups.
5. Put the local HTTP API behind authenticated access control if it must leave loopback.
6. Define model authorization, quotas, prompt retention and inference-runtime sandboxing.
7. Run failure tests for conductor restart, relay loss, peer churn, partial transfer and volume recovery.

See [Security and threat model](../../docs/SECURITY.md) for the remaining anonymity, Sybil-resistance, attestation and data-plane limitations.
