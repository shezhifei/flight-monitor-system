"""Static guards for atomic two-direction flight imports."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
IMPORT_SERVICE = ROOT / "services/api-server/crates/application/src/services/flight_import_service.rs"
WRITER = ROOT / "services/api-server/crates/application/src/services/flight_writer.rs"
DI = ROOT / "services/api-server/crates/server/src/di/flight.rs"


def test_import_service_prepares_both_flights_before_pair_write() -> None:
    source = IMPORT_SERVICE.read_text(encoding="utf-8")
    assert "prepare_create(inbound_payload)" in source
    assert "prepare_create(outbound_payload)" in source
    assert ".create_pair(&inbound, &outbound, &link, actor_id)" in source
    assert "Production DI always sets" in source


def test_pair_writer_commits_flights_link_and_projection_in_one_uow() -> None:
    source = WRITER.read_text(encoding="utf-8")
    assert "pub struct UowFlightImportPairWriter" in source
    assert "self.uow.begin().await?" in source
    assert source.index("save_with_created_event(&mut tx, inbound") < source.index("create_link_in_tx(&mut tx, link)")
    assert source.index("create_link_in_tx(&mut tx, link)") < source.index("merge_turnaround_in_tx(")
    assert "self.uow.commit(tx).await" in source


def test_server_wires_atomic_pair_writer() -> None:
    source = DI.read_text(encoding="utf-8")
    assert "UowFlightImportPairWriter" in source
    assert ".with_pair_transactional_writer(pair_writer)" in source
