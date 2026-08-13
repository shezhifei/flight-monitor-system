"""Tests for read-only ontology context enhancement."""

from __future__ import annotations

from typing import ClassVar

import pytest

from src.infrastructure.ai.context_envelope import (
    ContextEnvelope,
    EnvelopeContext,
    EnvelopeLimits,
    EnvelopeObject,
    EnvelopeOntology,
    EnvelopeRequester,
    EnvelopeTask,
)
from src.infrastructure.ai.runtime_context import enhance_context


class _SchemaMirrorStub:
    schema_cache: ClassVar[dict[str, dict[str, dict[str, object]]]] = {
        "objects": {"Flight": {}},
        "actions": {"Flight.add_note": {}},
    }

    def get_cached_schema_snapshot(self) -> dict[str, dict[str, dict[str, object]]]:
        return self.schema_cache


class _FailingSchemaMirrorStub:
    def get_cached_schema_snapshot(self) -> dict[str, object]:
        raise RuntimeError("schema cache crashed")


def _sample_envelope(**overrides) -> ContextEnvelope:
    base = ContextEnvelope(
        contract_version="ai-runtime.v1",
        job_id="job_test_001",
        run_id="run_test_001",
        correlation_id="corr_test",
        requester=EnvelopeRequester(user_id="user_1", roles=["ai:chat"]),
        ontology=EnvelopeOntology(
            version="flight-ops.v1",
            allowed_object_types=["Flight"],
            allowed_actions=["Flight.add_note"],
            risk_ceiling="medium",
        ),
        context=EnvelopeContext(
            objects=[
                EnvelopeObject(
                    object_type="Flight",
                    object_id="FL123",
                    data={"flight_number": "CA1234", "status": "scheduled"},
                )
            ],
            limits=EnvelopeLimits(),
        ),
        task=EnvelopeTask(task_type="nl_query", user_message="What is the status of CA1234?"),
    )
    for key, value in overrides.items():
        setattr(base, key, value)
    return base


class TestEnhanceContext:
    def test_returns_reasoning_steps(self):
        envelope = _sample_envelope()
        result = enhance_context(envelope)
        assert any(r.step == "intent_classify" for r in result.reasoning_steps)

    def test_returns_evidence_for_flight_object(self, monkeypatch):
        monkeypatch.setattr(
            "src.infrastructure.ai.runtime_context._get_schema_mirror",
            lambda: _SchemaMirrorStub(),
        )
        envelope = _sample_envelope()
        result = enhance_context(envelope)
        sources = {e.source for e in result.evidence}
        assert sources == {"schema_mirror.snapshot"}

    def test_degrades_gracefully_when_schema_mirror_missing(self, monkeypatch):
        monkeypatch.setattr(
            "src.infrastructure.ai.runtime_context._get_schema_mirror",
            lambda: None,
        )
        envelope = _sample_envelope()
        result = enhance_context(envelope)
        assert any("Schema mirror unavailable" in lim for lim in result.limitations)

    def test_no_500_on_empty_objects(self):
        envelope = _sample_envelope(context=EnvelopeContext(objects=[], limits=EnvelopeLimits()))
        result = enhance_context(envelope)
        assert isinstance(result.reasoning_steps, list)
        assert isinstance(result.evidence, list)
        assert isinstance(result.limitations, list)

    def test_action_inventory_step_present_when_actions_resolved(self, monkeypatch):
        monkeypatch.setattr(
            "src.infrastructure.ai.runtime_context._get_schema_mirror",
            lambda: _SchemaMirrorStub(),
        )
        envelope = _sample_envelope()
        result = enhance_context(envelope)
        steps = [r.step for r in result.reasoning_steps]
        assert "action_inventory" in steps

    def test_no_500_when_schema_mirror_unavailable(self, monkeypatch):
        """A missing schema mirror produces a limitation rather than an exception."""
        monkeypatch.setattr(
            "src.infrastructure.ai.runtime_context._get_schema_mirror",
            lambda: None,
        )
        envelope = _sample_envelope()
        result = enhance_context(envelope)
        assert isinstance(result.reasoning_steps, list)
        assert isinstance(result.evidence, list)
        assert isinstance(result.limitations, list)
        assert any("Schema mirror unavailable" in lim for lim in result.limitations)

    def test_no_500_when_schema_mirror_raises_exception(self, monkeypatch):
        """Schema mirror raises -> caught gracefully, no HTTP 500."""

        def _raise():
            raise RuntimeError("schema mirror crashed")

        monkeypatch.setattr(
            "src.infrastructure.ai.runtime_context._get_schema_mirror",
            _raise,
        )
        envelope = _sample_envelope()
        result = enhance_context(envelope)
        assert isinstance(result.reasoning_steps, list)
        assert isinstance(result.evidence, list)
        assert isinstance(result.limitations, list)
        assert any("Schema mirror unavailable" in lim for lim in result.limitations)

    def test_no_500_when_schema_mirror_cache_read_raises(self, monkeypatch):
        monkeypatch.setattr(
            "src.infrastructure.ai.runtime_context._get_schema_mirror",
            lambda: _FailingSchemaMirrorStub(),
        )
        result = enhance_context(_sample_envelope())
        assert result.evidence == []
        assert "Schema mirror unavailable" in result.limitations


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
