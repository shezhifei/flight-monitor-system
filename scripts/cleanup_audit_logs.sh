#!/usr/bin/env bash
# cleanup_audit_logs.sh — Purge system_audit_logs rows older than RETENTION_DAYS.
# Usage: ./scripts/cleanup_audit_logs.sh [--dry-run]
# Requires: psql (or use inside the Postgres container).

set -euo pipefail

RETENTION_DAYS="${AUDIT_RETENTION_DAYS:-90}"
DRY_RUN=false

if [[ "${1:-}" == "--dry-run" ]]; then
  DRY_RUN=true
fi

if [[ -z "${DATABASE_URL:-}" ]]; then
  echo "ERROR: DATABASE_URL is required; refusing to use default database credentials." >&2
  exit 2
fi

if ! [[ "${RETENTION_DAYS}" =~ ^[0-9]+$ ]] || (( RETENTION_DAYS < 1 || RETENTION_DAYS > 3650 )); then
  echo "ERROR: AUDIT_RETENTION_DAYS must be an integer between 1 and 3650." >&2
  exit 2
fi

SQL_DELETE="DELETE FROM system_audit_logs WHERE created_at < NOW() - INTERVAL '${RETENTION_DAYS} days';"
SQL_COUNT="SELECT COUNT(*) FROM system_audit_logs WHERE created_at < NOW() - INTERVAL '${RETENTION_DAYS} days';"

if $DRY_RUN; then
  echo "[dry-run] Would delete rows older than ${RETENTION_DAYS} days."
  echo "[dry-run] SQL: ${SQL_COUNT}"
  psql "${DATABASE_URL}" -c "${SQL_COUNT}"
else
  echo "Deleting system_audit_logs rows older than ${RETENTION_DAYS} days..."
  psql "${DATABASE_URL}" -c "${SQL_DELETE}"
  echo "Done."
fi
