#!/bin/sh
set -eu

: "${POSTGRES_USER:?POSTGRES_USER is required}"
: "${POSTGRES_PASSWORD:?POSTGRES_PASSWORD is required}"
: "${FLOWABLE_DB_PASSWORD:?FLOWABLE_DB_PASSWORD is required}"

if [ "${#FLOWABLE_DB_PASSWORD}" -lt 16 ]; then
    echo "FLOWABLE_DB_PASSWORD must contain at least 16 characters" >&2
    exit 1
fi

export PGUSER="$POSTGRES_USER"
export PGPASSWORD="$POSTGRES_PASSWORD"
export PGDATABASE=postgres

until pg_isready -h "${PGHOST:-postgres}" -p "${PGPORT:-5432}" -U "$PGUSER" -d "$PGDATABASE"; do
    echo "[flowable-bootstrap] waiting for PostgreSQL"
    sleep 2
done

psql \
    --host "${PGHOST:-postgres}" \
    --port "${PGPORT:-5432}" \
    --set ON_ERROR_STOP=1 \
    --set flowable_password="$FLOWABLE_DB_PASSWORD" \
    --file /scripts/bootstrap_flowable_database.sql
