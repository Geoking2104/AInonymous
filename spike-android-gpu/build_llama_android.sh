#!/usr/bin/env bash
# S2 (#28) — compilation croisée de llama.cpp pour Android arm64-v8a, en deux variantes :
# Vulkan (GGML_VULKAN=ON) et CPU (repli). Produit llama-bench, llama-cli et libllama.so.
#
#   ANDROID_NDK_HOME=~/android/sdk/ndk/27.2.12479018 bash build_llama_android.sh [vulkan|cpu]
#
# Le commit est le même que celui du patch pipeline-split : le binaire mesuré sera donc
# celui qui servira au relay.
set -euo pipefail
VARIANT="${1:-vulkan}"
NDK="${ANDROID_NDK_HOME:?ANDROID_NDK_HOME requis}"
API="${ANDROID_API:-28}"   # Vulkan 1.1 garanti à partir d'Android 9 (API 28)
COMMIT="${LLAMA_CPP_COMMIT:-42fc243060709331ff9b158a9ed2cbe37219ae83}"
SRC="${SRC:-/tmp/llama.cpp-android}"
OUT="${OUT:-$(pwd)/out/$VARIANT}"
if [ ! -d "$SRC/.git" ]; then git clone --filter=blob:none https://github.com/ggml-org/llama.cpp.git "$SRC"; fi
git -C "$SRC" fetch -q --filter=blob:none origin "$COMMIT" || true
git -C "$SRC" checkout -q "$COMMIT"
EXTRA=()
if [ "$VARIANT" = vulkan ]; then
  # glslc est fourni par le NDK (shader-tools). Les en-têtes C++ Vulkan (vulkan.hpp)
  # ne le sont pas : il faut fournir VULKAN_HPP_INCLUDE (Vulkan-Headers).
  GLSLC="$(ls "$NDK"/shader-tools/*/glslc | head -1)"
  EXTRA+=(-DGGML_VULKAN=ON -DVulkan_GLSLC_EXECUTABLE="$GLSLC")
  if [ -n "${VULKAN_HPP_INCLUDE:-}" ]; then EXTRA+=(-DCMAKE_CXX_FLAGS="-I$VULKAN_HPP_INCLUDE"); fi
fi
cmake -S "$SRC" -B "$OUT/build" -G "Unix Makefiles" \
  -DCMAKE_TOOLCHAIN_FILE="$NDK/build/cmake/android.toolchain.cmake" \
  -DANDROID_ABI=arm64-v8a -DANDROID_PLATFORM="android-$API" \
  -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=ON -DLLAMA_CURL=OFF \
  -DGGML_OPENMP=OFF -DGGML_NATIVE=OFF -DLLAMA_BUILD_SERVER=OFF "${EXTRA[@]}"
cmake --build "$OUT/build" -j"$(nproc)" --target llama-bench llama-cli
mkdir -p "$OUT/bin"
find "$OUT/build/bin" -maxdepth 1 \( -name 'llama-bench' -o -name 'llama-cli' -o -name '*.so' \) -exec cp {} "$OUT/bin/" \;
ls -l "$OUT/bin"
