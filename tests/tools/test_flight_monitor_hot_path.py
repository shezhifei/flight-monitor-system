"""Static guard for the F1 monitor-row read contract.

The legacy Flight repository may retain leg joins for detail/compatibility
paths during F4, but list/search/count reads must not silently fall back to
those queries or re-aggregate legs.
"""

from pathlib import Path
import re


ROOT = Path(__file__).resolve().parents[2]
FLIGHT_SERVICE = ROOT / "services/api-server/crates/application/src/services/flight_service.rs"
MONITOR_REPO = ROOT / "services/api-server/crates/infrastructure/src/repositories/pg_flight_monitor_row_repository.rs"
FLIGHT_WRITER = ROOT / "services/api-server/crates/application/src/services/flight_writer.rs"
MONITOR_SERVICE = ROOT / "services/api-server/crates/application/src/services/flight_monitor_row_service.rs"
ANOMALY_SERVICE = ROOT / "services/api-server/crates/application/src/services/anomaly_service.rs"
ONTOLOGY_WRITER = ROOT / "services/api-server/crates/application/src/services/ontology_service/writer.rs"


def _function_body(source: str, signature: str) -> str:
    start = source.index(signature)
    brace = source.index("{", start)
    depth = 0
    for index in range(brace, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[brace : index + 1]
    raise AssertionError(f"unclosed function: {signature}")


def test_flight_service_list_and_search_require_monitor_rows():
    source = FLIGHT_SERVICE.read_text(encoding="utf-8")
    for signature in ("pub async fn list_flights", "pub async fn search_flights"):
        body = _function_body(source, signature)
        assert "self.monitor_rows.as_ref().ok_or_else" in body
        assert "self.repo.find_all" not in body
        assert "self.repo.search" not in body
        assert "flight_legs" not in body


def test_monitor_repository_queries_are_single_table_only():
    source = MONITOR_REPO.read_text(encoding="utf-8")
    # Every SQL literal in the monitor-row adapter is expected to target the
    # persisted wide table directly; no flight/leg join or JSON aggregation is
    # allowed in this hot-path repository.
    assert "flight_legs" not in source
    assert "jsonb_agg" not in source
    assert not re.search(r"\bJOIN\s+flights\b", source, re.IGNORECASE)


def test_delete_paths_soft_retire_monitor_rows():
    source = FLIGHT_WRITER.read_text(encoding="utf-8")
    body_start = source.index("pub async fn delete_with_deleted_event")
    body = source[body_start : source.index("\n    }", body_start) + 6]
    assert "deactivate_flight_in_tx" in body
    assert "DELETE FROM flight_monitor_rows" not in source


def test_non_transactional_flight_writes_cannot_skip_projection():
    source = (ROOT / "services/api-server/crates/application/src/services/flight_service.rs").read_text(encoding="utf-8")
    for signature in ("async fn create_flight_inner", "async fn update_flight_inner"):
        body = _function_body(source, signature)
        assert "flight_monitor_rows repository is required for non-transactional flight writes" in body
        assert "repo.upsert(" in body


def test_anomaly_lifecycle_refreshes_monitor_flag():
    source = ANOMALY_SERVICE.read_text(encoding="utf-8")
    assert "with_monitor_row_repository" in source
    assert "refresh_anomaly_flag" in source
    assert "self.refresh_monitor_anomaly_flag(anomaly_id).await?" in source


def test_resource_sync_reprojects_monitor_rows_in_same_transaction():
    source = ONTOLOGY_WRITER.read_text(encoding="utf-8")
    assert "monitor_row_tx" in source
    assert "monitor.upsert_in_tx(tx, &row)" in source
    assert source.count("monitor.upsert_in_tx(tx, &row)") >= 2


def test_turnaround_projection_keeps_inbound_row_identity():
    source = MONITOR_SERVICE.read_text(encoding="utf-8")
    primary = source[source.index("let primary_id"):source.index("let direction", source.index("let primary_id"))]
    assert primary.index("inbound_flight_id") < primary.index("outbound_flight_id")
