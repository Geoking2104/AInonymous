#!/usr/bin/env bash
# Rebuild Holochain 0.7 bundles and replace the versioned local container stack.
set -euo pipefail

if [[ "${1:-}" != "--confirm-reset" ]]; then
    echo "Usage: $0 --confirm-reset" >&2
    echo "This removes only Docker Compose project ainonymous-hc07-v3 and its named volumes." >&2
    exit 2
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PROJECT_NAME=ainonymous-hc07-v3
HC_BIN="${HC_BIN:-${HC:-hc}}"
SECRET_DIR="$PROJECT_ROOT/deploy/secrets"
SECRET_FILE="$SECRET_DIR/holochain_keystore_password"

for command_name in cargo docker openssl; do
    command -v "$command_name" >/dev/null 2>&1 || {
        echo "ERROR: $command_name is required." >&2
        exit 1
    }
done

HC_VERSION="$("$HC_BIN" --version 2>&1)"
[[ "$HC_VERSION" == *"0.7.0"* ]] || {
    echo "ERROR: HC_BIN must point to hc 0.7.0; found: $HC_VERSION" >&2
    exit 1
}

mkdir -p "$SECRET_DIR"
if [[ ! -s "$SECRET_FILE" ]]; then
    umask 077
    openssl rand -base64 48 > "$SECRET_FILE"
fi

cd "$PROJECT_ROOT"
HC_BIN="$HC_BIN" "$SCRIPT_DIR/build-happ.sh" release
cargo run --locked -p dna-hashes -- \
    dnas/ainonymous-core/dnas/inference-mesh/workdir/inference-mesh.dna \
    dnas/ainonymous-core/dnas/agent-registry/workdir/agent-registry.dna \
    dnas/ainonymous-core/dnas/blackboard/workdir/blackboard.dna \
    dnas/hybridnode/dnas/hybridnode-core/workdir/hybridnode-core.dna

docker compose --project-name "$PROJECT_NAME" config --quiet
docker compose --project-name "$PROJECT_NAME" down --volumes --remove-orphans
docker compose --project-name "$PROJECT_NAME" build --pull --no-cache
docker compose --project-name "$PROJECT_NAME" up --detach --wait
docker compose --project-name "$PROJECT_NAME" ps
