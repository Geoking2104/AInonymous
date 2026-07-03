#!/usr/bin/env bash
# smoke_test_t41.sh — T4.1 : smoke test bout-en-bout du daemon AInonymous
#
# Valide les paliers T3.1 (QuicListenerSignal), T3.2 (mTLS client_pubkey)
# et le plan de contrôle REST en mode statique, sans conducteur Holochain.
#
# Prérequis : cargo, curl, jq, kill
# Usage     : bash scripts/smoke_test_t41.sh [--release]
#
# Exécution depuis Git Bash ou WSL :
#   bash scripts/smoke_test_t41.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

BUILD_MODE="debug"
CARGO_FLAGS=""
SKIP_DAEMON_TESTS=0
for arg in "$@"; do
    case "$arg" in
        --release|release) BUILD_MODE="release"; CARGO_FLAGS="--release" ;;
        --skip-daemon-tests) SKIP_DAEMON_TESTS=1 ;;
    esac
done

DAEMON_BIN="$PROJECT_ROOT/target/$BUILD_MODE/ainonymous-daemon"
TEST_CONFIG="$SCRIPT_DIR/testnet/daemon_t41.toml"
TEST_PORT=18889
LOG_FILE="$PROJECT_ROOT/target/daemon_t41.log"
DAEMON_PID=""

cleanup() {
    if [[ -n "$DAEMON_PID" ]] && kill -0 "$DAEMON_PID" 2>/dev/null; then
        echo "==> Arrêt daemon (pid $DAEMON_PID)"
        kill "$DAEMON_PID" 2>/dev/null || true
        wait "$DAEMON_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

PASS=0
FAIL=0

assert_eq() {
    local label="$1" actual="$2" expected="$3"
    if [[ "$actual" == "$expected" ]]; then
        echo "    ✓ $label"
        (( PASS++ )) || true
    else
        echo "    ✗ $label : attendu='$expected' obtenu='$actual'"
        (( FAIL++ )) || true
    fi
}

assert_ne() {
    local label="$1" actual="$2" unexpected="$3"
    if [[ "$actual" != "$unexpected" ]]; then
        echo "    ✓ $label"
        (( PASS++ )) || true
    else
        echo "    ✗ $label : valeur inattendue='$actual'"
        (( FAIL++ )) || true
    fi
}

# ─────────────────────────────────────────────────────────────────────────────
echo "╔══════════════════════════════════════════════════════════╗"
echo "║  T4.1 — Smoke test AInonymous daemon (mode statique)    ║"
echo "╚══════════════════════════════════════════════════════════╝"

# ── Étape 1 : tests unitaires ainonymous-quic (mTLS handshake QUIC) ──────────
echo ""
echo "══ [1/4] cargo test ainonymous-quic ════════════════════════"
cd "$PROJECT_ROOT"
cargo test --package ainonymous-quic --quiet 2>&1 | tail -3
echo "    ✓ ainonymous-quic"

# ── Étape 2 : tests unitaires ainonymous-daemon (signal parsing T3.1) ────────
echo ""
echo "══ [2/4] cargo test ainonymous-daemon (signal parsing) ═════"
if [[ $SKIP_DAEMON_TESTS -eq 1 ]]; then
    # cargo check suffit pour valider la compilation des tests (+ rapide)
    echo "    (--skip-daemon-tests : cargo check seulement)"
    cargo check --package ainonymous-daemon --quiet 2>&1 | tail -2
    echo "    ✓ ainonymous-daemon (check)"
else
    # NOTE : première exécution ~10-15 min (holochain_client deps non cachés).
    # Les runs suivants sont rapides (~30s).
    cargo test --package ainonymous-daemon --quiet 2>&1 | tail -5
    echo "    ✓ ainonymous-daemon"
fi

# ── Étape 3 : build daemon ────────────────────────────────────────────────────
echo ""
echo "══ [3/4] Build daemon ($BUILD_MODE) ════════════════════════"
cargo build --package ainonymous-daemon $CARGO_FLAGS --quiet
SIZE=$(du -sh "$DAEMON_BIN" | cut -f1)
echo "    ✓ build OK → $DAEMON_BIN ($SIZE)"

# ── Étape 4 : smoke test HTTP ─────────────────────────────────────────────────
echo ""
echo "══ [4/4] Smoke test HTTP ═══════════════════════════════════"

# Démarrer le daemon en fond avec la config de test
AINON_CONFIG="$TEST_CONFIG" \
    RUST_LOG="warn" \
    "$DAEMON_BIN" >"$LOG_FILE" 2>&1 &
DAEMON_PID=$!
echo "    Daemon pid=$DAEMON_PID — attente readiness sur :$TEST_PORT..."

# Readiness polling (max 30s)
ready=0
for i in $(seq 1 30); do
    if curl -sf "http://127.0.0.1:$TEST_PORT/mesh/status" >/dev/null 2>&1; then
        echo "    Readiness OK (${i}s)"
        ready=1
        break
    fi
    if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
        echo "    ERREUR : daemon crashé au démarrage"
        echo "    --- Dernières lignes du log ---"
        tail -20 "$LOG_FILE"
        exit 1
    fi
    sleep 1
done

if [[ $ready -eq 0 ]]; then
    echo "    ERREUR : timeout — daemon non joignable après 30s"
    tail -20 "$LOG_FILE"
    exit 1
fi

# ── Test A : GET /mesh/status ─────────────────────────────────────────────────
echo ""
echo "  [A] GET /mesh/status"
STATUS_RESP=$(curl -sf "http://127.0.0.1:$TEST_PORT/mesh/status")
LOCAL_STATUS=$(echo "$STATUS_RESP" | jq -r '.local_node.status')
# En mode statique les zome calls échouent → "degraded" est attendu et normal.
assert_ne "local_node.status non null" "$LOCAL_STATUS" "null"
echo "      local_node.status=$LOCAL_STATUS (attendu: active|degraded)"

# ── Test B : POST /mesh/session/negotiate (sans mTLS) ────────────────────────
echo ""
echo "  [B] POST /mesh/session/negotiate (sans requester_pubkey)"
NEGO_RESP=$(curl -sf -X POST "http://127.0.0.1:$TEST_PORT/mesh/session/negotiate" \
    -H "Content-Type: application/json" \
    -d '{"layer_range":[0,12]}')

TOKEN_LEN=$(echo "$NEGO_RESP" | jq '.session_token | length')
ENDPOINT=$(echo "$NEGO_RESP" | jq -r '.quic_endpoint')
LAYER=$(echo "$NEGO_RESP" | jq -rc '.layer_range')
CLIENT_PK=$(echo "$NEGO_RESP" | jq '.client_pubkey')

assert_eq "session_token length=32" "$TOKEN_LEN" "32"
assert_ne "quic_endpoint non null" "$ENDPOINT" "null"
assert_eq "layer_range=[0,12]" "$LAYER" "[0,12]"
assert_eq "client_pubkey=null (pas de mTLS)" "$CLIENT_PK" "null"

# ── Test C : POST /mesh/session/negotiate (avec requester_pubkey → mTLS T3.2) ─
echo ""
echo "  [C] POST /mesh/session/negotiate (avec requester_pubkey mTLS)"
# 32 octets 0x42 = 66 décimal
REQUESTER_PK="[$(printf '66,' $(seq 31) | sed 's/,$//')66]"
NEGO_RESP2=$(curl -sf -X POST "http://127.0.0.1:$TEST_PORT/mesh/session/negotiate" \
    -H "Content-Type: application/json" \
    -d "{\"layer_range\":[4,8],\"requester_pubkey\":$REQUESTER_PK}")

TOKEN_LEN2=$(echo "$NEGO_RESP2" | jq '.session_token | length')
LAYER2=$(echo "$NEGO_RESP2" | jq -rc '.layer_range')
CLIENT_PK2=$(echo "$NEGO_RESP2" | jq '.client_pubkey')
CPK_FIRST=$(echo "$CLIENT_PK2" | jq -r '.[0]')
CPK_LEN=$(echo "$CLIENT_PK2" | jq 'length')

assert_eq "session_token length=32 (mTLS)" "$TOKEN_LEN2" "32"
assert_eq "layer_range=[4,8] (mTLS)" "$LAYER2" "[4,8]"
assert_eq "client_pubkey[0]=66 (T3.2)" "$CPK_FIRST" "66"
assert_eq "client_pubkey length=32 (T3.2)" "$CPK_LEN" "32"

# ── Test D : tokens uniques (chaque negotiate génère un token différent) ───────
echo ""
echo "  [D] Tokens de session uniques"
T1=$(echo "$NEGO_RESP"  | jq -rc '.session_token[0:4]')
T2=$(echo "$NEGO_RESP2" | jq -rc '.session_token[0:4]')
assert_ne "tokens distincts (randomness)" "$T1" "$T2" || true
# Note: probabilité infime de collision sur les 4 premiers octets

# ── Résumé ────────────────────────────────────────────────────────────────────
echo ""
echo "Log daemon : $LOG_FILE"
echo ""
if [[ $FAIL -eq 0 ]]; then
    echo "╔══════════════════════════════════════════════════════════╗"
    echo "║  ✅ T4.1 PASSED — $PASS assertions OK, $FAIL échecs         ║"
    echo "╚══════════════════════════════════════════════════════════╝"
    exit 0
else
    echo "╔══════════════════════════════════════════════════════════╗"
    echo "║  ❌ T4.1 FAILED — $PASS OK, $FAIL ÉCHECS                    ║"
    echo "╚══════════════════════════════════════════════════════════╝"
    echo ""
    echo "--- Dernières lignes du log daemon ---"
    tail -30 "$LOG_FILE"
    exit 1
fi
