param(
    [Parameter(Mandatory = $true)]
    [switch]$ConfirmReset,
    [string]$HcBin = "hc"
)

$ErrorActionPreference = "Stop"
$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$projectName = "ainonymous-hc07-v3"
$secretDir = Join-Path $projectRoot "deploy\secrets"
$secretFile = Join-Path $secretDir "holochain_keystore_password"

if (-not $ConfirmReset) {
    throw "-ConfirmReset is required. Only Compose project $projectName and its named volumes are removed."
}

foreach ($commandName in @("cargo", "docker", $HcBin)) {
    if (-not (Get-Command $commandName -ErrorAction SilentlyContinue)) {
        throw "Required command not found: $commandName"
    }
}

$hcVersion = (& $HcBin --version 2>&1 | Out-String).Trim()
if ($hcVersion -notmatch "0\.7\.0") {
    throw "HcBin must point to hc 0.7.0; found: $hcVersion"
}

New-Item -ItemType Directory -Path $secretDir -Force | Out-Null
if (-not (Test-Path -LiteralPath $secretFile) -or (Get-Item -LiteralPath $secretFile).Length -eq 0) {
    $randomBytes = [System.Security.Cryptography.RandomNumberGenerator]::GetBytes(48)
    [Convert]::ToBase64String($randomBytes) | Set-Content -LiteralPath $secretFile -NoNewline
}

Push-Location $projectRoot
try {
    & cargo build --locked --manifest-path dnas/ainonymous-core/Cargo.toml --release --target wasm32-unknown-unknown
    if ($LASTEXITCODE -ne 0) { throw "AInonymous zome build failed." }

    $ainWasm = "dnas/ainonymous-core/target/wasm32-unknown-unknown/release"
    Copy-Item "$ainWasm/inference_mesh_integrity.wasm" "dnas/ainonymous-core/dnas/inference-mesh/zomes/inference-mesh-integrity.wasm" -Force
    Copy-Item "$ainWasm/inference_mesh_coordinator.wasm" "dnas/ainonymous-core/dnas/inference-mesh/zomes/inference-mesh-coordinator.wasm" -Force
    Copy-Item "$ainWasm/agent_registry_integrity.wasm" "dnas/ainonymous-core/dnas/agent-registry/zomes/agent-registry-integrity.wasm" -Force
    Copy-Item "$ainWasm/agent_registry_coordinator.wasm" "dnas/ainonymous-core/dnas/agent-registry/zomes/agent-registry-coordinator.wasm" -Force
    Copy-Item "$ainWasm/blackboard_integrity.wasm" "dnas/ainonymous-core/dnas/blackboard/zomes/blackboard-integrity.wasm" -Force
    Copy-Item "$ainWasm/blackboard_coordinator.wasm" "dnas/ainonymous-core/dnas/blackboard/zomes/blackboard-coordinator.wasm" -Force

    & cargo build --locked --manifest-path dnas/hybridnode/Cargo.toml --release --target wasm32-unknown-unknown
    if ($LASTEXITCODE -ne 0) { throw "HybridNode zome build failed." }

    $hybridWasm = "dnas/hybridnode/target/wasm32-unknown-unknown/release"
    Copy-Item "$hybridWasm/hybridnode_integrity.wasm" "dnas/hybridnode/dnas/hybridnode-core/zomes/hybridnode-integrity.wasm" -Force
    Copy-Item "$hybridWasm/hybridnode_coordinator.wasm" "dnas/hybridnode/dnas/hybridnode-core/zomes/hybridnode-coordinator.wasm" -Force

    foreach ($workdir in @(
        "dnas/ainonymous-core/dnas/inference-mesh/workdir",
        "dnas/ainonymous-core/dnas/agent-registry/workdir",
        "dnas/ainonymous-core/dnas/blackboard/workdir",
        "dnas/hybridnode/dnas/hybridnode-core/workdir"
    )) {
        & $HcBin dna pack $workdir
        if ($LASTEXITCODE -ne 0) { throw "DNA packaging failed: $workdir" }
    }

    & $HcBin app pack dnas/ainonymous-core
    if ($LASTEXITCODE -ne 0) { throw "AInonymous hApp packaging failed." }
    & $HcBin app pack dnas/hybridnode
    if ($LASTEXITCODE -ne 0) { throw "HybridNode hApp packaging failed." }

    & cargo run --locked -p dna-hashes -- --check deploy/holochain/dna-hashes.json `
        dnas/ainonymous-core/dnas/inference-mesh/workdir/inference-mesh.dna `
        dnas/ainonymous-core/dnas/agent-registry/workdir/agent-registry.dna `
        dnas/ainonymous-core/dnas/blackboard/workdir/blackboard.dna `
        dnas/hybridnode/dnas/hybridnode-core/workdir/hybridnode-core.dna
    if ($LASTEXITCODE -ne 0) { throw "DNA hash verification failed." }

    & docker compose --project-name $projectName config --quiet
    if ($LASTEXITCODE -ne 0) { throw "Docker Compose validation failed." }
    & docker compose --project-name $projectName down --volumes --remove-orphans
    if ($LASTEXITCODE -ne 0) { throw "Existing stack teardown failed." }
    & docker compose --project-name $projectName build --pull --no-cache
    if ($LASTEXITCODE -ne 0) { throw "Container build failed." }
    & docker compose --project-name $projectName up --detach --wait
    if ($LASTEXITCODE -ne 0) { throw "Container provisioning failed." }
    & docker compose --project-name $projectName ps
}
finally {
    Pop-Location
}
