# S3 (#29) — inventaire GPU Windows (DXGI via WMI, NVML via nvidia-smi, Vulkan via vulkaninfo).
# Usage : powershell -ExecutionPolicy Bypass -File detect_gpu.ps1 > results\gpu.json
$ErrorActionPreference = 'Continue'
$r = [ordered]@{ date = (Get-Date).ToString('s'); os = (Get-CimInstance Win32_OperatingSystem).Caption }
$r.adapters = Get-CimInstance Win32_VideoController | ForEach-Object {
  [ordered]@{ name = $_.Name; driver = $_.DriverVersion; vram_bytes_wmi = $_.AdapterRAM; pnp = $_.PNPDeviceID }
}
# AdapterRAM (WMI) plafonne à 4 Go : la VRAM réelle est lue via le registre DXGI.
$r.vram_registry = Get-ItemProperty 'HKLM:\SYSTEM\ControlSet001\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0*' -ErrorAction SilentlyContinue |
  Where-Object { $_.'HardwareInformation.qwMemorySize' } |
  ForEach-Object { [ordered]@{ name = $_.DriverDesc; vram_bytes = $_.'HardwareInformation.qwMemorySize' } }
if (Get-Command nvidia-smi -ErrorAction SilentlyContinue) {
  $r.nvml = (nvidia-smi --query-gpu=name,driver_version,memory.total,compute_cap --format=csv,noheader) -split "`n"
} else { $r.nvml = 'nvidia-smi absent' }
if (Get-Command vulkaninfo -ErrorAction SilentlyContinue) {
  $r.vulkan = (vulkaninfo --summary 2>$null | Select-String 'deviceName|apiVersion|driverVersion') -replace '\s+', ' '
} else { $r.vulkan = 'vulkaninfo absent (installer le Vulkan SDK ou le runtime)' }
$r | ConvertTo-Json -Depth 4
