#!/usr/bin/env bash
# Build script — compile tous les zomes Holochain en WASM et package le hApp
# Prérequis: cargo, cargo-nextest, holochain (hc), wasm-pack, hc-scaffold
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DNAS_ROOT="$PROJECT_ROOT/dnas/ainonymous-core"

TARGET_WASM="wasm32-unknown-unknown"
BUILD_MODE="${1:-release}"
CARGO_FLAGS=""
if [[ "$BUILD_MODE" == "release" ]]; then
    CARGO_FLAGS="--release"
fi

echo "==> Build zomes ($BUILD_MODE)"
cd "$DNAS_ROOT"
cargo build $CARGO_FLAGS --target "$TARGET_WASM"

WASM_OUT="$DNAS_ROOT/target/$TARGET_WASM/$BUILD_MODE"

echo "==> Copier les WASMs dans les workdirs"

# inference-mesh
INFER_DIR="$DNAS_ROOT/dnas/inference-mesh/zomes"
mkdir -p "$INFER_DIR"
cp "$WASM_OUT/inference_mesh_integrity.wasm"    "$INFER_DIR/inference-mesh-integrity.wasm"
cp "$WASM_OUT/inference_mesh_coordinator.wasm"  "$INFER_DIR/inference-mesh-coordinator.wasm"

# agent-registry
AGENT_DIR="$DNAS_ROOT/dnas/agent-registry/zomes"
mkdir -p "$AGENT_DIR"
cp "$WASM_OUT/agent_registry_integrity.wasm"    "$AGENT_DIR/agent-registry-integrity.wasm"
cp "$WASM_OUT/agent_registry_coordinator.wasm"  "$AGENT_DIR/agent-registry-coordinator.wasm"

# blackboard
BB_DIR="$DNAS_ROOT/dnas/blackboard/zomes"
mkdir -p "$BB_DIR"
cp "$WASM_OUT/blackboard_integrity.wasm"        "$BB_DIR/blackboard-integrity.wasm"
cp "$WASM_OUT/blackboard_coordinator.wasm"      "$BB_DIR/blackboard-coordinator.wasm"

echo "==> Packager les DNAs"
hc dna pack "$DNAS_ROOT/dnas/inference-mesh/workdir"
hc dna pack "$DNAS_ROOT/dnas/agent-registry/workdir"
hc dna pack "$DNAS_ROOT/dnas/blackboard/workdir"

echo "==> Packager le hApp"
hc app pack "$DNAS_ROOT"

echo ""
echo "✓ hApp packagé : $DNAS_ROOT/ainonymous-core.happ"
