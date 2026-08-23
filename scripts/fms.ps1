[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("start", "stop", "logs", "restart", "status")]
    [string]$Command,

    [Parameter(Mandatory = $true)]
    [ValidateSet("docker", "host", "edge")]
    [string]$Runtime,

    [switch]$UseCargoRun,
    [switch]$SkipBuild,
    [switch]$SkipMigrations
)

$ErrorActionPreference = "Stop"
if (Get-Variable -Name PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue) {
    $PSNativeCommandUseErrorActionPreference = $false
}

function Write-Step {
    param([string]$Message)
    Write-Host "[FMS] $Message" -ForegroundColor Cyan
}

function Write-Info {
    param([string]$Message)
    Write-Host "[INFO] $Message" -ForegroundColor Green
}

function Write-Warn {
    param([string]$Message)
    Write-Host "[WARN] $Message" -ForegroundColor Yellow
}

function Write-Err {
    param([string]$Message)
    Write-Host "[ERROR] $Message" -ForegroundColor Red
}

# =============================================================================
# Host Runtime Service Helpers
# =============================================================================
$script:HostServiceNames = @("postgres", "redis", "vault", "rocketmq-namesrv", "rocketmq-broker", "mq-gateway", "caddy", "fms-server")

function Get-HostRuntimeDir {
    $runtimeDir = Join-Path $repoRoot ".runtime\host-services"
    if (-not (Test-Path $runtimeDir)) {
        New-Item -ItemType Directory -Path $runtimeDir -Force | Out-Null
    }
    return $runtimeDir
}

function Get-HostServiceDir {
    param([string]$ServiceName)
    $serviceDir = Join-Path (Get-HostRuntimeDir) $ServiceName
    if (-not (Test-Path $serviceDir)) {
        New-Item -ItemType Directory -Path $serviceDir -Force | Out-Null
    }
    return $serviceDir
}

function Get-HostServiceLogPath {
    param([string]$ServiceName)
    $serviceDir = Get-HostServiceDir -ServiceName $ServiceName
    return @{
        stdout = Join-Path $serviceDir "$ServiceName.stdout.log"
        stderr = Join-Path $serviceDir "$ServiceName.stderr.log"
    }
}

function Get-HostRuntimePidFile {
    param([string]$ServiceName)
    return Join-Path (Get-HostRuntimeDir) "$ServiceName.pid"
}

function Save-HostRuntimePid {
    param([string]$ServiceName, [int]$ProcessId)
    $ProcessId | Out-File -FilePath (Get-HostRuntimePidFile -ServiceName $ServiceName) -Encoding utf8 -Force
}

function Remove-HostRuntimePid {
    param([string]$ServiceName)
    $pidFile = Get-HostRuntimePidFile -ServiceName $ServiceName
    if (Test-Path $pidFile) {
        Remove-Item -Path $pidFile -Force
    }
}

function Get-HostRuntimePid {
    param([string]$ServiceName)
    $pidFile = Get-HostRuntimePidFile -ServiceName $ServiceName
    if (-not (Test-Path $pidFile)) {
        return $null
    }
    $raw = Get-Content -Path $pidFile -Raw
    if ([int]::TryParse($raw, [ref]$null)) {
        return [int]::Parse($raw)
    }
    return $null
}

function Test-TcpPort {
    param([string]$HostName = "127.0.0.1", [int]$Port, [int]$TimeoutMs = 1000)
    try {
        $client = New-Object System.Net.Sockets.TcpClient
        $connect = $client.BeginConnect($HostName, $Port, $null, $null)
        $success = $connect.AsyncWaitHandle.WaitOne($TimeoutMs, $false)
        if (-not $success) { return $false }
        $client.EndConnect($connect)
        $client.Close()
        return $true
    } catch {
        return $false
    }
}

function Test-Postgres {
    if (-not $env:DATABASE_URL) {
        # Try to synthesize from discrete env vars
        $pgHost = if ($env:DB_HOST) { $env:DB_HOST } else { "localhost" }
        $pgPort = if ($env:DB_PORT) { $env:DB_PORT } else { "5432" }
        return (Test-TcpPort -HostName $pgHost -Port ([int]$pgPort))
    }
    # DATABASE_URL like postgres://user:pass@host:port/db
    if ($env:DATABASE_URL -match '@([^:/]+):(\d+)/') {
        return (Test-TcpPort -HostName $matches[1] -Port ([int]$matches[2]))
    }
    return $false
}

function Test-Redis {
    $port = if ($env:REDIS_PORT) { [int]$env:REDIS_PORT } else { 6379 }
    $hostName = if ($env:REDIS_HOST) { $env:REDIS_HOST } else { "127.0.0.1" }
    return (Test-TcpPort -HostName $hostName -Port $port)
}

function Test-Vault {
    $addr = if ($env:VAULT_ADDR) { $env:VAULT_ADDR } else { "http://127.0.0.1:8200" }
    try {
        $response = Invoke-WebRequest -Uri "$addr/v1/sys/health" -Method GET -TimeoutSec 2 -UseBasicParsing -ErrorAction SilentlyContinue
        return ($response.StatusCode -in 200, 429, 501, 503)
    } catch {
        return $false
    }
}

function Test-Caddy {
    $port = if ($env:FMS_CADDY_HTTPS_PORT) { [int]$env:FMS_CADDY_HTTPS_PORT } else { 18443 }
    return (Test-TcpPort -HostName "127.0.0.1" -Port $port)
}

function Test-RustApi {
    $port = if ($env:API_PORT) { [int]$env:API_PORT } else { 8000 }
    return (Test-TcpPort -HostName "127.0.0.1" -Port $port)
}

function Get-ProcessByPid {
    param([int]$ProcessId)
    return Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
}

function Start-BackgroundService {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ServiceName,

        [Parameter(Mandatory = $true)]
        [string]$DisplayName,

        [Parameter(Mandatory = $true)]
        [string]$FilePath,

        [string[]]$ArgumentList = @(),

        [string]$WorkingDirectory = "",

        [hashtable]$Environment = @{}
    )

    $existingPid = Get-HostRuntimePid -ServiceName $ServiceName
    if ($existingPid) {
        $proc = Get-ProcessByPid -ProcessId $existingPid
        if ($proc -and -not $proc.HasExited) {
            return @{ Status = "already-running"; Pid = $existingPid; Process = $proc }
        }
        Remove-HostRuntimePid -ServiceName $ServiceName
    }

    if (-not (Test-Path $FilePath)) {
        throw "$DisplayName 可执行文件不存在: $FilePath"
    }

    $logPaths = Get-HostServiceLogPath -ServiceName $ServiceName
    $serviceDir = Get-HostServiceDir -ServiceName $ServiceName

    # Rotate previous logs so each start gets a fresh file while keeping history.
    if (Test-Path $logPaths.stdout) {
        Move-Item -Path $logPaths.stdout -Destination "$($logPaths.stdout).old" -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path $logPaths.stderr) {
        Move-Item -Path $logPaths.stderr -Destination "$($logPaths.stderr).old" -Force -ErrorAction SilentlyContinue
    }

    $wd = if ($WorkingDirectory) { $WorkingDirectory } else { $serviceDir }

    # Build a cmd.exe wrapper so the real service is fully detached from the
    # calling PowerShell console/session. WMI creates the wrapper in a neutral
    # context, the wrapper reproduces the current process environment, applies
    # any extra environment variables, changes to the service directory, starts
    # the service, and redirects stdout/stderr to per-service log files. The
    # wrapper PID is tracked; the actual service is discovered by port/name for
    # status/stop.
    $batchPath = Join-Path $serviceDir "start_$ServiceName.bat"
    $batchLines = @("@echo off", "cd /d `"$repoRoot`"")

    # Reproduce the current process environment so the service sees the same
    # variables as this script (including those loaded from .env files).
    # Use foreach (not ForEach-Object pipeline) because ForEach-Object runs in
    # a child scope and += would not modify the outer $batchLines variable.
    foreach ($envVar in (Get-ChildItem Env:)) {
        $key = $envVar.Name
        $value = $envVar.Value -replace '%', '%%' -replace '"', '""'
        $batchLines += "set `"$key=$value`""
    }

    # Apply service-specific environment variable overrides last.
    foreach ($key in $Environment.Keys) {
        $value = $Environment[$key] -replace '%', '%%' -replace '"', '""'
        $batchLines += "set `"$key=$value`""
    }

    $quotedExe = '"' + $FilePath + '"'
    $quotedArgs = ($ArgumentList | ForEach-Object {
        if ($_ -match '\s') { '"' + $_ + '"' } else { $_ }
    }) -join " "
    $quotedWd = '"' + $wd + '"'
    $quotedStdout = '"' + $logPaths.stdout + '"'
    $quotedStderr = '"' + $logPaths.stderr + '"'

    $batchLines += "cd /d $quotedWd"
    # When calling a batch file from another batch file, CALL is required;
    # otherwise control flow and argument handling break.
    $callPrefix = if ($FilePath -match '\.(bat|cmd)$') { "CALL " } else { "" }
    $batchLines += "$callPrefix$quotedExe $quotedArgs > $quotedStdout 2> $quotedStderr"

    [System.IO.File]::WriteAllLines($batchPath, $batchLines, [System.Text.Encoding]::ASCII)

    $cmdLine = "cmd.exe /c `"$batchPath`""
    $result = Invoke-WmiMethod -Class Win32_Process -Name Create -ArgumentList $cmdLine
    if ($result.ReturnValue -ne 0) {
        throw "$DisplayName 启动失败 (WMI return $($result.ReturnValue))"
    }

    Save-HostRuntimePid -ServiceName $ServiceName -ProcessId $result.ProcessId

    return @{ Status = "started"; Pid = $result.ProcessId }
}

function Stop-BackgroundService {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ServiceName,

        [Parameter(Mandatory = $true)]
        [string]$DisplayName,

        [string]$FallbackProcessName = "",

        [scriptblock]$FallbackProcessFinder = $null
    )

    $pidValue = Get-HostRuntimePid -ServiceName $ServiceName
    $stopped = $false
    $proc = $null

    if ($pidValue) {
        $proc = Get-ProcessByPid -ProcessId $pidValue
    }

    if (-not $proc -and $FallbackProcessFinder) {
        $proc = & $FallbackProcessFinder
    }

    if (-not $proc -and $FallbackProcessName) {
        $proc = Get-Process -Name $FallbackProcessName -ErrorAction SilentlyContinue | Where-Object {
            $_.Path -and $_.Path.StartsWith($repoRoot, [System.StringComparison]::OrdinalIgnoreCase)
        } | Select-Object -First 1
    }

    if ($proc -and -not $proc.HasExited) {
        Write-Step "停止 $DisplayName (PID $($proc.Id))..."
        $proc | Stop-Process -Force -ErrorAction SilentlyContinue
        $stopped = $true
        Start-Sleep -Milliseconds 500
    }

    # Also terminate by process name in case the wrapper (cmd.exe) did not take
    # down the actual service, or the PID file was stale.
    if ($FallbackProcessName) {
        $nameProc = Get-Process -Name $FallbackProcessName -ErrorAction SilentlyContinue | Where-Object {
            $_.Path -and $_.Path.StartsWith($repoRoot, [System.StringComparison]::OrdinalIgnoreCase)
        } | Select-Object -First 1
        if ($nameProc -and -not $nameProc.HasExited) {
            Write-Step "停止 $DisplayName (name fallback PID $($nameProc.Id))..."
            $nameProc | Stop-Process -Force -ErrorAction SilentlyContinue
            $stopped = $true
        }
    }

    if ($pidValue -or (Test-Path (Get-HostRuntimePidFile -ServiceName $ServiceName))) {
        Remove-HostRuntimePid -ServiceName $ServiceName
    }

    if (-not $stopped) {
        Write-Warn "未找到本项目的 $DisplayName 进程"
    } else {
        Write-Info "$DisplayName 已停止"
    }
}

function Stop-HostRuntimeProcess {
    param(
        [string]$ServiceName,
        [string]$DisplayName,
        [string]$FallbackProcessName,
        [scriptblock]$FallbackProcessFinder = $null
    )
    Stop-BackgroundService -ServiceName $ServiceName -DisplayName $DisplayName -FallbackProcessName $FallbackProcessName -FallbackProcessFinder $FallbackProcessFinder
}

# =============================================================================
# Component-specific starters (special services: PostgreSQL, Rust API)
# =============================================================================
function Start-PostgresIfNeeded {
    if (Test-Postgres) {
        return @{ Name = "postgres"; Display = "PostgreSQL"; Status = "running"; Detail = "$($env:DB_HOST):$($env:DB_PORT)" }
    }

    Write-Step "PostgreSQL 未运行，尝试启动..."

    # Try Windows service first
    $pgService = Get-Service | Where-Object { $_.Name -like "postgresql*" } | Select-Object -First 1
    if ($pgService -and $pgService.Status -ne "Running") {
        try {
            Start-Service -Name $pgService.Name -ErrorAction Stop
            Start-Sleep -Seconds 3
            if (Test-Postgres) {
                return @{ Name = "postgres"; Display = "PostgreSQL"; Status = "started"; Detail = "service:$($pgService.Name)" }
            }
        } catch {
            Write-Warn "通过 Windows 服务启动 PostgreSQL 失败: $_"
        }
    }

    # Try pg_ctl
    $pgCtl = Get-Command pg_ctl.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source
    if ($pgCtl) {
        $pgHome = Split-Path -Parent (Split-Path -Parent $pgCtl)
        $dataDir = Join-Path $pgHome "data"
        if (Test-Path $dataDir) {
            try {
                $result = Start-BackgroundService `
                    -ServiceName "postgres" `
                    -DisplayName "PostgreSQL" `
                    -FilePath $pgCtl `
                    -ArgumentList @("start", "-D", $dataDir, "-l", (Join-Path (Get-HostServiceDir -ServiceName "postgres") "pg.log")) `
                    -WorkingDirectory $pgHome
                Start-Sleep -Seconds 4
                if (Test-Postgres) {
                    return @{ Name = "postgres"; Display = "PostgreSQL"; Status = "started"; Detail = "pg_ctl"; Pid = $result.Pid }
                }
            } catch {
                Write-Warn "通过 pg_ctl 启动 PostgreSQL 失败: $_"
            }
        }
    }

    return @{ Name = "postgres"; Display = "PostgreSQL"; Status = "failed"; Detail = "请手动启动 PostgreSQL" }
}

# =============================================================================
# Generic Host Service Functions (descriptor-driven)
# =============================================================================

function Start-HostService {
    param(
        [Parameter(Mandatory = $true)]
        [hashtable]$Descriptor
    )

    $name = $Descriptor.Name
    $display = $Descriptor.Display

    # Check if already running
    $isReady = & $Descriptor.TestReady
    if ($isReady) {
        $detail = & $Descriptor.RunningDetail
        return @{ Name = $name; Display = $display; Status = "running"; Detail = $detail }
    }

    # Find executable
    $exePath = & $Descriptor.FindExe
    if (-not $exePath -or -not (Test-Path $exePath)) {
        return @{ Name = $name; Display = $display; Status = $Descriptor.ExeNotFoundStatus; Detail = $Descriptor.ExeNotFoundDetail }
    }

    $startMsg = if ($Descriptor.StartMessage) { $Descriptor.StartMessage } else { "$display 未运行，尝试启动..." }
    Write-Step $startMsg

    # Pre-start hook
    if ($Descriptor.PreStart) {
        & $Descriptor.PreStart
    }

    # Build arguments and working directory
    $arguments = & $Descriptor.BuildArguments
    $workingDir = & $Descriptor.GetWorkingDirectory
    $environment = $Descriptor.Environment

    $result = Start-BackgroundService `
        -ServiceName $name `
        -DisplayName $display `
        -FilePath $exePath `
        -ArgumentList $arguments `
        -WorkingDirectory $workingDir `
        -Environment $environment

    Start-Sleep -Seconds $Descriptor.ReadyWaitSeconds

    # Verify ready
    $isReadyNow = & $Descriptor.TestReady
    if ($isReadyNow) {
        $detail = & $Descriptor.StartedDetail
        return @{ Name = $name; Display = $display; Status = "started"; Detail = $detail; Pid = $result.Pid }
    }

    $detail = & $Descriptor.FailedDetail
    return @{ Name = $name; Display = $display; Status = "failed"; Detail = $detail; Pid = $result.Pid }
}

function Stop-HostService {
    param(
        [Parameter(Mandatory = $true)]
        [hashtable]$Descriptor
    )

    if ($Descriptor.CustomStop) {
        & $Descriptor.CustomStop
        return
    }

    Stop-BackgroundService -ServiceName $Descriptor.Name -DisplayName $Descriptor.Display -FallbackProcessName $Descriptor.FallbackProcessName
}

function Get-HostServiceStatus {
    param(
        [Parameter(Mandatory = $true)]
        [hashtable]$Descriptor
    )

    $isReady = & $Descriptor.TestReady
    $status = if ($isReady) { "running" } else { "stopped" }
    $detail = & $Descriptor.StatusDetail

    return @{ Name = $Descriptor.Name; Display = $Descriptor.Display; Status = $status; Detail = $detail }
}

function Start-RustApiBackground {
    $apiServerDir = Join-Path $repoRoot "services\api-server"

    if (-not $SkipBuild) {
        if ($UseCargoRun) {
            Write-Step "使用 cargo run 启动 (开发模式) 将以前台运行..."
            Push-Location $apiServerDir
            try {
                cargo run --release
            } finally {
                Pop-Location
            }
            return @{ Name = "fms-server"; Display = "Rust API"; Status = "foreground"; Detail = "cargo run" }
        }

        Write-Step "构建 Rust API (cargo build --release)..."
        Push-Location $apiServerDir
        try {
            cargo build --release
            if ($LASTEXITCODE -ne 0) {
                throw "Rust 构建失败"
            }
        } finally {
            Pop-Location
        }
    }

    $rustBinary = Join-Path $apiServerDir "target\release\fms-server.exe"
    $rustBinaryDebug = Join-Path $apiServerDir "target\debug\fms-server.exe"

    $binaryToRun = $null
    if (Test-Path $rustBinary) {
        $binaryToRun = $rustBinary
    } elseif (Test-Path $rustBinaryDebug) {
        $binaryToRun = $rustBinaryDebug
        Write-Warn "使用 debug 构建，建议运行 'cargo build --release'"
    } else {
        throw "找不到 Rust 可执行文件。请先运行 'cargo build --release' 或使用 -UseCargoRun 参数"
    }

    Write-Step "启动 Rust API: $binaryToRun"

    $env:VAULT_RENDERED_ENV_FILE = $env:VAULT_RENDERED_ENV_FILE
    $result = Start-BackgroundService `
        -ServiceName "fms-server" `
        -DisplayName "Rust API" `
        -FilePath $binaryToRun `
        -WorkingDirectory $apiServerDir

    Start-Sleep -Seconds 5
    if (Test-RustApi) {
        return @{ Name = "fms-server"; Display = "Rust API"; Status = "started"; Detail = "http://$($env:API_HOST):$($env:API_PORT)"; Pid = $result.Pid }
    }
    return @{ Name = "fms-server"; Display = "Rust API"; Status = "failed"; Detail = "启动后端口 $($env:API_PORT) 未响应" }
}

function Write-ComponentStatus {
    param([array]$Results)

    Write-Host ""
    Write-Host "========================================" -ForegroundColor Cyan
    Write-Host "  Host Runtime Component Status" -ForegroundColor Cyan
    Write-Host "========================================" -ForegroundColor Cyan
    Write-Host ""

    $maxName = ($Results | ForEach-Object { $_.Display.Length } | Measure-Object -Maximum).Maximum
    $maxDetail = ($Results | ForEach-Object { ($_.Detail -as [string]).Length } | Measure-Object -Maximum).Maximum
    if ($maxDetail -lt 10) { $maxDetail = 10 }

    $header = "{0,-$maxName}  {1,-10}  {2,-8}  {3,-$maxDetail}" -f "Component", "Status", "PID", "Detail"
    Write-Host $header -ForegroundColor Yellow
    Write-Host ("-" * ($maxName + $maxDetail + 24)) -ForegroundColor DarkGray

    foreach ($r in $Results) {
        $statusColor = switch ($r.Status) {
            "running" { "Green" }
            "started" { "Green" }
            "failed" { "Red" }
            "skipped" { "Yellow" }
            "foreground" { "Cyan" }
            default { "White" }
        }
        $pidStr = if ($r.Pid) { $r.Pid.ToString() } else { "-" }
        $detail = if ($r.Detail) { $r.Detail } else { "" }
        $line = "{0,-$maxName}  {1,-10}  {2,-8}  {3,-$maxDetail}" -f $r.Display, $r.Status, $pidStr, $detail
        Write-Host $line -ForegroundColor $statusColor
    }

    Write-Host ""
    Write-Host "Log directory: $(Get-HostRuntimeDir)" -ForegroundColor DarkGray
    Write-Host ""
}

function Import-DotEnvFile {
    param([string]$Path)

    if (-not (Test-Path $Path)) {
        return
    }

    Get-Content -LiteralPath $Path | ForEach-Object {
        $line = $_.Trim()
        if (-not $line -or $line.StartsWith("#")) {
            return
        }

        $parts = $line -split "=", 2
        if ($parts.Count -ne 2) {
            return
        }

        $name = $parts[0].Trim()
        $value = $parts[1]
        if (-not $name) {
            return
        }

        [Environment]::SetEnvironmentVariable($name, $value, "Process")
    }
}

function Initialize-HostRuntimeEnvironment {
    Write-Step "加载 Host 运行时环境..."

    Import-DotEnvFile (Join-Path $repoRoot ".env")

    if (-not $env:VAULT_RENDERED_ENV_FILE) {
        $localRenderedEnv = Join-Path $repoRoot ".tmp\host-qps-runtime.env"
        if (Test-Path $localRenderedEnv) {
            $env:VAULT_RENDERED_ENV_FILE = $localRenderedEnv
        }
    }

    if ($env:VAULT_RENDERED_ENV_FILE) {
        Import-DotEnvFile $env:VAULT_RENDERED_ENV_FILE
    }

    if (-not $env:VAULT_RENDERED_ENV_FILE) {
        throw "VAULT_RENDERED_ENV_FILE 未设置。请先完成 Vault bootstrap 或提供本机渲染 env 文件。"
    }

    if (-not $env:API_HOST) {
        $env:API_HOST = "127.0.0.1"
    }
    if (-not $env:API_PORT) {
        $env:API_PORT = "8000"
    }
    if ($env:REDIS_PASSWORD) {
        $env:REDISCLI_AUTH = $env:REDIS_PASSWORD
    }
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path (Join-Path $scriptDir "..")).Path

# =============================================================================
# Host Service Descriptor Table
# =============================================================================
$HostServiceDescriptors = @(
    @{
        Name                = "redis"
        Display             = "Redis"
        TestReady           = { Test-Redis }
        FindExe             = { Join-Path $repoRoot "Redis\redis-server.exe" }
        ExeNotFoundStatus   = "failed"
        ExeNotFoundDetail   = "未找到 Redis\redis-server.exe"
        BuildArguments      = {
            $redisPassword = if ($env:REDIS_PASSWORD) { $env:REDIS_PASSWORD } else { $env:REDISCLI_AUTH }
            $redisPort = if ($env:REDIS_PORT) { $env:REDIS_PORT } else { "6379" }
            $buildArgs = @("--port", ([string]$redisPort), "--save", '""')
            if ($redisPassword) {
                $buildArgs += @("--requirepass", $redisPassword)
            }
            return $buildArgs
        }
        GetWorkingDirectory = { Split-Path -Parent (Join-Path $repoRoot "Redis\redis-server.exe") }
        ReadyWaitSeconds    = 2
        Environment         = @{}
        PreStart            = $null
        RunningDetail       = { "$($env:REDIS_HOST):$($env:REDIS_PORT)" }
        StartedDetail       = { "port:$($env:REDIS_PORT)" }
        FailedDetail        = { "启动后端口未响应" }
        StatusDetail        = {
            $redisHost = if ($env:REDIS_HOST) { $env:REDIS_HOST } else { "127.0.0.1" }
            $redisPort = if ($env:REDIS_PORT) { $env:REDIS_PORT } else { "6379" }
            "$redisHost`:$redisPort"
        }
        FallbackProcessName = "redis"
        CustomStop          = {
            $stopRedis = Join-Path $repoRoot "scripts\host\stop_redis.ps1"
            if (Test-Path $stopRedis) {
                Write-Step "停止 Redis..."
                & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $stopRedis
            }
        }
    },
    @{
        Name                = "vault"
        Display             = "Vault"
        TestReady           = { Test-Vault }
        FindExe             = {
            $vaultExe = Join-Path $repoRoot "vault\vault.exe"
            if (-not (Test-Path $vaultExe)) {
                $vaultExe = Get-Command vault.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source
            }
            return $vaultExe
        }
        ExeNotFoundStatus   = "skipped"
        ExeNotFoundDetail   = "未找到 vault.exe"
        StartMessage        = "Vault 未运行，以 dev 模式启动..."
        BuildArguments      = { @("server", "-dev", "-dev-root-token-id", "root", "-dev-listen-address", "127.0.0.1:8200") }
        GetWorkingDirectory = { Get-HostServiceDir -ServiceName "vault" }
        ReadyWaitSeconds    = 3
        Environment         = @{ VAULT_ADDR = "http://127.0.0.1:8200"; VAULT_TOKEN = "root" }
        PreStart            = {
            $vaultRenderedEnv = $env:VAULT_RENDERED_ENV_FILE
            if ($vaultRenderedEnv -and (Test-Path $vaultRenderedEnv)) {
                Write-Warn "检测到 VAULT_RENDERED_ENV_FILE，Vault 将以 dev 模式启动（仅本地开发）"
            }
            $env:VAULT_ADDR = "http://127.0.0.1:8200"
            $env:VAULT_TOKEN = "root"
        }
        RunningDetail       = { $env:VAULT_ADDR }
        StartedDetail       = { "http://127.0.0.1:8200" }
        FailedDetail        = { "启动后健康检查未通过" }
        StatusDetail        = {
            $vaultAddr = if ($env:VAULT_ADDR) { $env:VAULT_ADDR } else { "http://127.0.0.1:8200" }
            $vaultAddr
        }
        FallbackProcessName = "vault"
        CustomStop          = $null
    },
    @{
        Name                = "rocketmq-namesrv"
        Display             = "RocketMQ NameSrv"
        TestReady           = { Test-TcpPort -HostName "127.0.0.1" -Port 9876 }
        FindExe             = { Join-Path $repoRoot "libs\vendor\rocketmq-rust\target\release\rocketmq-namesrv-rust.exe" }
        ExeNotFoundStatus   = "failed"
        ExeNotFoundDetail   = "未找到 rocketmq-namesrv-rust.exe，请先构建: cargo build --release --manifest-path libs\vendor\rocketmq-rust\Cargo.toml -p rocketmq-namesrv"
        BuildArguments      = { @("--listenPort", "9876", "--bindAddress", "127.0.0.1") }
        GetWorkingDirectory = { Get-HostServiceDir -ServiceName "rocketmq-namesrv" }
        ReadyWaitSeconds    = 3
        Environment         = @{ ROCKETMQ_HOME = (Get-HostServiceDir -ServiceName "rocketmq-namesrv") }
        PreStart            = $null
        RunningDetail       = { "127.0.0.1:9876" }
        StartedDetail       = { "port:9876" }
        FailedDetail        = { "启动后端口未响应" }
        StatusDetail        = { "127.0.0.1:9876" }
        FallbackProcessName = "rocketmq-namesrv-rust"
        CustomStop          = $null
    },
    @{
        Name                = "rocketmq-broker"
        Display             = "RocketMQ Broker"
        TestReady           = { Test-TcpPort -HostName "127.0.0.1" -Port 10911 }
        FindExe             = { Join-Path $repoRoot "libs\vendor\rocketmq-rust\target\release\rocketmq-broker-rust.exe" }
        ExeNotFoundStatus   = "failed"
        ExeNotFoundDetail   = "未找到 rocketmq-broker-rust.exe，请先构建: cargo build --release --manifest-path libs\vendor\rocketmq-rust\Cargo.toml -p rocketmq-broker"
        BuildArguments      = {
            # 从模板生成 broker.toml（替换存储目录占位符）
            $brokerDir = Get-HostServiceDir -ServiceName "rocketmq-broker"
            $storeDir = Join-Path $brokerDir "store"
            if (-not (Test-Path $storeDir)) {
                New-Item -ItemType Directory -Path $storeDir -Force | Out-Null
            }
            $template = Get-Content -LiteralPath (Join-Path $repoRoot "deploy\host\broker.toml") -Raw
            $brokerToml = Join-Path $brokerDir "broker.toml"
            $template.Replace("{{STORE_DIR}}", $storeDir.Replace('\', '/')) | Out-File -LiteralPath $brokerToml -Encoding utf8 -Force
            return @("-c", $brokerToml, "-n", "127.0.0.1:9876")
        }
        GetWorkingDirectory = { Get-HostServiceDir -ServiceName "rocketmq-broker" }
        ReadyWaitSeconds    = 8
        Environment         = @{ ROCKETMQ_HOME = (Get-HostServiceDir -ServiceName "rocketmq-broker") }
        PreStart            = $null
        RunningDetail       = { "127.0.0.1:10911" }
        StartedDetail       = { "port:10911" }
        FailedDetail        = { "启动后端口未响应（首次启动建 store 较慢，可查日志）" }
        StatusDetail        = { "127.0.0.1:10911" }
        FallbackProcessName = "rocketmq-broker-rust"
        CustomStop          = $null
    },
    @{
        Name                = "mq-gateway"
        Display             = "MQ Gateway"
        TestReady           = {
            try {
                $response = Invoke-WebRequest -Uri "http://127.0.0.1:8097/health" -Method GET -TimeoutSec 2 -UseBasicParsing -ErrorAction SilentlyContinue
                return ($response.StatusCode -eq 200)
            } catch {
                return $false
            }
        }
        FindExe             = { Join-Path $repoRoot "services\mq-gateway\target\release\fms-mq-gateway.exe" }
        ExeNotFoundStatus   = "failed"
        ExeNotFoundDetail   = "未找到 fms-mq-gateway.exe，请先构建: cargo build --release --manifest-path services\mq-gateway\Cargo.toml --features rocketmq-backend"
        BuildArguments      = { @() }
        GetWorkingDirectory = { Get-HostServiceDir -ServiceName "mq-gateway" }
        ReadyWaitSeconds    = 4
        Environment         = @{
            ROCKETMQ_NAME_SERVER_ADDR   = "127.0.0.1:9876"
            NAMESRV_ADDR                = "127.0.0.1:9876"
            MQ_GATEWAY_HOST             = "127.0.0.1"
            MQ_GATEWAY_PORT             = "8097"
            MQ_GATEWAY_PRODUCER_GROUP   = "fms_mq_gateway"
            MQ_GATEWAY_BROKER_ADDR      = "127.0.0.1:10911"
            MQ_GATEWAY_BOOTSTRAP_TOPICS = "fms_domain_events,fms_realtime,fms_diagnostics,ai_runtime_events"
            ENVIRONMENT                 = "development"
        }
        PreStart            = $null
        RunningDetail       = { "http://127.0.0.1:8097" }
        StartedDetail       = { "http://127.0.0.1:8097" }
        FailedDetail        = { "启动后健康检查未通过" }
        StatusDetail        = { "http://127.0.0.1:8097" }
        FallbackProcessName = "fms-mq-gateway"
        CustomStop          = $null
    },
    @{
        Name                = "caddy"
        Display             = "Caddy"
        TestReady           = { Test-Caddy }
        FindExe             = { Join-Path $repoRoot ".tools\caddy\caddy.exe" }
        ExeNotFoundStatus   = "failed"
        ExeNotFoundDetail   = "未找到 .tools\caddy\caddy.exe"
        BuildArguments      = {
            $caddyDir = Get-HostServiceDir -ServiceName "caddy"
            $caddyFile = Join-Path $caddyDir "Caddyfile"
            $httpsPort = if ($env:FMS_CADDY_HTTPS_PORT) { $env:FMS_CADDY_HTTPS_PORT } else { "18443" }
            $apiPort = if ($env:API_PORT) { $env:API_PORT } else { "8000" }
            @"
https://localhost:$httpsPort {
    tls internal
    encode zstd gzip
    reverse_proxy 127.0.0.1:$apiPort
}
"@ | Out-File -LiteralPath $caddyFile -Encoding ascii -Force
            return @("run", "--config", $caddyFile, "--adapter", "caddyfile")
        }
        GetWorkingDirectory = { Get-HostServiceDir -ServiceName "caddy" }
        ReadyWaitSeconds    = 3
        Environment         = @{}
        PreStart            = $null
        RunningDetail       = { "https://localhost:$($env:FMS_CADDY_HTTPS_PORT)" }
        StartedDetail       = {
            $httpsPort = if ($env:FMS_CADDY_HTTPS_PORT) { $env:FMS_CADDY_HTTPS_PORT } else { "18443" }
            "https://localhost:$httpsPort"
        }
        FailedDetail        = { "启动后未响应" }
        StatusDetail        = {
            $caddyPort = if ($env:FMS_CADDY_HTTPS_PORT) { $env:FMS_CADDY_HTTPS_PORT } else { "18443" }
            "https://localhost:$caddyPort"
        }
        FallbackProcessName = "caddy"
        CustomStop          = $null
    }
)

# =============================================================================
# Docker Runtime
# =============================================================================
function Invoke-DockerStart {
    Write-Step "启动 Docker 拓扑..."

    $startScript = Join-Path $repoRoot "deploy\docker\Start-FlightMonitorDocker.ps1"
    if (-not (Test-Path $startScript)) {
        throw "Docker 启动脚本不存在: $startScript"
    }

    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $startScript
    if ($LASTEXITCODE -ne 0) {
        throw "Docker 启动失败"
    }

    Write-Info "Docker 拓扑已启动"
    Write-Info "访问地址:"
    Write-Info "  - https://localhost:18443/api/v2/health/ping"
    Write-Info "  - https://localhost:18443/frontend/login.html"
}

function Invoke-DockerStop {
    Write-Step "停止 Docker 拓扑..."

    $stopScript = Join-Path $repoRoot "deploy\docker\Stop-FlightMonitorDocker.ps1"
    if (-not (Test-Path $stopScript)) {
        throw "Docker 停止脚本不存在: $stopScript"
    }

    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $stopScript
    if ($LASTEXITCODE -ne 0) {
        throw "Docker 停止失败"
    }

    Write-Info "Docker 拓扑已停止"
}

function Invoke-DockerLogs {
    Write-Step "查看 Docker 日志..."

    $composeFile = Join-Path $repoRoot "deploy\docker\docker-compose.distributed.yml"
    if (-not (Test-Path $composeFile)) {
        throw "Compose 文件不存在: $composeFile"
    }

    $envFile = Join-Path $repoRoot "deploy\docker\.env.local"
    if (Test-Path $envFile) {
        & docker compose --file $composeFile --env-file $envFile logs -f
    } else {
        & docker compose --file $composeFile logs -f
    }
}

function Invoke-DockerStatus {
    Write-Step "查看 Docker 服务状态..."

    $composeFile = Join-Path $repoRoot "deploy\docker\docker-compose.distributed.yml"
    if (-not (Test-Path $composeFile)) {
        throw "Compose 文件不存在: $composeFile"
    }

    $envFile = Join-Path $repoRoot "deploy\docker\.env.local"
    if (Test-Path $envFile) {
        & docker compose --file $composeFile --env-file $envFile ps
    } else {
        & docker compose --file $composeFile ps
    }
}

function Invoke-DockerRestart {
    Invoke-DockerStop
    Invoke-DockerStart
}

# =============================================================================
# Host Runtime
# =============================================================================
function Invoke-DbSchemaVerification {
    if (-not $env:DATABASE_URL) {
        Write-Warn "DATABASE_URL 未设置，跳过数据库运行时结构校验。"
        return
    }

    $verificationScript = Join-Path $repoRoot "scripts\database\verify_runtime_schema.sql"
    if (-not (Test-Path $verificationScript)) {
        throw "数据库运行时结构校验脚本不存在: $verificationScript"
    }

    Write-Step "校验数据库运行时结构..."
    & psql -v ON_ERROR_STOP=1 --file $verificationScript $env:DATABASE_URL
    if ($LASTEXITCODE -ne 0) {
        throw "数据库运行时结构校验失败。请先运行完整迁移，或检查 _sqlx_migrations 与实际 schema 是否漂移。"
    }
}

function Invoke-DbMigrations {
    if (-not $env:DATABASE_URL) {
        Write-Warn "DATABASE_URL 未设置，跳过数据库迁移。如需自动迁移，请在 .env 中配置 DATABASE_URL。"
        return
    }

    $baselineScript = Join-Path $repoRoot "scripts\database\setup_postgresql.sql"
    if (-not (Test-Path $baselineScript)) {
        throw "数据库基线脚本不存在: $baselineScript"
    }

    # 1. 先跑基线脚本：创建 flights/departments/anomaly_rules 等核心表，
    #    使编号迁移在干净库上能自洽执行。
    Write-Step "运行数据库基线脚本 (setup_postgresql.sql)..."
    & psql -v ON_ERROR_STOP=1 --single-transaction --file $baselineScript $env:DATABASE_URL
    if ($LASTEXITCODE -ne 0) {
        throw "数据库基线脚本执行失败"
    }

    # 2. 跑编号迁移。
    Write-Step "运行 sqlx migrate run..."
    Push-Location $repoRoot
    try {
        & sqlx migrate run --database-url $env:DATABASE_URL
        if ($LASTEXITCODE -ne 0) {
            throw "sqlx migrate run 失败"
        }
    } finally {
        Pop-Location
    }
}

function Invoke-HostStart {
    Write-Step "启动 Host Rust 运行时..."

    Initialize-HostRuntimeEnvironment

    $results = @()

    # 1. PostgreSQL (special - detect only)
    $results += Start-PostgresIfNeeded

    # 2-3. Redis, Vault (from descriptor table)
    $results += Start-HostService ($HostServiceDescriptors | Where-Object { $_.Name -eq "redis" })
    $results += Start-HostService ($HostServiceDescriptors | Where-Object { $_.Name -eq "vault" })

    # 3b. MQ 栈：namesrv → broker → gateway（fms-server 的 push consumer 与事件发布依赖）
    $results += Start-HostService ($HostServiceDescriptors | Where-Object { $_.Name -eq "rocketmq-namesrv" })
    $results += Start-HostService ($HostServiceDescriptors | Where-Object { $_.Name -eq "rocketmq-broker" })
    $results += Start-HostService ($HostServiceDescriptors | Where-Object { $_.Name -eq "mq-gateway" })

    # 4. Vault bootstrap
    $vaultBootstrap = Join-Path $repoRoot "scripts\vault\Initialize-VaultCe.ps1"
    if (Test-Path $vaultBootstrap) {
        Write-Step "运行 Vault bootstrap..."
        & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $vaultBootstrap
        if ($LASTEXITCODE -ne 0) {
            Write-Warn "Vault bootstrap 返回非零退出码，继续尝试启动..."
        }
    }

    # 5. Database migrations (optional; baseline + sqlx migrate can be slow)
    if (-not $SkipMigrations) {
        Invoke-DbMigrations
    } else {
        Write-Step "跳过数据库迁移 (-SkipMigrations)"
    }
    Invoke-DbSchemaVerification

    # 6. Caddy (from descriptor table)
    $results += Start-HostService ($HostServiceDescriptors | Where-Object { $_.Name -eq "caddy" })

    # 8. Rust API (background unless -UseCargoRun)
    $results += Start-RustApiBackground

    # 9. Print status
    Write-ComponentStatus -Results $results

    # 10. Print access URLs
    $httpsPort = if ($env:FMS_CADDY_HTTPS_PORT) { $env:FMS_CADDY_HTTPS_PORT } else { "18443" }
    $apiPort = $env:API_PORT
    Write-Info "Host 运行时已启动"
    Write-Info "访问地址:"
    Write-Info "  - https://localhost:$httpsPort/api/v2/health/ping"
    Write-Info "  - https://localhost:$httpsPort/frontend/login.html"
    Write-Info "  - http://$($env:API_HOST):$apiPort (Rust API direct)"
}

function Invoke-HostStop {
    Write-Step "停止 Host Rust 运行时..."

    # Stop Rust API first (special)
    Stop-BackgroundService -ServiceName "fms-server" -DisplayName "Rust API" -FallbackProcessName "fms-server"

    # Stop descriptor-based services in reverse order (caddy → vault → redis)
    for ($i = $HostServiceDescriptors.Count - 1; $i -ge 0; $i--) {
        Stop-HostService $HostServiceDescriptors[$i]
    }

    # PostgreSQL is usually a system service; don't stop it automatically to avoid data loss.
    Write-Warn "PostgreSQL 未自动停止（请手动管理或停止 Windows 服务）"

    Write-Info "Host 运行时已停止"
}

function Invoke-HostLogs {
    Write-Step "查看 Host 运行时日志..."

    $logDir = Get-HostRuntimeDir
    if (Test-Path $logDir) {
        Write-Info "Host 服务日志目录: $logDir"
        Get-ChildItem $logDir -Recurse -Filter "*.log" | ForEach-Object {
            Write-Host "`n--- $($_.FullName.Substring($logDir.Length + 1)) ---" -ForegroundColor Yellow
            Get-Content $_.FullName -Tail 50
        }
    } else {
        Write-Warn "日志目录不存在: $logDir"
    }

    $rustProcess = Get-Process -Name "fms-server" -ErrorAction SilentlyContinue
    if ($rustProcess) {
        Write-Host "`n--- fms-server 进程 ---" -ForegroundColor Yellow
        $rustProcess | Select-Object Id, ProcessName, StartTime, WorkingSet64 | Format-Table -AutoSize
    }
}

function Invoke-HostStatus {
    Write-Step "查看 Host 运行时状态..."

    # PostgreSQL (special - detect only)
    $pgStatus = if (Test-Postgres) { "running" } else { "stopped" }
    $pgHost = if ($env:DB_HOST) { $env:DB_HOST } else { "localhost" }
    $pgPort = if ($env:DB_PORT) { $env:DB_PORT } else { "5432" }

    # Rust API (special)
    $apiStatus = if (Test-RustApi) { "running" } else { "stopped" }
    $apiHost = if ($env:API_HOST) { $env:API_HOST } else { "127.0.0.1" }
    $apiPort = if ($env:API_PORT) { $env:API_PORT } else { "8000" }

    $results = @(
        @{ Name = "postgres"; Display = "PostgreSQL"; Status = $pgStatus; Detail = "$pgHost`:$pgPort" }
    )

    # Descriptor-based services
    foreach ($descriptor in $HostServiceDescriptors) {
        $results += Get-HostServiceStatus $descriptor
    }

    $results += @{ Name = "fms-server"; Display = "Rust API"; Status = $apiStatus; Detail = "http://$apiHost`:$apiPort" }

    Write-ComponentStatus -Results $results
}

function Invoke-HostRestart {
    Invoke-HostStop
    Start-Sleep -Seconds 2
    Invoke-HostStart
}

# =============================================================================
# Edge Runtime
# =============================================================================
function Invoke-EdgeStart {
    Write-Step "启动 Edge 拓扑..."

    $startScript = Join-Path $repoRoot "deploy\docker\Start-FlightMonitorDocker-Edge.ps1"
    if (-not (Test-Path $startScript)) {
        throw "Edge 启动脚本不存在: $startScript"
    }

    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $startScript
    if ($LASTEXITCODE -ne 0) {
        throw "Edge 启动失败"
    }

    Write-Info "Edge 拓扑已启动"
    Write-Info "访问地址: http://localhost:18080/api/v2/health/ping"
}

function Invoke-EdgeStop {
    Write-Step "停止 Edge 拓扑..."

    $stopScript = Join-Path $repoRoot "deploy\docker\Stop-FlightMonitorDocker-Edge.ps1"
    if (-not (Test-Path $stopScript)) {
        throw "Edge 停止脚本不存在: $stopScript"
    }

    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $stopScript
    if ($LASTEXITCODE -ne 0) {
        throw "Edge 停止失败"
    }

    Write-Info "Edge 拓扑已停止"
}

function Invoke-EdgeLogs {
    Write-Step "查看 Edge 日志..."

    $composeFile = Join-Path $repoRoot "deploy\docker\docker-compose.edge.yml"
    if (-not (Test-Path $composeFile)) {
        throw "Edge Compose 文件不存在: $composeFile"
    }

    & docker compose --file $composeFile logs -f
}

function Invoke-EdgeStatus {
    Write-Step "查看 Edge 服务状态..."

    $composeFile = Join-Path $repoRoot "deploy\docker\docker-compose.edge.yml"
    if (-not (Test-Path $composeFile)) {
        throw "Edge Compose 文件不存在: $composeFile"
    }

    & docker compose --file $composeFile ps
}

function Invoke-EdgeRestart {
    Invoke-EdgeStop
    Invoke-EdgeStart
}

# =============================================================================
# Main Dispatcher
# =============================================================================
Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Flight Monitor System - FMS CLI" -ForegroundColor Cyan
Write-Host "  Command: $Command | Runtime: $Runtime" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

try {
    switch ($Runtime) {
        "docker" {
            switch ($Command) {
                "start" { Invoke-DockerStart }
                "stop" { Invoke-DockerStop }
                "logs" { Invoke-DockerLogs }
                "status" { Invoke-DockerStatus }
                "restart" { Invoke-DockerRestart }
            }
        }
        "host" {
            switch ($Command) {
                "start" { Invoke-HostStart }
                "stop" { Invoke-HostStop }
                "logs" { Invoke-HostLogs }
                "status" { Invoke-HostStatus }
                "restart" { Invoke-HostRestart }
            }
        }
        "edge" {
            switch ($Command) {
                "start" { Invoke-EdgeStart }
                "stop" { Invoke-EdgeStop }
                "logs" { Invoke-EdgeLogs }
                "status" { Invoke-EdgeStatus }
                "restart" { Invoke-EdgeRestart }
            }
        }
    }
} catch {
    Write-Err $_.Exception.Message
    exit 1
}
