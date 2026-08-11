$ErrorActionPreference = "Stop"

$processes = Get-Process -Name "redis-server" -ErrorAction SilentlyContinue
if (-not $processes) {
    Write-Host "[WARN] Redis is not running"
    exit 0
}

$processes | Stop-Process -Force
Write-Host "[INFO] Redis stopped"
