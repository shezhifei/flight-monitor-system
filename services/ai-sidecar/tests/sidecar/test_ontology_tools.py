"""Anti-stub contract tests for ``ontology_tools`` (Task F3).

``OntologyTools`` must be a thin adapter over the fail-closed
``OntologyActionClient``: no hardcoded entities, no in-process caches,
no permissive fallbacks when the registry is empty.
"""

from __future__ import annotations

import pytest

from src.infrastructure.ai.ontology.action_client import OntologyActionClientError
from src.infrastructure.ai.ontology_tools import (
    CONTROLLED_WRITE_ACTIONS,
    OntologyTools,
    UnregisteredActionError,
)


class RecordingClient:
    """Records calls; raises nothing unless configured."""

    def __init__(self, *, read_result: dict | None = None, advisory_result: dict | None = None) -> None:
        self.read_calls: list[dict] = []
        self.advisory_calls: list[dict] = []
        self._read_result = read_result if read_result is not None else {"flight": {"flight_id": "F1"}}
        self._advisory_result = advisory_result if advisory_result is not None else {"suggestions": []}

    async def read(self, *, run_id: str, action_name: str, arguments: dict) -> dict:
        self.read_calls.append({"run_id": run_id, "action_name": action_name, "arguments": arguments})
        return self._read_result

    async def advisory(self, *, run_id: str, action_name: str, arguments: dict) -> dict:
        self.advisory_calls.append({"run_id": run_id, "action_name": action_name, "arguments": arguments})
        return self._advisory_result


class FailingClient:
    def __init__(self) -> None:
        self.read_calls: list[dict] = []
        self.advisory_calls: list[dict] = []

    async def read(self, *, run_id: str, action_name: str, arguments: dict) -> dict:
        self.read_calls.append({"run_id": run_id, "action_name": action_name, "arguments": arguments})
        raise OntologyActionClientError("transport_error", "boom")

    async def advisory(self, *, run_id: str, action_name: str, arguments: dict) -> dict:
        raise OntologyActionClientError("transport_error", "boom")


@pytest.mark.asyncio
async def test_lookup_does_not_return_stub_flight_when_client_fails() -> None:
    tools = OntologyTools(client=FailingClient())
    with pytest.raises(OntologyActionClientError):
        await tools.lookup(run_id="run_1", entity_id="flight:CA1598")


@pytest.mark.asyncio
async def test_lookup_flight_id_calls_flight_get_context() -> None:
    client = RecordingClient()
    tools = OntologyTools(client=client)
    result = await tools.lookup(run_id="run_1", entity_id="flight:CA1598")

    assert client.read_calls == [
        {
            "run_id": "run_1",
            "action_name": "flight.get_context",
            "arguments": {"flight_id": "CA1598"},
        }
    ]
    # Rust response passes through with an evidence block attached.
    assert result["flight"] == {"flight_id": "F1"}
    assert result["evidence"]["source"] == "ontology.lookup"
    assert result["evidence"]["object_id"] == "CA1598"


@pytest.mark.asyncio
async def test_lookup_unknown_entity_prefix_fails_closed() -> None:
    client = RecordingClient()
    tools = OntologyTools(client=client)
    with pytest.raises(ValueError):
        await tools.lookup(run_id="run_1", entity_id="crew:XYZ")
    assert client.read_calls == []


@pytest.mark.asyncio
async def test_propose_unregistered_action_fails_closed() -> None:
    client = RecordingClient()
    tools = OntologyTools(client=client)
    with pytest.raises(UnregisteredActionError):
        await tools.propose_action(
            run_id="run_1",
            action_name="Flight.delete",
            parameters={"flight_id": "F1"},
            allowed_actions=["flight.suggest_stand_adjustment"],
        )
    assert client.advisory_calls == []
    assert client.read_calls == []


@pytest.mark.asyncio
async def test_propose_change_stand_is_proposal_only() -> None:
    assert "Flight.change_stand" in CONTROLLED_WRITE_ACTIONS
    client = RecordingClient()
    tools = OntologyTools(client=client)
    result = await tools.propose_action(
        run_id="run_1",
        action_name="Flight.change_stand",
        parameters={"flight_id": "F1", "new_stand_id": "A12"},
        allowed_actions=["Flight.change_stand"],
    )
    assert result["execution_mode"] == "proposal_only"
    assert result["action_name"] == "Flight.change_stand"
    assert result["parameters"] == {"flight_id": "F1", "new_stand_id": "A12"}
    # Controlled writes never hit the read/advisory HTTP surface.
    assert client.advisory_calls == []
    assert client.read_calls == []


@pytest.mark.asyncio
async def test_propose_advisory_action_calls_client() -> None:
    client = RecordingClient(advisory_result={"suggestions": [{"stand_id": "B2"}]})
    tools = OntologyTools(client=client)
    result = await tools.propose_action(
        run_id="run_1",
        action_name="flight.suggest_stand_adjustment",
        parameters={"flight_id": "F1"},
        allowed_actions=["flight.suggest_stand_adjustment"],
    )
    assert result == {"suggestions": [{"stand_id": "B2"}]}
    assert client.advisory_calls == [
        {
            "run_id": "run_1",
            "action_name": "flight.suggest_stand_adjustment",
            "arguments": {"flight_id": "F1"},
        }
    ]


@pytest.mark.asyncio
async def test_explain_unknown_mapping_is_hard_violation() -> None:
    client = RecordingClient()
    tools = OntologyTools(client=client)
    result = await tools.explain_constraints(
        run_id="run_1",
        entity_type="Flight",
        proposed_change={"action": "teleport"},
    )
    violations = result["violations"]
    assert len(violations) == 1
    assert violations[0]["severity"] == "hard"
    # No invented rules: nothing was fetched from anywhere.
    assert client.read_calls == []


@pytest.mark.asyncio
async def test_explain_stand_change_uses_rust_availability() -> None:
    client = RecordingClient(
        read_result={
            "is_available": False,
            "conflicts": [{"flight_id": "F9", "reason": "stand occupation overlaps requested window"}],
        }
    )
    tools = OntologyTools(client=client)
    result = await tools.explain_constraints(
        run_id="run_1",
        entity_type="Flight",
        proposed_change={
            "action": "change_stand",
            "new_stand_id": "A12",
            "time_window": {"start": "2026-08-18T10:00:00Z", "end": "2026-08-18T12:00:00Z"},
        },
    )
    assert client.read_calls == [
        {
            "run_id": "run_1",
            "action_name": "stand.check_availability",
            "arguments": {
                "stand_id": "A12",
                "time_window": {"start": "2026-08-18T10:00:00Z", "end": "2026-08-18T12:00:00Z"},
            },
        }
    ]
    assert len(result["violations"]) == 1
    assert result["violations"][0]["severity"] == "hard"


@pytest.mark.asyncio
async def test_explain_stand_change_without_time_window_fails_closed() -> None:
    client = RecordingClient()
    tools = OntologyTools(client=client)
    result = await tools.explain_constraints(
        run_id="run_1",
        entity_type="Flight",
        proposed_change={"action": "change_stand", "new_stand_id": "A12"},
    )
    assert len(result["violations"]) == 1
    assert result["violations"][0]["severity"] == "hard"
    assert client.read_calls == []
