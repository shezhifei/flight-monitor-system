[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("ai-sidecar-unavailable", "redis-unavailable", "mq-gateway-unavailable", "tcp-port-unreachable")]
    [string]$Scenario,

    [ValidateSet("distributed", "edge")]
    [string]$Topology = "distributed",

    [string]$ComposeFile,
    [string]$EnvFile,
    [string]$HealthUrl,
    [int]$DurationSeconds = 30,

    [string]$TcpHost = "127.0.0.1",
    [int]$TcpPort = 1,

    [switch]$Apply,
    [string]$ConfirmToken,
    [switch]$SkipHealthProbe
)

$ErrorActionPreference = "Stop"
if (Get-Variable -Name PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue) {
    $PSNativeCommandUseErrorActionPreference = $false
}

$RequiredConfirmToken = "I_UNDERSTAND_LOCAL_CHAOS"

function Write-Step {
    param([string]$Message)
    Write-Host "[CHAOS] $Message" -ForegroundColor Cyan
}

function Write-InfoLine {
    param([string]$Message)
    Write-Host "[INFO] $Message" -ForegroundColor Green
}

function Write-WarnLine {
    param([string]$Message)
    Write-Host "[WARN] $Message" -ForegroundColor Yellow
}

function Write-CommandLine {
    param([string[]]$Arguments)
    Write-Host ("docker compose " + ($Arguments -join " ")) -ForegroundColor DarkGray
}

function Assert-DockerComposeReady {
    if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
        throw "未检测到 docker 命令。请先安装并启动 Docker Desktop。"
    }

    & docker compose version *> $null
    if ($LASTEXITCODE -ne 0) {
        throw "当前 Docker CLI 不支持 'docker compose'。请升级 Docker Desktop。"
    }
}

function Invoke-DockerCompose {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments,
        [switch]$IgnoreExitCode
    )

    Write-CommandLine -Arguments $Arguments
    & docker compose @Arguments
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0 -and -not $IgnoreExitCode) {
        throw "docker compose 执行失败，退出码: $exitCode"
    }
    return $exitCode
}

function Test-HttpEndpoint {
    param([string]$Url)

    if (-not $Url) {
        return
    }

    try {
        $response = Invoke-WebRequest -Uri $Url -UseBasicParsing -TimeoutSec 5
        Write-InfoLine "HTTP probe $Url -> $($response.StatusCode)"
    }
    catch {
        Write-WarnLine "HTTP probe $Url failed: $($_.Exception.Message)"
    }
}

function Test-TcpPort {
    param(
        [string]$HostName,
        [int]$Port
    )

    $client = [System.Net.Sockets.TcpClient]::new()
    try {
        $connect = $client.BeginConnect($HostName, $Port, $null, $null)
        $connected = $connect.AsyncWaitHandle.WaitOne([TimeSpan]::FromSeconds(3))
        if ($connected) {
            $client.EndConnect($connect)
            Write-InfoLine "TCP probe ${HostName}:${Port} -> reachable"
        }
        else {
            Write-WarnLine "TCP probe ${HostName}:${Port} -> timeout"
        }
    }
    catch {
        Write-WarnLine "TCP probe ${HostName}:${Port} -> failed: $($_.Exception.Message)"
    }
    finally {
        $client.Dispose()
    }
}

function Get-DefaultHealthUrl {
    param([string]$SelectedTopology)

    if ($SelectedTopology -eq "edge") {
        return "http://localhost:18080/api/v2/health/ping"
    }

    return "http://localhost:18080/api/v2/health/ping"
}

function Get-ScenarioService {
    param(
        [string]$SelectedScenario,
        [string]$SelectedTopology
    )

    switch ($SelectedScenario) {
        "ai-sidecar-unavailable" {
            if ($SelectedTopology -ne "edge") {
                throw "ai-sidecar-unavailable 仅适用于 edge 拓扑。distributed Compose 当前未定义 ai-sidecar 服务。"
            }
            return "ai-sidecar"
        }
        "redis-unavailable" {
            return "redis"
        }
        "mq-gateway-unavailable" {
            if ($SelectedTopology -ne "distributed") {
                throw "mq-gateway-unavailable 仅适用于 distributed 拓扑。edge Compose 当前未定义 mq-gateway 服务。"
            }
            return "mq-gateway"
        }
        default {
            return $null
        }
    }
}

function Get-ComposeArguments {
    param(
        [string]$SelectedComposeFile,
        [string]$SelectedEnvFile
    )

    $arguments = @("--file", $SelectedComposeFile)
    if ($SelectedEnvFile) {
        $arguments += @("--env-file", $SelectedEnvFile)
    }
    return $arguments
}

if ($DurationSeconds -lt 1 -or $DurationSeconds -gt 300) {
    throw "DurationSeconds 必须在 1 到 300 之间，避免长时间影响本地环境。"
}

if ($TcpPort -lt 1 -or $TcpPort -gt 65535) {
    throw "TcpPort 必须在 1 到 65535 之间。"
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path (Join-Path $scriptDir "..\..")).Path
$dockerDir = Join-Path $repoRoot "deploy\docker"

if (-not $ComposeFile) {
    $composeName = if ($Topology -eq "edge") { "docker-compose.edge.yml" } else { "docker-compose.distributed.yml" }
    $ComposeFile = Join-Path $dockerDir $composeName
}

if (-not (Test-Path -LiteralPath $ComposeFile)) {
    throw "Compose 文件不存在: $ComposeFile"
}
$ComposeFile = (Resolve-Path -LiteralPath $ComposeFile).Path

if (-not $EnvFile) {
    $defaultRuntimeEnv = if ($Topology -eq "edge") {
        Join-Path $dockerDir ".vault\edge\runtime.env"
    }
    else {
        Join-Path $dockerDir ".vault\distributed\runtime.env"
    }

    if (Test-Path -LiteralPath $defaultRuntimeEnv) {
        $EnvFile = $defaultRuntimeEnv
    }
}

if ($EnvFile) {
    if (-not (Test-Path -LiteralPath $EnvFile)) {
        throw "EnvFile 不存在: $EnvFile"
    }
    $EnvFile = (Resolve-Path -LiteralPath $EnvFile).Path
}

if (-not $HealthUrl) {
    $HealthUrl = Get-DefaultHealthUrl -SelectedTopology $Topology
}

$targetService = Get-ScenarioService -SelectedScenario $Scenario -SelectedTopology $Topology
$composeBaseArgs = Get-ComposeArguments -SelectedComposeFile $ComposeFile -SelectedEnvFile $EnvFile

Write-Step "Local chaos experiment"
Write-InfoLine "Repo root: $repoRoot"
Write-InfoLine "Scenario: $Scenario"
Write-InfoLine "Topology: $Topology"
Write-InfoLine "Compose file: $ComposeFile"
if ($EnvFile) {
    Write-InfoLine "Env file: $EnvFile"
}
else {
    Write-WarnLine "未提供 EnvFile。若 Compose 文件包含必填变量，Apply 时可能失败。"
}
Write-InfoLine "Mode: $(if ($Apply) { "apply" } else { "dry-run" })"

if ($Scenario -eq "tcp-port-unreachable") {
    Write-InfoLine "TCP target: ${TcpHost}:${TcpPort}"
    Test-TcpPort -HostName $TcpHost -Port $TcpPort
    if (-not $Apply) {
        Write-Step "Dry-run complete. tcp-port-unreachable 只执行探针，不修改本地服务。"
        exit 0
    }

    Write-Step "tcp-port-unreachable 场景不支持破坏性注入；请通过防火墙/代理实验环境单独验证网络策略。"
    exit 0
}

Write-InfoLine "Target service: $targetService"
Write-InfoLine "Fault duration: $DurationSeconds seconds"

$stopArgs = $composeBaseArgs + @("stop", "--timeout", "10", $targetService)
$startArgs = $composeBaseArgs + @("start", $targetService)
$psArgs = $composeBaseArgs + @("ps", $targetService)

if (-not $Apply) {
    Write-Step "Dry-run: 将执行以下低风险故障注入步骤"
    Write-CommandLine -Arguments $psArgs
    if (-not $SkipHealthProbe) {
        Write-InfoLine "HTTP probe before/after fault: $HealthUrl"
    }
    Write-CommandLine -Arguments $stopArgs
    Write-InfoLine "Start-Sleep -Seconds $DurationSeconds"
    Write-CommandLine -Arguments $startArgs
    Write-CommandLine -Arguments $psArgs
    Write-Step "未传入 -Apply，未停止任何服务。"
    exit 0
}

if ($ConfirmToken -ne $RequiredConfirmToken) {
    throw "Apply 模式需要 -ConfirmToken $RequiredConfirmToken。脚本不会在未确认时停止本地服务。"
}

Assert-DockerComposeReady

Write-Step "Pre-flight service status"
Invoke-DockerCompose -Arguments $psArgs -IgnoreExitCode
if (-not $SkipHealthProbe) {
    Test-HttpEndpoint -Url $HealthUrl
}

$recoveryAttempted = $false
try {
    Write-Step "Injecting fault: stop $targetService"
    Invoke-DockerCompose -Arguments $stopArgs | Out-Null

    if (-not $SkipHealthProbe) {
        Test-HttpEndpoint -Url $HealthUrl
    }

    Write-Step "Holding fault for $DurationSeconds seconds"
    Start-Sleep -Seconds $DurationSeconds
}
finally {
    Write-Step "Recovering service: start $targetService"
    $recoveryAttempted = $true
    Invoke-DockerCompose -Arguments $startArgs -IgnoreExitCode | Out-Null
}

Write-Step "Post-recovery service status"
Invoke-DockerCompose -Arguments $psArgs -IgnoreExitCode
if (-not $SkipHealthProbe) {
    Test-HttpEndpoint -Url $HealthUrl
}

if ($recoveryAttempted) {
    Write-Step "Experiment complete. 请结合应用日志、健康接口和业务只读请求记录观测结果。"
}
