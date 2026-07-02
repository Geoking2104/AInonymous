#!/usr/bin/env bash
#
# Testnet 2 nœuds en loopback — MOCK (stdlib Python, aucune dépendance ML).
#
# Valide la plomberie distribuée AInonymous sans GPU ni modèle réel :
#   plan statique → négociation QUIC daemon↔daemon → pipeline-split A→B
#   → relay tokens → réponse coordinateur
#
# Topologie (loopback) :
#   daemon A  = coordinateur + étage 0  (couches [0,  8[)   mock :9340
#   daemon B  = étage 1 (dernier)       (couches [8, 16[)   mock :9341
#
# Prérequis :
#   cargo build   (binaire ainonymous-daemon dans target/debug)
#   python3       (stdlib uniquement — aucun pip install requis)
#
# Usage :
#   ./scripts/testnet/run_testnet_2_mock.sh
#   BIN=target/release PROMPT="Hello" ./scripts/testnet/run_testnet_2_mock.sh
#
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
MOCK="$ROOT/scripts/testnet/mock_pipeline_server.py"
BIN="${BIN:-$ROOT/target/debug}"
DAEMON_BIN="$BIN/ainonymous-daemon"

LAYERS_A=8; LAYERS_B=16
PROMPT="${PROMPT:-Bonjour, qui es-tu ?}"
MAX_TOKENS="${MAX_TOKENS:-10}"

# Ports loopback
A_DAEMON=8889; A_QUIC=9000; A_PIPE=9340
B_DAEMON=8890; B_QUIC=9001; B_PIPE=9341

RUN="$ROOT/.testnet-run"
mkdir -p "$RUN"
LOGS="$RUN/logs"; mkdir -p "$LOGS"

echo "▶ AInonymous mock testnet 2 nœuds (stdlib, pas de GPU)"
[ -f "$MOCK" ]       || { echo "✗ Absent: $MOCK"; exit 1; }
[ -x "$DAEMON_BIN" ] || { echo "✗ Absent: $DAEMON_BIN — lance 'cargo build' d'abord"; exit 1; }
python3 -c "" 2>/dev/null || { echo "✗ python3 introuvable"; exit 1; }

# ── Configs TOML ──────────────────────────────────────────────────────────────
COMMON_SECTIONS="
[network]
quic_relay_fallback = true
activation_compression = \"auto\"
compression_threshold_gbps = 1.0
max_activation_size_mb = 512
max_concurrent_quic_sessions = 4

[inference]
default_model = \"mock\"
context_size = 512
n_gpu_layers = -1
flash_attention = false
kv_cache_type = \"f16\"
parallel_requests = 4"

cat > "$RUN/node-a.toml" <<TOMLEOF
# Node A — coordinateur + étage 0
daemon_port          = $A_DAEMON
quic_port            = $A_QUIC
llama_server_port    = 8080
pipeline_server_port = $A_PIPE
llama_server_bin     = "llama-server"
models_dir           = "$HOME/.models"
holochain_conductor_url = "ws://127.0.0.1:8888"
holochain_app_id     = "ainonymous-core"
max_concurrent_requests = 4
quic_advertise       = "127.0.0.1:$A_QUIC"

[holochain]
backend       = "static"
identity_path = "$RUN/node-a-identity.key"
$COMMON_SECTIONS

[[peers]]
agent_id      = "node-a"
daemon_url    = "http://127.0.0.1:$A_DAEMON"
quic_endpoint = "127.0.0.1:$A_QUIC"

[[peers]]
agent_id      = "node-b"
daemon_url    = "http://127.0.0.1:$B_DAEMON"
quic_endpoint = "127.0.0.1:$B_QUIC"

[[pipeline_stages]]
agent_id    = "node-a"
layer_start = 0
layer_end   = $LAYERS_A

[[pipeline_stages]]
agent_id    = "node-b"
layer_start = $LAYERS_A
layer_end   = $LAYERS_B
TOMLEOF

cat > "$RUN/node-b.toml" <<TOMLEOF
# Node B — étage 1 (dernier, worker uniquement)
daemon_port          = $B_DAEMON
quic_port            = $B_QUIC
llama_server_port    = 8081
pipeline_server_port = $B_PIPE
llama_server_bin     = "llama-server"
models_dir           = "$HOME/.models"
holochain_conductor_url = "ws://127.0.0.1:8888"
holochain_app_id     = "ainonymous-core"
max_concurrent_requests = 4
quic_advertise       = "127.0.0.1:$B_QUIC"

[holochain]
backend       = "static"
identity_path = "$RUN/node-b-identity.key"
$COMMON_SECTIONS
TOMLEOF

echo "  Configs générées dans $RUN/"

# ── Cleanup ───────────────────────────────────────────────────────────────────
PIDS=()
cleanup() {
  echo; echo "▶ Arrêt des processus…"
  for pid in "${PIDS[@]:-}"; do kill "$pid" 2>/dev/null || true; done
}
trap cleanup EXIT INT TERM

wait_http() {
  local url="$1" label="$2" tries="${3:-30}"
  for _ in $(seq 1 "$tries"); do
    if curl -fsS "$url" >/dev/null 2>&1; then echo "  ✓ $label prêt"; return 0; fi
    sleep 1
  done
  echo "  ✗ $label injoignable après ${tries}s — logs dans $LOGS/"
  ls "$LOGS/"*.log 2>/dev/null | while read -r f; do echo "--- $f ---"; tail -10 "$f"; done
  return 1
}

# ── 1) Mock pipeline servers (démarrage instantané, stdlib only) ──────────────
echo; echo "▶ Démarrage mock_pipeline_server A (étage 0, first-node, port $A_PIPE)…"
python3 "$MOCK" --port "$A_PIPE" \
  --layer-start 0 --layer-end "$LAYERS_A" \
  --is-first-node >"$LOGS/mock-a.log" 2>&1 & PIDS+=($!)

echo "▶ Démarrage mock_pipeline_server B (étage 1, last-node, port $B_PIPE)…"
python3 "$MOCK" --port "$B_PIPE" \
  --layer-start "$LAYERS_A" --layer-end "$LAYERS_B" \
  --is-last-node >"$LOGS/mock-b.log" 2>&1 & PIDS+=($!)

wait_http "http://127.0.0.1:$A_PIPE/status" "mock pipeline A" 10
wait_http "http://127.0.0.1:$B_PIPE/status" "mock pipeline B" 10

# ── 2) Daemons ────────────────────────────────────────────────────────────────
echo; echo "▶ Démarrage daemon A (coordinateur, port $A_DAEMON)…"
AINON_CONFIG="$RUN/node-a.toml" RUST_LOG="${RUST_LOG:-ainonymous_daemon=info,ainonymous_quic=info}" \
  "$DAEMON_BIN" >"$LOGS/daemon-a.log" 2>&1 & PIDS+=($!)

echo "▶ Démarrage daemon B (worker, port $B_DAEMON)…"
AINON_CONFIG="$RUN/node-b.toml" RUST_LOG="${RUST_LOG:-ainonymous_daemon=info,ainonymous_quic=info}" \
  "$DAEMON_BIN" >"$LOGS/daemon-b.log" 2>&1 & PIDS+=($!)

wait_http "http://127.0.0.1:$A_DAEMON/mesh/status" "daemon A" 30
wait_http "http://127.0.0.1:$B_DAEMON/mesh/status" "daemon B" 30

# ── 3) Inférence distribuée ───────────────────────────────────────────────────
echo; echo "▶ Inférence distribuée via POST /mesh/infer (coordinateur = daemon A) :"
REQ=$(printf '{"model_id":"mock","messages":[{"role":"user","content":"%s"}],"max_tokens":%s}' \
  "$PROMPT" "$MAX_TOKENS")
echo "  prompt: $PROMPT"
echo "  ---"
RESP=$(curl -sS --max-time 30 -X POST "http://127.0.0.1:$A_DAEMON/mesh/infer" \
  -H 'Content-Type: application/json' \
  -d "$REQ")
echo "$RESP" | python3 -m json.tool 2>/dev/null || echo "$RESP"
echo "  ---"

# ── 4) Validation ─────────────────────────────────────────────────────────────
CONTENT=$(echo "$RESP" | python3 -c \
  "import sys,json; d=json.load(sys.stdin); print(d.get('content',''))" 2>/dev/null || echo "")
EXEC_MODE=$(echo "$RESP" | python3 -c \
  "import sys,json; d=json.load(sys.stdin); print(d.get('execution_mode',''))" 2>/dev/null || echo "")
NODE_COUNT=$(echo "$RESP" | python3 -c \
  "import sys,json; d=json.load(sys.stdin); print(len(d.get('node_ids',[])))" 2>/dev/null || echo "0")

echo
if [ "$EXEC_MODE" = "pipeline_split" ] && [ "$NODE_COUNT" -ge 2 ] && [ -n "$CONTENT" ]; then
  echo "✅  SUCCÈS — pipeline_split sur $NODE_COUNT nœuds"
  echo "    contenu: '$CONTENT'"
  echo
  echo "✓ Ctrl-C pour arrêter les processus (ou attendez la fin du script)."
  exit 0
else
  echo "❌  ÉCHEC — exec_mode='$EXEC_MODE'  nodes=$NODE_COUNT  content='$CONTENT'"
  echo
  echo "Logs daemon A (tail):"
  tail -20 "$LOGS/daemon-a.log" 2>/dev/null || true
  echo
  echo "Logs daemon B (tail):"
  tail -20 "$LOGS/daemon-b.log" 2>/dev/null || true
  exit 1
fi
