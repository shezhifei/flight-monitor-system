$ErrorActionPreference = "Stop"

$processes = Get-Process -Name "caddy" -ErrorAction SilentlyContinue
if (-not $processes) {
    Write-Host "[WARN] Caddy is not running"
    exit 0
}

$processes | Stop-Process -Force
Write-Host "[INFO] Caddy stopped"
