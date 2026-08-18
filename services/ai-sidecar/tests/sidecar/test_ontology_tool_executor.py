"""Task F4 — ontology tools route through the executor to the Rust-backed client.

Asserts:

1. ``ontology.lookup`` / ``ontology.explain_constraints`` / ``ontology.propose_action``
   are classified as a distinct ``ontology`` tool type (not read-only, not
   write-action, not MCP).
2. Read/explain calls are forwarded to the injected ``OntologyTools`` adapter
   with the run id — never to a local stub.
3. ``ontology.propose_action`` for a controlled write (``Flight.change_stand``)
   produces an approval proposal and never executes directly.
4. Without a wired adapter the tools fail closed (no fabricated results).
5. Without an MQ gate the tools fail closed (they are not public L0).
"""

from __future__ import annotations

from typing import Any

import pytest

from src.infrastructure.ai.ontology.action_client import OntologyActionClientError
from src.infrastructure.ai.ontology_tools import (
    ADVISORY_ACTIONS,
    CONTROLLED_WRITE_ACTIONS,
    UnregisteredActionError,
)
from src.infrastructure.ai.tools.tool_executor import (
    ToolExecutor,
    is_ontology_tool,
)

from tests.sidecar.tool_executor_test_support import (
    AuthorizedToolMqGate,
    FakeReadOnlyBackend,
)

ONTOLOGY_TOOL_NAMES = [
    "ontology.lookup",
    "ontology.explain_constraints",
    "ontology.propose_action",
]


class FakeOntologyTools:
    """Records adapter calls; mirrors the OntologyTools contract."""

    def __init__(self, *, propose_result: dict | None = None) -> None:
        self.lookup_calls: list[dict] = []
        self.explain_calls: list[dict] = []
        self.propose_calls: list[dict] = []
        self._propose_result = propose_result

    async def lookup(self, *, run_id: str, entity_id: str, include_relations: bool = True) -> dict:
        self.lookup_calls.append(
            {"run_id": run_id, "entity_id": entity_id, "include_relations": include_relations}
        )
        return {"flight": {"flight_id": "F1"}, "evidence": {"source": "rust", "object_id": "F1"}}

    async def explain_constraints(self, *, run_id: str, entity_type: str, proposed_change: dict) -> dict:
        self.explain_calls.append(
            {"run_id": run_id, "entity_type": entity_type, "proposed_change": proposed_change}
        )
        return {"violations": [], "evidence": {"source": "rust"}}

    async def propose_action(
        self,
        *,
        run_id: str,
        action_name: str,
        parameters: dict,
        allowed_actions: list[str],
    ) -> dict:
        self.propose_calls.append(
            {
                "run_id": run_id,
                "action_name": action_name,
                "parameters": parameters,
                "allowed_actions": list(allowed_actions),
            }
        )
        if self._propose_result is not None:
            return dict(self._propose_result)
        if action_name not in allowed_actions:
            raise UnregisteredActionError(action_name)
        if action_name in CONTROLLED_WRITE_ACTIONS:
            return {
                "execution_mode": "proposal_only",
                "action_name": action_name,
                "parameters": dict(parameters),
            }
        return {"suggestions": [{"stand_id": "B2"}], "evidence": {"source": "rust"}}


class FailingOntologyTools(FakeOntologyTools):
    async def lookup(self, *, run_id: str, entity_id: str, include_relations: bool = True) -> dict:
        raise OntologyActionClientError("transport_error", "boom")

    async def explain_constraints(self, *, run_id: str, entity_type: str, proposed_change: dict) -> dict:
        raise OntologyActionClientError("transport_error", "boom")

    async def propose_action(
        self, *, run_id: str, action_name: str, parameters: dict, allowed_actions: list[str]
    ) -> dict:
        raise OntologyActionClientError(
            "ontology_action_rejected", "denied", status_code=403, error_code="TOOL_ACTOR_PERMISSION_DENIED"
        )


def _executor(**kwargs: Any) -> ToolExecutor:
    kwargs.setdefault("mq_gate", AuthorizedToolMqGate())
    kwargs.setdefault("read_only_backend", FakeReadOnlyBackend())
    return ToolExecutor(**kwargs)


def test_tool_type_is_ontology() -> None:
    executor = _executor()
    for name in ONTOLOGY_TOOL_NAMES:
        assert is_ontology_tool(name)
        assert executor.get_tool_type(name) == "ontology"
    assert not is_ontology_tool("flight_status_lookup")
    assert not is_ontology_tool("mcp.server.tool")


@pytest.mark.asyncio
async def test_lookup_routes_to_ontology_client() -> None:
    fake = FakeOntologyTools()
    executor = _executor(ontology_tools=fake)
    result = await executor.execute(
        {"tool_call_id": "c1", "tool_name": "ontology.lookup", "arguments": {"entity_id": "flight:CA1598"}},
        run_id="run_1",
    )
    assert result.success is True
    assert fake.lookup_calls == [
        {"run_id": "run_1", "entity_id": "flight:CA1598", "include_relations": True}
    ]
    assert result.result["evidence"]["source"] == "rust"


@pytest.mark.asyncio
async def test_explain_constraints_routes_to_ontology_client() -> None:
    fake = FakeOntologyTools()
    executor = _executor(ontology_tools=fake)
    change = {"action": "change_stand", "new_stand_id": "A12"}
    result = await executor.execute(
        {
            "tool_call_id": "c2",
            "tool_name": "ontology.explain_constraints",
            "arguments": {"entity_type": "Flight", "proposed_change": change},
        },
        run_id="run_2",
    )
    assert result.success is True
    assert fake.explain_calls == [
        {"run_id": "run_2", "entity_type": "Flight", "proposed_change": change}
    ]


@pytest.mark.asyncio
async def test_propose_controlled_write_builds_approval_proposal() -> None:
    fake = FakeOntologyTools()
    executor = _executor(ontology_tools=fake)
    result = await executor.execute(
        {
            "tool_call_id": "c3",
            "tool_name": "ontology.propose_action",
            "arguments": {
                "action_name": "Flight.change_stand",
                "parameters": {"flight_id": "F1", "new_stand_id": "A12"},
            },
        },
        run_id="run_3",
    )
    assert result.success is True
    assert result.proposal is not None
    proposal = result.proposal
    assert proposal["object_type"] == "Flight"
    assert proposal["action_name"] == "change_stand"
    assert proposal["requires_approval"] is True
    assert proposal["execution_mode"] == "proposal_only"
    # Controlled writes never go through the read/advisory HTTP surface.
    assert fake.lookup_calls == []
    assert fake.explain_calls == []


@pytest.mark.asyncio
async def test_propose_advisory_action_passes_through() -> None:
    fake = FakeOntologyTools()
    executor = _executor(ontology_tools=fake)
    result = await executor.execute(
        {
            "tool_call_id": "c4",
            "tool_name": "ontology.propose_action",
            "arguments": {"action_name": "flight.suggest_stand_adjustment", "parameters": {"flight_id": "F1"}},
        },
        run_id="run_4",
    )
    assert result.success is True
    assert result.proposal is None
    assert result.result["suggestions"] == [{"stand_id": "B2"}]
    assert fake.propose_calls[0]["action_name"] == "flight.suggest_stand_adjustment"
    assert set(fake.propose_calls[0]["allowed_actions"]) >= ADVISORY_ACTIONS


@pytest.mark.asyncio
async def test_propose_unregistered_action_fails_closed() -> None:
    fake = FakeOntologyTools()
    executor = _executor(ontology_tools=fake)
    result = await executor.execute(
        {
            "tool_call_id": "c5",
            "tool_name": "ontology.propose_action",
            "arguments": {"action_name": "Flight.delete", "parameters": {}},
        },
        run_id="run_5",
    )
    assert result.success is False
    assert "UNREGISTERED_ACTION" in (result.error or "")


@pytest.mark.asyncio
async def test_missing_adapter_fails_closed() -> None:
    executor = _executor()  # no ontology_tools wired
    result = await executor.execute(
        {"tool_call_id": "c6", "tool_name": "ontology.lookup", "arguments": {"entity_id": "flight:F1"}},
        run_id="run_6",
    )
    assert result.success is False
    assert "ONTOLOGY_CLIENT_NOT_CONFIGURED" in (result.error or "")


@pytest.mark.asyncio
async def test_client_error_surfaces_as_failure() -> None:
    executor = _executor(ontology_tools=FailingOntologyTools())
    result = await executor.execute(
        {"tool_call_id": "c7", "tool_name": "ontology.lookup", "arguments": {"entity_id": "flight:F1"}},
        run_id="run_7",
    )
    assert result.success is False
    assert "transport_error" in (result.error or "")


@pytest.mark.asyncio
async def test_ontology_tools_require_mq_gate() -> None:
    fake = FakeOntologyTools()
    executor = ToolExecutor(read_only_backend=FakeReadOnlyBackend(), ontology_tools=fake)
    result = await executor.execute(
        {"tool_call_id": "c8", "tool_name": "ontology.lookup", "arguments": {"entity_id": "flight:F1"}},
        run_id="run_8",
    )
    assert result.success is False
    assert result.blocked_by is not None
    assert "MQ_GATE_UNAVAILABLE" in (result.rule or "")
    assert fake.lookup_calls == []


def test_available_tools_include_ontology() -> None:
    available = set(ToolExecutor().get_available_tools())
    assert set(ONTOLOGY_TOOL_NAMES) <= available
