#!/usr/bin/env bash
# init_project.sh — Bootstrap a new project using the HybridNode architecture
#
# Usage:
#   bash scripts/hybridnode/init_project.sh <project-name> [--private-network]
#
# This script:
#   1. Creates the hybridnode config directory for the project
#   2. Copies the generic template and pre-fills the project name
#   3. Optionally enables private-network mode
#   4. Validates the resulting config against the schema

set -euo pipefail

PROJECT="${1:-}"
PRIVATE_NETWORK=false

for arg in "$@"; do
    case $arg in
        --private-network) PRIVATE_NETWORK=true ;;
    esac
done

if [[ -z "$PROJECT" ]]; then
    echo "Usage: $0 <project-name> [--private-network]"
    exit 1
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TEMPLATE="$REPO_ROOT/hybridnode/configs/generic-project.hybridnode.yaml"
DEST="$REPO_ROOT/hybridnode/configs/${PROJECT}.hybridnode.yaml"

if [[ -f "$DEST" ]]; then
    echo "ERROR: Config already exists: $DEST"
    exit 1
fi

echo "→ Creating HybridNode config for project: $PROJECT"
cp "$TEMPLATE" "$DEST"

# Inject project name into otel_service_name
sed -i "s/otel_service_name: .*/otel_service_name: \"$PROJECT\"/" "$DEST" 2>/dev/null || true

# Enable private-network if requested
if [[ "$PRIVATE_NETWORK" == "true" ]]; then
    sed -i "s/private_network: false/private_network: true/" "$DEST" 2>/dev/null || true
    echo "  ✓ Private network mode enabled"
fi

echo "  ✓ Config created: $DEST"

# Validate against schema
if command -v python3 &>/dev/null; then
    echo "→ Validating config..."
    python3 "$REPO_ROOT/scripts/hybridnode/validate_config.py" "$DEST"
else
    echo "  ⚠ python3 not found — skipping validation"
fi

echo ""
echo "Next steps:"
echo "  1. Edit $DEST — fill in your SD-WAN controller URL and bootstrap URL"
echo "  2. Set environment variables: SDWAN_API_TOKEN, SDWAN_SITE_ID"
echo "  3. Run: hybridnode --config $DEST"
