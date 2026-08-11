<#
.SYNOPSIS
    Run Ontology V1 PostgreSQL integration tests against flight_monitor_test.

.DESCRIPTION
    Reads DB_HOST / DB_PORT / DB_USER / DB_PASSWORD from the project root .env,
    sets TEST_DATABASE_URL (never printed), verifies migration 119 tables exist,
    and runs ignored ontology_v1_integration tests. The tests fail when the
    database is unavailable or migration 119 is missing; they do not silently skip.

.PARAMETER DatabaseName
    Target database. Default: flight_monitor_test (never flight_monitor_dev).

.EXAMPLE
    .\scripts\dev\run_ontology_v1_db_tests.ps1
#>

param(
    [string]$DatabaseName = "flight_monitor_test"
)

$ErrorActionPreference = "Stop"

if ($DatabaseName -eq "flight_monitor_dev") {
    Write-Error "REFUSED: Do not run integration tests against flight_monitor_dev."
    exit 1
}

$projectRoot = Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path))
$envFile = Join-Path $projectRoot ".env"
if (-not (Test-Path $envFile)) {
    Write-Error ".env file not found at $envFile"
    exit 1
}

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

if (-not $dbUser -or -not $dbPassword) {
    Write-Error "DB_USER / DB_PASSWORD must be set in .env"
    exit 1
}

$testUrl = "postgres://${dbUser}:${dbPassword}@${dbHost}:${dbPort}/${DatabaseName}"
Write-Host "=== Ontology V1 DB tests ===" -ForegroundColor Cyan
Write-Host "Database: $DatabaseName"
Write-Host "Host: ${dbHost}:${dbPort}"
Write-Host "User: $dbUser"
Write-Host "(Connection string is NOT printed)" -ForegroundColor DarkGray

$env:PGPASSWORD = $dbPassword
try {
    $psql = "psql"
    if (Test-Path "C:\Program Files\PostgreSQL\18\bin\psql.exe") {
        $psql = "C:\Program Files\PostgreSQL\18\bin\psql.exe"
    }
    foreach ($relation in @(
        "aircraft",
        "stand_occupations",
        "gate_assignments",
        "turnaround_links",
        "resource_adjustment_suggestions",
        "flights",
        "flight_legs"
    )) {
        $status = & $psql -h $dbHost -p $dbPort -U $dbUser -d $DatabaseName -tAc "SELECT CASE WHEN to_regclass('public.$relation') IS NULL THEN 'missing' ELSE 'present' END;"
        if (($status | Select-Object -Last 1).ToString().Trim() -ne "present") {
            throw "Missing required relation public.$relation in $DatabaseName. Apply migrations/119_ontology_v1_core.sql (and base schema) first."
        }
    }
}
finally {
    Remove-Item Env:\PGPASSWORD -ErrorAction SilentlyContinue
}

$env:TEST_DATABASE_URL = $testUrl
$apiServer = Join-Path $projectRoot "services\api-server"
Push-Location $apiServer
try {
    cargo test -p fms-application --test ontology_v1_integration -- --ignored --nocapture
    $code = $LASTEXITCODE
}
finally {
    Pop-Location
    Remove-Item Env:\TEST_DATABASE_URL -ErrorAction SilentlyContinue
}

exit $code
