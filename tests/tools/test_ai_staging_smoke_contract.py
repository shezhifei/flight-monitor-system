"""Keep the staging smoke summary aligned with the current ontology contract."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/dev/run_aip_api_staging_smoke.ps1"


def test_api_smoke_reports_current_flight_write_contract() -> None:
    source = SCRIPT.read_text(encoding="utf-8")
    assert "Flight.add_note proposal executed via HTTP API end-to-end" in source
    assert "Todo.create proposal executed via HTTP API end-to-end" not in source
    assert "Business row created in todos table" not in source
