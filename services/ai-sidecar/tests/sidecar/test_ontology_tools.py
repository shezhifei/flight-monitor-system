"""Anti-stub contract tests for ``ontology_tools`` (Task F3).

``OntologyTools`` must be a thin adapter over the fail-closed
``OntologyActionClient``: no hardcoded entities, no in-process caches,
no permissive fallbacks when the registry is empty.
"""

from __future__ import annotations

import pytest

from src.infrastructure.ai.ontology.action_client import OntologyActionClientError
from src.infrastructure.ai.ontology_tools import OntologyTools, UnregisteredActionError


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
async def test_propose_deprecated_change_stand_fails_closed() -> None:
    # 信封是唯一允许名单：已废止动作不在 envelope.allowed_actions 内 → Unregistered。
    client = RecordingClient()
    tools = OntologyTools(client=client)
    with pytest.raises(UnregisteredActionError):
        await tools.propose_action(
            run_id="run_1",
            action_name="Flight.change_stand",
            parameters={"flight_id": "F1", "new_stand_id": "A12"},
            allowed_actions=["StandOccupation.allocate"],
        )
    assert client.read_calls == []
    assert client.advisory_calls == []


@pytest.mark.asyncio
async def test_propose_action_in_envelope_is_proposal_only() -> None:
    client = RecordingClient()
    tools = OntologyTools(client=client)
    result = await tools.propose_action(
        run_id="run_1",
        action_name="DispatchOrder.assign_slot",
        parameters={"dispatch_order_id": "DO1", "slot_code": "lead"},
        allowed_actions=["DispatchOrder.assign_slot"],
    )
    assert result["execution_mode"] == "proposal_only"
    assert result["action_name"] == "DispatchOrder.assign_slot"
    assert client.advisory_calls == []


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
            "action": "StandOccupation.allocate",
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
    assert result["violations"][0]["severity"] == "soft"
    assert result["violations"][0]["rule_id"] == "stand_occupation_conflict"


@pytest.mark.asyncio
async def test_explain_stand_change_without_time_window_fails_closed() -> None:
    client = RecordingClient()
    tools = OntologyTools(client=client)
    result = await tools.explain_constraints(
        run_id="run_1",
        entity_type="Flight",
        proposed_change={"action": "StandOccupation.allocate", "new_stand_id": "A12"},
    )
    assert len(result["violations"]) == 1
    assert result["violations"][0]["severity"] == "hard"
    assert client.read_calls == []


@pytest.mark.asyncio
async def test_explain_deprecated_change_stand_key_fails_closed() -> None:
    client = RecordingClient()
    tools = OntologyTools(client=client)
    result = await tools.explain_constraints(
        run_id="run_legacy",
        entity_type="Flight",
        proposed_change={"action": "change_stand", "new_stand_id": "A12", "time_window": {"start": "2026-08-18T10:00:00Z", "end": "2026-08-18T12:00:00Z"}},
    )
    assert result["violations"][0]["rule_id"] == "unsupported_constraint_mapping"
    assert client.read_calls == []


@pytest.mark.asyncio
async def test_propose_stand_allocate_simulates_soft_overlap() -> None:
    # 机位占用重叠是 soft（告警不硬拦）：有冲突仍 proposal_only，不 reject。
    client = RecordingClient(
        read_result={
            "is_available": False,
            "conflicts": [{"flight_id": "F9", "reason": "stand occupation overlaps requested window"}],
        }
    )
    tools = OntologyTools(client=client)
    result = await tools.propose_action(
        run_id="run_1",
        action_name="StandOccupation.allocate",
        parameters={
            "stand_code": "A12",
            "registration": "B-1234",
            "starts_at": "2026-08-18T10:00:00Z",
            "ends_at": "2026-08-18T12:00:00Z",
        },
        allowed_actions=["StandOccupation.allocate"],
    )
    assert result["execution_mode"] == "proposal_only"
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
    simulate = result["simulate"]
    assert simulate["after"]["stand"] == "A12"
    assert simulate["after"]["registration"] == "B-1234"
    assert len(simulate["violations"]) == 1
    assert simulate["violations"][0]["severity"] == "soft"


@pytest.mark.asyncio
async def test_propose_carousel_allocate_zero_constraint() -> None:
    # 转盘占用显式零约束：不触发任何冲突检查，violations 恒空。
    client = RecordingClient()
    tools = OntologyTools(client=client)
    result = await tools.propose_action(
        run_id="run_1",
        action_name="CarouselAssignment.allocate",
        parameters={
            "carousel_code": "C1",
            "flight_id": "FL1",
            "starts_at": "2026-08-18T10:00:00Z",
            "ends_at": "2026-08-18T12:00:00Z",
        },
        allowed_actions=["CarouselAssignment.allocate"],
    )
    assert result["execution_mode"] == "proposal_only"
    # 只读一次 before 快照（flight.get_context），无 check_availability / 冲突模拟。
    assert [c["action_name"] for c in client.read_calls] == ["flight.get_context"]
    simulate = result["simulate"]
    assert simulate["after"]["carousel"] == "C1"
    assert simulate["after"]["flight_id"] == "FL1"
    assert simulate["violations"] == []


@pytest.mark.asyncio
async def test_propose_dispatch_order_slot_actions_proposal_only() -> None:
    # PR5 派工槽位：assign/unassign/add/remove_slot 与规则同一领域函数。
    # 无资源可占 → proposal 走 `_simulate_controlled_write` proposal_only 分支，
    # 不触发 check_availability，不产生冲突模拟。
    for action_name in ("assign_slot", "unassign_slot", "add_slot", "remove_slot"):
        client = RecordingClient()
        tools = OntologyTools(client=client)
        result = await tools.propose_action(
            run_id="run_1",
            action_name=f"DispatchOrder.{action_name}",
            parameters={"order_id": "DP9", "slot_code": "S1", "user_id": "U1"},
            allowed_actions=[f"DispatchOrder.{action_name}"],
        )
        assert result["execution_mode"] == "proposal_only"
        assert result["action_name"] == f"DispatchOrder.{action_name}"
        # 无 flight_id → 不发起任何读取；仅 proposal 信封返回。
        assert client.read_calls == []
        assert client.advisory_calls == []
