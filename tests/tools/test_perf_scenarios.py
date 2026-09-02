from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SCENARIO = ROOT / "scripts" / "perf" / "scenarios" / "airport_ops.json"


def test_airport_ops_weights_and_write_mix() -> None:
    data = json.loads(SCENARIO.read_text(encoding="utf-8"))
    assert data["name"] == "airport_ops"
    endpoints = data["endpoints"]
    assert endpoints
    total_weight = sum(item["weight"] for item in endpoints)
    write_weight = sum(
        item["weight"] for item in endpoints if item["method"].upper() in {"POST", "PUT", "PATCH", "DELETE"}
    )
    names = {item["name"] for item in endpoints}
    assert total_weight == 100
    assert 8 <= write_weight <= 20
    assert "flights_list" in names
    assert "todo_create" in names
    assert "flight_patch_remarks" in names
    assert any("{flight_id}" in item["path"] for item in endpoints)


def test_airport_ops_paths_are_v2() -> None:
    data = json.loads(SCENARIO.read_text(encoding="utf-8"))
    for item in data["endpoints"]:
        assert item["path"].startswith("/api/v2/")
        assert item["weight"] > 0
