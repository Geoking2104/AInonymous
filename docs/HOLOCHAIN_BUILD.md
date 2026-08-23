# Holochain Build and Operations

## Required toolchain

Use Holochain and `hc` 0.7.0 with Lair 0.7.1. The Rust crates are pinned separately: HDK 0.7.0, HDI 0.8.0 and `holochain_client` 0.9.0.

```bash
rustup target add wasm32-unknown-unknown
hc --version          # must report 0.7.0
holochain --version   # must report 0.7.0
```

## Build and package

The repository contains two independent zome workspaces. The packaging script builds both and copies the resulting WASM modules into each DNA work directory before invoking `hc dna pack` and `hc app pack`.

```bash
bash scripts/build-happ.sh release
```

Expected outputs:

```text
dnas/ainonymous-core/ainonymous-core.happ
dnas/hybridnode/hybridnode.happ
```

To compile without packaging:

```bash
cargo build --manifest-path dnas/ainonymous-core/Cargo.toml \
  --release --target wasm32-unknown-unknown
cargo build --manifest-path dnas/hybridnode/Cargo.toml \
  --release --target wasm32-unknown-unknown
```

## Conductor 0.7 baseline

Use a fresh data root and keep the admin interface on loopback. `scripts/testnet/conductor_t51.yaml` is the development baseline. Its `keystore.type: danger_test_keystore` and disabled durable sync are for disposable testnets only.

For a persistent deployment:

- configure a Lair keystore;
- use `db_sync_level: Normal`;
- set the intended private bootstrap and relay URLs;
- protect bootstrap/relay auth material as secrets;
- bind admin WebSockets to `127.0.0.1` or an isolated management namespace;
- expose an app interface only where required.

Holochain 0.7 uses Iroh as its network transport. Do not copy obsolete `transport_pool`, WebRTC or tx5 fields into the conductor configuration; unknown configuration fields are rejected.

## Install

Install with the 0.7 admin API or CLI. The app IDs configured in the daemons must match the installed IDs. A private role needs its membrane proof at install time through `RoleSettings::Provisioned`.

The daemon helper `install_app_with_membrane_proof` accepts:

- the app bundle path;
- the target app ID;
- the role name that requires admission;
- the serialized proof bytes.

It installs and enables the app. Ordinary zome calls never carry membrane proofs.

## Application connection

The native clients:

1. connect to the loopback admin WebSocket;
2. issue an app authentication token;
3. connect to the app WebSocket with a `ClientAgentSigner`;
4. select the provisioned cell and read its agent public key;
5. sign zome calls through the Holochain client.

Conductor mode fails closed. Use the explicit static backend only for isolated tests that do not claim Holochain identity.

## Upgrade from 0.6

Do not point Holochain 0.7 at a 0.6 database. Preserve the old directory, create a new data root, repack with `hc` 0.7.0 and reinstall all roles. See [Holochain 0.7 migration](HOLOCHAIN_0_7_MIGRATION.md).

## Troubleshooting

- **Manifest rejected:** confirm `hc --version`; this repository's manifests use version `0` and `path` references.
- **No cell found:** verify the configured app ID, that the app is enabled and that its role is provisioned.
- **Unauthorized app WebSocket:** issue a fresh auth token through the local admin interface.
- **Genesis rejected:** confirm the proof agent key, network ID, signature and the DNA properties used when packing.
- **Peers do not discover each other:** confirm identical DNA hashes, network seeds, bootstrap/relay endpoints and network auth material.
