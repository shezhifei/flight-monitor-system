Set-StrictMode -Version Latest

function Read-FmsEnvFile {
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

function Get-FmsEnvValue {
    param(
        [hashtable]$Values,
        [string]$Name,
        [string]$Default = ""
    )

    if ($Values.ContainsKey($Name) -and $null -ne $Values[$Name] -and "$($Values[$Name])".Trim() -ne "") {
        return "$($Values[$Name])".Trim()
    }

    return $Default
}

function Resolve-FmsWorkspacePython {
    param([string]$RepoRoot)

    $pythonExe = Join-Path $RepoRoot ".venv\Scripts\python.exe"
    if (-not (Test-Path -LiteralPath $pythonExe)) {
        throw "未找到工作区虚拟环境 Python: $pythonExe"
    }
    return $pythonExe
}

function Test-FmsExecutableAvailable {
    param([string]$Command)

    $normalized = "$Command".Trim()
    if (-not $normalized) {
        return $false
    }

    if (
        [System.IO.Path]::IsPathRooted($normalized) `
            -or $normalized.Contains("\") `
            -or $normalized.Contains("/")
    ) {
        return Test-Path -LiteralPath $normalized
    }

    return $null -ne (Get-Command -Name $normalized -ErrorAction SilentlyContinue)
}

function Resolve-FmsVaultBootstrapMode {
    param(
        [hashtable]$BaseEnvValues,
        [string]$RequestedMode = "",
        [string]$DefaultMode = "local"
    )

    $configuredMode = "$RequestedMode".Trim().ToLowerInvariant()
    $modeWasExplicit = $configuredMode -ne ""
    if (-not $configuredMode) {
        $processMode = "$env:VAULT_BOOTSTRAP_MODE".Trim().ToLowerInvariant()
        if ($processMode) {
            $configuredMode = $processMode
            $modeWasExplicit = $true
        } elseif ($BaseEnvValues.ContainsKey("VAULT_BOOTSTRAP_MODE")) {
            $configuredMode = "$($BaseEnvValues["VAULT_BOOTSTRAP_MODE"])".Trim().ToLowerInvariant()
            $modeWasExplicit = $configuredMode -ne ""
        } else {
            $configuredMode = $DefaultMode
        }
    }
    if (-not $configuredMode) {
        $configuredMode = $DefaultMode
    }

    if ($configuredMode -notin @("local", "docker")) {
        throw "VAULT_BOOTSTRAP_MODE must be 'local' or 'docker'. Actual: $configuredMode"
    }

    $agentBinary = Get-FmsEnvValue -Values $BaseEnvValues -Name "VAULT_AGENT_BINARY" -Default $env:VAULT_AGENT_BINARY
    if (-not $agentBinary) {
        $agentBinary = "vault"
    }
    $hasVaultBinary = Test-FmsExecutableAvailable -Command $agentBinary
    $hasDocker = Test-FmsExecutableAvailable -Command "docker"

    if ($configuredMode -eq "docker") {
        if (-not $hasDocker) {
            throw "VAULT_BOOTSTRAP_MODE=docker requires the Docker CLI on PATH."
        }
        return "docker"
    }

    if ($hasVaultBinary) {
        return "local"
    }

    if ($modeWasExplicit) {
        throw "VAULT_BOOTSTRAP_MODE=local requires the Vault CLI ('$agentBinary') on PATH or a valid VAULT_AGENT_BINARY path."
    }

    if ($hasDocker) {
        Write-Host "Vault CLI '$agentBinary' not found. Falling back to docker bootstrap mode." -ForegroundColor Yellow
        return "docker"
    }

    throw "Vault bootstrap requires either the Vault CLI ('$agentBinary') for local mode or Docker for docker mode. Install Vault, set VAULT_AGENT_BINARY, or set VAULT_BOOTSTRAP_MODE=docker after ensuring Docker is available."
}

function Invoke-FmsVaultBootstrap {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RepoRoot,
        [Parameter(Mandatory = $true)]
        [string]$BaseEnvFile,
        [Parameter(Mandatory = $true)]
        [string]$TemplatePath,
        [Parameter(Mandatory = $true)]
        [string]$RenderedEnvFile,
        [Parameter(Mandatory = $true)]
        [string]$RuntimeEnvFile,
        [Parameter(Mandatory = $true)]
        [string]$AgentConfigFile,
        [ValidateSet("auto", "local", "docker")]
        [string]$Mode = "local"
    )

    $pythonExe = Resolve-FmsWorkspacePython -RepoRoot $RepoRoot
    $bootstrapScript = Join-Path $RepoRoot "scripts\vault\bootstrap_runtime_env.py"
    $baseEnvValues = Read-FmsEnvFile -Path $BaseEnvFile
    $resolvedMode = if ($Mode -eq "auto") {
        Resolve-FmsVaultBootstrapMode -BaseEnvValues $baseEnvValues
    } else {
        Resolve-FmsVaultBootstrapMode -BaseEnvValues $baseEnvValues -RequestedMode $Mode
    }
    $arguments = @(
        $bootstrapScript,
        "--base-env", $BaseEnvFile,
        "--template", $TemplatePath,
        "--runtime-env", $RuntimeEnvFile,
        "--rendered-env", $RenderedEnvFile,
        "--agent-config", $AgentConfigFile,
        "--mode", $resolvedMode
    )

    $bootstrapOutput = & $pythonExe @arguments 2>&1
    if ($LASTEXITCODE -ne 0) {
        if ($bootstrapOutput) {
            $bootstrapOutput | ForEach-Object { Write-Host $_ }
        }
        throw "Vault bootstrap 失败，退出码: $LASTEXITCODE"
    }

    if ($bootstrapOutput) {
        $bootstrapOutput | ForEach-Object { Write-Host $_ }
    }

    return [pscustomobject]@{
        RuntimeEnvFile = $RuntimeEnvFile
        RenderedEnvFile = $RenderedEnvFile
        AgentConfigFile = $AgentConfigFile
        Mode = $resolvedMode
        RuntimeValues = Read-FmsEnvFile -Path $RuntimeEnvFile
    }
}
