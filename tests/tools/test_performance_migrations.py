"""Root migrations for performance indexes and shadow metrics."""

from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MIGRATIONS = ROOT / "migrations"


def test_concurrent_indexes_are_one_per_file_with_no_transaction() -> None:
    ai_runs = (MIGRATIONS / "129_idx_ai_runs_job_id_created_at.sql").read_text(encoding="utf-8")
    dispatch = (MIGRATIONS / "130_idx_dispatch_orders_released_status.sql").read_text(encoding="utf-8")
    for text, name in (
        (ai_runs, "idx_ai_runs_job_id_created_at"),
        (dispatch, "idx_dispatch_orders_released_status"),
    ):
        assert text.splitlines()[0].strip() == "-- no-transaction"
        assert "BEGIN" not in text.upper().replace("NO-TRANSACTION", "")
        assert "CREATE INDEX CONCURRENTLY" in text
        assert name in text
        assert text.upper().count("CREATE INDEX CONCURRENTLY") == 1


def test_dispatch_orders_index_is_partial_on_live_rows() -> None:
    text = (MIGRATIONS / "130_idx_dispatch_orders_released_status.sql").read_text(encoding="utf-8")
    assert "ON dispatch_orders (released_at, status)" in text
    assert "WHERE deleted_at IS NULL" in text
    deleted = (MIGRATIONS / "128_add_dispatch_orders_deleted_at.sql").read_text(encoding="utf-8")
    assert "ADD COLUMN IF NOT EXISTS deleted_at" in deleted


def test_shadow_mode_performance_metrics_columns() -> None:
    text = (MIGRATIONS / "131_create_shadow_mode_performance_metrics.sql").read_text(encoding="utf-8")
    for column in (
        "query_type",
        "old_latency_ms",
        "new_latency_ms",
        "accuracy_diff",
        "operator_feedback",
    ):
        assert column in text
    assert "CREATE TABLE IF NOT EXISTS shadow_mode_performance_metrics" in text
    assert "idx_spm_query_type" in text
    assert "USING hnsw" not in text.lower()
    assert "vector_cosine_ops" not in text.lower()
