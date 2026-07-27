#!/usr/bin/env bash
# Build automatise du binaire pipeline_server natif (llama.cpp patche, Gemma3
# Dense, pipeline-split) -- remplace la sequence manuelle documentee dans
# README.md ("Comment reproduire" etapes 1-3 + 7). Idempotent : peut etre
# relance sans tout re-telecharger/recompiler si WORKDIR existe deja avec le
# bon commit et le patch deja applique (cmake/g++ ne refont que ce qui a
# change). Utiliser FORCE_CLEAN=1 pour repartir de zero.
#
# Usage :
#   bash build_native.sh
#   WORKDIR=/tmp/llama.cpp OUT_BIN=/tmp/pipeline_server bash build_native.sh
#   FORCE_CLEAN=1 bash build_native.sh
#
# Variables d'environnement :
#   WORKDIR       - ou cloner/construire llama.cpp (defaut: /tmp/llama.cpp)
#   OUT_BIN       - chemin du binaire final (defaut: $WORKDIR/pipeline_server)
#   JOBS          - parallelisme cmake --build (defaut: nproc)
#   FORCE_CLEAN   - si "1", supprime WORKDIR et repart de zero
#
# Prerequis : git, cmake, g++ (C++17) sur le PATH.
#
# Sortie : le binaire pipeline_server pret a l'emploi (voir README.md pour les
# flags --model/--layer-start/--layer-end/--is-first-node/--is-last-node).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LLAMA_CPP_REPO="https://github.com/ggml-org/llama.cpp.git"
LLAMA_CPP_COMMIT="42fc243060709331ff9b158a9ed2cbe37219ae83"
WORKDIR="${WORKDIR:-/tmp/llama.cpp}"
OUT_BIN="${OUT_BIN:-$WORKDIR/pipeline_server}"
JOBS="${JOBS:-$(command -v nproc >/dev/null 2>&1 && nproc || echo 4)}"

log() { echo ">>> $*"; }

if [ "${FORCE_CLEAN:-0}" = "1" ] && [ -d "$WORKDIR" ]; then
    log "FORCE_CLEAN=1 : suppression de $WORKDIR"
    rm -rf "$WORKDIR"
fi

# ---------------------------------------------------------------------------
# 1. Clone / checkout au commit pin (idempotent)
# ---------------------------------------------------------------------------
if [ ! -d "$WORKDIR/.git" ]; then
    # --filter=blob:none : clone "partiel", ne telecharge les blobs (contenu des
    # fichiers) qu'a la demande lors du checkout -- beaucoup plus rapide qu'un
    # clone complet puisqu'on n'a besoin que d'un seul commit, pas de tout
    # l'historique. Reste un repo git normal (HEAD, commits, etc. fonctionnent).
    log "Clonage (partiel, blob:none) de llama.cpp dans $WORKDIR"
    git clone --filter=blob:none "$LLAMA_CPP_REPO" "$WORKDIR"
fi

cd "$WORKDIR"
CURRENT_COMMIT="$(git rev-parse HEAD 2>/dev/null || echo "")"
if [ "$CURRENT_COMMIT" != "$LLAMA_CPP_COMMIT" ]; then
    log "Checkout du commit pin $LLAMA_CPP_COMMIT (actuel: ${CURRENT_COMMIT:-aucun})"
    git fetch --filter=blob:none origin "$LLAMA_CPP_COMMIT" 2>/dev/null || true
    git checkout "$LLAMA_CPP_COMMIT"
fi

# ---------------------------------------------------------------------------
# 2. Appliquer le patch pipeline-split (idempotent : verifie une marque du
#    patch avant de l'appliquer, pour ne pas planter sur un re-run)
# ---------------------------------------------------------------------------
PATCH_FILE="$SCRIPT_DIR/0001-pipeline-split-poc.patch"
if grep -q "pipeline_layer_end" src/llama-cparams.h 2>/dev/null; then
    log "Patch pipeline-split deja applique, on passe."
else
    log "Application du patch pipeline-split ($PATCH_FILE)"
    git apply --check "$PATCH_FILE"
    git apply "$PATCH_FILE"
fi

# ---------------------------------------------------------------------------
# 3. Verifier les dependances vendorees (deja presentes dans un clone complet
#    upstream, mais on echoue proprement sinon plutot que de planter a la
#    compilation avec une erreur cryptique)
# ---------------------------------------------------------------------------
for f in vendor/cpp-httplib/httplib.h vendor/cpp-httplib/httplib.cpp vendor/nlohmann/json.hpp; do
    if [ ! -f "$f" ]; then
        echo "ERREUR: $f manquant -- clone incomplet/sparse ? Refaire un clone complet (pas de --depth/--filter)." >&2
        exit 1
    fi
done

# ---------------------------------------------------------------------------
# 4. Configurer + builder llama.cpp (statique, CPU-only, cibles inutiles OFF)
# ---------------------------------------------------------------------------
NEED_CONFIGURE=0
if [ ! -f build/CMakeCache.txt ]; then
    NEED_CONFIGURE=1
elif ! grep -q "BUILD_SHARED_LIBS:BOOL=OFF" build/CMakeCache.txt 2>/dev/null; then
    log "Config cmake existante incompatible (shared libs), reconfiguration"
    rm -rf build
    NEED_CONFIGURE=1
fi

if [ "$NEED_CONFIGURE" = "1" ]; then
    log "Configuration cmake (build/)"
    cmake -B build -DBUILD_SHARED_LIBS=OFF -DLLAMA_BUILD_COMMON=OFF -DLLAMA_BUILD_TESTS=OFF \
        -DLLAMA_BUILD_TOOLS=OFF -DLLAMA_BUILD_EXAMPLES=OFF -DLLAMA_BUILD_SERVER=OFF \
        -DLLAMA_BUILD_APP=OFF -DLLAMA_BUILD_MTMD=OFF
fi

log "Compilation llama.cpp (jobs=$JOBS) -- peut prendre plusieurs minutes la premiere fois"
cmake --build build -j"$JOBS"

# ---------------------------------------------------------------------------
# 5. Compiler pipeline_server.cpp (toujours refait : source rapide a compiler,
#    et on veut prendre en compte un pipeline_server.cpp modifie depuis)
# ---------------------------------------------------------------------------
log "Compilation de pipeline_server -> $OUT_BIN"
g++ -std=c++17 -O2 -pthread \
    -I include -I ggml/include -I src -I vendor/cpp-httplib -I vendor/nlohmann \
    "$SCRIPT_DIR/pipeline_server.cpp" vendor/cpp-httplib/httplib.cpp \
    build/src/libllama.a build/ggml/src/libggml.a build/ggml/src/libggml-cpu.a build/ggml/src/libggml-base.a \
    -fopenmp -lpthread -ldl -lm -o "$OUT_BIN"

log "OK -- binaire pret : $OUT_BIN"
log "Exemple : $OUT_BIN --model /chemin/vers/modele.gguf --layer-start 0 --is-last-node --port 9340"
