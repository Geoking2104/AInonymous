# S3 (#29) / G3 — installeur per-user : durée d'installation et de désinstallation, absence
# d'élévation, signature. À lancer dans une session utilisateur standard (pas admin).
# Usage : powershell -File installer_check.ps1 -Installer .\AInonymous-Relay_0.0.1_x64-setup.exe
param([Parameter(Mandatory)][string]$Installer)
$id = [Security.Principal.WindowsIdentity]::GetCurrent()
$admin = (New-Object Security.Principal.WindowsPrincipal $id).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
"Session admin : $admin (doit être False)"
Get-AuthenticodeSignature $Installer | Format-List Status, SignerCertificate
$t = Measure-Command { Start-Process $Installer -ArgumentList '/S' -Wait }
"Installation silencieuse : $([math]::Round($t.TotalSeconds,1)) s"
$app = Get-ChildItem "$env:LOCALAPPDATA\Programs" -Directory | Where-Object Name -like '*AInonymous*'
"Installé dans : $($app.FullName) (doit être sous %LOCALAPPDATA%)"
$un = Get-ChildItem $app.FullName -Filter 'uninstall*.exe' | Select-Object -First 1
$t2 = Measure-Command { Start-Process $un.FullName -ArgumentList '/S' -Wait }
"Désinstallation silencieuse : $([math]::Round($t2.TotalSeconds,1)) s ; reste : $(Test-Path $app.FullName)"
