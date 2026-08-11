[CmdletBinding()]
param(
    [switch]$AttachLogs
)

$ErrorActionPreference = "Stop"
if (Get-Variable -Name PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue) {
    $PSNativeCommandUseErrorActionPreference = $false
}

. (Join-Path $PSScriptRoot "RedisDocker.Common.ps1")

function New-RandomSecret {
    param([int]$ByteLength = 32)

    $bytes = New-Object byte[] $ByteLength
    $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
    try {
        $rng.GetBytes($bytes)
    }
    finally {
        $rng.Dispose()
    }

    return (($bytes | ForEach-Object { $_.ToString("x2") }) -join "")
}

function Ensure-RedisEnvFile {
    $envFilePath = Get-RedisEnvFile
    if (Test-Path $envFilePath) {
        $existing = Read-EnvFile -EnvFilePath $envFilePath
        if ($existing.ContainsKey("REDIS_PASSWORD") -and $existing["REDIS_PASSWORD"].Trim()) {
            return $existing
        }
    }

    $values = [ordered]@{
        REDIS_PASSWORD = (New-RandomSecret 24)
        HOST_REDIS_PORT = "6379"
    }
    $lines = $values.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }
    Set-Content -Path $envFilePath -Value $lines -Encoding ASCII
    return $values
}

function Wait-RedisHealthy {
    param([int]$TimeoutSeconds = 60)

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        $status = Get-RedisContainerStatus
        if ($status -eq "healthy" -or $status -eq "running") {
            return $true
        }
        Start-Sleep -Seconds 2
    }

    return $false
}

Write-Step "准备启动本地 Redis Docker 容器"
Ensure-DockerCli
Ensure-DockerCompose
Ensure-DockerDesktopRunning
$redisEnv = Ensure-RedisEnvFile

Invoke-RedisCompose -ComposeArguments @("up", "-d") | Out-Null

if (-not (Wait-RedisHealthy)) {
    Write-WarnLine "Redis 容器未在预期时间内进入健康状态，输出当前状态与日志。"
    Invoke-RedisCompose -ComposeArguments @("ps") -IgnoreExitCode | Out-Null
    $containerName = Get-RedisContainerName
    docker logs --tail 200 $containerName
    throw "Redis Docker 启动失败，请查看上面的日志。"
}

$status = Get-RedisContainerStatus
Write-Step "Redis 已就绪，当前状态: $status"

Write-Host ""
Write-Host "连接地址: redis://:***@localhost:$($redisEnv['HOST_REDIS_PORT'])/0" -ForegroundColor Cyan
Write-Host "环境文件: deploy\\docker\\.env.redis.local" -ForegroundColor Cyan
Write-Host "查看日志: deploy\\docker\\Logs-RedisDocker.bat" -ForegroundColor Cyan
Write-Host "停止 Redis: deploy\\docker\\Stop-RedisDocker.bat" -ForegroundColor Cyan

if ($AttachLogs) {
    Write-Step "开始跟随 Redis 日志"
    $containerName = Get-RedisContainerName
    docker logs --tail 200 -f $containerName
}
