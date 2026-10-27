# S3 (#29) — compiler llama.cpp (même commit que le patch pipeline-split) en CUDA et en Vulkan.
# Prérequis : Visual Studio 2022 Build Tools (MSVC, CMake), Git, CUDA Toolkit 12.x
# (pour la variante cuda), Vulkan SDK (pour la variante vulkan).
# Usage : powershell -File build_llama_windows.ps1 -Variant cuda|vulkan|cpu
param([ValidateSet('cuda','vulkan','cpu')][string]$Variant = 'vulkan',
      [string]$Src = "$env:TEMP\llama.cpp-win",
      [string]$Commit = '42fc243060709331ff9b158a9ed2cbe37219ae83')
$ErrorActionPreference = 'Stop'
if (-not (Test-Path "$Src\.git")) { git clone --filter=blob:none https://github.com/ggml-org/llama.cpp.git $Src }
git -C $Src fetch --filter=blob:none origin $Commit; git -C $Src checkout $Commit
$flags = @('-DCMAKE_BUILD_TYPE=Release','-DLLAMA_CURL=OFF')
switch ($Variant) { 'cuda' { $flags += '-DGGML_CUDA=ON' } 'vulkan' { $flags += '-DGGML_VULKAN=ON' } }
$out = Join-Path $PSScriptRoot "out\$Variant"
cmake -S $Src -B $out @flags
cmake --build $out --config Release -j --target llama-bench llama-server llama-cli
Get-ChildItem "$out\bin\Release" | Select-Object Name, Length
