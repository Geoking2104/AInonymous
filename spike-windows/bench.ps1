# S3 (#29) — débit de référence d'un 7B Q4 par backend (porte G2 : ≥ 80 % de la baseline).
# Baseline = binaire officiel llama.cpp (release b-xxxx du même commit si possible) ;
# candidat = binaire compilé par build_llama_windows.ps1 (celui qu'embarquera le relay).
# Usage : powershell -File bench.ps1 -Model C:\models\qwen2.5-7b-instruct-q4_k_m.gguf -Baseline C:\llama-official
param([Parameter(Mandatory)][string]$Model, [Parameter(Mandatory)][string]$Baseline, [int]$Reps = 5)
$ErrorActionPreference = 'Stop'
$res = Join-Path $PSScriptRoot ("results\" + (Get-Date -Format 'yyyyMMdd-HHmm')); New-Item -ItemType Directory -Force $res | Out-Null
foreach ($v in 'cuda','vulkan') {
  $cand = Join-Path $PSScriptRoot "out\$v\bin\Release\llama-bench.exe"
  if (Test-Path $cand) { & $cand -m $Model -ngl 99 -p 512 -n 128 -r $Reps -o json | Out-File "$res\candidate-$v.json" }
  $base = Get-ChildItem -Recurse -Filter llama-bench.exe "$Baseline\$v" -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($base) { & $base.FullName -m $Model -ngl 99 -p 512 -n 128 -r $Reps -o json | Out-File "$res\baseline-$v.json" }
}
"Résultats bruts : $res — ratio G2 = candidat.tg128 / baseline.tg128 (à calculer, ne pas arrondir à la hausse)"
