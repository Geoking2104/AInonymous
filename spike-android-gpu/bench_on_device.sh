#!/usr/bin/env bash
# S2 (#28) — mesures sur un téléphone réel via adb (USB ou adb over Wi-Fi).
#
#   bash bench_on_device.sh <out/vulkan|out/cpu> <modele.gguf> [label]
#
# Le script produit dans results/<date>-<label>/ :
#   device.txt     modèle, SoC, Android, GPU / pilote Vulkan, RAM
#   bench.txt      llama-bench pp512/tg128 avec -ngl 99 (GPU) puis -ngl 0 (CPU)
#   soak.csv       10 min de génération continue : t, tok/s, températures, batterie
# Ces mesures ne peuvent pas être faites sur la box : il faut l'appareil de Geoffroy.
set -euo pipefail
BUILD="${1:?dossier build}"; MODEL="${2:?modèle gguf}"; LABEL="${3:-$(basename "$MODEL" .gguf)}"
D=/data/local/tmp/relay-s2
R="results/$(date +%Y%m%d-%H%M)-$LABEL"; mkdir -p "$R"
adb shell mkdir -p $D
adb push "$BUILD/bin/." $D/ >/dev/null
adb push "$MODEL" $D/model.gguf >/dev/null
adb shell chmod +x $D/llama-bench $D/llama-cli
{
  adb shell getprop ro.product.model; adb shell getprop ro.soc.model; adb shell getprop ro.build.version.release
  adb shell cat /proc/meminfo | head -2
  adb shell dumpsys SurfaceFlinger | grep -i 'GLES' || true
  adb shell cmd gpu vkjson 2>/dev/null | head -c 2000 || true
} > "$R/device.txt"
for NGL in 99 0; do
  echo "== -ngl $NGL" >> "$R/bench.txt"
  adb shell "cd $D && LD_LIBRARY_PATH=$D ./llama-bench -m model.gguf -ngl $NGL -p 512 -n 128 -r 3" >> "$R/bench.txt" 2>&1 || echo "ÉCHEC -ngl $NGL" >> "$R/bench.txt"
done
echo "t_s,tok_s,temp_max_c,battery_level,battery_temp_dc,charging" > "$R/soak.csv"
T0=$(date +%s)
while [ $(( $(date +%s) - T0 )) -lt 600 ]; do
  OUT=$(adb shell "cd $D && LD_LIBRARY_PATH=$D ./llama-bench -m model.gguf -ngl 99 -p 0 -n 64 -r 1 -o jsonl" 2>/dev/null | tail -1)
  TOKS=$(echo "$OUT" | sed -n 's/.*"avg_ts": *\([0-9.]*\).*/\1/p')
  TMAX=$(adb shell 'for z in /sys/class/thermal/thermal_zone*/temp; do cat $z; done' 2>/dev/null | sort -n | tail -1)
  BAT=$(adb shell dumpsys battery)
  LVL=$(echo "$BAT" | awk '/level:/{print $2}'); BT=$(echo "$BAT" | awk '/temperature:/{print $2}')
  CH=$(echo "$BAT" | awk '/USB powered:|AC powered:/{if($3=="true")c=1}END{print c+0}')
  echo "$(( $(date +%s) - T0 )),$TOKS,$(( ${TMAX:-0} / 1000 )),$LVL,$BT,$CH" >> "$R/soak.csv"
done
echo "Résultats : $R"
