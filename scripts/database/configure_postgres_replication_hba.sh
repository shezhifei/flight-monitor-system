#!/bin/sh
set -eu

: "${PGDATA:?PGDATA is required}"

hba_file="${PGDATA}/pg_hba.conf"
replication_rule="host replication fm_replicator all scram-sha-256"

if [ ! -f "$hba_file" ]; then
    echo "PostgreSQL HBA file does not exist: $hba_file" >&2
    exit 1
fi

# The official postgres image's default `host all all ...` rule does not match
# physical-replication connections. Add the dedicated rule only during first
# database initialization; keep the operation idempotent for manual re-runs.
if ! grep -Fqx "$replication_rule" "$hba_file"; then
    printf '\n%s\n' "$replication_rule" >> "$hba_file"
fi
