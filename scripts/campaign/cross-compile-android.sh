#!/usr/bin/env bash
#
# Compilation croisée de `quic-spike` pour Android (aarch64) — campagne CH3/S4.
#
# Produit dist/android/quic-spike (strippé) + dist/android/SHA256SUMS.
# Nécessite le NDK Android (r23 ou plus récent) et la cible Rust
# aarch64-linux-android (installée automatiquement par ce script).
#
# Le NDK est recherché dans cet ordre :
#   $ANDROID_NDK_HOME | $ANDROID_NDK_ROOT | $ANDROID_HOME/ndk/<plus récent>
#   | $LOCALAPPDATA/Android/Sdk/ndk/<plus récent> (Windows)
#
# Aucune modification de .cargo/config.toml : le linker et les outils C sont
# passés par variables d'environnement uniquement.
#
# Usage :
#   bash scripts/campaign/cross-compile-android.sh
#   API=26 bash scripts/campaign/cross-compile-android.sh   # niveau API min
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
API="${API:-24}"

# ── Détection du NDK ─────────────────────────────────────────────────────────
ndk_from_dir() { # <répertoire contenant des sous-dossiers ndk>
  local base="$1" cand best=""
  [ -d "$base/ndk" ] || return 1
  for cand in "$base"/ndk/*; do
    [ -d "$cand" ] || continue
    best="$cand"   # itération dans l'ordre : le dernier (plus récent) gagne
  done
  [ -n "$best" ] && printf '%s\n' "$best"
}

NDK="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}"
if [ -z "$NDK" ]; then
  NDK="$(ndk_from_dir "${ANDROID_HOME:-}" || true)"
fi
if [ -z "$NDK" ] && [ -n "${LOCALAPPDATA:-}" ]; then
  NDK="$(ndk_from_dir "$LOCALAPPDATA/Android/Sdk" || true)"
fi
if [ -z "$NDK" ] && [ -d "$HOME/Android/Sdk" ]; then
  NDK="$(ndk_from_dir "$HOME/Android/Sdk" || true)"
fi
if [ -z "$NDK" ]; then
  echo "✗ NDK Android introuvable." >&2
  echo "  Installe-le (sdkmanager \"ndk;latest\") puis définis ANDROID_NDK_HOME." >&2
  exit 1
fi

case "$(uname -s)" in
  Linux*)          HOST_TAG=linux-x86_64 ;;
  Darwin*)         HOST_TAG=darwin-x86_64 ;;
  MINGW*|MSYS*|CYGWIN*) HOST_TAG=windows-x86_64 ;;
  *) echo "✗ Hôte non supporté : $(uname -s)"; exit 1 ;;
esac

TOOLCHAIN="$NDK/toolchains/llvm/prebuilt/$HOST_TAG"
CLANG="$TOOLCHAIN/bin/aarch64-linux-android${API}-clang"
if [ ! -e "$CLANG" ] && [ ! -e "$CLANG.exe" ]; then
  echo "✗ Clang NDK introuvable : $CLANG" >&2
  echo "  NDK détecté : $NDK — vérifie la version (r23+ requis) et API=$API." >&2
  exit 1
fi

echo "▶ NDK     : $NDK"
echo "▶ Hôte    : $HOST_TAG   API min : $API"

# ── Compilation ──────────────────────────────────────────────────────────────
cd "$ROOT"
rustup target add aarch64-linux-android

export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CLANG"
export CC_aarch64_linux_android="$CLANG"
export AR_aarch64_linux_android="$TOOLCHAIN/bin/llvm-ar"

echo "▶ cargo build --release -p relay-quic-spike --target aarch64-linux-android"
cargo build --release -p relay-quic-spike --target aarch64-linux-android

# ── Conditionnement ──────────────────────────────────────────────────────────
DIST="$ROOT/dist/android"
mkdir -p "$DIST"
cp "target/aarch64-linux-android/release/quic-spike" "$DIST/quic-spike"
"$TOOLCHAIN/bin/llvm-strip" "$DIST/quic-spike"

( cd "$DIST" && sha256sum quic-spike > SHA256SUMS )

SIZE=$(wc -c < "$DIST/quic-spike" | tr -d ' ')
echo "✓ dist/android/quic-spike ($SIZE octets) + dist/android/SHA256SUMS"
echo "  Prochaine étape : bash scripts/campaign/android/prepare-device.sh"
