#!/usr/bin/env bash
# Spike S4 (#30) — harnais d'équivalence numérique « plage de couches » sur 2 étages,
# CPU, avec un modèle Gemma3 minuscule (poids aléatoires). Reproductible sur
# n'importe quelle machine Linux sans GPU.
#
#   bash spike-pipeline/run_equivalence.sh
#
# Variables : WORKDIR (llama.cpp patché, défaut /tmp/llama.cpp), MODEL_HF (dépôt HF
# source, défaut katuni4ka/tiny-random-gemma3-text), MODEL_GGUF (défaut
# /tmp/models/gemma3-tiny.gguf, chemin codé en dur dans pipeline_test*.cpp),
# PY (python avec torch/transformers/gguf/sentencepiece), OUT (dossier résultats).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
PATCH_DIR="$REPO/patches/llama-cpp-pipeline-split"
WORKDIR="${WORKDIR:-/tmp/llama.cpp}"
MODEL_HF="${MODEL_HF:-katuni4ka/tiny-random-gemma3-text}"
MODEL_DIR="${MODEL_DIR:-/tmp/models/$(basename "$MODEL_HF")}"
MODEL_GGUF="${MODEL_GGUF:-/tmp/models/gemma3-tiny.gguf}"
PY="${PY:-python3}"
OUT="${OUT:-$HERE/results}"
mkdir -p "$OUT" "$(dirname "$MODEL_GGUF")"

echo ">>> 1. llama.cpp patché (pipeline-split) + pipeline_server"
WORKDIR="$WORKDIR" bash "$PATCH_DIR/build_native.sh"

echo ">>> 2. Modèle minuscule -> GGUF F32"
if [ ! -f "$MODEL_GGUF" ]; then
  if [ ! -f "$MODEL_DIR/config.json" ]; then
    "$PY" -c "from huggingface_hub import snapshot_download as s; s('$MODEL_HF', local_dir='$MODEL_DIR')"
  fi
  "$PY" "$WORKDIR/convert_hf_to_gguf.py" "$MODEL_DIR" --outfile "$MODEL_GGUF" --outtype f32
fi
sha256sum "$MODEL_GGUF" | tee "$OUT/model.sha256"

echo ">>> 3. Tests en processus (pipeline_test, _test2, _test3_earlyexit)"
cd "$WORKDIR"
LIBS="build/src/libllama.a build/ggml/src/libggml.a build/ggml/src/libggml-cpu.a build/ggml/src/libggml-base.a"
rc=0
for t in pipeline_test pipeline_test2 pipeline_test3_earlyexit; do
  g++ -std=c++17 -O2 -pthread -I include -I ggml/include -I src "$PATCH_DIR/$t.cpp" $LIBS -fopenmp -ldl -lm -o "$WORKDIR/$t"
  if "./$t" > "$OUT/$t.log" 2>&1; then echo "  $t : OK"; else echo "  $t : ÉCHEC"; rc=1; fi
  grep -E 'RESULT|match|OVERALL|diff' "$OUT/$t.log" | sed 's/^/    /' || true
done

echo ">>> 4. Chaîne HTTP 2 processus (node0 -> node1) vs nœud unique"
BIN="$WORKDIR/pipeline_server"
"$BIN" --model "$MODEL_GGUF" --layer-start 0 --last --port 9100 > "$OUT/server_single.log" 2>&1 & P1=$!
"$BIN" --model "$MODEL_GGUF" --layer-start 0 --layer-end 1 --is-first-node --port 9101 > "$OUT/server_node0.log" 2>&1 & P2=$!
"$BIN" --model "$MODEL_GGUF" --layer-start 1 --is-last-node --port 9102 > "$OUT/server_node1.log" 2>&1 & P3=$!
trap 'kill $P1 $P2 $P3 2>/dev/null || true' EXIT
for p in 9100 9101 9102; do
  for _ in $(seq 1 100); do curl -sf "http://127.0.0.1:$p/status" >/dev/null && break; sleep 0.1; done
done
if "$PY" "$HERE/two_stage_driver.py" --tokens 16 > "$OUT/two_stage.json"; then echo "  chaîne 2 étages : IDENTIQUE"; else echo "  chaîne 2 étages : DIVERGENCE"; rc=1; fi
cat "$OUT/two_stage.json"
exit $rc
