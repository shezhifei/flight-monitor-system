[CmdletBinding()]
param(
    [switch]$OpenBrowser = $true
)

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

function Write-ErrorLine {
    param([string]$Message)
    Write-Host "[ERROR] $Message" -ForegroundColor Red
}

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

function Test-UnsafeSecretValue {
    param([string]$Value)

    if (-not $Value) {
        return $true
    }

    $normalized = $Value.Trim().ToLowerInvariant()
    return $normalized -in @("test", "password", "default", "changeme", "replace_me")
}

function Assert-RequiredSecret {
    param(
        [string]$Name,
        [string]$Value
    )

    if (-not $Value) {
        throw "$Name 未配置。请更新环境文件后重试。"
    }

    if (Test-UnsafeSecretValue -Value $Value) {
        throw "检测到弱 $Name。请更新环境文件中的该值，或删除环境文件后重新生成。"
    }
}

function Invoke-DockerCompose {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$ComposeArguments,
        [switch]$IgnoreExitCode
    )

    & docker compose @ComposeArguments
    $exitCode = $LASTEXITCODE

    if ($exitCode -ne 0 -and -not $IgnoreExitCode) {
        throw "docker compose 执行失败，退出码: $exitCode"
    }

    return $exitCode
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

function Ensure-EnvFile {
    param([string]$EnvFilePath)

    $desiredEntries = [ordered]@{
        COMPOSE_PROJECT_NAME = "flight-monitor-distributed"
        DB_NAME = "flight_monitor_dev"
        DB_USER = "postgres"
        DB_HOST = "postgres"
        DB_PORT = "5432"
        DB_REPLICATION_HOST = "postgres"
        DB_REPLICATION_PORT = "5432"
        DB_REPLICATION_NAME = "flight_monitor_dev"
        DB_REPLICATION_USER = "fm_replicator"
        REDIS_HOST = "redis"
        REDIS_PORT = "6379"
        REDIS_DB = "0"
        SYSTEM_CONFIG_TABLE = "system_config"
        LOG_FORMAT = "json"
        NGINX_PORT = "8088"
        RUST_API_HOST_PORT = "18080"
        FLOWABLE_HOST_PORT = "8082"
        FLOWABLE_ADMIN_USER = "rest-admin"
        FLOWABLE_DB_NAME = "flowable"
        FLOWABLE_DB_USER = "flowable"
        FLOWABLE_JAVA_OPTS = "-Xms512m -Xmx1024m"
        FLOWABLE_API_URL = "http://flowable:8080/flowable-rest/service"
        CORS_ALLOWED_ORIGINS = "https://localhost:18443,https://127.0.0.1:18443"
        VAULT_ADDR = "https://127.0.0.1:8200"
        VAULT_ROLE_ID_FILE = "deploy/vault/approle/fms-ops-bootstrap.role_id"
        VAULT_SECRET_ID_FILE = "deploy/vault/approle/fms-ops-bootstrap.secret_id"
        VAULT_AGENT_CONFIG = "deploy/docker/.vault/distributed/vault-agent.hcl"
        VAULT_RENDERED_ENV_FILE = "deploy/docker/.vault/distributed/rendered.env"
    }

    if (-not (Test-Path $EnvFilePath)) {
        Write-Step "生成本地部署环境文件: $EnvFilePath"
        $lines = $desiredEntries.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }
        Set-Content -Path $EnvFilePath -Value $lines -Encoding ASCII
        return
    }

    Write-Step "复用现有环境文件: $EnvFilePath"
    $existing = Read-FmsEnvFile -Path $EnvFilePath

    $missingLines = @()

    foreach ($entry in $desiredEntries.GetEnumerator()) {
        if (-not $existing.ContainsKey($entry.Key)) {
            $missingLines += "$($entry.Key)=$($entry.Value)"
        }
    }

    if ($missingLines.Count -gt 0) {
        Add-Content -Path $EnvFilePath -Value $missingLines -Encoding ASCII
        Write-Step "已补齐环境文件缺失项: $($missingLines.Count) 个"
    }
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path (Join-Path $scriptDir "..\..")).Path
$vaultHelper = Join-Path $repoRoot "scripts\vault\VaultBootstrap.Common.ps1"
. $vaultHelper
$composeFile = (Resolve-Path (Join-Path $scriptDir "docker-compose.distributed.yml")).Path
$envFile = Join-Path $scriptDir ".env.local"
$hostCaddyScript = Join-Path $repoRoot "scripts\host\start_caddy_http3_proxy.ps1"

Write-Step "工作目录: $repoRoot"

Ensure-DockerCli
Ensure-DockerCompose
Ensure-DockerDesktopRunning
Ensure-EnvFile -EnvFilePath $envFile
$vaultArtifactsRoot = Join-Path $scriptDir ".vault\distributed"
$bootstrap = Invoke-FmsVaultBootstrap `
    -RepoRoot $repoRoot `
    -BaseEnvFile $envFile `
    -TemplatePath (Join-Path $repoRoot "deploy\vault\templates\docker-all.env.ctmpl") `
    -RenderedEnvFile (Join-Path $vaultArtifactsRoot "rendered.env") `
    -RuntimeEnvFile (Join-Path $vaultArtifactsRoot "runtime.env") `
    -AgentConfigFile (Join-Path $vaultArtifactsRoot "vault-agent.hcl") `
    -Mode "docker"
$envValues = $bootstrap.RuntimeValues
$runtimeEnvFile = $bootstrap.RuntimeEnvFile

Write-Step "开始构建并启动 Rust-first 分布式容器栈"
Invoke-DockerCompose -ComposeArguments @("--file", $composeFile, "--env-file", $runtimeEnvFile, "down", "--remove-orphans") -IgnoreExitCode | Out-Null
Invoke-DockerCompose -ComposeArguments @("--file", $composeFile, "--env-file", $runtimeEnvFile, "up", "-d", "--build") | Out-Null

Write-Step "等待 Rust API 对外服务"
$rustPort = if ($envValues.ContainsKey("RUST_API_HOST_PORT") -and $envValues["RUST_API_HOST_PORT"]) { $envValues["RUST_API_HOST_PORT"] } else { "18080" }
$healthUrl = "http://localhost:$rustPort/api/v2/health/ping"
$deadline = (Get-Date).AddMinutes(5)
$healthy = $false

while ((Get-Date) -lt $deadline) {
    try {
        $response = Invoke-WebRequest -Uri $healthUrl -UseBasicParsing -TimeoutSec 5
        if ($response.StatusCode -eq 200) {
            $healthy = $true
            break
        }
    }
    catch {
        Start-Sleep -Seconds 5
    }
}

if (-not $healthy) {
    Write-WarnLine "Rust API 健康检查未在预期时间内通过，下面输出当前容器状态。"
    Invoke-DockerCompose -ComposeArguments @("--file", $composeFile, "--env-file", $runtimeEnvFile, "ps") | Out-Null
    throw "部署未完成，请查看容器状态与日志。"
}

$flowablePort = if ($envValues.ContainsKey("FLOWABLE_HOST_PORT") -and $envValues["FLOWABLE_HOST_PORT"]) { $envValues["FLOWABLE_HOST_PORT"] } else { "8082" }
$flowableUser = if ($envValues.ContainsKey("FLOWABLE_ADMIN_USER") -and $envValues["FLOWABLE_ADMIN_USER"]) { $envValues["FLOWABLE_ADMIN_USER"] } else { "rest-admin" }
$flowablePassword = if ($envValues.ContainsKey("FLOWABLE_ADMIN_PASSWORD") -and $envValues["FLOWABLE_ADMIN_PASSWORD"]) { $envValues["FLOWABLE_ADMIN_PASSWORD"] } else { "" }
Assert-RequiredSecret -Name "FLOWABLE_ADMIN_PASSWORD" -Value $flowablePassword
$flowableHealthUrl = "http://localhost:$flowablePort/flowable-rest/service/management/engine"
$flowableAuthBytes = [System.Text.Encoding]::ASCII.GetBytes("${flowableUser}:${flowablePassword}")
$flowableHeaders = @{ Authorization = "Basic $([Convert]::ToBase64String($flowableAuthBytes))" }

Write-Step "等待 Flowable / Tomcat 就绪"
$flowableDeadline = (Get-Date).AddMinutes(8)
$flowableReady = $false

while ((Get-Date) -lt $flowableDeadline) {
    try {
        $response = Invoke-WebRequest -Uri $flowableHealthUrl -Headers $flowableHeaders -UseBasicParsing -TimeoutSec 5
        if ($response.StatusCode -eq 200) {
            $flowableReady = $true
            break
        }
    }
    catch {
        Start-Sleep -Seconds 5
    }
}

if (-not $flowableReady) {
    Write-WarnLine "Flowable/Tomcat 未在预期时间内就绪，下面输出当前容器状态。"
    Invoke-DockerCompose -ComposeArguments @("--file", $composeFile, "--env-file", $runtimeEnvFile, "ps") | Out-Null
    throw "Flowable/Tomcat 启动失败，请查看 flowable 容器日志。"
}

Write-Step "启动宿主机 Caddy HTTP/3 入口"
$entryUrl = "https://localhost:18443"
try {
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $hostCaddyScript
    if ($LASTEXITCODE -ne 0) {
        throw "Caddy 启动失败，退出码: $LASTEXITCODE"
    }
}
catch {
    Write-WarnLine "Caddy HTTP/3 入口未能启动，将保留 Rust 直连入口。错误: $($_.Exception.Message)"
    $entryUrl = "http://localhost:$rustPort"
}

Write-Step "部署完成"
Invoke-DockerCompose -ComposeArguments @("--file", $composeFile, "--env-file", $runtimeEnvFile, "ps") | Out-Null

Write-Host ""
Write-Host "访问地址: $entryUrl" -ForegroundColor Cyan
Write-Host "健康检查: $healthUrl" -ForegroundColor Cyan
Write-Host "Flowable API: $flowableHealthUrl" -ForegroundColor Cyan
Write-Host "停止脚本: deploy\\docker\\Stop-FlightMonitorDocker.bat" -ForegroundColor Cyan
Write-Host "注意: Python API / nginx 已降级为 deprecated legacy 入口，不再属于默认拓扑。" -ForegroundColor Yellow

if ($OpenBrowser) {
    Start-Process $entryUrl | Out-Null
}
