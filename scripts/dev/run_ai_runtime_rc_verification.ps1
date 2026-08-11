<#
.SYNOPSIS
    Run AI Runtime RC verification suite.

.DESCRIPTION
    Runs the complete AI Runtime RC verification suite including Python compilation,
    Python tests, Rust checks, and Rust tests. Supports optional flags for DB tests
    and live OpenAI smoke tests.

    SAFETY:
    - Does NOT print secrets or connection strings.
    - Does NOT modify repo files, DB schema, or .env.
    - Stops on first failure.
    - Preserves/restores TEST_DATABASE_URL and RUN_LIVE_OPENAI_STREAM_SMOKE.

.PARAMETER RunDb
    Run the DB-dependent test suite (calls run_ai_runtime_db_tests.ps1).

.PARAMETER RunLiveOpenAI
    Run the live OpenAI smoke test (calls run_ai_runtime_live_openai_smoke.ps1 -RunLive).

.PARAMETER SkipFullCargo
    Skip the full cargo test -p fms-api for faster local loop.

.EXAMPLE
    .\scripts\dev\run_ai_runtime_rc_verification.ps1
    # Run all non-destructive checks

.EXAMPLE
    .\scripts\dev\run_ai_runtime_rc_verification.ps1 -RunDb
    # Run all checks including DB tests

.EXAMPLE
    .\scripts\dev\run_ai_runtime_rc_verification.ps1 -RunLiveOpenAI -SkipFullCargo
    # Run with live OpenAI smoke but skip full cargo tests
#>

param(
    [switch]$RunDb,
    [switch]$RunLiveOpenAI,
    [switch]$SkipFullCargo
)

$ErrorActionPreference = "Stop"

# Save original environment values
$originalTestDbUrl = $env:TEST_DATABASE_URL
$originalLiveSmoke = $env:RUN_LIVE_OPENAI_STREAM_SMOKE

# Initialize results tracking
$results = @()
$skippedSuites = @()
$failedSuite = $null

function Invoke-Suite {
    param(
        [string]$Name,
        [scriptblock]$Command,
        [bool]$Skip = $false
    )
    
    if ($Skip) {
        Write-Host "=== SKIP: $Name ===" -ForegroundColor Yellow
        $script:skippedSuites += $Name
        return
    }
    
    Write-Host "=== RUNNING: $Name ===" -ForegroundColor Cyan
    Write-Host ""
    
    try {
        & $Command
        $exitCode = $LASTEXITCODE
        
        if ($exitCode -eq 0) {
            Write-Host ""
            Write-Host "=== PASS: $Name ===" -ForegroundColor Green
            $script:results += @{Name=$Name; Status="PASS"}
        } else {
            Write-Host ""
            Write-Host "=== FAIL: $Name (exit code $exitCode) ===" -ForegroundColor Red
            $script:results += @{Name=$Name; Status="FAIL"; ExitCode=$exitCode}
            $script:failedSuite = $Name
            throw "Suite failed: $Name"
        }
    } catch {
        if (-not $script:failedSuite) {
            $script:failedSuite = $Name
        }
        throw
    }
}

try {
    Write-Host "=== AI Runtime RC Verification Suite ===" -ForegroundColor Cyan
    Write-Host "Start time: $(Get-Date)" -ForegroundColor Gray
    Write-Host ""
    
    # Get project root (scripts\dev\script.ps1 -> project root)
    $scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
    $projectRoot = Split-Path -Parent (Split-Path -Parent $scriptDir)
    
    # 1. Python py_compile
    Invoke-Suite "Python py_compile" {
        Write-Host "Checking Python syntax..."
        $pythonExe = Join-Path $projectRoot ".venv\Scripts\python.exe"
        if (-not (Test-Path $pythonExe)) {
            Write-Host "Python executable not found at $pythonExe" -ForegroundColor Red
            exit 1
        }
        
        # Use a single python invocation to check all files
        $checkScript = @"
import py_compile, sys, os
errors = []
src = sys.argv[1]
for root, dirs, files in os.walk(src):
    for f in files:
        if f.endswith('.py'):
            path = os.path.join(root, f)
            try:
                py_compile.compile(path, doraise=True)
            except py_compile.PyCompileError as e:
                errors.append(str(e))
if errors:
    for e in errors:
        print(e, file=sys.stderr)
    sys.exit(1)
print(f'All {sum(1 for r,d,fs in os.walk(src) for f in fs if f.endswith(".py"))} Python files compile successfully')
"@
        $checkFile = Join-Path $env:TEMP "py_compile_check.py"
        Set-Content -Path $checkFile -Value $checkScript -Encoding UTF8
        $srcDir = Join-Path $projectRoot "services\ai-sidecar\src"
        $prev = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        & $pythonExe $checkFile $srcDir
        $ec = $LASTEXITCODE
        $ErrorActionPreference = $prev
        Remove-Item $checkFile -ErrorAction SilentlyContinue
        if ($ec -ne 0) { exit $ec }
    }
    
    # 2. Python tests/sidecar
    Invoke-Suite "Python tests/sidecar" {
        Write-Host "Running Python sidecar tests..."
        $pythonExe = Join-Path $projectRoot ".venv\Scripts\python.exe"
        $testPath = "services\ai-sidecar\tests\sidecar"
        
        Push-Location $projectRoot
        $prev = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        try {
            & $pythonExe -m pytest $testPath -q
            $exitCode = $LASTEXITCODE
        } finally {
            $ErrorActionPreference = $prev
            Pop-Location
        }
        
        if ($exitCode -ne 0) {
            exit $exitCode
        }
    }
    
    # 3-10. cargo checks and tests
    $cargoSuites = @(
        @{Name="cargo check -p fms-api"; Args=@("check", "-p", "fms-api"); Skip=$false},
        @{Name="cargo check -p fms-server"; Args=@("check", "-p", "fms-server"); Skip=$false},
        @{Name="cargo test -p fms-api nl_query"; Args=@("test", "-p", "fms-api", "nl_query", "--", "--nocapture"); Skip=$false},
        @{Name="cargo test -p fms-api streaming_finalizer"; Args=@("test", "-p", "fms-api", "streaming_finalizer", "--", "--nocapture"); Skip=$false},
        @{Name="cargo test -p fms-api service_identity"; Args=@("test", "-p", "fms-api", "service_identity", "--", "--nocapture"); Skip=$false},
        @{Name="cargo test -p fms-api python_sidecar_proxy"; Args=@("test", "-p", "fms-api", "python_sidecar_proxy", "--", "--nocapture"); Skip=$false},
        @{Name="cargo test -p fms-api sse_stream_parser"; Args=@("test", "-p", "fms-api", "sse_stream_parser", "--", "--nocapture"); Skip=$false},
        @{Name="cargo test -p fms-api (full)"; Args=@("test", "-p", "fms-api"); Skip=$SkipFullCargo}
    )
    
    foreach ($s in $cargoSuites) {
        $suiteName = $s.Name
        $suiteArgs = $s.Args
        $suiteSkip = $s.Skip
        Invoke-Suite $suiteName -Skip $suiteSkip -Command {
            Write-Host ("Running: cargo " + ($suiteArgs -join " "))
            Push-Location (Join-Path $projectRoot "services\api-server")
            $prev = $ErrorActionPreference
            $ErrorActionPreference = "Continue"
            try {
                & cargo @suiteArgs
                $exitCode = $LASTEXITCODE
            } finally {
                $ErrorActionPreference = $prev
                Pop-Location
            }
            if ($exitCode -ne 0) { exit $exitCode }
        }
    }
    
    # 11. Optional: DB tests
    Invoke-Suite "DB tests (flight_monitor_test)" {
        Write-Host "Running DB-dependent tests..."
        $dbTestScript = Join-Path $projectRoot "scripts\dev\run_ai_runtime_db_tests.ps1"
        if (-not (Test-Path $dbTestScript)) {
            Write-Host "DB test script not found at $dbTestScript" -ForegroundColor Red
            exit 1
        }
        
        $prev = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        try {
            & $dbTestScript -DatabaseName flight_monitor_test
            $exitCode = $LASTEXITCODE
        } finally {
            $ErrorActionPreference = $prev
        }
        
        if ($exitCode -ne 0) {
            exit $exitCode
        }
    } -Skip (-not $RunDb)
    
    # 12. Optional: Live OpenAI smoke test
    Invoke-Suite "Live OpenAI smoke test" {
        Write-Host "Running live OpenAI smoke test..."
        $liveSmokeScript = Join-Path $projectRoot "scripts\dev\run_ai_runtime_live_openai_smoke.ps1"
        if (-not (Test-Path $liveSmokeScript)) {
            Write-Host "Live smoke script not found at $liveSmokeScript" -ForegroundColor Red
            exit 1
        }
        
        $prev = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        try {
            & $liveSmokeScript -RunLive
            $exitCode = $LASTEXITCODE
        } finally {
            $ErrorActionPreference = $prev
        }
        
        if ($exitCode -ne 0) {
            exit $exitCode
        }
    } -Skip (-not $RunLiveOpenAI)
    
    # All suites passed
    Write-Host ""
    Write-Host "=== ALL RC VERIFICATION SUITES PASSED ===" -ForegroundColor Green
    Write-Host "End time: $(Get-Date)" -ForegroundColor Gray
    
} catch {
    # A suite failed
    Write-Host ""
    Write-Host "=== RC VERIFICATION FAILED ===" -ForegroundColor Red
    Write-Host "Failed suite: $failedSuite" -ForegroundColor Red
    Write-Host "End time: $(Get-Date)" -ForegroundColor Gray
    
    # Print summary
    Write-Host ""
    Write-Host "=== SUMMARY ===" -ForegroundColor Cyan
    foreach ($result in $results) {
        $statusColor = if ($result.Status -eq "PASS") { "Green" } else { "Red" }
        Write-Host "$($result.Name): $($result.Status)" -ForegroundColor $statusColor
    }
    
    foreach ($suite in $skippedSuites) {
        Write-Host "$suite : SKIPPED" -ForegroundColor Yellow
    }
    
    exit 1
    
} finally {
    # Restore original environment
    if ($originalTestDbUrl -ne $null) {
        $env:TEST_DATABASE_URL = $originalTestDbUrl
    } else {
        Remove-Item Env:\TEST_DATABASE_URL -ErrorAction SilentlyContinue
    }
    
    if ($originalLiveSmoke -ne $null) {
        $env:RUN_LIVE_OPENAI_STREAM_SMOKE = $originalLiveSmoke
    } else {
        Remove-Item Env:\RUN_LIVE_OPENAI_STREAM_SMOKE -ErrorAction SilentlyContinue
    }
}

# Print final summary
Write-Host ""
Write-Host "=== FINAL SUMMARY ===" -ForegroundColor Cyan
foreach ($result in $results) {
    $statusColor = if ($result.Status -eq "PASS") { "Green" } else { "Red" }
    Write-Host "$($result.Name): $($result.Status)" -ForegroundColor $statusColor
}

foreach ($suite in $skippedSuites) {
    Write-Host "$suite : SKIPPED" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "Total suites: $($results.Count + $skippedSuites.Count)" -ForegroundColor Gray
Write-Host "Passed: $($results.Count)" -ForegroundColor Green
Write-Host "Skipped: $($skippedSuites.Count)" -ForegroundColor Yellow
Write-Host "Failed: 0" -ForegroundColor Green
