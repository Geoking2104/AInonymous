#!/usr/bin/env bash
#
# Campagne CH3/S4 — vérification du kit côté nœud B (Termux, Android).
#
# Usage : bash check-termux.sh    (depuis ~/campaign, dans Termux)
#
# Sortie conçue pour être collée dans la page site/install-android.html :
#   CHECK clé=valeur / MISSING clé / WARN clé=valeur / SUMMARY ok/total
#
set -u
cd "$(dirname "$0")"

checks=0
total=0
ck() { # ck <clé> <valeur> (vide = manquant)
  total=$((total+1))
  if [ -n "$2" ]; then
    checks=$((checks+1))
    echo "CHECK $1=$2"
  else
    echo "MISSING $1"
  fi
}

echo "=== AINONYMOUS CAMPAIGN CHECK / node B (Termux) ==="

# ── Appareil ─────────────────────────────────────────────────────────────────
ck android-version "$(getprop ro.build.version.release 2>/dev/null)"
ck abi "$(getprop ro.product.cpu.abi 2>/dev/null)"

# ── Binaire de mesure ────────────────────────────────────────────────────────
if [ -x ./quic-spike ]; then
  ck quic-spike present
  PUB="$(./quic-spike pubkey --key-file check.key 2>/dev/null | head -n1 | tr -d '[:space:]')"
  if [ -n "$PUB" ]; then
    ck quic-spike-run "${PUB:0:8}..."
  else
    ck quic-spike-run ""
  fi
else
  ck quic-spike ""
  ck quic-spike-run ""
fi

# ── Intégrité ────────────────────────────────────────────────────────────────
if [ -f ./quic-spike.SHA256SUMS ] && command -v sha256sum >/dev/null 2>&1; then
  if sha256sum -c ./quic-spike.SHA256SUMS >/dev/null 2>&1; then
    ck sha256 ok
  else
    ck sha256 ""
  fi
else
  ck sha256 ""
fi

# ── Scripts du kit ───────────────────────────────────────────────────────────
if [ -f ./worker-start.sh ]; then ck worker-script present; else ck worker-script ""; fi

# Fichier poussé par prepare-device.sh côté PC (encore sur /sdcard ?)
if [ -f /sdcard/Download/quic-spike ]; then ck pushed-binary present; else ck pushed-binary ""; fi

# ── Canal de sortie de l'offre (/sdcard) ─────────────────────────────────────
SD=/sdcard/Download/campaign
if mkdir -p "$SD" 2>/dev/null && touch "$SD/.write-test" 2>/dev/null; then
  ck storage-mirror writable
  rm -f "$SD/.write-test"
else
  ck storage-mirror ""
fi

# ── IP Wi-Fi (informative : liaison USB sinon) ───────────────────────────────
WIFI_IP="$(ip route get 1.1.1.1 2>/dev/null | awk '{for(i=1;i<=NF;i++) if($i=="src"){print $(i+1); exit}}')"
if [ -n "$WIFI_IP" ]; then
  echo "WARN wifi-ip=$WIFI_IP"
else
  echo "WARN wifi-ip=absent (liaison USB)"
fi

echo "SUMMARY $checks/$total"
