# =====================================================================
# Campagne CH3/S4 - verification de l'installation cote noeud A (Windows)
#
# Usage (depuis la racine du depot, PowerShell) :
#   powershell -ExecutionPolicy Bypass -File scripts\campaign\check-windows.ps1
#
# Sortie concue pour etre colle dans la page site/install-windows.html :
#   CHECK cle=valeur / MISSING cle / WARN cle=valeur / SUMMARY ok/total
#
# Remarque : la campagne CH3/S4 n'exige PAS Holochain/Lair (le spike S4
# remplace le plan de controle par l'echange d'offre hors bande). Les
# prerequis ci-dessous sont donc uniquement ceux du kit de mesure.
# =====================================================================

$ErrorActionPreference = 'SilentlyContinue'

# Racine du depot (le script vit dans scripts\campaign)
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$repo = Split-Path -Parent (Split-Path -Parent $root)

function FirstLine([string]$exe, [string]$arg) {
    $out = & $exe $arg 2>$null | Select-Object -First 1
    if ($out) { return ($out -join ' ').Trim() }
    return $null
}

$script:checks = 0
$script:total  = 0
function Check([string]$key, $value) {
    $script:total++
    if ($value) { $script:checks++; Write-Output "CHECK $key=$value" }
    else { Write-Output "MISSING $key" }
}

Write-Output '=== AINONYMOUS CAMPAIGN CHECK / node A (Windows) ==='

# ── Outils requis ────────────────────────────────────────────────────────────
Check 'git'    (FirstLine 'git' '--version')
Check 'rustc'  (FirstLine 'rustc' '--version')
Check 'cargo'  (FirstLine 'cargo' '--version')
Check 'python' (FirstLine 'python' '--version')
Check 'adb'    (FirstLine 'adb' 'version')

# ── NDK (optionnel : requis seulement pour l'etape de compilation croisee) ──
$ndk = $env:ANDROID_NDK_HOME
if (-not $ndk) { $ndk = $env:ANDROID_NDK_ROOT }
if (-not $ndk -and $env:LOCALAPPDATA) {
    $ndkRoot = Join-Path $env:LOCALAPPDATA 'Android\Sdk\ndk'
    if (Test-Path $ndkRoot) {
        $latest = Get-ChildItem $ndkRoot -Directory | Sort-Object Name | Select-Object -Last 1
        if ($latest) { $ndk = $latest.FullName }
    }
}
if ($ndk) { Write-Output "WARN ndk=$ndk" } else { Write-Output 'WARN ndk=absent' }

# ── Binaires de campagne ─────────────────────────────────────────────────────
$pcBin = Join-Path $repo 'target\release\quic-spike.exe'
if (Test-Path $pcBin) {
    Check 'quic-spike-pc' 'ok'
    $null = & $pcBin --help 2>&1
    if ($LASTEXITCODE -eq 0) { Check 'quic-spike-run' 'ok' } else { Check 'quic-spike-run' $null }
} else {
    Check 'quic-spike-pc' $null
    Check 'quic-spike-run' $null
}

$androidBin = Join-Path $repo 'dist\android\quic-spike'
if (Test-Path $androidBin) { Check 'quic-spike-android' 'ok' } else { Check 'quic-spike-android' $null }

$sums = Join-Path $repo 'dist\android\SHA256SUMS'
if (Test-Path $sums) { Check 'sha256sums' 'ok' } else { Check 'sha256sums' $null }

Write-Output "SUMMARY $script:checks/$script:total"
