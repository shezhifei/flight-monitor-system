<#
.SYNOPSIS
    Run AI Runtime DB-dependent (--ignored) tests against flight_monitor_test.

.DESCRIPTION
    Reads DB_HOST, DB_PORT, DB_USER, DB_PASSWORD from the project root .env file,
    constructs a TEST_DATABASE_URL pointing at the specified database (default:
    flight_monitor_test), and runs the ignored test suite.

    SAFETY:
    - Does NOT print the full connection string or password.
    - Does NOT write to .env or modify the database schema.
    - Does NOT use flight_monitor_dev.

.PARAMETER DatabaseName
    Target database name. Default: flight_monitor_test

.EXAMPLE
    .\scripts\dev\run_ai_runtime_db_tests.ps1
    .\scripts\dev\run_ai_runtime_db_tests.ps1 -DatabaseName flight_monitor_test
#>

param(
    [string]$DatabaseName = "flight_monitor_test"
)

$ErrorActionPreference = "Stop"

# Prevent accidental use of dev database
if ($DatabaseName -eq "flight_monitor_dev") {
    Write-Error "REFUSED: Cannot run --ignored tests against flight_monitor_dev. Use flight_monitor_test."
    exit 1
}

# Locate .env
$projectRoot = Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path))
$envFile = Join-Path $projectRoot ".env"

if (-not (Test-Path $envFile)) {
    Write-Error ".env file not found at $envFile"
    exit 1
}

# Parse .env for DB_* variables
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

Write-Host "=== AI Runtime DB Test Runner ===" -ForegroundColor Cyan
Write-Host "Database: $DatabaseName" -ForegroundColor Yellow
Write-Host "Host: $dbHost`:$dbPort" -ForegroundColor Yellow
Write-Host "User: $dbUser" -ForegroundColor Yellow
Write-Host "(Connection string is NOT printed for security)" -ForegroundColor DarkGray
Write-Host ""

# Set env and run
$env:TEST_DATABASE_URL = $testUrl

# Validate the test database instead of pretending this script can bootstrap an
# empty database.  The historical migrations in this repository assume a base
# FMS schema already exists (for example, migrations/002 alters `flights`).
Write-Host "Verifying test database baseline..." -ForegroundColor Green

$requiredRelations = @(
    "flights",
    "schema_migrations",
    "ai_jobs",
    "ai_runs",
    "ai_run_events",
    "ai_action_proposals",
    "domain_event_outbox",
    "aip_ontology_objects",
    "aip_ontology_actions",
    "todos",
    "users"
)

$env:PGPASSWORD = $dbPassword
try {
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
        throw "Database '$DatabaseName' is missing required FMS base/test schema relations: $missingList. This script does not bootstrap an empty database. Create or migrate the test database with the project's base schema before running AI runtime DB tests."
    }

    $migrateInfo = sqlx migrate info --database-url $testUrl 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Host $migrateInfo -ForegroundColor Red
        throw "sqlx migrate info failed for database '$DatabaseName'. Fix migration metadata before running AI runtime DB tests."
    }

    if ($migrateInfo | Select-String -Pattern "^\s*\d+/pending\b" -Quiet) {
        Write-Host $migrateInfo -ForegroundColor Yellow
        throw "Database '$DatabaseName' has pending SQLx migrations. Apply them with the project's migration process before running AI runtime DB tests."
    }

    if ($migrateInfo | Select-String -Pattern "different checksum" -Quiet) {
        Write-Host "  Warning: sqlx reports historical checksum differences. Continuing because the required test schema is present." -ForegroundColor Yellow
    }

    Write-Host "  Required test schema is present." -ForegroundColor Green
} finally {
    Remove-Item Env:\PGPASSWORD -ErrorAction SilentlyContinue
}

try {
    Push-Location (Join-Path $projectRoot "services\api-server")
    Write-Host "Running: cargo test -p fms-api nl_query -- --ignored --nocapture" -ForegroundColor Green
    cargo test -p fms-api nl_query -- --ignored --nocapture
    $exitCode = $LASTEXITCODE
} catch {
    Write-Error "Test execution failed: $_"
    $exitCode = 1
} finally {
    Pop-Location
    Remove-Item Env:\TEST_DATABASE_URL -ErrorAction SilentlyContinue
}

if ($exitCode -ne 0) {
    Write-Host ""
    Write-Host "=== TESTS FAILED ===" -ForegroundColor Red
    Write-Host "If you see 'relation ai_jobs does not exist', you are pointing at" -ForegroundColor Red
    Write-Host "the wrong database or migrations have not been applied." -ForegroundColor Red
    Write-Host "Ensure $DatabaseName has all migrations applied." -ForegroundColor Red
    exit $exitCode
}

Write-Host ""
Write-Host "=== ALL DB TESTS PASSED ===" -ForegroundColor Green
