#!/usr/bin/env bash
#
# Côté téléphone (Termux) — worker QUIC de la campagne CH3/S4.
#
# À lancer DANS TERMUX, dans ~/campaign. Étapes :
#   1. recopie le binaire depuis /sdcard/Download si le binaire interne manque ;
#   2. lit la clé publique du coordinateur (PC) depuis client-pubkey.txt
#      (poussée par run-campaign.sh via /sdcard) ;
#   3. affiche la clé publique de ce téléphone (à noter côté PC si besoin) ;
#   4. démarre `quic-spike serve` : offre régénérée toutes les 20 s vers
#      ~/campaign/offer.json ET /sdcard/Download/campaign/offer.json
#      (si la permission stockage a été accordée via `termux-setup-storage`).
#
# Le PC récupère l'offre par : adb pull /sdcard/Download/campaign/offer.json
# (canal le plus fiable : pas de run-as ni de paquet Termux supplémentaire).
#
# Usage :
#   bash worker-start.sh                          # advertise auto (wifi)
#   ADVERTISE=127.0.0.1:7700 bash worker-start.sh # liaison USB (adb forward)
#   PORT=7700 CLIENT_KEY_FILE=client-pubkey.txt bash worker-start.sh
set -u

cd "$(dirname "$0")"
PORT="${PORT:-7700}"
KEY_FILE="${KEY_FILE:-b.key}"
CLIENT_KEY_FILE="${CLIENT_KEY_FILE:-client-pubkey.txt}"
OFFER="${OFFER:-offer.json}"
SDCARD_DIR="/sdcard/Download/campaign"

# ── 1. Binaire ───────────────────────────────────────────────────────────────
if [ ! -x ./quic-spike ] && [ -f /sdcard/Download/quic-spike ]; then
  echo "▶ Copie du binaire depuis /sdcard/Download…"
  cp /sdcard/Download/quic-spike ./quic-spike || {
    echo "✗ Copie impossible ; vérifie l'espace disque."; exit 1; }
  chmod 700 ./quic-spike
fi
[ -x ./quic-spike ] || { echo "✗ ./quic-spike absent — lance prepare-device.sh côté PC."; exit 1; }

# Vérifie la somme de contrôle si sha256sum est disponible (termux-tools).
if command -v sha256sum >/dev/null 2>&1 && [ -f /sdcard/Download/quic-spike.SHA256SUMS ]; then
  echo "▶ Vérification SHA-256…"
  if ( cd /sdcard/Download && sha256sum -c quic-spike.SHA256SUMS >/dev/null 2>&1 ); then
    echo "  ✓ somme de contrôle vérifiée"
  else
    echo "  ⚠ somme non vérifiée (binaire local ≠ SHA256SUMS ?)"
  fi
fi

# ── 2. Clé publique du coordinateur ──────────────────────────────────────────
if [ -f "$SDCARD_DIR/client-pubkey.txt" ] && [ ! -f "$CLIENT_KEY_FILE" ]; then
  cp "$SDCARD_DIR/client-pubkey.txt" "$CLIENT_KEY_FILE"
fi
CLIENT_KEY="$(cat "$CLIENT_KEY_FILE" 2>/dev/null | tr -d '[:space:]')"
[ -n "$CLIENT_KEY" ] || {
  echo "✗ $CLIENT_KEY_FILE absent/vide."
  echo "  Côté PC : bash scripts/campaign/run-campaign.sh … (pousse la clé),"
  echo "  ou crée-le : echo <clé-hex-du-PC> > $CLIENT_KEY_FILE"
  exit 1; }

# ── 3. Clé publique de ce téléphone ──────────────────────────────────────────
echo "▶ Clé publique de CE téléphone (nœud B) :"
B_PUBKEY="$(./quic-spike pubkey --key-file "$KEY_FILE")"
echo "  $B_PUBKEY"
if [ -d "$SDCARD_DIR" ]; then
  printf '%s\n' "$B_PUBKEY" > "$SDCARD_DIR/b-pubkey.txt" 2>/dev/null || true
fi

# ── 4. Adresse annoncée ──────────────────────────────────────────────────────
if [ -z "${ADVERTISE:-}" ]; then
  # Auto-détection de l'IP Wi-Fi (meilleur effort) ; sinon fallback USB.
  WIFI_IP="$(ip route get 1.1.1.1 2>/dev/null | awk '{for(i=1;i<=NF;i++) if($i=="src"){print $(i+1); exit}}')"
  if [ -n "$WIFI_IP" ]; then
    ADVERTISE="$WIFI_IP:$PORT"
    echo "▶ Wi-Fi détecté : advertise=$ADVERTISE (surcharge avec ADVERTISE=...)"
  else
    ADVERTISE="127.0.0.1:$PORT"
    echo "▶ Pas d'IP Wi-Fi détectée : advertise=$ADVERTISE (liaison USB)"
    echo "  Pour le Wi-Fi : ADVERTISE=<IP-du-téléphone>:$PORT bash worker-start.sh"
  fi
fi

mkdir -p "$SDCARD_DIR" 2>/dev/null || true
rm -f "$SDCARD_DIR/offer.json" 2>/dev/null || true

# Miroir de l'offre vers /sdcard (canal de sortie vers le PC), en arrière-plan.
# L'offre contient un jeton à usage unique TTL 30 s : cadence 3 s suffisante.
mirror_loop() {
  while true; do
    [ -f "$OFFER" ] && cp "$OFFER" "$SDCARD_DIR/" 2>/dev/null
    sleep 3
  done
}
mirror_loop &
MIRROR_PID=$!
trap 'kill "$MIRROR_PID" 2>/dev/null' EXIT INT TERM

echo
echo "▶ Démarrage du worker — bind 0.0.0.0:$PORT, annonce $ADVERTISE"
echo "  Offre            : ~/campaign/$OFFER + $SDCARD_DIR/$OFFER"
echo "  Client autorisé  : $CLIENT_KEY"
echo "  (Ctrl-C pour arrêter ; l'offre est régénérée toutes les 20 s)"
echo
./quic-spike serve \
  --bind "0.0.0.0:$PORT" \
  --advertise "$ADVERTISE" \
  --key-file "$KEY_FILE" \
  --client-key "$CLIENT_KEY" \
  --offer-out "$OFFER"
STATUS=$?
kill "$MIRROR_PID" 2>/dev/null
exit $STATUS
