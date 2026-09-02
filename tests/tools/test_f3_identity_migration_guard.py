"""Static guards for the F3 identity cutover migration."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MIGRATION = ROOT / "migrations/155_split_flight_identity.sql"


def test_identity_map_is_a_persistent_audit_record() -> None:
    source = MIGRATION.read_text(encoding="utf-8")
    assert "CREATE TABLE IF NOT EXISTS f3_identity_map" in source
    assert "mapping_version" in source
    assert "mapped_at" in source
    assert "COMMENT ON TABLE f3_identity_map" in source
    assert "TRUNCATE TABLE f3_identity_map" not in source


def test_scalar_remap_uses_explicit_whitelist_and_fails_on_unknown_tables() -> None:
    source = MIGRATION.read_text(encoding="utf-8")
    assert "这里必须是显式白名单" in source
    assert "not in remap whitelist" in source
    assert "flight_runtime_list_projection" in source
    assert "dispatch_collaboration_events" in source
    assert "resource_adjustment_suggestions" in source


def test_post_cutover_residual_reference_guard_is_present() -> None:
    source = MIGRATION.read_text(encoding="utf-8")
    assert "All explicitly remapped scalar tables must be free" in source
    assert "still point to split aggregate ids" in source
    assert "active flight with invalid direction remains" in source
    assert "direction IS NULL OR direction NOT IN" in source


def test_direction_constraint_is_deferred_until_backfill_finishes() -> None:
    source = MIGRATION.read_text(encoding="utf-8")
    assert "CHECK (" in source
    assert "direction IS NOT NULL AND direction IN" in source
    assert "NOT VALID" in source
    assert "ALTER TABLE flights VALIDATE CONSTRAINT chk_flights_direction_contract" in source
