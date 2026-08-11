"""Validate setup_postgresql.sql and migrations/000 keep flights aligned.

Both files create the flights table. If setup_postgresql.sql creates a
different baseline first, later CREATE TABLE IF NOT EXISTS migrations silently
skip the canonical definition and leave schema drift behind.
"""

import re
from pathlib import Path


REPO_ROOT = Path(__file__).parent.parent.parent
SETUP_SQL = REPO_ROOT / "scripts" / "database" / "setup_postgresql.sql"
MIGRATION_000 = REPO_ROOT / "migrations" / "000_create_users_table.sql"


def _extract_flights_create_table(text: str) -> str:
    """Extract the CREATE TABLE IF NOT EXISTS flights (...) block."""
    match = re.search(
        r"CREATE TABLE IF NOT EXISTS flights\s*\((.*?)\);",
        text,
        re.DOTALL | re.IGNORECASE,
    )
    assert match, "flights table creation not found"
    return match.group(1)


def _normalize_sql(sql: str) -> str:
    return re.sub(r"\s+", " ", sql.upper()).strip()


def test_flights_primary_key_definition_matches() -> None:
    """flights primary key definition must match migrations/000."""
    setup_block = _extract_flights_create_table(SETUP_SQL.read_text(encoding="utf-8"))
    migration_block = _extract_flights_create_table(
        MIGRATION_000.read_text(encoding="utf-8")
    )

    assert "SERIAL PRIMARY KEY" not in setup_block.upper(), (
        "setup_postgresql.sql flights must not use SERIAL PRIMARY KEY; "
        "migrations/000 uses flight_id VARCHAR(26) PRIMARY KEY"
    )
    assert "FLIGHT_ID VARCHAR(26) PRIMARY KEY" in _normalize_sql(migration_block)
    assert "FLIGHT_ID VARCHAR(26) PRIMARY KEY" in _normalize_sql(setup_block)


def test_flights_column_types_match() -> None:
    """Key column types must match migrations/000."""
    setup_block = _normalize_sql(
        _extract_flights_create_table(SETUP_SQL.read_text(encoding="utf-8"))
    )
    migration_block = _normalize_sql(
        _extract_flights_create_table(MIGRATION_000.read_text(encoding="utf-8"))
    )

    for column_spec in [
        "flight_number VARCHAR(32)",
        "airline_code VARCHAR(8)",
        "status INTEGER",
    ]:
        normalized_spec = column_spec.upper()
        assert normalized_spec in migration_block, (
            f"migrations/000 missing expected flights definition: {column_spec}"
        )
        assert normalized_spec in setup_block, (
            f"setup_postgresql.sql flights missing or mismatched: {column_spec}"
        )


def _extract_ai_query_ro_role_block(text: str) -> str:
    """Extract the DO block that creates/alters the ai_query_ro role."""
    # Anchor on the ai_query_ro existence check so we do not swallow earlier DO blocks
    # (e.g. fm_replicator) that also reference pg_roles.
    match = re.search(
        r"DO\s*\$\$\s*BEGIN\s*"
        r"IF NOT EXISTS\s*\(\s*SELECT 1 FROM pg_roles WHERE rolname = 'ai_query_ro'\s*\)"
        r".*?END\s*\$\$;",
        text,
        re.DOTALL | re.IGNORECASE,
    )
    assert match, "ai_query_ro role bootstrap DO block not found"
    return match.group(0)


def test_ai_query_ro_role_block_has_no_password_literal() -> None:
    """ai_query_ro bootstrap must not embed a fixed PASSWORD literal.

    Role, grants, and read-only timeouts remain in setup SQL; secrets are set
    by Vault/ops outside the repository (no fake SQL env interpolation).
    """
    setup_sql = SETUP_SQL.read_text(encoding="utf-8")
    role_block = _extract_ai_query_ro_role_block(setup_sql)

    assert "CREATE ROLE ai_query_ro" in role_block
    assert "ALTER ROLE ai_query_ro" in role_block
    # Reject SQL PASSWORD clauses (PASSWORD '...' / PASSWORD %L), not prose comments.
    assert re.search(
        r"\bPASSWORD\s+('([^']*)'|%L)",
        role_block,
        re.IGNORECASE,
    ) is None, (
        "ai_query_ro role block must not contain PASSWORD literal; set secrets via Vault/ops"
    )
    assert "ai_query_ro_dev_change_me" not in setup_sql

    # Permissions and read-only session limits must still be provisioned.
    assert "GRANT USAGE ON SCHEMA ai_query TO ai_query_ro" in setup_sql
    assert "default_transaction_read_only = on" in setup_sql
    assert "statement_timeout" in setup_sql
    assert "idle_in_transaction_session_timeout" in setup_sql
