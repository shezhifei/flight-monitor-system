<#
.SYNOPSIS
    Smoke tests for the API mutation testing pilot runner.

.DESCRIPTION
    This script intentionally avoids Pester so it can run on a stock Windows
    PowerShell environment. It validates the local runner contract that matters
    for the #136 pilot: missing cargo-mutants is reported as a skip by default
    with a clear installation hint.
#>

$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Split-Path -Parent (Split-Path -Parent $scriptDir)
$runner = Join-Path $projectRoot "scripts\dev\run_api_mutation_pilot.ps1"

function Assert-True {
    param(
        [bool]$Condition,
        [string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Assert-Contains {
    param(
        [string]$Text,
        [string]$Needle,
        [string]$Message
    )

    Assert-True ($Text.Contains($Needle)) $Message
}

Assert-True (Test-Path -LiteralPath $runner) "Expected mutation pilot runner at $runner"

$previousPath = $env:PATH
$emptyPath = Join-Path $env:TEMP ("fms-empty-path-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $emptyPath | Out-Null

try {
    $env:PATH = $emptyPath
    $powershellExe = (Get-Process -Id $PID).Path
    $output = & $powershellExe -NoProfile -ExecutionPolicy Bypass -File $runner -List 2>&1 | Out-String
    $exitCode = $LASTEXITCODE

    Assert-True ($exitCode -eq 0) "Expected missing cargo-mutants to be a non-failing skip, got exit code $exitCode. Output: $output"
    Assert-Contains $output "SKIP: cargo-mutants is not installed" "Expected missing-tool skip message. Output: $output"
    Assert-Contains $output "cargo install cargo-mutants" "Expected install hint. Output: $output"
} finally {
    $env:PATH = $previousPath
    Remove-Item -LiteralPath $emptyPath -Force -ErrorAction SilentlyContinue
}

Write-Host "api mutation pilot smoke tests passed" -ForegroundColor Green
