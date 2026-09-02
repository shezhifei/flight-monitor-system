param(
    [int]$StackMemoryMb = 3072,
    [int]$MaxConnections = 64,
    [switch]$LowLatencyWrites,
    [switch]$ApplyPostgres,
    [int]$RedisMaxMemoryMb = 128
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$python = Join-Path $repoRoot ".venv\Scripts\python.exe"
if (-not (Test-Path $python)) {
    $python = "python"
}

$outDir = Join-Path $repoRoot ".tmp\perf"
New-Item -ItemType Directory -Path $outDir -Force | Out-Null
$confPath = Join-Path $outDir "postgresql-host-perf.conf"
$jsonPath = Join-Path $outDir "postgres-tune.json"

$tuneArgs = @(
    (Join-Path $repoRoot "scripts\perf\tune_postgres.py"),
    "--stack-memory-mb", "$StackMemoryMb",
    "--max-connections", "$MaxConnections",
    "--out-conf", $confPath,
    "--out-json", $jsonPath
)
if ($LowLatencyWrites) {
    $tuneArgs += "--low-latency-writes"
}
if ($ApplyPostgres) {
    if (-not $env:DATABASE_URL) {
        throw "DATABASE_URL is required for -ApplyPostgres"
    }
    $tuneArgs += @("--apply", "--iterate", "--rounds", "1")
}

Write-Host "Computing PostgreSQL host-perf settings (stack ${StackMemoryMb}MB)..."
& $python @tuneArgs
if ($LASTEXITCODE -ne 0) {
    throw "tune_postgres.py failed"
}

$redisHost = if ($env:REDIS_HOST) { $env:REDIS_HOST } else { "127.0.0.1" }
$redisPort = if ($env:REDIS_PORT) { $env:REDIS_PORT } else { "6379" }
$previous = $env:REDISCLI_AUTH
try {
    if ($env:REDIS_PASSWORD) {
        $env:REDISCLI_AUTH = $env:REDIS_PASSWORD
    }
    $redisCli = Get-Command redis-cli -ErrorAction SilentlyContinue
    if ($redisCli) {
        Write-Host "Setting Redis maxmemory=${RedisMaxMemoryMb}mb"
        & redis-cli -h $redisHost -p $redisPort CONFIG SET maxmemory "${RedisMaxMemoryMb}mb" | Out-Null
        & redis-cli -h $redisHost -p $redisPort CONFIG SET maxmemory-policy allkeys-lru | Out-Null
        & redis-cli -h $redisHost -p $redisPort CONFIG SET save "" | Out-Null
    } else {
        Write-Warning "redis-cli not on PATH; skip Redis maxmemory"
    }
} finally {
    $env:REDISCLI_AUTH = $previous
}

$envFile = Join-Path $outDir "host-perf.env"
@(
    "ANTI_REPLAY_STORE=local",
    "DB_POOL_MAX_CONNECTIONS=24",
    "DB_POOL_MIN_CONNECTIONS=8",
    "DB_POOL_ACQUIRE_TIMEOUT_SECS=5",
    "REDIS_POOL_MAX_SIZE=64",
    "REDIS_POOL_MIN_IDLE=16",
    "AUTH_FRESHNESS_CACHE_TTL_MS=5000",
    "AUTH_PERMISSION_VERSION_CACHE_TTL_MS=5000",
    "AUTH_CLAIMS_CACHE_TTL_MS=2000",
    "AUTH_CACHE_MAX_ENTRIES=50000",
    "FMS_HTTP_ACCESS_LOG=0",
    "NOTIFICATION_UNREAD_CACHE_TTL_MS=2000"
) | Set-Content -Path $envFile -Encoding utf8

Write-Host "Wrote $confPath"
Write-Host "Wrote $envFile"
Write-Host "Restart fms-server after loading host-perf.env so ANTI_REPLAY_STORE=local and pool sizes take effect."
Write-Host "shared_buffers / max_connections / shared_preload_libraries need a PostgreSQL restart."
