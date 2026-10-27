#!/usr/bin/env bash
# Compilation croisée de la sonde pour aarch64-linux-android avec le NDK seul
# (sans Nix). Usage : ANDROID_NDK_HOME=... bash build-android.sh
set -euo pipefail
NDK="${ANDROID_NDK_HOME:-${NDK_HOME:?NDK_HOME ou ANDROID_NDK_HOME requis}}"
API="${ANDROID_API:-24}"
TC="$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin"
export CC_aarch64_linux_android="$TC/aarch64-linux-android$API-clang"
export CXX_aarch64_linux_android="$TC/aarch64-linux-android$API-clang++"
export AR_aarch64_linux_android="$TC/llvm-ar"
# OpenSSL vendu (via libsqlite3-sys/sqlcipher) appelle « aarch64-linux-android-ranlib »,
# absent du NDK r27 (seul llvm-ranlib existe) : on fournit l'alias.
SHIM="$(mktemp -d)"; ln -s "$TC/llvm-ranlib" "$SHIM/aarch64-linux-android-ranlib"
export PATH="$SHIM:$TC:$PATH" RANLIB_aarch64_linux_android="$TC/llvm-ranlib"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC_aarch64_linux_android"
cd "$(dirname "$0")"
cargo build --release --target aarch64-linux-android "$@"
