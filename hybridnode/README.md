# HybridNode Configuration Kit

This directory contains the reusable configuration contract for the Holochain 0.7 HybridNode layer.

```text
configs/    validated development and project templates
policies/   scheduling, security and observability policy examples
schemas/    JSON Schema for HybridNode YAML
specs/      human-readable configuration specification
```

Validate a configuration with both JSON Schema and cross-field security checks:

```bash
python scripts/hybridnode/validate_config.py hybridnode/configs/ainonymous.hybridnode.yaml
```

Add `--production` to reject mock SD-WAN, public trust mode and exposed metrics in the current baseline. Holochain network seeds belong to the hApp/DNA manifests, not this runtime YAML. Bootstrap and relay behavior is ultimately controlled by the conductor configuration.

See [HybridNode integration](../HYBRIDNODE_APPLY.md) and [security](../docs/SECURITY.md).
