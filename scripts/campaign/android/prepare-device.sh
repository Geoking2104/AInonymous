#!/usr/bin/env bash
#
# Préparation d'un téléphone Android comme nœud B (worker) de la campagne CH3/S4.
#
# Étapes :
#   1. vérifie l'appareil connecté (aarch64, SDK >= 24) ;
#   2. installe Termux si absent (APK F-Droid fourni via TERMUX_APK, ou
#      téléchargé depuis downloads.termux.dev) ;
#   3. pousse dist/android/quic-spike + SHA256SUMS dans Termux
#      (/data/data/com.termux/files/home/campaign) via adb push + termux-setup ;
#   4. définit la clé du nœud B (générée à la première exécution sur le
#      téléphone) et l'affiche pour enregistrement côté nœud A ;
#   5. établit la redirection adb forward tcp:7700 -> device tcp:7700 ;
#   6. démarre le worker en mode campagne via termux-job-service ou run-as
#      à travers `adb shell` (selon disponibilité).
#
# Le binaire natif s'exécute dans le shell Termux : Android 7+ exécute les
# binaires natifs linked bionic (API 24+) sans root ni runtime ART.
#
# Usage (depuis la racine du dépôt) :
#   bash scripts/campaign/android/prepare-device.sh
#
# Env :
#   ADB          chemin adb (défaut : adb)
#   TERMUX_APK   chemin d'un APK Termux F-Droid déjà téléchargé (facultatif,
#                évite le téléchargement)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
ADB="${ADB:-adb}"
DIST="$ROOT/dist/android"
BIN="$DIST/quic-spike"
DEV_DIR="/data/data/com.termux/files/home/campaign"
PORT="${PORT:-7700}"

[ -e "$BIN" ] || { echo "✗ $BIN absent — lance d'abord scripts/campaign/cross-compile-android.sh"; exit 1; }
command -v "$ADB" >/dev/null 2>&1 || { echo "✗ adb introuvable — installe platform-tools et/ou définis ADB=/chemin/adb"; exit 1; }

# ── 1. Appareil ──────────────────────────────────────────────────────────────
"$ADB" wait-for-device
SERIAL="$("$ADB" get-serialno)"
MODEL="$("$ADB" shell getprop ro.product.model | tr -d '\r')"
ANDROID_VER="$("$ADB" shell getprop ro.build.version.release | tr -d '\r')"
SDK="$("$ADB" shell getprop ro.build.version.sdk | tr -d '\r')"
ABI="$("$ADB" shell getprop ro.product.cpu.abi | tr -d '\r')"
echo "▶ Appareil : $MODEL — Android $ANDROID_VER (SDK $SDK, $ABI, $SERIAL)"

[ "${ABI:-}" = "arm64-v8a" ] || { echo "✗ ABI $ABI != arm64-v8a : ce kit cible aarch64."; exit 1; }
[ "${SDK:-0}" -ge 24 ] || { echo "✗ SDK $SDK < 24."; exit 1; }

# ── 2. Termux ────────────────────────────────────────────────────────────────
if ! "$ADB" shell pm list packages 2>/dev/null | tr -d '\r' | grep -q '^package:com.termux$'; then
  echo "▶ Termux absent — installation"
  APK_CACHE="$ROOT/dist/android/termux.apk"
  if [ -n "${TERMUX_APK:-}" ] && [ -f "$TERMUX_APK" ]; then
    cp "$TERMUX_APK" "$APK_CACHE"
  elif [ ! -f "$APK_CACHE" ]; then
    # APK « github-debug » : le seul canal qui correspond à l'application
    # F-Droid (signature identique). downloads.termux.dev est le miroir officiel.
    TERMUX_URL="https://downloads.termux.dev/android/dists/stable/packages/termux-app_1000.apk"
    echo "  téléchargement : $TERMUX_URL"
    if command -v curl >/dev/null 2>&1; then
      curl -fL "$TERMUX_URL" -o "$APK_CACHE"
    elif command -v wget >/dev/null 2>&1; then
      wget "$TERMUX_URL" -O "$APK_CACHE"
    else
      echo "✗ ni curl ni wget — télécharge Termux manuellement et passe TERMUX_APK=/chemin/termux.apk" >&2
      exit 1
    fi
  fi
  "$ADB" install -r "$APK_CACHE"
fi

# Termux doit avoir été ouvert au moins une fois pour que son répertoire de
# données existe (installation ≠ initialisation).
if ! "$ADB" shell run-as com.termux ls /data/data/com.termux/files/home >/dev/null 2>&1 \
   && ! "$ADB" shell ls /data/data/com.termux/files/home >/dev/null 2>&1; then
  cat >&2 <<'EOF'
✗ Répertoire de données Termux inexistant.
  Ouvre Termux une fois sur le téléphone (icône), attends le prompt, puis relance.
  (Android ne crée /data/data/com.termux qu'au premier démarrage de l'app.)
EOF
  exit 1
fi

# ── 3. Transfert des fichiers ────────────────────────────────────────────────
echo "▶ Poussée des fichiers vers Termux…"
# Le répertoire home de Termux n'est pas accessible en écriture via adb push
# direct sur Android >= 12 ; on passe par /sdcard puis déplacement interne
# (am broadcast RunScript n'existe qu'avec Termux:API, on reste sur le chemin
# compatible : /sdcard/Download puis cp via run-as).
"$ADB" push "$BIN" /sdcard/Download/quic-spike
"$ADB" push "$DIST/SHA256SUMS" /sdcard/Download/quic-spike.SHA256SUMS

# Copie interne + chmod. run-as fonctionne si l'APK est debuggable (build
# Termux github-debug) ; sinon repli sur une copie depuis /sdcard au premier
# lancement du script côté Termux (voir worker-start.sh, qui recopie depuis
# /sdcard/Download si le binaire interne manque).
"$ADB" shell run-as com.termux mkdir -p files/home/campaign 2>/dev/null \
  || "$ADB" shell mkdir -p "$DEV_DIR" 2>/dev/null \
  || true
"$ADB" shell run-as com.termux cp /sdcard/Download/quic-spike files/home/campaign/quic-spike 2>/dev/null \
  || "$ADB" shell cp /sdcard/Download/quic-spike "$DEV_DIR/quic-spike" 2>/dev/null \
  || true
"$ADB" shell run-as com.termux chmod 700 files/home/campaign/quic-spike 2>/dev/null \
  || "$ADB" shell chmod 700 "$DEV_DIR/quic-spike" 2>/dev/null \
  || true

# ── 4. Clé du nœud B ─────────────────────────────────────────────────────────
# Générée à la première exécution de campaign-worker.sh sur le téléphone ;
# ici on affiche seulement la procédure (la clé ne quitte pas l'appareil).
cat <<EOF
▶ Prochaine étape (sur le téléphone, dans Termux) :
    cd ~/campaign
    bash worker-start.sh          # ou : ./quic-spike pubkey --key-file b.key
    # → affiche la clé publique hex du nœud B : note-la pour le nœud A
EOF

# ── 5. Redirection de port adb (USB) ─────────────────────────────────────────
if "$ADB" forward tcp:$PORT tcp:$PORT >/dev/null 2>&1; then
  echo "✓ Redirection adb : 127.0.0.1:$PORT (PC) → téléphone :$PORT (USB)"
  echo "  (le pare-feu du téléphone n'a pas à être ouvert pour ce chemin)"
else
  echo "⚠ Impossible d'établir la redirection adb forward — mode Wi-Fi requis."
fi

# ── 6. Note sur le transfert de l'offre (facultatif) ───────────────────────
# L'offre (offer.json) doit voyager du téléphone vers le PC. En USB, adb reverse
# permet au téléphone de pousser vers le PC : démarré par run-campaign.sh côté A.
echo "✓ Préparation terminée. Ensuite :"
echo "   nœud A : bash scripts/campaign/run-campaign.sh --links usb --runs 3"
