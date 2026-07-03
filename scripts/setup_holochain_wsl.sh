#!/usr/bin/env bash
# T5.1 — Installer Holochain 0.6.2 + lair-keystore dans WSL2 (Ubuntu x86_64)
#
# Usage : bash scripts/setup_holochain_wsl.sh
# Prérequis : WSL2 Ubuntu (wsl --install Ubuntu depuis PowerShell)
#
# Ce script est conçu pour tourner DANS WSL2 (pas depuis PowerShell).
# Depuis PowerShell : wsl bash scripts/setup_holochain_wsl.sh
set -euo pipefail

HOLOCHAIN_VERSION="0.6.2"
RELEASE_TAG="holochain-${HOLOCHAIN_VERSION}"
BASE_URL="https://github.com/holochain/holochain/releases/download/${RELEASE_TAG}"
ARCH="x86_64-unknown-linux-gnu"
INSTALL_DIR="${HOME}/.local/bin"

echo "=== Holochain ${HOLOCHAIN_VERSION} installer (WSL2 Ubuntu) ==="

# Créer le répertoire d'installation
mkdir -p "${INSTALL_DIR}"

# Vérifier si PATH contient déjà ~/.local/bin
if [[ ":${PATH}:" != *":${INSTALL_DIR}:"* ]]; then
    echo "export PATH=\"\${HOME}/.local/bin:\${PATH}\"" >> ~/.bashrc
    export PATH="${INSTALL_DIR}:${PATH}"
    echo "→ ${INSTALL_DIR} ajouté au PATH"
fi

# Installer les binaires si pas déjà présents
for BIN in holochain hc lair-keystore; do
    TARGET="${INSTALL_DIR}/${BIN}"
    if [[ -f "${TARGET}" ]]; then
        echo "✓ ${BIN} déjà installé ($(${TARGET} --version 2>&1 | head -1))"
        continue
    fi
    echo "→ Téléchargement ${BIN}..."
    curl -fsSL "${BASE_URL}/${BIN}-${ARCH}" -o "${TARGET}"
    chmod +x "${TARGET}"
    echo "✓ ${BIN} installé"
done

# Vérifications
echo ""
echo "=== Versions installées ==="
holochain --version
hc --version
lair-keystore --version

echo ""
echo "=== Installation terminée ==="
echo "Lancez le test : bash scripts/run_t51_signal_test.sh"
