# backup_vault.ps1 — Take a Raft snapshot of the running Vault instance.
# Usage: .\scripts\vault\backup_vault.ps1 [-OutputDir <path>]
# Requires: vault CLI (or runs inside the container via docker compose).

param(
    [string]$OutputDir = (Join-Path $PSScriptRoot "..\..\deploy\vault\backups")
)

$ErrorActionPreference = "Stop"

$Timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$SnapshotFile = Join-Path $OutputDir "vault-raft-$Timestamp.snap"

if (-not (Test-Path $OutputDir)) {
    New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
}

$vaultCmd = Get-Command vault -ErrorAction SilentlyContinue
if ($vaultCmd) {
    Write-Host "Saving Vault Raft snapshot to $SnapshotFile..."
    vault operator raft snapshot save $SnapshotFile
} else {
    Write-Host "vault CLI not found; falling back to docker compose exec..."
    $composeFile = Join-Path $PSScriptRoot "..\..\deploy\vault\docker-compose.vault.yml"
    docker compose -f $composeFile exec vault vault operator raft snapshot save /tmp/vault-snapshot.snap
    docker compose -f $composeFile cp vault:/tmp/vault-snapshot.snap $SnapshotFile
}

Write-Host "Snapshot saved: $SnapshotFile"
