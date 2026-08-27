<#
.SYNOPSIS
    AIP API Staging Smoke Execution — runs API-level DB smoke tests
    against the test database with full safety guards.

.DESCRIPTION
    This script:
    1. Validates the target database is NOT production or dev
    2. Reads DB credentials from .env
    3. Validates required schema exists
    4. Checks migrations are up to date
    5. Verifies no test trigger/function residue
    6. Runs the API DB smoke tests (happy path, disable, permission, readiness)

    Unlike run_aip_staging_smoke.ps1, which tests the application service layer
    directly, this script tests the HTTP API boundary:
    - JWT authentication and permission enforcement
    - Route wiring and HTTP status code semantics
    - Readiness API and execution API linkage
    - Production-like execution boundary via actix_web::test

    SAFETY:
    - Refuses to run against flight_monitor, flight_monitor_prod, flight_monitor_dev, prod, production.
    - Does NOT print the full connection string or password.
    - Does NOT write to .env or modify the database schema.
    - Tests set/unset their own env vars (FMS_AI_PROPOSAL_EXECUTION_ENABLED, etc.)

.PARAMETER DatabaseName
    Target database name. Default: flight_monitor_test.
    Refuses prod, production, flight_monitor, flight_monitor_dev.

.PARAMETER SkipReadiness
    Skip the readiness API tests.

.EXAMPLE
    .\scripts\dev\run_aip_api_staging_smoke.ps1
    .\scripts\dev\run_aip_api_staging_smoke.ps1 -DatabaseName flight_monitor_test
    .\scripts\dev\run_aip_api_staging_smoke.ps1 -SkipReadiness
#>

[CmdletBinding()]
param(
    [string]$DatabaseName = "flight_monitor_test",
    [switch]$SkipReadiness
)

$ErrorActionPreference = "Stop"

# ── Safety: refuse dangerous databases ──────────────────────────────────
$blockedNames = @("flight_monitor", "flight_monitor_prod", "flight_monitor_production", "flight_monitor_dev", "prod", "production")
if ($blockedNames -contains $DatabaseName.ToLower()) {
    Write-Error "REFUSED: Database '$DatabaseName' is not allowed for API smoke testing. Use flight_monitor_test or an explicit staging database."
    exit 1
}

# ── Locate .env ─────────────────────────────────────────────────────────
$projectRoot = Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path))
$envFile = Join-Path $projectRoot ".env"

if (-not (Test-Path $envFile)) {
    Write-Error ".env file not found at $envFile"
    exit 1
}

# ── Parse .env for DB_* variables ───────────────────────────────────────
$dbHost = "localhost"
$dbPort = "5432"
$dbUser = ""
$dbPassword = ""

foreach ($line in Get-Content $envFile) {
    $line = $line.Trim()
    if ($line -match "^#" -or $line -eq "") { continue }
    if ($line -match "^DB_HOST\s*=\s*(.+)$") { $dbHost = $Matches[1].Trim().Trim('"').Trim("'") }
    if ($line -match "^DB_PORT\s*=\s*(.+)$") { $dbPort = $Matches[1].Trim().Trim('"').Trim("'") }
    if ($line -match "^DB_USER\s*=\s*(.+)$") { $dbUser = $Matches[1].Trim().Trim('"').Trim("'") }
    if ($line -match "^DB_PASSWORD\s*=\s*(.+)$") { $dbPassword = $Matches[1].Trim().Trim('"').Trim("'") }
}

if (-not $dbUser) {
    Write-Error "DB_USER not found in .env"
    exit 1
}
if (-not $dbPassword) {
    Write-Error "DB_PASSWORD not found in .env"
    exit 1
}

# Construct TEST_DATABASE_URL (never printed)
$testUrl = "postgres://${dbUser}:${dbPassword}@${dbHost}:${dbPort}/${DatabaseName}"

# ── Print target info (no password) ─────────────────────────────────────
Write-Host ""
Write-Host "=== AIP API Staging Smoke Execution ===" -ForegroundColor Cyan
Write-Host "Mode     : API boundary (HTTP layer)" -ForegroundColor Yellow
Write-Host "Database : $DatabaseName" -ForegroundColor Yellow
Write-Host "Host     : ${dbHost}:${dbPort}" -ForegroundColor Yellow
Write-Host "User     : $dbUser" -ForegroundColor Yellow
Write-Host "(Connection string is NOT printed for security)" -ForegroundColor DarkGray
Write-Host ""
Write-Host "Difference from service smoke:" -ForegroundColor Yellow
Write-Host "  Service smoke (run_aip_staging_smoke.ps1):"
Write-Host "    - Tests application/executor/outbox layer directly"
Write-Host "  API smoke (this script):"
Write-Host "    - Tests auth, route wiring, HTTP error semantics"
Write-Host "    - Tests production-like execution boundary via actix_web::test"
Write-Host ""

# ── Set environment ─────────────────────────────────────────────────────
$env:TEST_DATABASE_URL = $testUrl
$env:PGPASSWORD = $dbPassword

try {
    # ── Validate required schema ────────────────────────────────────────
    Write-Host "Validating required schema..." -ForegroundColor Green

    $requiredRelations = @(
        "ai_action_proposals",
        "ai_run_events",
        "domain_event_outbox",
        "aip_ontology_objects",
        "aip_ontology_actions",
        "aip_object_policies",
        "aip_functions",
        "todos",
        "users"
    )

    $missingRelations = @()
    foreach ($relation in $requiredRelations) {
        $query = "SELECT CASE WHEN to_regclass('public.$relation') IS NULL THEN 'missing' ELSE 'present' END;"
        $result = psql -h $dbHost -p $dbPort -U $dbUser -d $DatabaseName -tAc $query 2>&1
        if ($LASTEXITCODE -ne 0) {
            Write-Host $result -ForegroundColor Red
            throw "Failed to inspect required relation '$relation' in database '$DatabaseName'."
        }

        $status = ($result | Select-Object -Last 1).Trim()
        if ($status -ne "present") {
            $missingRelations += $relation
        }
    }

    if ($missingRelations.Count -gt 0) {
        $missingList = $missingRelations -join ", "
        throw "Database '$DatabaseName' is missing required relations: $missingList. Run migrations first."
    }

    Write-Host "  Schema OK ($($requiredRelations.Count)/$($requiredRelations.Count) tables present)" -ForegroundColor Green

    # ── Check migrations ────────────────────────────────────────────────
    Write-Host "Checking migrations..." -ForegroundColor Green

    $migrateInfo = sqlx migrate info --database-url $testUrl 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Host $migrateInfo -ForegroundColor Red
        throw "sqlx migrate info failed for database '$DatabaseName'."
    }

    if ($migrateInfo | Select-String -Pattern '^\s*\d+/pending\b|different checksum' -Quiet) {
        Write-Host $migrateInfo -ForegroundColor Yellow
        throw "Database '$DatabaseName' has pending migrations or checksum differences. Fix migrations before running API smoke tests."
    }

    Write-Host "  Migrations OK (no pending/checksum mismatch)" -ForegroundColor Green

    # ── Check trigger/function residue ──────────────────────────────────
    Write-Host "Checking trigger/function residue..." -ForegroundColor Green

    $trgResidue = psql -h $dbHost -p $dbPort -U $dbUser -d $DatabaseName -tAc "SELECT tgname FROM pg_trigger WHERE tgname='trg_test_outbox_fail'" 2>&1
    $fnResidue = psql -h $dbHost -p $dbPort -U $dbUser -d $DatabaseName -tAc "SELECT proname FROM pg_proc WHERE proname='fn_test_outbox_fail'" 2>&1

    if ($trgResidue -and $trgResidue.Trim() -ne "" -and $trgResidue.Trim() -ne "(0 rows)") {
        throw "Test trigger 'trg_test_outbox_fail' found. Clean up before running API smoke."
    }
    if ($fnResidue -and $fnResidue.Trim() -ne "" -and $fnResidue.Trim() -ne "(0 rows)") {
        throw "Test function 'fn_test_outbox_fail' found. Clean up before running API smoke."
    }

    Write-Host "  Residue clean" -ForegroundColor Green

    # ── Run API smoke tests ─────────────────────────────────────────────
    Write-Host ""
    Write-Host "Running API DB smoke tests..." -ForegroundColor Cyan

    Push-Location (Join-Path $projectRoot "services\api-server")

    # Run proposal API smoke tests
    Write-Host "  [1/2] Proposal execution API smoke tests..." -ForegroundColor Yellow
    cargo test -p fms-api api_proposal_smoke -- --ignored --nocapture --test-threads=1
    $proposalExitCode = $LASTEXITCODE

    $readinessExitCode = 0
    if (-not $SkipReadiness) {
        Write-Host "  [2/2] Readiness API smoke tests..." -ForegroundColor Yellow
        cargo test -p fms-api api_readiness_smoke -- --ignored --nocapture --test-threads=1
        $readinessExitCode = $LASTEXITCODE
    } else {
        Write-Host "  [2/2] Readiness API smoke tests... SKIPPED (-SkipReadiness)" -ForegroundColor DarkGray
    }

    $exitCode = if ($proposalExitCode -ne 0) { $proposalExitCode } elseif ($readinessExitCode -ne 0) { $readinessExitCode } else { 0 }

    Pop-Location

} finally {
    Remove-Item Env:\TEST_DATABASE_URL -ErrorAction SilentlyContinue
    Remove-Item Env:\PGPASSWORD -ErrorAction SilentlyContinue
    Remove-Item Env:\FMS_AI_PROPOSAL_EXECUTION_ENABLED -ErrorAction SilentlyContinue
    Remove-Item Env:\FMS_AI_EXECUTION_READINESS_OVERRIDE -ErrorAction SilentlyContinue
}

if ($exitCode -ne 0) {
    Write-Host ""
    Write-Host "=== API SMOKE TESTS FAILED ===" -ForegroundColor Red
    Write-Host ""
    Write-Host "Troubleshooting:" -ForegroundColor Red
    Write-Host "  - 'API execute should return 200' → DomainActionExecutor not wired or Flight.add_note missing"
    Write-Host "  - 'API execute should return 409' → env var not cleaned; check --test-threads=1"
    Write-Host "  - 'API execute should return 403' → permission or readiness gate issue"
    Write-Host "  - 'readiness should be Ready' → env var override not set in time"
    Write-Host "  - 'readiness should be NotReady' → override leaked from prior test"
    Write-Host "  - 'proposal should remain Approved' → proposal already executed by prior run; ULID ids should prevent this"
    exit $exitCode
}

Write-Host ""
Write-Host "=== ALL API SMOKE TESTS PASSED ===" -ForegroundColor Green
Write-Host ""
Write-Host "Verification summary:" -ForegroundColor Cyan
Write-Host "  [PASS] Todo.create proposal executed via HTTP API end-to-end"
Write-Host "  [PASS] Business row created in todos table"
Write-Host "  [PASS] Domain event outbox entry written"
Write-Host "  [PASS] Audit events: execution_requested, execution_started, execution_succeeded"
Write-Host "  [PASS] Execution-disabled: API returns 409 Conflict, no side effects"
Write-Host "  [PASS] Permission denied: API returns 403 Forbidden, no side effects"
Write-Host "  [PASS] Readiness not ready: API returns 403 Forbidden, audit event recorded"
if (-not $SkipReadiness) {
    Write-Host "  [PASS] Readiness API returns Ready with staging override"
    Write-Host "  [PASS] Readiness API returns NotReady without staging override"
}
Write-Host ""
