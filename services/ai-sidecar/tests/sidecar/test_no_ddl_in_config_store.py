"""Verify that PostgresAIConfigStore does not contain DDL."""

from pathlib import Path

CONFIG_STORE_PATH = Path(__file__).resolve().parents[2] / "src/infrastructure/ai/postgres_config_store.py"


def test_no_create_table_in_config_store():
    source = CONFIG_STORE_PATH.read_text(encoding="utf-8")
    assert "CREATE TABLE" not in source, "DDL (CREATE TABLE) should be in migrations, not in code"
