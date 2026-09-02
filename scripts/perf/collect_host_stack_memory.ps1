param(
    [string]$OutputFile = ""
)

$ErrorActionPreference = "Stop"

$names = @(
    "postgres",
    "pg_ctl",
    "redis-server",
    "redis",
    "vault",
    "caddy",
    "fms-server",
    "rocketmq-namesrv-rust",
    "rocketmq-broker-rust",
    "fms-mq-gateway",
    "fms_mq_gateway"
)

$matched = @()
$totalBytes = [int64]0
$seen = @{}

Get-Process -ErrorAction SilentlyContinue | ForEach-Object {
    $procName = $_.ProcessName
    if ($names -notcontains $procName) {
        return
    }
    $key = "{0}:{1}" -f $procName, $_.Id
    if ($seen.ContainsKey($key)) {
        return
    }
    $seen[$key] = $true
    $ws = [int64]$_.WorkingSet64
    $totalBytes += $ws
    $matched += [ordered]@{
        name           = $procName
        pid            = $_.Id
        working_set_mb = [math]::Round($ws / 1MB, 1)
        private_mb     = [math]::Round(($_.PrivateMemorySize64 / 1MB), 1)
    }
}

$payload = [ordered]@{
    timestamp          = (Get-Date -Format "o")
    process_count      = $matched.Count
    total_working_set_mb = [math]::Round($totalBytes / 1MB, 1)
    processes          = $matched
}

$json = $payload | ConvertTo-Json -Depth 6
if ($OutputFile) {
    $dir = Split-Path -Parent $OutputFile
    if ($dir) {
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
    }
    $json | Out-File -FilePath $OutputFile -Encoding utf8
}

Write-Output $json
