<#
.SYNOPSIS
    Run AI Runtime live OpenAI stream smoke test.

.DESCRIPTION
    Runs the live OpenAI stream smoke test (TestLiveOpenAiStreamSmoke) with proper
    safety checks. This test requires both OPENAI_API_KEY to be set and
    RUN_LIVE_OPENAI_STREAM_SMOKE=1 (or -RunLive parameter).

    SAFETY:
    - Does NOT print OPENAI_API_KEY or any secrets.
    - Does NOT run unless explicitly enabled via environment or parameter.
    - Exits with 0 on SKIP, non-zero on failure.

.PARAMETER RunLive
    Explicitly enable the live smoke test (sets RUN_LIVE_OPENAI_STREAM_SMOKE=1).

.EXAMPLE
    .\scripts\dev\run_ai_runtime_live_openai_smoke.ps1
    # Checks environment only; skips if not enabled

.EXAMPLE
    .\scripts\dev\run_ai_runtime_live_openai_smoke.ps1 -RunLive
    # Runs the live smoke test if OPENAI_API_KEY is set
#>

param(
    [switch]$RunLive
)

$ErrorActionPreference = "Stop"

Write-Host "=== AI Runtime Live OpenAI Smoke Test ===" -ForegroundColor Cyan
Write-Host ""

# Check if OPENAI_API_KEY is available
$hasApiKey = $false
if ($env:OPENAI_API_KEY -and $env:OPENAI_API_KEY.Trim()) {
    $hasApiKey = $true
    Write-Host "OPENAI_API_KEY: [SET]" -ForegroundColor Green
} else {
    Write-Host "OPENAI_API_KEY: [NOT SET]" -ForegroundColor Yellow
}

# Check if live smoke is enabled
$liveEnabled = $false
if ($RunLive) {
    $liveEnabled = $true
    Write-Host "RUN_LIVE_OPENAI_STREAM_SMOKE: [ENABLED via -RunLive parameter]" -ForegroundColor Green
} elseif ($env:RUN_LIVE_OPENAI_STREAM_SMOKE -eq "1") {
    $liveEnabled = $true
    Write-Host "RUN_LIVE_OPENAI_STREAM_SMOKE: [ENABLED via environment]" -ForegroundColor Green
} else {
    Write-Host "RUN_LIVE_OPENAI_STREAM_SMOKE: [DISABLED]" -ForegroundColor Yellow
}

Write-Host ""

# Determine if we should run the test
$shouldRun = $hasApiKey -and $liveEnabled

if (-not $shouldRun) {
    Write-Host "=== SKIP: Live OpenAI smoke test ===" -ForegroundColor Yellow
    if (-not $hasApiKey) {
        Write-Host "Reason: OPENAI_API_KEY is not set" -ForegroundColor Yellow
    }
    if (-not $liveEnabled) {
        Write-Host "Reason: RUN_LIVE_OPENAI_STREAM_SMOKE is not enabled" -ForegroundColor Yellow
        Write-Host "To enable: set RUN_LIVE_OPENAI_STREAM_SMOKE=1 or use -RunLive parameter" -ForegroundColor Yellow
    }
    Write-Host ""
    Write-Host "This is not a failure. Live smoke tests are opt-in for security." -ForegroundColor DarkGray
    exit 0
}

# Run the live smoke test
Write-Host "=== Running Live OpenAI Smoke Test ===" -ForegroundColor Green
Write-Host ""

# Set environment for the test
$env:RUN_LIVE_OPENAI_STREAM_SMOKE = "1"

try {
    # Get project root
    $projectRoot = Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path))
    
    # Run pytest with the live smoke test
    $pythonExe = Join-Path $projectRoot ".venv\Scripts\python.exe"
    if (-not (Test-Path $pythonExe)) {
        Write-Error "Python executable not found at $pythonExe"
        exit 1
    }
    
    $testPath = "services\ai-sidecar\tests\sidecar\test_runtime_streaming.py::TestLiveOpenAiStreamSmoke"
    $testArgs = @("-m", "pytest", $testPath, "-q")
    
    Write-Host "Running: $pythonExe -m pytest $testPath -q" -ForegroundColor Green
    Write-Host ""
    
    & $pythonExe @testArgs
    $exitCode = $LASTEXITCODE
    
    Write-Host ""
    if ($exitCode -eq 0) {
        Write-Host "=== LIVE OPENAI SMOKE TEST PASSED ===" -ForegroundColor Green
        $summaryFile = Join-Path $projectRoot "services\ai-sidecar\tests\sidecar\.smoke_summary.json"
        if (Test-Path $summaryFile) {
            Write-Host "=== LIVE SMOKE SUMMARY ===" -ForegroundColor Cyan
            Get-Content $summaryFile -Raw | Write-Host
            Write-Host "==========================" -ForegroundColor Cyan
            Remove-Item $summaryFile -Force
        }
        Write-Host "Provider smoke: ENABLED" -ForegroundColor Green
        Write-Host "pytest result: PASS" -ForegroundColor Green
        Write-Host "Secrets printed: NONE" -ForegroundColor Green
    } else {
        Write-Host "=== LIVE OPENAI SMOKE TEST FAILED ===" -ForegroundColor Red
        Write-Host "Provider smoke: ENABLED" -ForegroundColor Red
        Write-Host "pytest result: FAIL (exit code $exitCode)" -ForegroundColor Red
        Write-Host "Secrets printed: NONE" -ForegroundColor Green
    }
    
    exit $exitCode
    
} catch {
    Write-Error "Test execution failed: $_"
    exit 1
} finally {
    # Clean up environment
    Remove-Item Env:\RUN_LIVE_OPENAI_STREAM_SMOKE -ErrorAction SilentlyContinue
}