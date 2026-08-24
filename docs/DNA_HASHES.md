# DNA Hash Registry

DNA epoch `v3-20260823` is the canonical Holochain 0.7.0 deployment epoch for this repository. These values were produced on the Linux GitHub Actions runner by `hc` 0.7.0 and independently decoded by the repository's `dna-hashes` utility using `holochain_types` 0.7.0.

| DNA | Network seed | Effective hash |
| --- | --- | --- |
| inference-mesh | `ainonymous-core-hc07-v3-20260823` | `uhC0kYFyQtLQaaKs--WrpAQdNh44E0SXDX8Kk409Tan7Us4QLU5Q_` |
| agent-registry | `ainonymous-core-hc07-v3-20260823` | `uhC0kF5tmjLHIMwemHJ7efEd-HlZxl2P77_LJoQM_PYmeYkGft9Ap` |
| blackboard | `ainonymous-core-hc07-v3-20260823` | `uhC0k7wOBmtXg_cY2UwORA_Q5yG37Wmc551T582wgG1nlFWezcZuB` |
| hybridnode-core | `ainonymous-hybridnode-hc07-v3-20260823` | `uhC0k-sq0wS1opMFIrNzZKb2r94Uynr84xdUWg0Y9ix3WJc9zDLNw` |

The machine-readable source is [`deploy/holochain/dna-hashes.json`](../deploy/holochain/dna-hashes.json). Each hApp role also pins its `installed_hash`, so installation fails instead of silently joining a network produced by different WASM, properties or modifiers.

## Verification

After packaging with `hc` 0.7.0, run:

```bash
cargo run --locked -p dna-hashes -- --check deploy/holochain/dna-hashes.json \
  dnas/ainonymous-core/dnas/inference-mesh/workdir/inference-mesh.dna \
  dnas/ainonymous-core/dnas/agent-registry/workdir/agent-registry.dna \
  dnas/ainonymous-core/dnas/blackboard/workdir/blackboard.dna \
  dnas/hybridnode/dnas/hybridnode-core/workdir/hybridnode-core.dna
```

Any intentional change to an integrity zome, DNA properties, seed or manifest modifiers requires a new epoch. Rebuild with the pinned CLI, record all new hashes together, update the hApp `installed_hash` values, use a new conductor-volume suffix and reinstall every peer from fresh state.
