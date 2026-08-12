# Apply local-only write-path schema patches (checklist varchar, notifications.updated_at).
param(
    [string]$DbHost = "localhost",
    [int]$Port = 5432,
    [string]$Db = "flight_monitor_dev",
    [string]$User = "postgres"
)

$ErrorActionPreference = "Stop"
$sql = Join-Path $PSScriptRoot "patch_local_write_paths.sql"
if (-not $env:PGPASSWORD) { $env:PGPASSWORD = "password" }

Write-Host "Applying $sql to ${User}@${DbHost}:${Port}/$Db"
psql -h $DbHost -p $Port -U $User -d $Db -f $sql
if ($LASTEXITCODE -ne 0) { throw "psql failed: $LASTEXITCODE" }
Write-Host "LOCAL_WRITE_PATHS_PATCHED"
