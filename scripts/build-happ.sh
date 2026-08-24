#!/usr/bin/env bash
# Build every Holochain zome as WASM and package both hApps with hc 0.7.0.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
AIN_ROOT="$PROJECT_ROOT/dnas/ainonymous-core"
HYBRID_ROOT="$PROJECT_ROOT/dnas/hybridnode"
TARGET_WASM="wasm32-unknown-unknown"
BUILD_MODE="${1:-release}"
CARGO_BIN="${CARGO:-cargo}"
HC_BIN="${HC_BIN:-${HC:-hc}}"

if [[ "$BUILD_MODE" != "release" && "$BUILD_MODE" != "dev" ]]; then
    echo "Usage: $0 [release|dev]" >&2
    exit 2
fi

if ! command -v "$HC_BIN" >/dev/null 2>&1; then
    echo "ERROR: hc 0.7.0 is required to validate and package the hApps." >&2
    exit 1
fi

HC_VERSION="$("$HC_BIN" --version 2>&1)"
if [[ "$HC_VERSION" != *"0.7.0"* ]]; then
    echo "ERROR: expected hc 0.7.0, found: $HC_VERSION" >&2
    exit 1
fi

CARGO_FLAGS=()
if [[ "$BUILD_MODE" == "release" ]]; then
    CARGO_FLAGS+=(--release)
fi

echo "==> Building AInonymous zomes ($BUILD_MODE)"
"$CARGO_BIN" build --manifest-path "$AIN_ROOT/Cargo.toml" \
    "${CARGO_FLAGS[@]}" --target "$TARGET_WASM"

AIN_WASM="$AIN_ROOT/target/$TARGET_WASM/$BUILD_MODE"
mkdir -p \
    "$AIN_ROOT/dnas/inference-mesh/zomes" \
    "$AIN_ROOT/dnas/agent-registry/zomes" \
    "$AIN_ROOT/dnas/blackboard/zomes"
cp "$AIN_WASM/inference_mesh_integrity.wasm" \
    "$AIN_ROOT/dnas/inference-mesh/zomes/inference-mesh-integrity.wasm"
cp "$AIN_WASM/inference_mesh_coordinator.wasm" \
    "$AIN_ROOT/dnas/inference-mesh/zomes/inference-mesh-coordinator.wasm"
cp "$AIN_WASM/agent_registry_integrity.wasm" \
    "$AIN_ROOT/dnas/agent-registry/zomes/agent-registry-integrity.wasm"
cp "$AIN_WASM/agent_registry_coordinator.wasm" \
    "$AIN_ROOT/dnas/agent-registry/zomes/agent-registry-coordinator.wasm"
cp "$AIN_WASM/blackboard_integrity.wasm" \
    "$AIN_ROOT/dnas/blackboard/zomes/blackboard-integrity.wasm"
cp "$AIN_WASM/blackboard_coordinator.wasm" \
    "$AIN_ROOT/dnas/blackboard/zomes/blackboard-coordinator.wasm"

echo "==> Building HybridNode zomes ($BUILD_MODE)"
"$CARGO_BIN" build --manifest-path "$HYBRID_ROOT/Cargo.toml" \
    "${CARGO_FLAGS[@]}" --target "$TARGET_WASM"

HYBRID_WASM="$HYBRID_ROOT/target/$TARGET_WASM/$BUILD_MODE"
mkdir -p "$HYBRID_ROOT/dnas/hybridnode-core/zomes"
cp "$HYBRID_WASM/hybridnode_integrity.wasm" \
    "$HYBRID_ROOT/dnas/hybridnode-core/zomes/hybridnode-integrity.wasm"
cp "$HYBRID_WASM/hybridnode_coordinator.wasm" \
    "$HYBRID_ROOT/dnas/hybridnode-core/zomes/hybridnode-coordinator.wasm"

echo "==> Packing DNAs and hApps with $HC_VERSION"
"$HC_BIN" dna pack "$AIN_ROOT/dnas/inference-mesh/workdir"
"$HC_BIN" dna pack "$AIN_ROOT/dnas/agent-registry/workdir"
"$HC_BIN" dna pack "$AIN_ROOT/dnas/blackboard/workdir"
"$HC_BIN" app pack "$AIN_ROOT"

"$HC_BIN" dna pack "$HYBRID_ROOT/dnas/hybridnode-core/workdir"
"$HC_BIN" app pack "$HYBRID_ROOT"

echo "Built:"
ls -lh "$AIN_ROOT/ainonymous-core.happ" "$HYBRID_ROOT/hybridnode.happ"
