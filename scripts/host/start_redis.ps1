param(
    [int]$Port = 6379
)

$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path (Join-Path $scriptDir "..\..")).Path
$redisExe = Join-Path $repoRoot "Redis\redis-server.exe"

if (-not (Test-Path $redisExe)) {
    throw "redis-server.exe not found: $redisExe"
}

$existing = Get-Process -Name "redis-server" -ErrorAction SilentlyContinue | Select-Object -First 1
if ($existing) {
    Write-Host "[INFO] Redis already running (PID: $($existing.Id))"
    exit 0
}

$redisPassword = $env:REDIS_PASSWORD
if (-not $redisPassword) {
    $redisPassword = $env:REDISCLI_AUTH
}

$runtimeDir = Join-Path $repoRoot ".runtime\host-services\redis"
New-Item -ItemType Directory -Force -Path $runtimeDir | Out-Null
$stdout = Join-Path $runtimeDir "redis.stdout.log"
$stderr = Join-Path $runtimeDir "redis.stderr.log"

$arguments = @("--port", [string]$Port, "--save", '""')
if ($redisPassword) {
    $arguments += @("--requirepass", $redisPassword)
}

$process = Start-Process -FilePath $redisExe `
    -ArgumentList $arguments `
    -WorkingDirectory (Split-Path -Parent $redisExe) `
    -WindowStyle Hidden `
    -PassThru `
    -RedirectStandardOutput $stdout `
    -RedirectStandardError $stderr

Start-Sleep -Seconds 2

if ($process.HasExited) {
    $tail = if (Test-Path $stdout) { Get-Content -LiteralPath $stdout -Tail 20 | Out-String } else { "" }
    throw "Redis exited during startup. Log: $tail"
}

Write-Host "[INFO] Redis started (PID: $($process.Id), port: $Port)"
