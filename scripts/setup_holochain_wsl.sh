#!/usr/bin/env bash
# Install the exact Holochain toolchain used by this repository on x86_64 Linux/WSL2.
set -euo pipefail

HOLOCHAIN_VERSION="0.7.0"
RELEASE_TAG="holochain-${HOLOCHAIN_VERSION}"
BASE_URL="https://github.com/holochain/holochain/releases/download/${RELEASE_TAG}"
ARCH="x86_64-unknown-linux-gnu"
INSTALL_DIR="${HOME}/.local/bin"

declare -A SHA256=(
    [hc]="f1eca56b97bc2261324e00e0e86a274f7dc8363f73264e697fd0c0216b2aac23"
    [holochain]="ffa40a0c6fab5ce062c4af76328dfe2de143256ddf791a504d72bca698a9ba20"
    [lair-keystore]="7a77822ab5e0020d0f3c358030d4ccfa8c6c144407a5c075d302c7b0fcf670c1"
)

command -v curl >/dev/null 2>&1 || { echo "ERROR: curl is required" >&2; exit 1; }
command -v sha256sum >/dev/null 2>&1 || { echo "ERROR: sha256sum is required" >&2; exit 1; }

mkdir -p "${INSTALL_DIR}"
DOWNLOAD_DIR="$(mktemp -d)"
cleanup() { rm -rf "${DOWNLOAD_DIR}"; }
trap cleanup EXIT

echo "Installing Holochain ${HOLOCHAIN_VERSION} from ${RELEASE_TAG}"
for bin in holochain hc lair-keystore; do
    asset="${bin}-${ARCH}"
    downloaded="${DOWNLOAD_DIR}/${asset}"
    echo "Downloading ${asset}"
    curl --fail --location --silent --show-error "${BASE_URL}/${asset}" --output "${downloaded}"
    printf '%s  %s\n' "${SHA256[$bin]}" "${downloaded}" | sha256sum --check --status || {
        echo "ERROR: SHA-256 verification failed for ${asset}" >&2
        exit 1
    }
    chmod 0755 "${downloaded}"
    install -m 0755 "${downloaded}" "${INSTALL_DIR}/${bin}"
done

export PATH="${INSTALL_DIR}:${PATH}"
grep -Fq '${HOME}/.local/bin' "${HOME}/.bashrc" 2>/dev/null || \
    printf '\nexport PATH="${HOME}/.local/bin:${PATH}"\n' >> "${HOME}/.bashrc"

[[ "$(holochain --version 2>&1)" == *"${HOLOCHAIN_VERSION}"* ]] || {
    echo "ERROR: unexpected holochain version" >&2
    exit 1
}
[[ "$(hc --version 2>&1)" == *"${HOLOCHAIN_VERSION}"* ]] || {
    echo "ERROR: unexpected hc version" >&2
    exit 1
}

holochain --version
hc --version
lair-keystore --version
echo "Installation complete. Open a new shell or keep ${INSTALL_DIR} on PATH."
