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
- All hApp and DNA manifests use manifest version `0` and `path` bundle references accepted by the pinned CLI.
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

The migration intentionally changes the network seeds to `*-hc07-v2`; old and new peers cannot share one DHT.

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

Pack-validation must be performed with `hc` 0.7.0. An older locally installed CLI is not a valid test.

## Upstream references

- [Official Holochain 0.7 upgrade guide](https://developer.holochain.org/resources/upgrade/upgrade-holochain-0.7/)
- [Official compatibility matrix](https://developer.holochain.org/resources/compatibility/holochain-0.7/)
- [Holochain releases](https://github.com/holochain/holochain/releases)
