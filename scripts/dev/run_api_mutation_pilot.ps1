<#
.SYNOPSIS
    Run the Rust API mutation testing pilot.

.DESCRIPTION
    This is a local/manual runner for audit technical debt #136. It targets
    pure Rust domain logic by default and does not require PostgreSQL, Redis,
    MQ, Python sidecars, or external network services.

    If cargo-mutants is not installed, the runner prints a clear SKIP message
    and exits successfully by default. Use -RequireTool when a scheduled job
    should fail on a missing toolchain.

.PARAMETER List
    List candidate mutants without running mutation tests.

.PARAMETER CheckOnly
    Build mutants without running tests.

.PARAMETER RequireTool
    Return a non-zero exit code when cargo-mutants is missing.

.PARAMETER Jobs
    Number of concurrent cargo-mutants jobs. Keep this conservative because
    each job may run its own cargo process.

.PARAMETER Package
    Cargo package whose tests should catch the selected mutants.

.PARAMETER File
    Optional source files to examine. Paths are relative to services/api-server
    unless absolute. When omitted, .cargo/mutants.toml supplies the pilot set.

.PARAMETER OutputDir
    Output root for cargo-mutants reports. Relative paths are resolved from
    the repository root.

.EXAMPLE
    .\scripts\dev\run_api_mutation_pilot.ps1 -List
    List the default pilot mutants.

.EXAMPLE
    .\scripts\dev\run_api_mutation_pilot.ps1 -Jobs 2
    Run the default domain mutation pilot.

.EXAMPLE
    .\scripts\dev\run_api_mutation_pilot.ps1 -File crates\domain\src\models\ai_job.rs -Jobs 1
    Run the pilot against one explicit pure-logic file.
#>

param(
    [switch]$List,
    [switch]$CheckOnly,
    [switch]$RequireTool,
    [ValidateRange(1, 8)]
    [int]$Jobs = 2,
    [ValidateNotNullOrEmpty()]
    [string]$Package = "fms-domain",
    [string[]]$File = @(),
    [ValidateNotNullOrEmpty()]
    [string]$OutputDir = ".tmp\mutation\api-server-domain-pilot"
)

$ErrorActionPreference = "Stop"

function Write-SkipAndExit {
    param(
        [string]$Reason,
        [int]$MissingToolExitCode = 2
    )

    Write-Host "SKIP: cargo-mutants is not installed or not runnable. $Reason" -ForegroundColor Yellow
    Write-Host "Install with: cargo install cargo-mutants" -ForegroundColor Yellow
    Write-Host "Then run: .\scripts\dev\run_api_mutation_pilot.ps1 -List" -ForegroundColor Yellow

    if ($RequireTool) {
        exit $MissingToolExitCode
    }

    exit 0
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Split-Path -Parent (Split-Path -Parent $scriptDir)
$apiServerDir = Join-Path $projectRoot "services\api-server"
$configFile = Join-Path $apiServerDir ".cargo\mutants.toml"

if (-not (Test-Path -LiteralPath $apiServerDir)) {
    throw "API server workspace not found at $apiServerDir"
}

if (-not (Test-Path -LiteralPath $configFile)) {
    throw "cargo-mutants pilot config not found at $configFile"
}

$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $cargo) {
    Write-SkipAndExit "The cargo command is not on PATH; install the Rust toolchain first."
}

$previousErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"
try {
    & cargo mutants --version *> $null
    $mutantsVersionExitCode = $LASTEXITCODE
} finally {
    $ErrorActionPreference = $previousErrorActionPreference
}

if ($mutantsVersionExitCode -ne 0) {
    Write-SkipAndExit "cargo is available, but the 'mutants' subcommand is missing."
}

if ([System.IO.Path]::IsPathRooted($OutputDir)) {
    $resolvedOutputDir = $OutputDir
} else {
    $resolvedOutputDir = Join-Path $projectRoot $OutputDir
}

New-Item -ItemType Directory -Force -Path $resolvedOutputDir | Out-Null

$mutantsArgs = @(
    "mutants",
    "--jobs", $Jobs.ToString(),
    "--output", $resolvedOutputDir,
    "--test-package", $Package
)

if ($List) {
    $mutantsArgs += "--list"
}

if ($CheckOnly) {
    $mutantsArgs += "--check"
}

foreach ($sourceFile in $File) {
    $normalizedFile = $sourceFile
    if ([System.IO.Path]::IsPathRooted($sourceFile)) {
        $normalizedFile = [System.IO.Path]::GetRelativePath($apiServerDir, $sourceFile)
    }
    $mutantsArgs += @("--file", $normalizedFile)
}

Write-Host "=== Rust API mutation testing pilot (#136) ===" -ForegroundColor Cyan
Write-Host "Workspace: $apiServerDir"
Write-Host "Config:    $configFile"
Write-Host "Package:   $Package"
Write-Host "Output:    $resolvedOutputDir"
Write-Host "Mode:      $(if ($List) { "list" } elseif ($CheckOnly) { "check-only" } else { "mutation run" })"
Write-Host ""
Write-Host "Running: cargo $($mutantsArgs -join ' ')" -ForegroundColor Green

Push-Location $apiServerDir
try {
    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & cargo @mutantsArgs
        $exitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
} finally {
    Pop-Location
}

exit $exitCode
