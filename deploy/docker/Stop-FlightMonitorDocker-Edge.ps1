[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
if (Get-Variable -Name PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue) {
    $PSNativeCommandUseErrorActionPreference = $false
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path (Join-Path $scriptDir "..\..")).Path
. (Join-Path $repoRoot "scripts\vault\VaultBootstrap.Common.ps1")

$composeFile = (Resolve-Path (Join-Path $scriptDir "docker-compose.edge.yml")).Path
$envFile = Join-Path $scriptDir ".env.edge"
if (-not (Test-Path -LiteralPath $envFile)) {
    throw "环境文件不存在: $envFile"
}

$vaultArtifactsRoot = Join-Path $scriptDir ".vault\edge"
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

docker compose --file $composeFile --env-file $runtimeEnvFile down
if ($LASTEXITCODE -ne 0) {
    throw "边缘容器停止失败，退出码: $LASTEXITCODE"
}
