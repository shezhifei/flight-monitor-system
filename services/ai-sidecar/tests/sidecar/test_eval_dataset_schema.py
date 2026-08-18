"""Eval dataset schema gate (Task G1).

The JSONL fixtures under ``docs/fixtures`` are the CI gate inputs for the
agent evaluation harness. This test locks their shape so a prompt/template/
model change can only be merged against a well-formed, stable dataset:

- every line is valid JSON with ``id`` / ``task_type`` / ``entity_id`` /
  ``user_query`` / ``expected``;
- ``expected`` carries ``allowed_tools`` / ``forbidden_tools`` /
  ``required_object_ids`` / ``plan_required``;
- query_ops samples are plan-free and must forbid the retired SQL face;
- dispatch_ops samples are plan-first and must route through solver
  candidates or ontology proposals — never through a local schedule apply.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[4]
FIXTURES = REPO_ROOT / "docs" / "fixtures"

QUERY_OPS_FIXTURE = FIXTURES / "agent_query_ops_eval.jsonl"
DISPATCH_OPS_FIXTURE = FIXTURES / "agent_dispatch_ops_eval.jsonl"

_EXPECTED_KEYS = {"allowed_tools", "forbidden_tools", "required_object_ids", "plan_required"}


def _load_samples(path: Path) -> list[dict]:
    samples = []
    for line_no, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        stripped = raw.strip()
        if not stripped or stripped.startswith("#"):
            continue
        try:
            samples.append(json.loads(stripped))
        except json.JSONDecodeError as exc:  # pragma: no cover - failure path
            pytest.fail(f"{path.name}:{line_no} is not valid JSON: {exc}")
    return samples


def _assert_common_shape(samples: list[dict], fixture: Path, task_type: str) -> None:
    assert len(samples) >= 5, f"{fixture.name} must hold at least 5 samples"
    ids = [sample.get("id") for sample in samples]
    assert all(isinstance(sample_id, str) and sample_id for sample_id in ids)
    assert len(set(ids)) == len(ids), f"{fixture.name} ids must be unique"
    for sample in samples:
        assert sample["task_type"] == task_type, f"{sample['id']}: wrong task_type"
        assert isinstance(sample["entity_id"], str) and sample["entity_id"]
        assert isinstance(sample["user_query"], str) and sample["user_query"]
        expected = sample["expected"]
        assert _EXPECTED_KEYS <= set(expected), f"{sample['id']}: missing expected keys"
        for key in ("allowed_tools", "forbidden_tools", "required_object_ids"):
            value = expected[key]
            assert isinstance(value, list) and all(isinstance(v, str) and v for v in value), (
                f"{sample['id']}: expected.{key} must be a list of non-empty strings"
            )
        assert isinstance(expected["plan_required"], bool)
        assert not set(expected["allowed_tools"]) & set(expected["forbidden_tools"]), (
            f"{sample['id']}: allowed/forbidden tools must not overlap"
        )


def test_query_ops_fixture_schema() -> None:
    samples = _load_samples(QUERY_OPS_FIXTURE)
    _assert_common_shape(samples, QUERY_OPS_FIXTURE, "query_ops")
    for sample in samples:
        expected = sample["expected"]
        assert expected["plan_required"] is False, f"{sample['id']}: query_ops is plan-free"
        assert "sql_query_readonly" in expected["forbidden_tools"], (
            f"{sample['id']}: SQL must stay out of the query_ops face"
        )
        assert not any(
            tool.endswith("change_stand") and not tool.startswith("ontology.")
            for tool in expected["allowed_tools"]
        ), f"{sample['id']}: stand changes flow through ontology proposals only"


def test_dispatch_ops_fixture_schema() -> None:
    samples = _load_samples(DISPATCH_OPS_FIXTURE)
    _assert_common_shape(samples, DISPATCH_OPS_FIXTURE, "dispatch_ops")
    for sample in samples:
        expected = sample["expected"]
        assert expected["plan_required"] is True, f"{sample['id']}: dispatch_ops is plan-first"
        assert "update_plan" in expected["allowed_tools"], f"{sample['id']}: plan board required"
        assert {"ontology.propose_action", "dispatch.list_solver_candidates"} & set(
            expected["allowed_tools"]
        ), f"{sample['id']}: solver candidates or ontology proposals must be allowed"
        assert {"apply_schedule", "replan-apply"} & set(expected["forbidden_tools"]), (
            f"{sample['id']}: local schedule application must be forbidden"
        )
