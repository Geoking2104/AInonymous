#!/usr/bin/env bash
#
# Testnet 2 nœuds en loopback — AInonymous pipeline-split (topologie chaîne).
#
# Topologie :
#   daemon A  = coordinateur + étage 0  (couches [0, SPLIT[)   pipeline_server :PA
#   daemon B  = étage 1 (dernier)       (couches [SPLIT, N[)   pipeline_server :PB
#   Flux : A(coord) → A(stage0) → B(stage1) → tokens relayés en amont → A(coord)
#
# Prérequis :
#   - binaires compilés : `cargo build` (ou `make build-rust`)
#   - pipeline_server.py : pip install fastapi uvicorn transformers accelerate torch numpy
#   - le modèle HF (MODEL) téléchargeable, et son nombre de couches (TOTAL_LAYERS)
#
# Usage :
#   TOTAL_LAYERS=18 MODEL=google/gemma-3-1b-it ./scripts/testnet/run_testnet_2.sh
#   (TOTAL_LAYERS = "num_hidden_layers" du config.json du modèle)
#
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

# ── Paramètres (surchargeables par l'environnement) ───────────────────────────
MODEL="${MODEL:-google/gemma-3-1b-it}"
: "${TOTAL_LAYERS:?Définis TOTAL_LAYERS = num_hidden_layers du modèle (cf. config.json)}"
SPLIT="${SPLIT:-$((TOTAL_LAYERS / 2))}"
DEVICE="${DEVICE:-cpu}"           # cpu | cuda
DTYPE="${DTYPE:-bf16}"            # fp16 | bf16
PROMPT="${PROMPT:-Bonjour, présente-toi en une phrase.}"
MAX_TOKENS="${MAX_TOKENS:-32}"

# Binaires (debug par défaut ; export BIN=target/release pour la release)
BIN="${BIN:-$ROOT/target/debug}"
DAEMON_BIN="$BIN/ainonymous-daemon"

# Ports loopback
A_DAEMON=8889; A_QUIC=9000; A_PIPE=9340
B_DAEMON=8890; B_QUIC=9001; B_PIPE=9341

RUN="$ROOT/.testnet-run"
mkdir -p "$RUN"
LOGS="$RUN/logs"; mkdir -p "$LOGS"

echo "▶ Modèle=$MODEL  couches=$TOTAL_LAYERS  split=$SPLIT  device=$DEVICE"
[ -x "$DAEMON_BIN" ] || { echo "✗ Binaire absent: $DAEMON_BIN — lance d'abord 'cargo build'"; exit 1; }

# ── Génération des configs (source unique de vérité) ──────────────────────────
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
# Node A — coordinateur + étage 0
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
# Node B — étage 1 (dernier). Ne coordonne pas : ni peers ni pipeline_stages.
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
cleanup() {
  echo; echo "▶ Arrêt des processus…"
  for pid in "${PIDS[@]:-}"; do kill "$pid" 2>/dev/null || true; done
}
trap cleanup EXIT INT TERM

wait_http() { # url, label, tries
  local url="$1" label="$2" tries="${3:-60}"
  for _ in $(seq 1 "$tries"); do
    if curl -fsS "$url" >/dev/null 2>&1; then echo "  ✓ $label prêt"; return 0; fi
    sleep 1
  done
  echo "  ✗ $label injoignable après ${tries}s — voir $LOGS"; return 1
}

# ── 1) Pipeline servers (étages GPU/CPU) ──────────────────────────────────────
echo "▶ Démarrage pipeline_server A (couches 0..$SPLIT, is-first)…"
python3 "$ROOT/scripts/pipeline_server.py" --model "$MODEL" --port "$A_PIPE" \
  --layer-start 0 --layer-end "$SPLIT" --is-first-node \
  --device "$DEVICE" --dtype "$DTYPE" >"$LOGS/pipe-a.log" 2>&1 & PIDS+=($!)

echo "▶ Démarrage pipeline_server B (couches $SPLIT..$TOTAL_LAYERS, is-last)…"
python3 "$ROOT/scripts/pipeline_server.py" --model "$MODEL" --port "$B_PIPE" \
  --layer-start "$SPLIT" --layer-end "$TOTAL_LAYERS" --is-last-node \
  --device "$DEVICE" --dtype "$DTYPE" >"$LOGS/pipe-b.log" 2>&1 & PIDS+=($!)

wait_http "http://127.0.0.1:$A_PIPE/status" "pipeline A" 180
wait_http "http://127.0.0.1:$B_PIPE/status" "pipeline B" 180

# ── 2) Daemons ────────────────────────────────────────────────────────────────
echo "▶ Démarrage daemon A (coordinateur)…"
AINON_CONFIG="$RUN/node-a.toml" RUST_LOG="${RUST_LOG:-ainonymous_daemon=info}" \
  "$DAEMON_BIN" >"$LOGS/daemon-a.log" 2>&1 & PIDS+=($!)

echo "▶ Démarrage daemon B…"
AINON_CONFIG="$RUN/node-b.toml" RUST_LOG="${RUST_LOG:-ainonymous_daemon=info}" \
  "$DAEMON_BIN" >"$LOGS/daemon-b.log" 2>&1 & PIDS+=($!)

# Les daemons exposent /mesh/status ; on attend qu'ils répondent.
wait_http "http://127.0.0.1:$A_DAEMON/mesh/status" "daemon A" 60
wait_http "http://127.0.0.1:$B_DAEMON/mesh/status" "daemon B" 60

# ── 3) Requête d'inférence distribuée (coordinateur = daemon A) ───────────────
echo; echo "▶ Inférence distribuée via daemon A /mesh/infer :"
REQ=$(printf '{"model_id":"%s","messages":[{"role":"user","content":"%s"}],"max_tokens":%s}' \
  "$MODEL" "$PROMPT" "$MAX_TOKENS")
echo "  prompt: $PROMPT"
echo "  ---"
curl -sS -X POST "http://127.0.0.1:$A_DAEMON/mesh/infer" \
  -H 'Content-Type: application/json' -d "$REQ" | tee "$RUN/last_response.json"
echo; echo "  ---"
echo "✓ Réponse ci-dessus (logs détaillés dans $LOGS). Ctrl-C pour tout arrêter."

# Garder les daemons vivants pour inspection / requêtes supplémentaires
wait
