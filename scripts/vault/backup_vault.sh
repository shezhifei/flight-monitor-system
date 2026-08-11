#!/usr/bin/env bash
# backup_vault.sh — Take a Raft snapshot of the running Vault instance.
# Usage: ./scripts/backup_vault.sh [output_dir]
# Requires: vault CLI (or runs inside the container via docker compose).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
BACKUP_DIR="${1:-${PROJECT_ROOT}/deploy/vault/backups}"
TIMESTAMP="$(date +%Y%m%d_%H%M%S)"
SNAPSHOT_FILE="${BACKUP_DIR}/vault-raft-${TIMESTAMP}.snap"

mkdir -p "${BACKUP_DIR}"

if command -v vault &>/dev/null; then
  echo "Saving Vault Raft snapshot to ${SNAPSHOT_FILE}..."
  vault operator raft snapshot save "${SNAPSHOT_FILE}"
else
  echo "vault CLI not found; falling back to docker compose exec..."
  docker compose -f "${PROJECT_ROOT}/deploy/vault/docker-compose.vault.yml" \
    exec vault vault operator raft snapshot save /tmp/vault-snapshot.snap
  docker compose -f "${PROJECT_ROOT}/deploy/vault/docker-compose.vault.yml" \
    cp vault:/tmp/vault-snapshot.snap "${SNAPSHOT_FILE}"
fi

echo "Snapshot saved: ${SNAPSHOT_FILE}"
