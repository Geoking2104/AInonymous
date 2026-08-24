# Holochain 0.7 Migration

## Result

The repository targets Holochain 0.7.0 end to end. Native clients, zomes, manifests, conductor examples, CI and documentation use one compatible version family.

| Layer | Version |
| --- | --- |
| Holochain / `hc` | 0.7.0 |
| `holochain_client` | 0.9.0 |
| HDK | 0.7.0 |
| HDI | 0.8.0 |
| Lair | 0.7.1 |

## Code changes

- Integrity callbacks use the Holochain 0.7 operation variants such as `FlatOp::CreateEntry`.
- Create validation uses typed actions (`TypedAction<CreateData>`).
- Link queries use the 0.7 `LinkQuery`/`GetStrategy` API.
- The native client uses the 0.9 admin/app WebSocket types.
- Both WASM workspaces select HDK's Holochain-compatible custom `getrandom` backend.
- App installation supplies membrane proofs through per-role `RoleSettings::Provisioned`.
- HybridNode obtains its real agent identity from an authenticated app WebSocket instead of deriving or exporting a private key.
- All hApp and DNA manifests use manifest version `0` and `path` bundle references accepted by `hc` 0.7.0.
- Conductor examples use the 0.7 Iroh-only network configuration.

## Security corrections made during migration

- Private membership validation no longer disappears when a Cargo feature is omitted.
- Membrane proofs are bound to both the joining agent and a DNA `network_id`.
- The QUIC public key announced by a node is checked against the Holochain caller's authored capability entry.
- Agent discovery links are validated against their base, target entry and action author to prevent index poisoning.
- Explicit conductor failures no longer fall back to static peers.
- Malformed or unreadable membrane-proof configuration is an error.
- The JSON Schema no longer advertises a runtime `network_seed` setting that the Rust configuration ignores; network seeds belong to hApp/DNA manifests.

## Breaking operational changes

Holochain 0.7 does not migrate a 0.6 conductor database. Before upgrading:

1. Export any application data that must be preserved through application-level APIs.
2. Stop the 0.6 conductor.
3. Preserve the old data directory as a read-only backup.
4. Create a new 0.7 data root or run the appropriate sandbox clean command for disposable environments.
5. Pack the migrated DNAs with `hc` 0.7.0.
6. Install fresh hApps and generate fresh app authentication tokens.
7. Join every peer using the new bundle and network seed.

The deployment epoch intentionally rotates the network identifiers to:

- `ainonymous-core-hc07-v3-20260823` for inference mesh, agent registry and blackboard;
- `ainonymous-hybridnode-hc07-v3-20260823` with network ID `ainonymous-hybridnode-public-v3` for HybridNode.

The seeds are present in both the DNA work manifests and the hApp modifiers. This makes the packed DNA hash auditable before installation. Old and new peers cannot share one DHT.

The Compose deployment uses project name `ainonymous-hc07-v3` and volume `holochain-data-hc07-v3`. It never mounts a 0.6 or earlier 0.7 epoch database into the new conductor. Run one of the guarded `reprovision-containers` scripts to rebuild bundles, display their effective hashes, remove the old v3 local volumes and install fresh cells.

## Verification gates

```bash
cargo check --workspace
cargo test --workspace
cargo build --manifest-path dnas/ainonymous-core/Cargo.toml \
  --release --target wasm32-unknown-unknown
cargo build --manifest-path dnas/hybridnode/Cargo.toml \
  --release --target wasm32-unknown-unknown
python scripts/hybridnode/validate_config.py \
  hybridnode/configs/ainonymous.hybridnode.yaml \
  hybridnode/configs/generic-project.hybridnode.yaml
```

Pack-validation must be performed with `hc` 0.7.0. `scripts/build-happ.sh` accepts `HC_BIN=/absolute/path/to/hc` and checks its version, preventing an older global CLI from silently producing incompatible bundles. The `dna-hashes` utility decodes the result with `holochain_types` 0.7.0 and prints the effective hashes.

## Upstream references

- [Official Holochain 0.7 upgrade guide](https://developer.holochain.org/resources/upgrade/upgrade-holochain-0.7/)
- [Official compatibility matrix](https://developer.holochain.org/resources/compatibility/holochain-0.7/)
- [Holochain releases](https://github.com/holochain/holochain/releases)
