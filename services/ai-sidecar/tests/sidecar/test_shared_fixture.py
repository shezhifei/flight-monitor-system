"""Validate the shared Rust/Python AI runtime contract.

Two layers of drift protection live here:

1. **Field-manifest introspection** (fixture-independent): every pydantic contract
   model's declared field set must equal the manifest's field set for that type,
   after subtracting the documented Python-internal fields. Adding or removing a
   field on the Python side fails this test until the manifest (the cross-language
   single source of truth) is consciously updated — which is exactly the signal a
   contract change should require. The Rust side asserts the same manifest via a
   fixture round-trip in nl_query/tests.rs.

2. **Exhaustive fixture round-trip**: the shared fixture is populated with every
   optional/nested contract field, so parsing it through the models exercises the
   full surface (not just the fields a minimal example happens to use).
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from src.infrastructure.ai.context_envelope import (
    ContextEnvelope,
    EnvelopeContext,
    EnvelopeEvidence,
    EnvelopeLimits,
    EnvelopeObject,
    EnvelopeOntology,
    EnvelopeRelation,
    EnvelopeRequester,
    EnvelopeTask,
)
from src.infrastructure.ai.structured_output import (
    AiStructuredOutput,
    OutputEvidence,
    OutputMetrics,
    OutputProposal,
    ReasoningStep,
    TokenUsage,
)

FIXTURES_DIR = Path(__file__).parent.parent / "fixtures"
FIXTURE_PATH = FIXTURES_DIR / "runtime_contract.json"
MANIFEST_PATH = FIXTURES_DIR / "contract_field_manifest.json"

# Manifest type name -> pydantic model class.
MODEL_BY_NAME = {
    "ContextEnvelope": ContextEnvelope,
    "EnvelopeRequester": EnvelopeRequester,
    "EnvelopeOntology": EnvelopeOntology,
    "EnvelopeContext": EnvelopeContext,
    "EnvelopeObject": EnvelopeObject,
    "EnvelopeRelation": EnvelopeRelation,
    "EnvelopeEvidence": EnvelopeEvidence,
    "EnvelopeLimits": EnvelopeLimits,
    "EnvelopeTask": EnvelopeTask,
    "AiStructuredOutput": AiStructuredOutput,
    "ReasoningStep": ReasoningStep,
    "OutputEvidence": OutputEvidence,
    "OutputProposal": OutputProposal,
    "OutputMetrics": OutputMetrics,
    "TokenUsage": TokenUsage,
}


@pytest.fixture(scope="module")
def fixture() -> dict:
    with open(FIXTURE_PATH, encoding="utf-8") as f:
        return json.load(f)


@pytest.fixture(scope="module")
def manifest() -> dict:
    with open(MANIFEST_PATH, encoding="utf-8") as f:
        return json.load(f)


def _all_contract_types(manifest: dict) -> dict:
    merged: dict[str, list[str]] = {}
    for contract in ("context_envelope_contract", "structured_output_contract"):
        merged.update(manifest[contract]["types"])
    return merged


def _internal_fields(manifest: dict, type_name: str) -> set[str]:
    internal: set[str] = set()
    for contract in ("context_envelope_contract", "structured_output_contract"):
        internal.update(manifest[contract].get("python_internal_fields", {}).get(type_name, []))
    return internal


class TestFieldManifestParity:
    """The manifest is the cross-language source of truth; both sides assert against it."""

    def test_every_manifest_type_has_a_model(self, manifest: dict) -> None:
        for type_name in _all_contract_types(manifest):
            assert type_name in MODEL_BY_NAME, f"manifest type {type_name} has no Python model mapping"

    def test_model_field_sets_match_manifest(self, manifest: dict) -> None:
        """Pydantic declared fields (minus documented internals) == manifest wire fields."""
        for type_name, wire_fields in _all_contract_types(manifest).items():
            model = MODEL_BY_NAME[type_name]
            declared = set(model.model_fields.keys())
            wire = declared - _internal_fields(manifest, type_name)
            expected = set(wire_fields)
            assert wire == expected, (
                f"{type_name} wire-field drift:\n"
                f"  extra on Python (add to manifest or mark internal): {sorted(wire - expected)}\n"
                f"  missing on Python (removed from model?):            {sorted(expected - wire)}"
            )

    def test_internal_fields_actually_exist_on_models(self, manifest: dict) -> None:
        """A field marked python_internal must really be declared, else the marker is stale."""
        for contract in ("context_envelope_contract", "structured_output_contract"):
            for type_name, fields in manifest[contract].get("python_internal_fields", {}).items():
                declared = set(MODEL_BY_NAME[type_name].model_fields.keys())
                for field in fields:
                    assert field in declared, (
                        f"{type_name}.{field} marked python_internal but not declared on the model"
                    )


class TestExhaustiveFixtureCoversManifest:
    """The fixture must exercise every wire field so round-trip tests have full coverage."""

    def test_envelope_fixture_keys_cover_wire_fields(self, fixture: dict, manifest: dict) -> None:
        env = fixture["context_envelope"]
        expected = set(manifest["context_envelope_contract"]["types"]["ContextEnvelope"])
        assert expected.issubset(env.keys()), (
            f"fixture context_envelope missing wire fields: {sorted(expected - set(env.keys()))}"
        )
        requester_fields = set(manifest["context_envelope_contract"]["types"]["EnvelopeRequester"])
        assert requester_fields.issubset(env["requester"].keys())

    def test_structured_output_fixture_keys_cover_wire_fields(self, fixture: dict, manifest: dict) -> None:
        out = fixture["ai_structured_output"]
        expected = set(manifest["structured_output_contract"]["types"]["AiStructuredOutput"])
        assert expected.issubset(out.keys()), (
            f"fixture ai_structured_output missing wire fields: {sorted(expected - set(out.keys()))}"
        )
        proposal_fields = set(manifest["structured_output_contract"]["types"]["OutputProposal"])
        assert proposal_fields.issubset(out["proposals"][0].keys())


class TestContextEnvelopeFixture:
    def test_fixture_parses_into_model(self, fixture: dict) -> None:
        envelope = ContextEnvelope(**fixture["context_envelope"])
        assert envelope.contract_version == "ai-runtime.v1"
        assert envelope.run_id == "run_fixture_001"
        assert envelope.requester.user_id == "user_1"
        assert envelope.requester.permission_version == "7"
        assert envelope.ontology.risk_ceiling == "medium"
        assert len(envelope.context.objects) == 2
        assert envelope.context.objects[0].object_type == "Flight"
        assert envelope.context.objects[0].version == 3
        assert len(envelope.context.relations) == 1
        assert envelope.context.relations[0].relation_type == "depends_on"

    def test_fixture_semantic_consistency(self, fixture: dict) -> None:
        """user_message must match proposal action_name (no semantic drift)."""
        envelope = ContextEnvelope(**fixture["context_envelope"])
        output = AiStructuredOutput(**fixture["ai_structured_output"])
        user_msg = envelope.task.user_message.lower()
        has_note_intent = any(tok in user_msg for tok in ("备注", "note", "add"))
        if output.proposals:
            assert has_note_intent, (
                f"user_message '{envelope.task.user_message}' does not match "
                f"proposal action '{output.proposals[0].action_name}'"
            )
            assert output.proposals[0].action_name == "add_note"

    def test_fixture_serializes_stably(self, fixture: dict) -> None:
        envelope = ContextEnvelope(**fixture["context_envelope"])
        serialized = envelope.model_dump_json()
        parsed = json.loads(serialized)
        assert parsed["run_id"] == "run_fixture_001"
        assert parsed["ontology"]["allowed_actions"] == ["Flight.add_note"]


class TestAiStructuredOutputFixture:
    def test_fixture_parses_into_model(self, fixture: dict) -> None:
        output = AiStructuredOutput(**fixture["ai_structured_output"])
        assert output.status == "succeeded"
        assert output.run_id == "run_fixture_001"
        assert len(output.reasoning_steps) == 3
        assert len(output.evidence) == 1
        assert output.evidence[0].field == "flight_number"
        assert len(output.proposals) == 1
        assert output.proposals[0].proposal_id == "prop_fixture_001"
        assert output.proposals[0].action_name == "add_note"
        assert output.proposals[0].risk_level == "low"
        assert output.metrics is not None
        assert output.metrics.model == "fake-llm-v1"
        assert output.token_usage is not None
        assert output.token_usage.total_tokens == 80

    def test_fixture_matches_expected_keys(self, fixture: dict) -> None:
        output = AiStructuredOutput(**fixture["ai_structured_output"])
        payload = json.loads(output.model_dump_json())
        required = {
            "contract_version",
            "run_id",
            "status",
            "answer",
            "reasoning_steps",
            "evidence",
            "proposals",
            "limitations",
            "metrics",
            "token_usage",
        }
        assert required.issubset(payload.keys())


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
