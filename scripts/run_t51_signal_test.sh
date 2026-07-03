#!/usr/bin/env bash
# T5.1 — Test intégration signal Holochain → daemon QUIC (WSL2)
#
# Ce script vérifie le flux complet :
#   zome negotiate_quic_session → QuicListenerSignal → daemon.listen_quic_signals()
#   → SessionRegistry.register(offer avec client_pubkey)
#
# Prérequis :
#   1. Holochain 0.6.2 installé (bash scripts/setup_holochain_wsl.sh)
#   2. Daemon compilé  : cargo build --package ainonymous-daemon
#   3. Tournez depuis la racine du repo DANS WSL2 :
#        wsl bash scripts/run_t51_signal_test.sh
#
# Ce que le test valide :
#   ✓ Conducteur Holochain démarre + accepte l'admin WS
#   ✓ hApp ainonymous-core installée et activée
#   ✓ Daemon se connecte au conducteur (mode "conductor")
#   ✓ Signal QuicListenerSignal reçu par listen_quic_signals()
#   ✓ Session QUIC enregistrée dans le registre (log daemon)

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

HAPP="dnas/ainonymous-core/ainonymous-core.happ"
CONDUCTOR_CFG="scripts/testnet/conductor_t51.yaml"
DAEMON_CFG="scripts/testnet/daemon_t51.toml"
CONDUCTOR_DATA="/tmp/ainonymous-t51/conductor-data"
LOG_CONDUCTOR="/tmp/ainonymous-t51/conductor.log"
LOG_DAEMON="/tmp/ainonymous-t51/daemon.log"
ADMIN_PORT=65000
APP_PORT=65001
DAEMON_REST="http://127.0.0.1:18890"

# Couleurs
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
pass() { echo -e "${GREEN}✓${NC} $*"; }
fail() { echo -e "${RED}✗${NC} $*"; exit 1; }
info() { echo -e "${YELLOW}→${NC} $*"; }

cleanup() {
    info "Nettoyage des processus..."
    kill "${CONDUCTOR_PID:-}" "${DAEMON_PID:-}" 2>/dev/null || true
    sleep 1
}
trap cleanup EXIT

# ─── 0. Prérequis ─────────────────────────────────────────────────────────────
info "Vérification des prérequis..."
command -v holochain >/dev/null || fail "holochain non trouvé (bash scripts/setup_holochain_wsl.sh)"
command -v hc >/dev/null || fail "hc non trouvé"
[[ -f "${HAPP}" ]] || fail "hApp introuvable : ${HAPP} (cargo build WASM + hc app pack)"
[[ -f "target/debug/ainonymous-daemon" ]] || fail "daemon non compilé (cargo build -p ainonymous-daemon)"
pass "Prérequis OK"

# ─── 1. Démarrer le conducteur Holochain ─────────────────────────────────────
info "Démarrage du conducteur Holochain (port admin=${ADMIN_PORT})..."
rm -rf "${CONDUCTOR_DATA}"
mkdir -p "$(dirname "${LOG_CONDUCTOR}")"
holochain -c "${CONDUCTOR_CFG}" > "${LOG_CONDUCTOR}" 2>&1 &
CONDUCTOR_PID=$!

# Attendre que le port admin soit prêt (max 30s)
for i in $(seq 1 30); do
    if nc -z 127.0.0.1 "${ADMIN_PORT}" 2>/dev/null; then
        pass "Conducteur démarré (PID ${CONDUCTOR_PID})"
        break
    fi
    [[ $i -eq 30 ]] && {
        cat "${LOG_CONDUCTOR}"
        fail "Conducteur non démarré après 30s"
    }
    sleep 1
done

# ─── 2. Installer + activer le hApp ──────────────────────────────────────────
info "Installation du hApp ainonymous-core via admin WebSocket..."

python3 - "${HAPP}" "${ADMIN_PORT}" "${APP_PORT}" <<'PYEOF'
import asyncio, sys, os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'scripts'))

async def install_and_enable(happ_path, admin_port, app_port):
    """Installe le hApp et ouvre un port app interface."""
    try:
        import websockets, json, struct, msgpack
    except ImportError:
        print("Installation de websockets + msgpack...")
        os.system("pip install websockets msgpack --quiet")
        import websockets, json, struct, msgpack

    uri = f"ws://127.0.0.1:{admin_port}"

    async with websockets.connect(uri) as ws:
        # Lire le hApp
        with open(happ_path, "rb") as f:
            happ_bytes = f.read()

        # install_app
        call = {
            "type": "install_app",
            "data": {
                "installed_app_id": "ainonymous",
                "agent_key": None,  # génère un agent test
                "source": {
                    "type": "path",
                    "value": os.path.abspath(happ_path)
                }
            }
        }
        await ws.send(msgpack.dumps(call, use_bin_type=True))
        resp = msgpack.loads(await ws.recv(), raw=False)
        if resp.get("type") == "error":
            # Peut-être déjà installé
            print(f"  install_app: {resp}")
        else:
            print(f"  App installée : {resp.get('data', {}).get('installed_app_id')}")

        # enable_app
        call = {"type": "enable_app", "data": {"installed_app_id": "ainonymous"}}
        await ws.send(msgpack.dumps(call, use_bin_type=True))
        resp = msgpack.loads(await ws.recv(), raw=False)
        print(f"  App activée : {resp.get('type')}")

        # attach_app_interface (port app)
        call = {
            "type": "attach_app_interface",
            "data": {"port": int(app_port), "allowed_origins": "*", "installed_app_id": None}
        }
        await ws.send(msgpack.dumps(call, use_bin_type=True))
        resp = msgpack.loads(await ws.recv(), raw=False)
        print(f"  Interface app sur port {app_port} : {resp.get('type')}")

asyncio.run(install_and_enable(sys.argv[1], int(sys.argv[2]), int(sys.argv[3])))
PYEOF

pass "hApp installé et activé"

# ─── 3. Démarrer le daemon en mode conducteur ────────────────────────────────
info "Démarrage du daemon (mode conductor, port ${DAEMON_REST})..."
AINON_CONFIG="${DAEMON_CFG}" ./target/debug/ainonymous-daemon > "${LOG_DAEMON}" 2>&1 &
DAEMON_PID=$!

# Attendre que le REST soit prêt (max 20s)
for i in $(seq 1 20); do
    if curl -sf "${DAEMON_REST}/health" >/dev/null 2>&1; then
        pass "Daemon démarré (PID ${DAEMON_PID})"
        break
    fi
    [[ $i -eq 20 ]] && {
        cat "${LOG_DAEMON}"
        fail "Daemon non démarré après 20s"
    }
    sleep 1
done

# Vérifier que le daemon est en mode conducteur (log "Conducteur Holochain connecté")
if grep -q "Conducteur Holochain connecté" "${LOG_DAEMON}"; then
    pass "Daemon connecté au conducteur Holochain"
else
    info "Log daemon (dernier 20 lignes) :"
    tail -20 "${LOG_DAEMON}"
    fail "Daemon n'a pas pu se connecter au conducteur"
fi

# ─── 4. Déclencher negotiate_quic_session via REST daemon ────────────────────
info "Appel POST /mesh/session/negotiate (déclenche le signal zome)..."
RESP=$(curl -sf -X POST "${DAEMON_REST}/mesh/session/negotiate" \
    -H "Content-Type: application/json" \
    -d '{"layer_range":[0,12],"next_agent_id":null,"next_layer_range":null,"requester_pubkey":null}' \
    2>&1)
echo "  Réponse : ${RESP}"

# ─── 5. Vérifier que le signal a été reçu ────────────────────────────────────
sleep 2  # laisser le signal se propager
info "Vérification du signal dans les logs daemon..."

if grep -q "Session QUIC entrante enregistrée via signal Holochain" "${LOG_DAEMON}"; then
    pass "Signal QuicListenerSignal reçu et session enregistrée ! (T5.1 ✓)"
else
    info "Log daemon (dernier 30 lignes) :"
    tail -30 "${LOG_DAEMON}"
    fail "Signal QuicListenerSignal non reçu — vérifier les logs"
fi

# ─── 6. Résumé ───────────────────────────────────────────────────────────────
echo ""
echo "══════════════════════════════════════════"
echo "  T5.1 PASS — Signal Holochain → QUIC OK"
echo "══════════════════════════════════════════"
echo "  Conducteur PID : ${CONDUCTOR_PID}"
echo "  Daemon PID     : ${DAEMON_PID}"
echo "  Logs : ${LOG_CONDUCTOR}"
echo "         ${LOG_DAEMON}"
