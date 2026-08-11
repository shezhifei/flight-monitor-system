[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
if (Get-Variable -Name PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue) {
    $PSNativeCommandUseErrorActionPreference = $false
}

function Write-Step {
    param([string]$Message)
    Write-Host "[INFO] $Message" -ForegroundColor Green
}

function Write-WarnLine {
    param([string]$Message)
    Write-Host "[WARN] $Message" -ForegroundColor Yellow
}

function Ensure-DockerCli {
    if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
        throw "未检测到 docker 命令。请先安装 Docker Desktop。"
    }
}

function Ensure-DockerCompose {
    try {
        docker compose version *> $null
    }
    catch {
        throw "当前 Docker 不支持 docker compose。请升级 Docker Desktop。"
    }
}

function Test-DockerReady {
    try {
        docker info *> $null
        return $true
    }
    catch {
        return $false
    }
}

function Wait-DockerReady {
    param([int]$TimeoutSeconds = 180)

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        if (Test-DockerReady) {
            return $true
        }
        Start-Sleep -Seconds 3
    }

    return $false
}

function Ensure-DockerDesktopRunning {
    if (Test-DockerReady) {
        Write-Step "Docker 引擎已就绪"
        return
    }

    $desktopExe = "C:\Program Files\Docker\Docker\Docker Desktop.exe"
    if (-not (Test-Path $desktopExe)) {
        throw "Docker Desktop 未安装，未找到: $desktopExe"
    }

    Write-Step "正在启动 Docker Desktop"
    Start-Process -FilePath $desktopExe | Out-Null

    if (-not (Wait-DockerReady)) {
        throw "Docker Desktop 启动超时，请确认 Docker Desktop 能正常打开。"
    }

    Write-Step "Docker 引擎已启动"
}

function Read-EnvFile {
    param([string]$EnvFilePath)

    $values = @{}
    if (-not (Test-Path $EnvFilePath)) {
        return $values
    }

    foreach ($line in Get-Content $EnvFilePath) {
        $trimmed = $line.Trim()
        if (-not $trimmed -or $trimmed.StartsWith("#") -or -not $trimmed.Contains("=")) {
            continue
        }
        $parts = $trimmed.Split("=", 2)
        $values[$parts[0]] = $parts[1]
    }

    return $values
}

function Get-RedisComposeFile {
    return (Resolve-Path (Join-Path $PSScriptRoot "docker-compose.redis.yml")).Path
}

function Get-RedisEnvFile {
    return (Join-Path $PSScriptRoot ".env.redis.local")
}

function Get-RedisContainerName {
    return "flight-monitor-redis"
}

function Test-RedisContainerExists {
    $containerName = Get-RedisContainerName
    $result = docker ps -a --filter "name=^/${containerName}$" --format "{{.Names}}"
    return $result -and $result.Trim() -eq $containerName
}

function Get-RedisContainerStatus {
    if (-not (Test-RedisContainerExists)) {
        return $null
    }

    $containerName = Get-RedisContainerName
    $status = docker inspect --format "{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}" $containerName 2>$null
    if ($LASTEXITCODE -ne 0) {
        return $null
    }

    return $status.Trim()
}

function Invoke-RedisCompose {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$ComposeArguments,
        [switch]$IgnoreExitCode
    )

    $composeFile = Get-RedisComposeFile
    $envFile = Get-RedisEnvFile
    $dockerArgs = @("--file", $composeFile)
    if (Test-Path $envFile) {
        $dockerArgs += @("--env-file", $envFile)
    }
    $dockerArgs += $ComposeArguments

    & docker compose @dockerArgs
    $exitCode = $LASTEXITCODE

    if ($exitCode -ne 0 -and -not $IgnoreExitCode) {
        throw "docker compose 执行失败，退出码: $exitCode"
    }

    return $exitCode
}
