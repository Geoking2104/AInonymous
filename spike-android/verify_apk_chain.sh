#!/usr/bin/env bash
# S1 x S5 : double vérification d'un APK. (1) Signature Android v2/v3 (apksigner).
# (2) Manifeste de release signé (relay-release) : hash et anti-rejeu.
# Puis on contrôle que les deux mécanismes refusent un APK altéré d'un octet.
# Usage : ANDROID_HOME=... bash verify_apk_chain.sh <apk non signé>
set -euo pipefail
APK_IN="$(realpath "${1:?apk}")"
BT="$ANDROID_HOME/build-tools/35.0.0"
REPO="$(cd "$(dirname "$0")/.." && pwd)"
RR="$REPO/target/debug/relay-release"
[ -x "$RR" ] || (cd "$REPO" && cargo build -q -p relay-release-spike)
W="$(mktemp -d)"; cd "$W"
keytool -genkeypair -keystore test.jks -storepass testpass -keypass testpass -alias spike \
  -keyalg EC -groupname secp256r1 -validity 30 -dname "CN=AInonymous spike test" >/dev/null 2>&1
"$BT/zipalign" -f -p 4 "$APK_IN" aligned.apk
"$BT/apksigner" sign --ks test.jks --ks-pass pass:testpass --out relay.apk aligned.apk
echo "APK signé : $(stat -c %s relay.apk) octets"
"$BT/apksigner" verify relay.apk && echo "apksigner : OK"
PUB=$("$RR" keygen --out release.key | tail -1 | awk '{print $NF}')
"$RR" manifest --platform android-arm64 --version 0.0.1 --sequence 1 --artifact relay.apk \
  --kind apk --url https://example.invalid/relay.apk --out m.json
"$RR" sign --key release.key --manifest m.json --out m.signed.json
mkdir -p good bad && cp relay.apk good/ && cp relay.apk bad/
printf 'X' | dd of=bad/relay.apk bs=1 seek=$(( $(stat -c %s relay.apk) / 2 )) conv=notrunc 2>/dev/null
if "$BT/apksigner" verify bad/relay.apk >/dev/null 2>&1; then echo "ERREUR : apksigner accepte l'APK altéré"; exit 1; else echo "apksigner : APK altéré REFUSÉ"; fi
if "$RR" install --signed m.signed.json --trust "$PUB" --platform android-arm64 --artifact bad/relay.apk --root rootA 2>err.txt; then
  echo "ERREUR : relay-release accepte l'APK altéré"; exit 1; else echo "relay-release : APK altéré REFUSÉ ($(tail -1 err.txt))"; fi
"$RR" install --signed m.signed.json --trust "$PUB" --platform android-arm64 --artifact good/relay.apk --root rootB && echo "relay-release : APK intègre installé"
if "$RR" install --signed m.signed.json --trust "$PUB" --platform android-arm64 --artifact good/relay.apk --root rootB 2>err.txt; then
  echo "ERREUR : rejeu accepté"; exit 1; else echo "relay-release : rejeu REFUSÉ ($(tail -1 err.txt))"; fi
