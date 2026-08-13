<#
.SYNOPSIS
    AIP Staging Smoke Execution — runs Todo.create end-to-end smoke tests
    against the test database with full safety guards.

.DESCRIPTION
    This script:
    1. Validates the target database is NOT production or dev
    2. Reads DB credentials from .env
    3. Validates required schema exists
    4. Checks migrations are up to date
    5. Verifies no test trigger/function residue
    6. Runs the staging smoke DB tests (happy path + disable + readiness drills)

    SAFETY:
    - Refuses to run against flight_monitor, flight_monitor_prod, flight_monitor_dev, prod, production.
    - Does NOT print the full connection string or password.
    - Does NOT write to .env or modify the database schema.
    - Tests set/unset their own env vars (FMS_AI_PROPOSAL_EXECUTION_ENABLED, etc.)

.PARAMETER DatabaseName
    Target database name. Default: flight_monitor_test.
    Refuses prod, production, flight_monitor, flight_monitor_dev.

.PARAMETER SkipDisableDrill
    Skip the execution-disabled and readiness-not-ready drill tests.

.EXAMPLE
    .\scripts\dev\run_aip_staging_smoke.ps1
    .\scripts\dev\run_aip_staging_smoke.ps1 -DatabaseName flight_monitor_test
    .\scripts\dev\run_aip_staging_smoke.ps1 -SkipDisableDrill
#>

[CmdletBinding()]
param(
    [string]$DatabaseName = "flight_monitor_test",
    [switch]$SkipDisableDrill
)

$ErrorActionPreference = "Stop"

# ── Safety: refuse dangerous databases ──────────────────────────────────
$blockedNames = @("flight_monitor", "flight_monitor_prod", "flight_monitor_production", "flight_monitor_dev", "prod", "production")
if ($blockedNames -contains $DatabaseName.ToLower()) {
    Write-Error "REFUSED: Database '$DatabaseName' is not allowed for smoke testing. Use flight_monitor_test or an explicit staging database."
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
Write-Host "=== AIP Staging Smoke Execution ===" -ForegroundColor Cyan
Write-Host "Database : $DatabaseName" -ForegroundColor Yellow
Write-Host "Host     : ${dbHost}:${dbPort}" -ForegroundColor Yellow
Write-Host "User     : $dbUser" -ForegroundColor Yellow
Write-Host "(Connection string is NOT printed for security)" -ForegroundColor DarkGray
Write-Host ""
Write-Host "Flags (set by tests):" -ForegroundColor Yellow
Write-Host "  FMS_AI_PROPOSAL_EXECUTION_ENABLED    = true"
Write-Host "  FMS_AI_EXECUTION_READINESS_OVERRIDE  = staging"
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

    if ($migrateInfo | Select-String -Pattern "^\s*\d+/pending\b" -Quiet) {
        Write-Host $migrateInfo -ForegroundColor Yellow
        throw "Database '$DatabaseName' has pending migrations. Apply them before running smoke tests."
    }

    Write-Host "  Migrations OK (no pending)" -ForegroundColor Green

    # ── Check trigger/function residue ──────────────────────────────────
    Write-Host "Checking trigger/function residue..." -ForegroundColor Green

    $trgResidue = psql -h $dbHost -p $dbPort -U $dbUser -d $DatabaseName -tAc "SELECT tgname FROM pg_trigger WHERE tgname='trg_test_outbox_fail'" 2>&1
    $fnResidue = psql -h $dbHost -p $dbPort -U $dbUser -d $DatabaseName -tAc "SELECT proname FROM pg_proc WHERE proname='fn_test_outbox_fail'" 2>&1

    if ($trgResidue -and $trgResidue.Trim() -ne "" -and $trgResidue.Trim() -ne "(0 rows)") {
        throw "Test trigger 'trg_test_outbox_fail' found. Clean up before running smoke."
    }
    if ($fnResidue -and $fnResidue.Trim() -ne "" -and $fnResidue.Trim() -ne "(0 rows)") {
        throw "Test function 'fn_test_outbox_fail' found. Clean up before running smoke."
    }

    Write-Host "  Residue clean" -ForegroundColor Green

    # ── Run smoke tests ─────────────────────────────────────────────────
    Write-Host ""
    Write-Host "Running staging smoke tests..." -ForegroundColor Cyan

    $testFilter = "staging_smoke"
    Write-Host "  Test filter: $testFilter"
    Write-Host "  Test threads: 1 (sequential, env var isolation)"
    Write-Host ""

    Push-Location (Join-Path $projectRoot "services\api-server")

    if ($SkipDisableDrill) {
        Write-Host "  Running happy-path only (-SkipDisableDrill)..." -ForegroundColor Yellow
        cargo test -p fms-application staging_smoke_todo_create_executes_end_to_end -- --ignored --nocapture --test-threads=1
    } else {
        cargo test -p fms-application $testFilter -- --ignored --nocapture --test-threads=1
    }
    $exitCode = $LASTEXITCODE

    Pop-Location

} finally {
    Remove-Item Env:\TEST_DATABASE_URL -ErrorAction SilentlyContinue
    Remove-Item Env:\PGPASSWORD -ErrorAction SilentlyContinue
    Remove-Item Env:\FMS_AI_PROPOSAL_EXECUTION_ENABLED -ErrorAction SilentlyContinue
    Remove-Item Env:\FMS_AI_EXECUTION_READINESS_OVERRIDE -ErrorAction SilentlyContinue
}

if ($exitCode -ne 0) {
    Write-Host ""
    Write-Host "=== STAGING SMOKE TESTS FAILED ===" -ForegroundColor Red
    Write-Host ""
    Write-Host "Troubleshooting:" -ForegroundColor Red
    Write-Host "  - 'execution must be blocked when flag is off' → env var not cleaned; check test-threads=1"
    Write-Host "  - 'todo row should exist' → DomainActionExecutor not wired or TodoService missing"
    Write-Host "  - 'outbox should contain Todo.create' → transaction not committed"
    Write-Host "  - 'should record readiness block audit' → readiness service has pool; use new(None)"
    Write-Host "  - 'proposal should remain Approved' → proposal already executed by prior run; ULID ids should prevent this"
    exit $exitCode
}

Write-Host ""
Write-Host "=== ALL STAGING SMOKE TESTS PASSED ===" -ForegroundColor Green
Write-Host ""
Write-Host "Verification summary:" -ForegroundColor Cyan
Write-Host "  [PASS] Todo.create proposal executed end-to-end"
Write-Host "  [PASS] Business row created in todos table"
Write-Host "  [PASS] Domain event outbox entry written"
Write-Host "  [PASS] Audit events: execution_requested, execution_started, execution_succeeded"
if (-not $SkipDisableDrill) {
    Write-Host "  [PASS] Execution-disabled: proposal blocked, no side effects"
    Write-Host "  [PASS] Readiness-not-ready: proposal blocked, audit event recorded"
}
Write-Host ""
