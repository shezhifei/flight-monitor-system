"""Contract test: sidecar 必须能解析与 Rust 共享的 Ontology V1 schema fixture。

fixture 由 Rust `fms-domain` 的 `flight_ops_v1_export_matches_fixture` 测试维护
（`docs/fixtures/flight_ops_v1_ontology_schema.json`，稳定 schema export 结构）。
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from src.infrastructure.ai.ontology.schema_mirror import FLIGHT_OPS_ONTOLOGY_VERSION

FIXTURE_PATH = Path(__file__).resolve().parents[4] / "docs" / "fixtures" / "flight_ops_v1_ontology_schema.json"


@pytest.fixture(scope="module")
def schema_export() -> dict:
    with FIXTURE_PATH.open(encoding="utf-8") as handle:
        return json.load(handle)


def test_fixture_uses_contract_envelope_fields(schema_export):
    for key in ("ontology_version", "description", "exported_at", "objects", "actions", "risk_policies", "constraints"):
        assert key in schema_export, f"export missing contract field {key}"
    assert schema_export["ontology_version"] == FLIGHT_OPS_ONTOLOGY_VERSION


def test_fixture_declares_core_objects(schema_export):
    for object_type in ("Flight", "Stand", "DispatchOrder", "Anomaly"):
        assert object_type in schema_export["objects"], f"object {object_type} missing"


def test_fixture_actions_carry_contract_fields(schema_export):
    actions = schema_export["actions"]
    assert actions, "exported actions must not be empty"
    for key, action in actions.items():
        assert key == f"{action['object_type']}.{action['action_name']}"
        assert action["ontology_version"] == FLIGHT_OPS_ONTOLOGY_VERSION
        assert action["risk_level"] in schema_export["risk_policies"]
        assert action["approval_policy"]
        assert action["required_permissions"], f"{key} must declare required_permissions"
        assert action["arguments_schema"].get("type") == "object", f"{key} arguments_schema must be object"


def test_fixture_risk_policies_cover_all_levels(schema_export):
    assert set(schema_export["risk_policies"]) == {"low", "medium", "high", "critical"}
