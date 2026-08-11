param(
    [Parameter(Mandatory)]
    [string]$OutputFile
)

$output = @{
    timestamp     = (Get-Date -Format "o")
    process_found = $false
    process       = @{}
    network       = @()
    redis         = @{}
    postgres      = @{}
}

$proc = Get-Process -Name "fms-server" -ErrorAction SilentlyContinue | Select-Object -First 1
if ($proc) {
    $output.process_found = $true
    $output.process = @{
        pid            = $proc.Id
        working_set_mb = [math]::Round($proc.WorkingSet64 / 1MB, 1)
        handle_count   = $proc.HandleCount
        thread_count   = $proc.Threads.Count
    }

    $connections = Get-NetTCPConnection -OwningProcess $proc.Id -ErrorAction SilentlyContinue |
        Select-Object LocalAddress, LocalPort, RemoteAddress, RemotePort, State
    foreach ($conn in $connections) {
        $output.network += @{
            local  = "$($conn.LocalAddress):$($conn.LocalPort)"
            remote = "$($conn.RemoteAddress):$($conn.RemotePort)"
            state  = $conn.State
        }
    }
}

$redisHost = if ($env:REDIS_HOST) { $env:REDIS_HOST } else { "127.0.0.1" }
$redisPort = if ($env:REDIS_PORT) { $env:REDIS_PORT } else { "6379" }
$redisPassword = $env:REDIS_PASSWORD
if (-not $redisPassword) { $redisPassword = $env:REDISCLI_AUTH }

$redisArgs = @("-h", $redisHost, "-p", $redisPort)

$previousRedisCliAuth = $env:REDISCLI_AUTH
try {
    if ($redisPassword) {
        $env:REDISCLI_AUTH = $redisPassword
    }
    $redisInfo = & redis-cli @redisArgs INFO stats 2>&1
    if ($LASTEXITCODE -eq 0) { $output.redis.stats = $redisInfo -join "`n" }
    $redisCmdStats = & redis-cli @redisArgs INFO commandstats 2>&1
    if ($LASTEXITCODE -eq 0) { $output.redis.commandstats = $redisCmdStats -join "`n" }
} catch {
} finally {
    $env:REDISCLI_AUTH = $previousRedisCliAuth
}

try {
    $pgHost = if ($env:PGHOST) { $env:PGHOST } else { "127.0.0.1" }
    $pgPort = if ($env:PGPORT) { $env:PGPORT } else { "5432" }
    $pgDb = if ($env:PGDATABASE) { $env:PGDATABASE } else { "flight_monitor_dev" }
    $pgUser = if ($env:PGUSER) { $env:PGUSER } else { "postgres" }

    $activity = & psql -h $pgHost -p $pgPort -d $pgDb -U $pgUser -v dbname="$pgDb" -c "SELECT state, count(*) FROM pg_stat_activity WHERE datname = :'dbname' GROUP BY state;" -t -A 2>&1
    if ($LASTEXITCODE -eq 0) { $output.postgres.activity = ($activity | Out-String) }
} catch {
    $output.postgres.error = $_.Exception.Message
}

$output | ConvertTo-Json -Depth 10 | Out-File -FilePath $OutputFile -Encoding utf8
Write-Host "Resources collected to $OutputFile"
