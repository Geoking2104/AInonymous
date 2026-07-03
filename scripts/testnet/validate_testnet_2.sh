#!/usr/bin/env bash
#
# validate_testnet_2.sh — Validation bout-en-bout du pipeline-split AInonymous.
#
# Lance le testnet 2 nœuds, envoie une requête d'inférence et valide les
# assertions sur la réponse. Conçu pour fonctionner en mode CPU (CI/CD sans GPU).
#
# Sorties :
#   0 — toutes les assertions passent
#   1 — au moins une assertion a échoué (voir stderr)
#
# Usage :
#   # CPU, modèle léger (défaut — pour CI)
#   ./scripts/testnet/validate_testnet_2.sh
#
#   # GPU CUDA, modèle plus lourd
#   DEVICE=cuda MODEL=google/gemma-3-4b-it TOTAL_LAYERS=34 \
#     ./scripts/testnet/validate_testnet_2.sh
#
# Variables d'environnement :
#   MODEL         HuggingFace model ID  (défaut: google/gemma-3-1b-it)
#   TOTAL_LAYERS  num_hidden_layers du modèle (défaut: 18)
#   SPLIT         index de découpe (défaut: TOTAL_LAYERS/2)
#   DEVICE        cpu | cuda (défaut: cpu)
#   DTYPE         fp16 | bf16 (défaut: bf16)
#   MAX_TOKENS    tokens à générer (défaut: 16)
#   BIN           répertoire des binaires Rust (défaut: target/debug)
#
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# ── Paramètres ────────────────────────────────────────────────────────────────
MODEL="${MODEL:-google/gemma-3-1b-it}"
TOTAL_LAYERS="${TOTAL_LAYERS:-18}"
SPLIT="${SPLIT:-$((TOTAL_LAYERS / 2))}"
DEVICE="${DEVICE:-cpu}"
DTYPE="${DTYPE:-bf16}"
MAX_TOKENS="${MAX_TOKENS:-16}"
PROMPT="${PROMPT:-Réponds en un mot : quelle est la capitale de la France ?}"
BIN="${BIN:-$ROOT/target/debug}"

A_DAEMON=8889; A_QUIC=9000; A_PIPE=9340
B_DAEMON=8890; B_QUIC=9001; B_PIPE=9341

RUN="$ROOT/.testnet-validate"
mkdir -p "$RUN"
LOGS="$RUN/logs"; mkdir -p "$LOGS"

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  AInonymous — Validation testnet 2 nœuds"
echo "  Modèle    : $MODEL"
echo "  Couches   : $TOTAL_LAYERS  (split=$SPLIT)"
echo "  Device    : $DEVICE / $DTYPE"
echo "  Max tokens: $MAX_TOKENS"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Vérifier les prérequis
DAEMON_BIN="$BIN/ainonymous-daemon"
[ -x "$DAEMON_BIN" ] || {
  echo "✗ Binaire absent: $DAEMON_BIN"
  echo "  → Lance 'cargo build' avant de valider."
  exit 1
}
python3 -c "import fastapi, uvicorn, transformers, torch" 2>/dev/null || {
  echo "✗ Dépendances Python manquantes."
  echo "  → pip install fastapi uvicorn transformers accelerate torch numpy"
  exit 1
}
command -v jq >/dev/null 2>&1 || {
  echo "✗ 'jq' requis pour les assertions JSON."
  echo "  → sudo apt-get install jq  (ou brew install jq)"
  exit 1
}

# ── Génération des configs ────────────────────────────────────────────────────
common_sections() {
  local identity_path="$1"
  cat <<EOF

[holochain]
backend       = "static"
identity_path = "$identity_path"

[network]
quic_relay_fallback = true
activation_compression = "auto"
compression_threshold_gbps = 1.0
max_activation_size_mb = 512
max_concurrent_quic_sessions = 4

[inference]
default_model = "$MODEL"
context_size = 8192
n_gpu_layers = -1
flash_attention = true
kv_cache_type = "q8_0"
parallel_requests = 4
EOF
}

cat > "$RUN/node-a.toml" <<EOF
daemon_port = $A_DAEMON
quic_port = $A_QUIC
llama_server_port = 8080
pipeline_server_port = $A_PIPE
llama_server_bin = "llama-server"
models_dir = "$HOME/.models"
holochain_conductor_url = "ws://127.0.0.1:8888"
holochain_app_id = "ainonymous-core"
max_concurrent_requests = 4
quic_advertise = "127.0.0.1:$A_QUIC"
$(common_sections "$RUN/node-a-identity.key")

[[peers]]
agent_id = "node-a"
daemon_url = "http://127.0.0.1:$A_DAEMON"
quic_endpoint = "127.0.0.1:$A_QUIC"

[[peers]]
agent_id = "node-b"
daemon_url = "http://127.0.0.1:$B_DAEMON"
quic_endpoint = "127.0.0.1:$B_QUIC"

[[pipeline_stages]]
agent_id = "node-a"
layer_start = 0
layer_end = $SPLIT

[[pipeline_stages]]
agent_id = "node-b"
layer_start = $SPLIT
layer_end = $TOTAL_LAYERS
EOF

cat > "$RUN/node-b.toml" <<EOF
daemon_port = $B_DAEMON
quic_port = $B_QUIC
llama_server_port = 8081
pipeline_server_port = $B_PIPE
llama_server_bin = "llama-server"
models_dir = "$HOME/.models"
holochain_conductor_url = "ws://127.0.0.1:8888"
holochain_app_id = "ainonymous-core"
max_concurrent_requests = 4
quic_advertise = "127.0.0.1:$B_QUIC"
$(common_sections "$RUN/node-b-identity.key")
EOF

# ── Nettoyage à la sortie ─────────────────────────────────────────────────────
PIDS=()
FAILED=0
cleanup() {
  echo
  echo "▶ Arrêt des processus…"
  for pid in "${PIDS[@]:-}"; do kill "$pid" 2>/dev/null || true; done
  if [ "$FAILED" -eq 0 ]; then
    echo "✓ Toutes les assertions ont passé."
  else
    echo "✗ $FAILED assertion(s) ont échoué — voir $LOGS"
  fi
}
trap cleanup EXIT INT TERM

wait_http() {
  local url="$1" label="$2" tries="${3:-120}"
  echo -n "  Attente $label "
  for _ in $(seq 1 "$tries"); do
    if curl -fsS "$url" >/dev/null 2>&1; then echo " ✓"; return 0; fi
    echo -n "."; sleep 1
  done
  echo " ✗ (timeout ${tries}s)"
  echo "  → Logs : $LOGS"
  return 1
}

assert() {
  local label="$1" result="$2" expected="$3"
  if [ "$result" = "$expected" ] || [[ "$result" =~ $expected ]]; then
    echo "  ✓ $label"
  else
    echo "  ✗ $label  (got: '$result'  expected: '$expected')"
    FAILED=$((FAILED + 1))
  fi
}

assert_not_empty() {
  local label="$1" value="$2"
  if [ -n "$value" ] && [ "$value" != "null" ] && [ "$value" != "0" ]; then
    echo "  ✓ $label ($value)"
  else
    echo "  ✗ $label est vide/null/0 (got: '$value')"
    FAILED=$((FAILED + 1))
  fi
}

assert_gt() {
  local label="$1" value="$2" threshold="$3"
  if [ "$value" -gt "$threshold" ] 2>/dev/null; then
    echo "  ✓ $label ($value > $threshold)"
  else
    echo "  ✗ $label: $value n'est pas > $threshold"
    FAILED=$((FAILED + 1))
  fi
}

# ── 1) Pipeline servers ───────────────────────────────────────────────────────
echo
echo "▶ [1/4] Démarrage pipeline_server A (couches 0..$SPLIT)…"
python3 "$ROOT/scripts/pipeline_server.py" \
  --model "$MODEL" --port "$A_PIPE" \
  --layer-start 0 --layer-end "$SPLIT" --is-first-node \
  --device "$DEVICE" --dtype "$DTYPE" \
  >"$LOGS/pipe-a.log" 2>&1 & PIDS+=($!)

echo "▶ [1/4] Démarrage pipeline_server B (couches $SPLIT..$TOTAL_LAYERS)…"
python3 "$ROOT/scripts/pipeline_server.py" \
  --model "$MODEL" --port "$B_PIPE" \
  --layer-start "$SPLIT" --layer-end "$TOTAL_LAYERS" --is-last-node \
  --device "$DEVICE" --dtype "$DTYPE" \
  >"$LOGS/pipe-b.log" 2>&1 & PIDS+=($!)

wait_http "http://127.0.0.1:$A_PIPE/status" "pipeline A" 240
wait_http "http://127.0.0.1:$B_PIPE/status" "pipeline B" 240

# Assertions sur /status
echo
echo "▶ [2/4] Assertions /status…"
STATUS_A=$(curl -fsS "http://127.0.0.1:$A_PIPE/status")
STATUS_B=$(curl -fsS "http://127.0.0.1:$B_PIPE/status")

assert "pipeline A — is_first_node"  "$(echo "$STATUS_A" | jq -r .is_first_node)" "true"
assert "pipeline A — is_last_node"   "$(echo "$STATUS_A" | jq -r .is_last_node)"  "false"
assert "pipeline B — is_first_node"  "$(echo "$STATUS_B" | jq -r .is_first_node)" "false"
assert "pipeline B — is_last_node"   "$(echo "$STATUS_B" | jq -r .is_last_node)"  "true"
assert_not_empty "pipeline A — eos_token_id" "$(echo "$STATUS_A" | jq -r .eos_token_id)"
assert_not_empty "pipeline B — total_layers" "$(echo "$STATUS_B" | jq -r .total_layers)"

# ── 2) Daemons ────────────────────────────────────────────────────────────────
echo
echo "▶ [3/4] Démarrage daemons…"
AINON_CONFIG="$RUN/node-a.toml" RUST_LOG="ainonymous_daemon=info" \
  "$DAEMON_BIN" >"$LOGS/daemon-a.log" 2>&1 & PIDS+=($!)
AINON_CONFIG="$RUN/node-b.toml" RUST_LOG="ainonymous_daemon=info" \
  "$DAEMON_BIN" >"$LOGS/daemon-b.log" 2>&1 & PIDS+=($!)

wait_http "http://127.0.0.1:$A_DAEMON/mesh/status" "daemon A" 60
wait_http "http://127.0.0.1:$B_DAEMON/mesh/status" "daemon B" 60

# ── 3) Inférence + assertions ─────────────────────────────────────────────────
echo
echo "▶ [4/4] Inférence distribuée via /mesh/infer…"
echo "  Prompt : $PROMPT"

REQ=$(printf '{"model_id":"%s","messages":[{"role":"user","content":"%s"}],"max_tokens":%d}' \
  "$MODEL" "$PROMPT" "$MAX_TOKENS")

RESP=$(curl -fsS -X POST "http://127.0.0.1:$A_DAEMON/mesh/infer" \
  -H 'Content-Type: application/json' -d "$REQ" 2>"$RUN/curl_err.txt" || true)

echo "  Réponse brute : $RESP"
echo "$RESP" > "$RUN/last_response.json"

if [ -z "$RESP" ]; then
  echo "  ✗ Pas de réponse de /mesh/infer (curl error: $(cat "$RUN/curl_err.txt"))"
  FAILED=$((FAILED + 1))
else
  echo
  echo "  Assertions sur la réponse :"
  CONTENT="$(echo "$RESP" | jq -r '.content // empty')"
  TOKEN_COUNT="$(echo "$RESP" | jq -r '.token_count // 0')"
  NODE_COUNT="$(echo "$RESP" | jq -r '.node_ids | length')"
  EXEC_MODE="$(echo "$RESP" | jq -r '.execution_mode // empty')"

  assert_not_empty "content non vide"          "$CONTENT"
  assert_gt        "token_count > 0"           "$TOKEN_COUNT" 0
  assert_gt        "node_ids a 2 nœuds"        "$NODE_COUNT" 1
  assert           "execution_mode=pipeline"   "$EXEC_MODE" "pipeline_split"

  # Optionnel : speculative_acceptance_rate si spéculatif activé
  SPEC_RATE="$(echo "$RESP" | jq -r '.speculative_acceptance_rate // "N/A"')"
  echo "  ℹ speculative_acceptance_rate : $SPEC_RATE"
fi

# Résumé
echo
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Résultat : $FAILED assertion(s) échouée(s)"
echo "  Logs     : $LOGS"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

exit "$FAILED"
