[CmdletBinding()]
param(
    [string]$ComposeFile = "deploy/vault/docker-compose.vault.yml",
    [string]$EnvFile = "",
    [int]$WaitTimeoutSeconds = 60
)

$ErrorActionPreference = "Stop"
if (Get-Variable -Name PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue) {
    $PSNativeCommandUseErrorActionPreference = $false
}

function Invoke-VaultCompose {
    param([string[]]$Arguments)

    & docker compose @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "docker compose failed with exit code $LASTEXITCODE"
    }
}

function Wait-VaultPort {
    param(
        [string]$HostName,
        [int]$Port,
        [int]$TimeoutSeconds
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        try {
            $client = [System.Net.Sockets.TcpClient]::new()
            $iar = $client.BeginConnect($HostName, $Port, $null, $null)
            if ($iar.AsyncWaitHandle.WaitOne(1000, $false)) {
                $client.EndConnect($iar)
                $client.Dispose()
                return
            }
            $client.Dispose()
        }
        catch {
        }
        Start-Sleep -Seconds 2
    }

    throw "Vault port ${HostName}:${Port} did not become reachable within ${TimeoutSeconds}s"
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

Invoke-VaultCompose -Arguments ($composeArgs + @("up", "-d"))
Wait-VaultPort -HostName "127.0.0.1" -Port 8200 -TimeoutSeconds $WaitTimeoutSeconds

Write-Host "Vault CE is reachable at https://127.0.0.1:8200" -ForegroundColor Green
Write-Host "Next step: powershell -ExecutionPolicy Bypass -File scripts/vault/Initialize-VaultCe.ps1" -ForegroundColor Cyan

