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

function Ensure-DockerReady {
    docker info *> $null
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path (Join-Path $scriptDir "..\..")).Path
. (Join-Path $repoRoot "scripts\vault\VaultBootstrap.Common.ps1")

$composeFile = (Resolve-Path (Join-Path $scriptDir "docker-compose.edge.yml")).Path
$envFile = Join-Path $scriptDir ".env.edge"

if (-not (Test-Path -LiteralPath $envFile)) {
    Copy-Item -LiteralPath (Join-Path $scriptDir ".env.edge.example") -Destination $envFile
    Write-Step "已生成 .env.edge，请先确认 Vault AppRole 文件路径。"
}

Ensure-DockerReady

$vaultArtifactsRoot = Join-Path $scriptDir ".vault\edge"
$bootstrap = Invoke-FmsVaultBootstrap `
    -RepoRoot $repoRoot `
    -BaseEnvFile $envFile `
    -TemplatePath (Join-Path $repoRoot "deploy\vault\templates\docker-all.env.ctmpl") `
    -RenderedEnvFile (Join-Path $vaultArtifactsRoot "rendered.env") `
    -RuntimeEnvFile (Join-Path $vaultArtifactsRoot "runtime.env") `
    -AgentConfigFile (Join-Path $vaultArtifactsRoot "vault-agent.hcl") `
    -Mode "docker"

$runtimeEnvFile = $bootstrap.RuntimeEnvFile
$runtimeValues = $bootstrap.RuntimeValues

Write-Step "构建边缘镜像"
docker compose --file $composeFile --env-file $runtimeEnvFile build --no-cache
if ($LASTEXITCODE -ne 0) {
    throw "边缘镜像构建失败，退出码: $LASTEXITCODE"
}

Write-Step "启动边缘容器"
docker compose --file $composeFile --env-file $runtimeEnvFile up -d
if ($LASTEXITCODE -ne 0) {
    throw "边缘容器启动失败，退出码: $LASTEXITCODE"
}

$rustPort = Get-FmsEnvValue -Values $runtimeValues -Name "RUST_API_HOST_PORT" -Default "18080"
$healthUrl = "http://localhost:$rustPort/api/v2/health/ping"
$deadline = (Get-Date).AddMinutes(5)
while ((Get-Date) -lt $deadline) {
    try {
        $response = Invoke-WebRequest -Uri $healthUrl -UseBasicParsing -TimeoutSec 5
        if ($response.StatusCode -eq 200) {
            Write-Step "Rust API 已就绪: $healthUrl"
            exit 0
        }
    }
    catch {
        Start-Sleep -Seconds 3
    }
}

throw "Rust API 未在预期时间内就绪，请检查 docker compose logs rust-api"
