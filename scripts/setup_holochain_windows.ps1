# Install the exact Holochain toolchain used by this repository on x86_64 Windows.
#
# Mirrors scripts/setup_holochain_wsl.sh: same pinned release (holochain-0.7.0)
# and the same SHA-256 verification discipline, using the official
# x86_64-pc-windows-msvc release assets.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts/setup_holochain_windows.ps1
#   powershell -ExecutionPolicy Bypass -File scripts/setup_holochain_windows.ps1 `
#       -InstallDir C:\tools\holochain\bin -AddToPath
#
# Notes:
#   * The repository requires exactly 0.7.0. Any older installation (0.4.1) is
#     left untouched: this script only writes to -InstallDir.
#   * -AddToPath prepends -InstallDir to the *user* PATH. Without it, call the
#     binaries by absolute path or adjust $env:PATH for the session.
[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path $HOME 'holochain-0.7.0\bin'),
    [switch]$AddToPath,
    [switch]$KeepDownloads
)

$ErrorActionPreference = 'Stop'

$Version = '0.7.0'
$ReleaseTag = "holochain-$Version"
$BaseUrl = "https://github.com/holochain/holochain/releases/download/$ReleaseTag"
$Arch = 'x86_64-pc-windows-msvc'

# Pinned SHA-256 of the official Windows assets for holochain-0.7.0.
# Provenance: the GitHub release asset digests, cross-checked against the Linux
# values already pinned in .github/workflows/ci.yml (the same three assets,
# same release). Update both files together when bumping Holochain.
$Assets = @(
    @{ Name = 'hc';            Asset = "hc-$Arch.exe";            Sha256 = '643ed26e84a7ae72a454bd78b9427b50883fb1306406ba7b5fa4857c61c6b322' },
    @{ Name = 'holochain';     Asset = "holochain-$Arch.exe";     Sha256 = '5d9bd8cf28c45645158d56198cfec38d04ac4cf620ba89ca37db2f7c46d7b3d2' },
    @{ Name = 'lair-keystore'; Asset = "lair-keystore-$Arch.exe"; Sha256 = '2c1fded6eb7c6ec4b3d033c6a310d7c2cc916d6e0ac10c2f71047ebb6b623f6b' }
)

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("holochain-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null

try {
    foreach ($a in $Assets) {
        $url = "$BaseUrl/$($a.Asset)"
        $downloaded = Join-Path $tmp $a.Asset
        Write-Host "Downloading $($a.Asset)"
        Invoke-WebRequest -Uri $url -OutFile $downloaded -UseBasicParsing

        $hash = (Get-FileHash -Path $downloaded -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($hash -ne $a.Sha256) {
            throw "SHA-256 mismatch for $($a.Asset): got $hash, expected $($a.Sha256)"
        }

        $target = Join-Path $InstallDir ($a.Name + '.exe')
        Copy-Item -Path $downloaded -Destination $target -Force
        Write-Host "Installed $target"
    }
}
finally {
    if (-not $KeepDownloads) {
        Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
    }
}

$hc = Join-Path $InstallDir 'hc.exe'
$holochain = Join-Path $InstallDir 'holochain.exe'
$lair = Join-Path $InstallDir 'lair-keystore.exe'

& $holochain --version | Out-Host
& $hc --version | Out-Host
& $lair --version | Out-Host

foreach ($exe in @($hc, $holochain)) {
    $out = (& $exe --version 2>&1 | Out-String)
    if ($out -notmatch [regex]::Escape($Version)) {
        throw "Unexpected version reported by ${exe}: $out"
    }
}

# Surface asserted by .github/workflows/ci.yml.
$help = (& $hc client call --help 2>&1 | Out-String)
foreach ($needle in @('new-agent', 'list-app-ws', 'install-app')) {
    if ($help -notmatch $needle) {
        throw "hc client call --help is missing '$needle'"
    }
}

if ($AddToPath) {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (-not $userPath) { $userPath = '' }
    $parts = @($userPath -split ';' | Where-Object { $_ -ne '' })
    if ($parts -notcontains $InstallDir) {
        [Environment]::SetEnvironmentVariable(
            'Path',
            ((@($InstallDir) + $parts) -join ';'),
            'User'
        )
        Write-Host "Prepended $InstallDir to the user PATH (new terminals only)."
    }
    else {
        Write-Host "$InstallDir is already on the user PATH."
    }
}

Write-Host ''
Write-Host "Holochain $Version installed in $InstallDir"
Write-Host "Current session: `$env:PATH = '$InstallDir;' + `$env:PATH"
Write-Host 'Older installations (e.g. 0.4.1) are untouched; keep them in a separate directory.'
