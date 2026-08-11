[CmdletBinding()]
param(
    [string]$VaultAddr = "http://127.0.0.1:8200",
    [string]$RuntimeDir = "deploy/vault/.runtime",
    [string]$AppRoleDir = "deploy/vault/approle",
    [string]$SeedEnvFile = "deploy/vault/bootstrap.secrets.env",
    [string]$VaultComposeFile = "deploy/vault/docker-compose.vault.yml",
    [switch]$SkipSecretSeed,
    [switch]$RotateSecretIds
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-RepoPath {
    param([string]$RelativeOrAbsolutePath)

    if ([System.IO.Path]::IsPathRooted($RelativeOrAbsolutePath)) {
        return $RelativeOrAbsolutePath
    }

    return (Join-Path $script:RepoRoot $RelativeOrAbsolutePath)
}

function Resolve-VaultExecutable {
    $configuredBinary = "$env:VAULT_AGENT_BINARY".Trim()
    if ($configuredBinary) {
        if ([System.IO.Path]::IsPathRooted($configuredBinary) -or $configuredBinary.Contains("\") -or $configuredBinary.Contains("/")) {
            if (Test-Path -LiteralPath $configuredBinary) {
                return (Resolve-Path -LiteralPath $configuredBinary).Path
            }
        } else {
            $command = Get-Command -Name $configuredBinary -ErrorAction SilentlyContinue
            if ($command) {
                return $command.Source
            }
        }
    }

    $workspaceVault = Join-Path $script:RepoRoot "vault\vault.exe"
    if (Test-Path -LiteralPath $workspaceVault) {
        return (Resolve-Path -LiteralPath $workspaceVault).Path
    }

    $command = Get-Command -Name "vault" -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }

    return ""
}

function Invoke-VaultCommand {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments,
        [switch]$AllowNonZeroExit
    )

    if (-not $Arguments -or $Arguments.Count -eq 0) {
        throw "Invoke-VaultCommand received no arguments."
    }

    Write-Host "[vault-init] vault $($Arguments -join ' ')" -ForegroundColor DarkGray

    if ($script:UseDockerVaultCli) {
        $dockerEnvArgs = @("-e", "VAULT_ADDR=$script:VaultAddr")
        if ($env:VAULT_TOKEN) {
            $dockerEnvArgs += @("-e", "VAULT_TOKEN=$($env:VAULT_TOKEN)")
        }

        $command = @(
            "compose",
            "-f", $script:ResolvedVaultComposeFile,
            "exec",
            "-T"
        )
        $command += $dockerEnvArgs
        $command += @(
            "vault",
            "vault"
        )
        $command += $Arguments
        $output = & docker @command 2>&1
    }
    else {
        $output = & $script:VaultExecutable @Arguments 2>&1
    }

    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0 -and -not $AllowNonZeroExit) {
        $prefix = if ($script:UseDockerVaultCli) { "docker compose exec vault vault" } else { "vault" }
        throw "$prefix $($Arguments -join ' ') failed with exit code ${exitCode}: $output"
    }

    return @{
        ExitCode = $exitCode
        Output = ($output -join [Environment]::NewLine).Trim()
    }
}

function Invoke-VaultJson {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments,
        [switch]$AllowNonZeroExit
    )

    $result = Invoke-VaultCommand -Arguments $Arguments -AllowNonZeroExit:$AllowNonZeroExit
    if (-not $result.Output) {
        return $null
    }

    return $result.Output | ConvertFrom-Json
}

function Get-VaultObjectPropertyNames {
    param($InputObject)

    if ($null -eq $InputObject) {
        return @()
    }

    return @($InputObject.PSObject.Properties | ForEach-Object { $_.Name })
}

function Read-DotEnv {
    param([string]$Path)

    $values = @{}
    if (-not (Test-Path -LiteralPath $Path)) {
        return $values
    }

    foreach ($line in Get-Content -LiteralPath $Path) {
        $trimmed = $line.Trim()
        if (-not $trimmed -or $trimmed.StartsWith("#") -or -not $trimmed.Contains("=")) {
            continue
        }

        $parts = $trimmed.Split("=", 2)
        $values[$parts[0].Trim()] = $parts[1].Trim()
    }

    return $values
}

function Get-VaultContainerId {
    $output = & docker compose -f $script:ResolvedVaultComposeFile ps -q vault 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "docker compose -f $script:ResolvedVaultComposeFile ps -q vault failed: $output"
    }

    return ($output -join [Environment]::NewLine).Trim()
}

function Initialize-VaultCliMode {
    $resolvedVaultExecutable = Resolve-VaultExecutable
    if ($resolvedVaultExecutable) {
        $script:UseDockerVaultCli = $false
        $script:VaultExecutable = $resolvedVaultExecutable
        return
    }

    if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
        throw "vault CLI was not found on PATH, and docker is not available for container fallback."
    }

    $containerId = Get-VaultContainerId
    if (-not $containerId) {
        throw "vault CLI was not found on PATH, and the Vault compose service is not running. Start deploy/vault/Start-VaultDocker.ps1 first."
    }

    $script:UseDockerVaultCli = $true
    $script:VaultContainerId = $containerId
}

function Ensure-Directory {
    param([string]$Path)

    New-Item -ItemType Directory -Path $Path -Force | Out-Null
}

function Get-VaultStatus {
    return Invoke-VaultJson -Arguments @("status", "-format=json") -AllowNonZeroExit
}

function Initialize-VaultIfNeeded {
    param(
        [string]$RootTokenPath,
        [string]$UnsealKeysPath
    )

    $status = Get-VaultStatus
    if ($status.initialized) {
        return
    }

    $init = Invoke-VaultJson -Arguments @("operator", "init", "-format=json")
    if (-not $init) {
        throw "vault operator init returned no JSON payload"
    }

    ($init.root_token + [Environment]::NewLine) | Set-Content -LiteralPath $RootTokenPath -Encoding ASCII
    ($init.unseal_keys_b64 | ConvertTo-Json -Depth 4) | Set-Content -LiteralPath $UnsealKeysPath -Encoding UTF8
}

function Unseal-VaultIfNeeded {
    param([string]$UnsealKeysPath)

    $status = Get-VaultStatus
    if (-not $status.sealed) {
        return
    }

    if (-not (Test-Path -LiteralPath $UnsealKeysPath)) {
        throw "Vault is sealed and unseal keys file is missing: $UnsealKeysPath"
    }

    $keys = Get-Content -LiteralPath $UnsealKeysPath -Raw | ConvertFrom-Json
    foreach ($key in ($keys | Select-Object -First 3)) {
        Invoke-VaultCommand -Arguments @("operator", "unseal", $key) | Out-Null
    }
}

function Login-WithRootToken {
    param([string]$RootTokenPath)

    if (-not (Test-Path -LiteralPath $RootTokenPath)) {
        throw "Vault root token file is missing: $RootTokenPath"
    }

    $env:VAULT_TOKEN = (Get-Content -LiteralPath $RootTokenPath -Raw).Trim()
    if (-not $env:VAULT_TOKEN) {
        throw "Vault root token file is empty: $RootTokenPath"
    }
}

function Enable-VaultMountIfMissing {
    param(
        [string]$Type,
        [string]$Path,
        [string[]]$ExtraArgs = @()
    )

    if ($Type -eq "auth") {
        $authMounts = Invoke-VaultJson -Arguments @("auth", "list", "-format=json")
        if ((Get-VaultObjectPropertyNames -InputObject $authMounts) -contains "${Path}/") {
            return
        }

        $arguments = @("auth", "enable", "-path=$Path", $Path)
        $arguments += $ExtraArgs
        Invoke-VaultCommand -Arguments $arguments | Out-Null
        return
    }

    $secretMounts = Invoke-VaultJson -Arguments @("secrets", "list", "-format=json")
    if ((Get-VaultObjectPropertyNames -InputObject $secretMounts) -contains "${Path}/") {
        return
    }

    $arguments = @("secrets", "enable", "-path=$Path")
    $arguments += $ExtraArgs
    Invoke-VaultCommand -Arguments $arguments | Out-Null
}

function Ensure-VaultAuditDevice {
    $audits = Invoke-VaultJson -Arguments @("audit", "list", "-format=json")
    if ((Get-VaultObjectPropertyNames -InputObject $audits) -contains "file/") {
        return
    }

    Invoke-VaultCommand -Arguments @("audit", "enable", "file", "file_path=/vault/logs/audit.log") | Out-Null
}

function Ensure-VaultPolicy {
    param(
        [string]$Name,
        [string]$PolicyFile
    )

    Invoke-VaultCommand -Arguments @("policy", "write", $Name, $PolicyFile) | Out-Null
}

function Ensure-AppRole {
    param(
        [string]$RoleName,
        [string]$PolicyName
    )

    Invoke-VaultCommand -Arguments @(
        "write",
        "auth/approle/role/$RoleName",
        "token_policies=$PolicyName",
        "token_ttl=1h",
        "token_max_ttl=4h",
        "secret_id_ttl=0",
        "secret_id_num_uses=0"
    ) | Out-Null
}

function Write-AppRoleArtifacts {
    param(
        [string]$RoleName,
        [string]$RoleIdPath,
        [string]$SecretIdPath,
        [bool]$Rotate
    )

    $roleId = Invoke-VaultCommand -Arguments @("read", "-field=role_id", "auth/approle/role/$RoleName/role-id")
    ($roleId.Output + [Environment]::NewLine) | Set-Content -LiteralPath $RoleIdPath -Encoding ASCII

    $secretIdResponse = Invoke-VaultJson -Arguments @("write", "-f", "-format=json", "auth/approle/role/$RoleName/secret-id")
    $secretId = "$($secretIdResponse.data.secret_id)".Trim()
    if (-not $secretId) {
        throw "Vault did not return a secret_id for AppRole '$RoleName'."
    }
    ($secretId + [Environment]::NewLine) | Set-Content -LiteralPath $SecretIdPath -Encoding ASCII
}

function Write-SeedSecrets {
    param([string]$Path)

    $values = Read-DotEnv -Path $Path
    if ($values.Count -eq 0) {
        throw "Vault seed env file is empty: $Path"
    }

    $groups = @{}
    $prefixToVaultPath = @{
        "FMS_SHARED" = "kv/fms/shared"
        "FMS_API" = "kv/fms/api"
        "FMS_WORKER" = "kv/fms/worker"
        "FMS_RUST_API" = "kv/fms/rust-api"
        "FMS_FLOWABLE" = "kv/fms/flowable"
    }

    foreach ($entry in $values.GetEnumerator()) {
        $key = $entry.Key
        $separatorIndex = $key.IndexOf("__")
        if ($separatorIndex -lt 1) {
            continue
        }

        $prefix = $key.Substring(0, $separatorIndex)
        $secretKey = $key.Substring($separatorIndex + 2)
        if (-not $prefixToVaultPath.ContainsKey($prefix)) {
            continue
        }

        if (-not $groups.ContainsKey($prefix)) {
            $groups[$prefix] = @{}
        }
        $groups[$prefix][$secretKey] = $entry.Value
    }

    foreach ($prefix in $groups.Keys) {
        $secretPath = $prefixToVaultPath[$prefix]
        $arguments = @("kv", "put", $secretPath)
        foreach ($kv in $groups[$prefix].GetEnumerator()) {
            $arguments += "$($kv.Key)=$($kv.Value)"
        }
        Invoke-VaultCommand -Arguments $arguments | Out-Null
    }
}

function Sync-VaultPoliciesToContainer {
    if (-not $script:UseDockerVaultCli) {
        return
    }

    if (-not $script:VaultContainerId) {
        $script:VaultContainerId = Get-VaultContainerId
    }

    & docker cp "$($script:ResolvedPoliciesDir)\." "$($script:VaultContainerId):/tmp/fms-policies" 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to copy Vault policies into the running Vault container."
    }
}

$script:RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$script:VaultAddr = $VaultAddr
$resolvedRuntimeDir = Resolve-RepoPath -RelativeOrAbsolutePath $RuntimeDir
$resolvedAppRoleDir = Resolve-RepoPath -RelativeOrAbsolutePath $AppRoleDir
$resolvedSeedEnvFile = Resolve-RepoPath -RelativeOrAbsolutePath $SeedEnvFile
$resolvedPoliciesDir = Join-Path $script:RepoRoot "deploy\vault\policies"
$script:ResolvedPoliciesDir = $resolvedPoliciesDir
$script:ResolvedVaultComposeFile = Resolve-RepoPath -RelativeOrAbsolutePath $VaultComposeFile
$rootTokenPath = Join-Path $resolvedRuntimeDir "root-token.txt"
$unsealKeysPath = Join-Path $resolvedRuntimeDir "unseal-keys.json"
$script:UseDockerVaultCli = $false
$script:VaultContainerId = ""
$script:VaultExecutable = ""

Initialize-VaultCliMode
Ensure-Directory -Path $resolvedRuntimeDir
Ensure-Directory -Path $resolvedAppRoleDir

$env:VAULT_ADDR = $VaultAddr

Initialize-VaultIfNeeded -RootTokenPath $rootTokenPath -UnsealKeysPath $unsealKeysPath
Unseal-VaultIfNeeded -UnsealKeysPath $unsealKeysPath
Login-WithRootToken -RootTokenPath $rootTokenPath
Ensure-VaultAuditDevice
Enable-VaultMountIfMissing -Type "secrets" -Path "kv" -ExtraArgs @("kv-v2")
Enable-VaultMountIfMissing -Type "auth" -Path "approle"
Sync-VaultPoliciesToContainer

foreach ($policyFile in Get-ChildItem -LiteralPath $resolvedPoliciesDir -Filter *.hcl) {
    $policyPath = if ($script:UseDockerVaultCli) {
        "/tmp/fms-policies/$($policyFile.Name)"
    } else {
        $policyFile.FullName
    }

    Ensure-VaultPolicy -Name $policyFile.BaseName -PolicyFile $policyPath
}

$approles = @(
    [pscustomobject]@{ Name = "fms-api"; Policy = "fms-api" },
    [pscustomobject]@{ Name = "fms-worker"; Policy = "fms-worker" },
    [pscustomobject]@{ Name = "fms-rust-api"; Policy = "fms-rust-api" },
    [pscustomobject]@{ Name = "fms-ops-bootstrap"; Policy = "fms-ops-bootstrap" }
)

foreach ($approle in $approles) {
    Ensure-AppRole -RoleName $approle.Name -PolicyName $approle.Policy
    Write-AppRoleArtifacts `
        -RoleName $approle.Name `
        -RoleIdPath (Join-Path $resolvedAppRoleDir "$($approle.Name).role_id") `
        -SecretIdPath (Join-Path $resolvedAppRoleDir "$($approle.Name).secret_id") `
        -Rotate:$RotateSecretIds.IsPresent
}

if (-not $SkipSecretSeed -and (Test-Path -LiteralPath $resolvedSeedEnvFile)) {
    Write-SeedSecrets -Path $resolvedSeedEnvFile
}

Write-Host "Vault bootstrap complete." -ForegroundColor Green
Write-Host "Vault CLI mode: $(if ($script:UseDockerVaultCli) { 'docker-compose' } else { 'local-binary' })" -ForegroundColor DarkGray
Write-Host "Root token: $rootTokenPath" -ForegroundColor DarkGray
Write-Host "Unseal keys: $unsealKeysPath" -ForegroundColor DarkGray
Write-Host "AppRole artifacts: $resolvedAppRoleDir" -ForegroundColor DarkGray
if (-not $SkipSecretSeed) {
    Write-Host "Seed file: $resolvedSeedEnvFile" -ForegroundColor DarkGray
}
