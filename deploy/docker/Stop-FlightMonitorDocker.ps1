[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
if (Get-Variable -Name PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue) {
    $PSNativeCommandUseErrorActionPreference = $false
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path (Join-Path $scriptDir "..\..")).Path
$vaultHelper = Join-Path $repoRoot "scripts\vault\VaultBootstrap.Common.ps1"
. $vaultHelper
$composeFile = (Resolve-Path (Join-Path $scriptDir "docker-compose.distributed.yml")).Path
$envFile = Join-Path $scriptDir ".env.local"
$stopCaddyScript = Join-Path $repoRoot "scripts\host\stop_caddy_http3_proxy.ps1"

if (-not (Test-Path $envFile)) {
    throw "环境文件不存在: $envFile"
}

Write-Host "[INFO] 停止 Flight Monitor Docker 容器栈" -ForegroundColor Green
if (Test-Path $stopCaddyScript) {
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $stopCaddyScript
}
$vaultArtifactsRoot = Join-Path $scriptDir ".vault\distributed"
$runtimeEnvFile = Join-Path $vaultArtifactsRoot "runtime.env"
if (-not (Test-Path -LiteralPath $runtimeEnvFile)) {
    $bootstrap = Invoke-FmsVaultBootstrap `
        -RepoRoot $repoRoot `
        -BaseEnvFile $envFile `
        -TemplatePath (Join-Path $repoRoot "deploy\vault\templates\docker-all.env.ctmpl") `
        -RenderedEnvFile (Join-Path $vaultArtifactsRoot "rendered.env") `
        -RuntimeEnvFile $runtimeEnvFile `
        -AgentConfigFile (Join-Path $vaultArtifactsRoot "vault-agent.hcl") `
        -Mode "docker"
    $runtimeEnvFile = $bootstrap.RuntimeEnvFile
}
& docker compose --file $composeFile --env-file $runtimeEnvFile down
if ($LASTEXITCODE -ne 0) {
    throw "docker compose down 执行失败，退出码: $LASTEXITCODE"
}
Write-Host "[INFO] 已停止。数据卷保留，可下次直接启动。" -ForegroundColor Green
