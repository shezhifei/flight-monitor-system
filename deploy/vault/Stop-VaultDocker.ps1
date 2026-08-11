[CmdletBinding()]
param(
    [string]$ComposeFile = "deploy/vault/docker-compose.vault.yml",
    [string]$EnvFile = ""
)

$ErrorActionPreference = "Stop"
if (Get-Variable -Name PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue) {
    $PSNativeCommandUseErrorActionPreference = $false
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$resolvedComposeFile = if ([System.IO.Path]::IsPathRooted($ComposeFile)) {
    $ComposeFile
} else {
    Join-Path $repoRoot $ComposeFile
}

$composeArgs = @("-f", $resolvedComposeFile)
if ($EnvFile) {
    $resolvedEnvFile = if ([System.IO.Path]::IsPathRooted($EnvFile)) {
        $EnvFile
    } else {
        Join-Path $repoRoot $EnvFile
    }
    $composeArgs += @("--env-file", $resolvedEnvFile)
}

& docker compose @composeArgs down
if ($LASTEXITCODE -ne 0) {
    throw "docker compose failed with exit code $LASTEXITCODE"
}

