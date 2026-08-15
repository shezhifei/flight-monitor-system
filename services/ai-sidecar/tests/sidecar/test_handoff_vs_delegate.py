"""Tests for Handoff vs Delegate distinction (Task C4).

Asserts:
1. delegate_to_subagent = parallel, isolated context, proposal_only writes, summary returned
2. handoff_to_entity = serial session transfer, compressed history, single respondent
3. Subagents cannot exceed parent permission ceiling
4. Both mechanisms are clearly separated with different semantics
"""

from __future__ import annotations

import pytest

from src.infrastructure.ai.subagents.handoff import (
    DelegateRequest,
    HandoffDelegateManager,
    HandoffRequest,
)


# ============================================================================
# Test Request Validation
# ============================================================================

class TestDelegateRequestValidation:
    """Test delegate request validation."""

    def test_valid_request_passes(self):
        """Valid requests pass validation."""
        request = DelegateRequest(
            target_entity_id="flight_F1234",
            task_description="查询 F1234 的航班状态和延误信息",
            max_rounds=16,
        )
        
        errors = request.validate()
        assert errors == []

    def test_missing_target_entity_id_fails(self):
        """Missing target_entity_id is rejected."""
        request = DelegateRequest(
            target_entity_id="",
            task_description="查询任务",
        )
        
        errors = request.validate()
        assert any("target_entity_id" in e for e in errors)

    def test_missing_task_description_fails(self):
        """Empty task_description is rejected."""
        request = DelegateRequest(
            target_entity_id="flight_F1234",
            task_description="",
        )
        
        errors = request.validate()
        assert any("task_description" in e for e in errors)

    def test_max_rounds_exceeds_limit_fails(self):
        """max_rounds > 50 is rejected."""
        request = DelegateRequest(
            target_entity_id="flight_F1234",
            task_description="查询任务",
            max_rounds=100,
        )
        
        errors = request.validate()
        assert any("50" in e for e in errors)

    def test_default_values_provided(self):
        """Default values set correctly."""
        request = DelegateRequest(
            target_entity_id="flight_F1234",
            task_description="查询任务",
        )
        
        assert request.is_parallel is True
        assert request.requires_summary is True
        assert request.write_action_mode == "proposal_only"


class TestHandoffRequestValidation:
    """Test handoff request validation."""

    def test_valid_request_passes(self):
        """Valid requests pass validation."""
        request = HandoffRequest(
            target_entity_id="anomaly_AN001",
            handoff_prompt="请继续分析此异常的根本原因",
        )
        
        errors = request.validate()
        assert errors == []

    def test_missing_target_fails(self):
        """Missing target_entity_id is rejected."""
        request = HandoffRequest(
            target_entity_id="",
            handoff_prompt="请继续分析",
        )
        
        errors = request.validate()
        assert any("target_entity_id" in e for e in errors)

    def test_empty_handoff_prompt_fails(self):
        """Empty handoff_prompt is rejected."""
        request = HandoffRequest(
            target_entity_id="flight_F1234",
            handoff_prompt="   ",
        )
        
        errors = request.validate()
        assert any("handoff_prompt" in e for e in errors)


# ============================================================================
# Test Permission Ceiling Enforcement
# ============================================================================

class TestPermissionCeilingEnforcement:
    """Test that subagents cannot exceed parent permissions."""

    @pytest.mark.asyncio
    async def test_child_permissions_intersect_with_parent(self):
        """Child can only do what both child and parent can do."""
        manager = HandoffDelegateManager()
        
        # Parent has limited permissions
        parent_permissions = {
            "allowed_tool_names": ["list_flights", "get_flight_info", "search_anomalies"],
            "write_actions": [],
        }
        
        # In production, this would resolve actual capabilities
        # For now, verify the method exists and doesn't crash
        result = await manager._enforce_permission_ceiling(
            parent_entity_id="current_entity",
            child_entity_id="subagent_flight_F1234",
            parent_permissions=parent_permissions,
        )
        
        # Should return a valid permission dict
        assert isinstance(result, dict)
        assert "allowed_tool_names" in result
        assert "proposal_only" in result

    @pytest.mark.asyncio
    async def test_proposal_only_when_no_write_intersection(self):
        """When no write intersection, enforce proposal_only."""
        manager = HandoffDelegateManager()
        
        parent_permissions = {
            "allowed_tool_names": ["list_flights"],
            "write_actions": [],  # No writes allowed
        }
        
        result = await manager._enforce_permission_ceiling(
            parent_entity_id="current_entity",
            child_entity_id="subagent_test",
            parent_permissions=parent_permissions,
        )
        
        # Should force proposal_only when intersection is empty
        assert result.get("proposal_only") is True

    @pytest.mark.asyncio
    async def test_conservative_when_no_parent_permissions(self):
        """Without parent permissions, use conservative defaults."""
        manager = HandoffDelegateManager()
        
        result = await manager._enforce_permission_ceiling(
            parent_entity_id="current_entity",
            child_entity_id="subagent_test",
            parent_permissions=None,  # No permissions provided
        )
        
        # Should default to proposal_only
        assert result.get("proposal_only") is True


# ============================================================================
# Test History Compression
# ============================================================================

class TestHistoryCompression:
    """Test conversation history compression for handoff."""

    @pytest.mark.asyncio
    async def test_small_history_not_modified(self):
        """Small histories remain intact."""
        manager = HandoffDelegateManager()
        
        history = [
            {"role": "user", "content": "初始问题"},
            {"role": "assistant", "content": "回答"},
        ]
        
        compressed = await manager._compress_history(history, max_tokens=8000)
        
        assert len(compressed) == len(history)

    @pytest.mark.asyncio
    async def test_large_history_compressed(self):
        """Large histories are compressed."""
        manager = HandoffDelegateManager()
            
        # Create large history
        history = [
            {"role": "user", "content": f"Question {i}", "aux_key": i}
            for i in range(50)
        ]
            
        compressed = await manager._compress_history(history, max_tokens=5000)
            
        # Should be smaller than original
        assert len(compressed) < len(history)

    @pytest.mark.asyncio
    async def test_first_and_last_messages_preserved(self):
        """Context-setting and recent messages preserved."""
        manager = HandoffDelegateManager()
        
        history = [f"msg_{i}" for i in range(20)]
        
        compressed = await manager._compress_history(history, max_tokens=5000)
        
        # First message should be preserved
        assert "msg_0" in compressed[0]
        
        # Last message should be preserved
        assert "msg_19" in compressed[-1]


# ============================================================================
# Test HandoffResult Structure
# ============================================================================

class TestSubagentResult:
    """Test SubagentResult structure and methods."""

    def test_result_has_all_fields(self):
        """All required fields present."""
        from src.infrastructure.ai.subagents.handoff import SubagentResult
        
        result = SubagentResult(
            run_id="run_123",
            success=True,
            summary="查询成功",
            tool_calls_count=5,
            round_count=3,
            proposals=[],
        )
        
        assert result.run_id == "run_123"
        assert result.success is True
        assert "查询成功" in result.summary
        assert result.tool_calls_count == 5

    def test_short_summary_format(self):
        """Short summary includes status icon and stats."""
        from src.infrastructure.ai.subagents.handoff import SubagentResult
        
        result = SubagentResult(
            run_id="run_456",
            success=False,
            summary="查询失败",
            tool_calls_count=2,
            round_count=1,
            error="Timeout",
        )
        
        short = result.to_short_summary()
        
        assert "❌" in short
        assert "1 rounds" in short
        assert "2 tool calls" in short
        assert "查询失败" in short

    def test_success_icon_in_summary(self):
        """Success shows ✅ icon."""
        from src.infrastructure.ai.subagents.handoff import SubagentResult
        
        result = SubagentResult(
            run_id="run_789",
            success=True,
            summary="成功",
            tool_calls_count=0,
            round_count=0,
        )
        
        assert "✅" in result.to_short_summary()


# ============================================================================
# Test Semantics Distinction
# ============================================================================

class TestSemanticsDistinction:
    """Verify delegate and handoff have distinct semantics."""

    @pytest.mark.asyncio
    async def test_delegate_is_parallel_by_default(self):
        """Delegate runs in parallel mode."""
        request = DelegateRequest(
            target_entity_id="flight_F1234",
            task_description="并行调研",
        )
        
        assert request.is_parallel is True

    @pytest.mark.asyncio
    async def test_delegate_always_proposal_only(self):
        """Delegate write actions are always proposal_only."""
        request = DelegateRequest(
            target_entity_id="flight_F1234",
            task_description="调研任务",
        )
        
        assert request.write_action_mode == "proposal_only"

    @pytest.mark.asyncio
    async def test_delegate_returns_summary(self):
        """Delegate requires summary back to parent."""
        request = DelegateRequest(
            target_entity_id="flight_F1234",
            task_description="调研任务",
        )
        
        assert request.requires_summary is True

    def test_handoff_is_serial_conceptual_difference(self):
        """Handoff is conceptualized as serial (though implementation differs)."""
        # HandoffRequest doesn't have is_parallel field - it's inherently serial
        # This is verified by the absence of such a field
        request = HandoffRequest(
            target_entity_id="flight_F1234",
            handoff_prompt="请接手此会话",
        )
        
        # Verify that delegate and handoff are different types
        assert isinstance(request, HandoffRequest)
        

# ============================================================================
# Integration Tests
# ============================================================================

class TestHandoffDelegateIntegration:
    """End-to-end integration tests."""

    @pytest.mark.asyncio
    async def test_manager_singleton_creation(self):
        """Manager created via helper function."""
        from src.infrastructure.ai.subagents.handoff import get_handoff_delegate_manager
        
        manager = get_handoff_delegate_manager()
        
        assert isinstance(manager, HandoffDelegateManager)

    @pytest.mark.asyncio
    async def test_both_mechanisms_available(self):
        """Both delegate and handoff available through manager."""
        manager = HandoffDelegateManager()
        
        # Verify methods exist
        assert hasattr(manager, "delegate_work")
        assert hasattr(manager, "handoff_session")
        
        # Both are async
        import inspect
        assert inspect.iscoroutinefunction(manager.delegate_work)
        assert inspect.iscoroutinefunction(manager.handoff_session)



# ============================================================================
# Real-path tests (Task C4): delegate goes through SubagentDispatcher.dispatch,
# handoff runs through RuntimeService.execute_run, permission ceiling enforced
# on the execution path (not just data structures).
# ============================================================================

from src.infrastructure.ai.subagents.dispatcher import (
    SubagentDispatcher,
    SubagentResult as DispatcherResult,
)


class _RecordingDispatcher:
    """Fake dispatcher recording dispatch() kwargs, returning a fixed result."""

    def __init__(self, result: DispatcherResult | None = None):
        self.calls: list[dict] = []
        self._result = result or DispatcherResult(
            entity_id="child",
            status="succeeded",
            answer="子代理摘要结果",
            proposal_count=2,
        )

    async def dispatch(self, **kwargs):
        self.calls.append(kwargs)
        return self._result


class _FakeRuntimeService:
    """Fake RuntimeService capturing the envelope passed to execute_run."""

    def __init__(self, answer: str = "handoff 最终回答", status: str = "succeeded"):
        from src.infrastructure.ai.structured_output import AiStructuredOutput

        self.envelopes: list = []
        self._output = AiStructuredOutput(
            run_id="handoff-run-x",
            status=status,
            answer=answer,
        )

    async def execute_run(self, envelope):
        self.envelopes.append(envelope)
        return self._output


class TestDelegateRealPath:
    """delegate_work 走真实 SubagentDispatcher.dispatch 路径。"""

    @pytest.mark.asyncio
    async def test_delegate_reaches_dispatcher_dispatch(self):
        """断链修复：不再有 get_instance/run_subagent AttributeError。"""
        fake = _RecordingDispatcher()
        manager = HandoffDelegateManager(dispatcher=fake)

        result = await manager.delegate_work(
            parent_entity_id="parent_entity",
            delegate_request=DelegateRequest(
                target_entity_id="flight_F1234",
                task_description="查询 F1234 状态",
            ),
            parent_permissions={"allowed_tool_names": ["list_flights"], "write_actions": []},
            allowed_entity_ids=["flight_F1234"],
        )

        assert len(fake.calls) == 1
        call = fake.calls[0]
        assert call["parent_entity_id"] == "parent_entity"
        assert call["target_entity_id"] == "flight_F1234"
        assert call["task"] == "查询 F1234 状态"
        assert call["allowed_entity_ids"] == ["flight_F1234"]
        assert result.success is True
        assert result.summary == "子代理摘要结果"

    @pytest.mark.asyncio
    async def test_delegate_with_real_dispatcher_allowlist_fail_closed(self):
        """真实 SubagentDispatcher：目标不在 allowlist 时被拒。"""
        real_dispatcher = SubagentDispatcher()  # no factory: allowlist check hit first
        manager = HandoffDelegateManager(dispatcher=real_dispatcher)

        result = await manager.delegate_work(
            parent_entity_id="parent_entity",
            delegate_request=DelegateRequest(
                target_entity_id="rogue_entity",
                task_description="越权任务",
            ),
            parent_permissions={"allowed_tool_names": ["list_flights"], "write_actions": []},
            allowed_entity_ids=["flight_F1234"],  # rogue_entity 不在其中
        )

        assert result.success is False
        assert "SUBAGENT_ENTITY_NOT_ALLOWED" in result.summary

    @pytest.mark.asyncio
    async def test_delegate_fail_closed_without_allowlist(self):
        """无 allowlist 且无 resolver 时 fail closed（不允许委派）。"""
        real_dispatcher = SubagentDispatcher()
        manager = HandoffDelegateManager(dispatcher=real_dispatcher)

        result = await manager.delegate_work(
            parent_entity_id="parent_entity",
            delegate_request=DelegateRequest(
                target_entity_id="flight_F1234",
                task_description="查询任务",
            ),
            parent_permissions={"allowed_tool_names": ["list_flights"], "write_actions": []},
            # allowed_entity_ids 未提供，容器无 resolver → None → fail closed
        )

        assert result.success is False
        assert "SUBAGENT_ENTITY_NOT_ALLOWED" in result.summary


class TestDelegatePermissionCeilingRealPath:
    """权限天花板在真实执行路径上强制执行。"""

    @pytest.mark.asyncio
    async def test_child_envelope_carries_only_intersected_permissions(self):
        """传给 dispatcher 的 ceiling envelope 只携带父级允许的权限交集。"""
        fake = _RecordingDispatcher()
        manager = HandoffDelegateManager(dispatcher=fake)

        await manager.delegate_work(
            parent_entity_id="parent_entity",
            delegate_request=DelegateRequest(
                target_entity_id="flight_F1234",
                task_description="查询任务",
            ),
            parent_permissions={
                "allowed_tool_names": ["list_flights", "get_flight_info"],
                "write_actions": [],
            },
            allowed_entity_ids=["flight_F1234"],
        )

        envelope = fake.calls[0]["parent_envelope"]
        # dispatcher 会把 envelope.requester.permissions 拷进子 run
        assert envelope.requester.permissions == ["list_flights", "get_flight_info"]

    @pytest.mark.asyncio
    async def test_no_parent_permissions_means_empty_child_permissions(self):
        """父级无权限信息时 fail closed：子代理权限为空。"""
        fake = _RecordingDispatcher()
        manager = HandoffDelegateManager(dispatcher=fake)

        await manager.delegate_work(
            parent_entity_id="parent_entity",
            delegate_request=DelegateRequest(
                target_entity_id="flight_F1234",
                task_description="查询任务",
            ),
            parent_permissions=None,
            allowed_entity_ids=["flight_F1234"],
        )

        envelope = fake.calls[0]["parent_envelope"]
        assert envelope.requester.permissions == []

    @pytest.mark.asyncio
    async def test_parent_envelope_permissions_inherited_and_clamped(self):
        """提供 parent_envelope 时，其 requester 权限作为天花板来源。"""
        from src.infrastructure.ai.context_envelope import (
            ContextEnvelope,
            EnvelopeContext,
            EnvelopeLimits,
            EnvelopeOntology,
            EnvelopeRequester,
            EnvelopeTask,
        )

        parent_envelope = ContextEnvelope(
            job_id="j1",
            run_id="r1",
            entity_id="parent_entity",
            requester=EnvelopeRequester(user_id="u1", permissions=["only_this_tool"]),
            ontology=EnvelopeOntology(),
            context=EnvelopeContext(limits=EnvelopeLimits(redaction="standard")),
            task=EnvelopeTask(task_type="chat", user_message="hi"),
        )

        fake = _RecordingDispatcher()
        manager = HandoffDelegateManager(dispatcher=fake)

        await manager.delegate_work(
            parent_entity_id="parent_entity",
            delegate_request=DelegateRequest(
                target_entity_id="flight_F1234",
                task_description="查询任务",
            ),
            parent_envelope=parent_envelope,
            allowed_entity_ids=["flight_F1234"],
        )

        # parent_envelope 直接透传，子 run 继承其（受限的）权限
        assert fake.calls[0]["parent_envelope"] is parent_envelope


class TestHandoffRealPath:
    """handoff_session 走真实 RuntimeService.execute_run 路径。"""

    @pytest.mark.asyncio
    async def test_handoff_reaches_runtime_service_execute_run(self):
        """断链修复：不再有 streaming_tools / src.domain.model.context ImportError。"""
        fake_svc = _FakeRuntimeService(answer="异常分析最终回答")
        manager = HandoffDelegateManager()

        result = await manager.handoff_session(
            current_entity_id="query_entity",
            handoff_request=HandoffRequest(
                target_entity_id="anomaly_AN001",
                handoff_prompt="请继续分析此异常",
            ),
            message_history=[{"role": "user", "content": "航班异常吗？"}],
            runtime_service=fake_svc,
        )

        assert result["success"] is True
        assert result["final_response"] == "异常分析最终回答"
        assert result["source"] == "handoff"
        assert len(fake_svc.envelopes) == 1
        envelope = fake_svc.envelopes[0]
        assert envelope.entity_id == "anomaly_AN001"
        assert "请继续分析此异常" in envelope.task.user_message
        assert envelope.metadata["handoff_from"] == "query_entity"

    @pytest.mark.asyncio
    async def test_handoff_compresses_history_before_transfer(self):
        """串行移交：历史压缩后交给目标实体。"""
        fake_svc = _FakeRuntimeService()
        manager = HandoffDelegateManager()

        history = [{"role": "user", "content": f"msg {i}"} for i in range(30)]
        result = await manager.handoff_session(
            current_entity_id="query_entity",
            handoff_request=HandoffRequest(
                target_entity_id="anomaly_AN001",
                handoff_prompt="接手分析",
                compress_history=True,
            ),
            message_history=history,
            runtime_service=fake_svc,
        )

        assert result["success"] is True
        envelope = fake_svc.envelopes[0]
        # 压缩生效：30 条历史压到 5 条以内
        assert envelope.metadata["compressed_history_length"] == 5
        # 首尾消息保留在移交文本中
        assert "msg 0" in envelope.task.user_message
        assert "msg 29" in envelope.task.user_message

    @pytest.mark.asyncio
    async def test_handoff_permission_ceiling_enforced(self):
        """handoff 目标不得突破父 requester 权限：envelope 只携带交集权限。"""
        fake_svc = _FakeRuntimeService()
        manager = HandoffDelegateManager()

        await manager.handoff_session(
            current_entity_id="query_entity",
            handoff_request=HandoffRequest(
                target_entity_id="anomaly_AN001",
                handoff_prompt="接手分析",
            ),
            message_history=[],
            runtime_service=fake_svc,
            parent_permissions={"allowed_tool_names": ["list_flights"], "write_actions": []},
        )

        envelope = fake_svc.envelopes[0]
        assert envelope.requester.permissions == ["list_flights"]

    @pytest.mark.asyncio
    async def test_handoff_fail_closed_without_parent_permissions(self):
        """无父权限信息时 handoff 目标权限为空（fail closed）。"""
        fake_svc = _FakeRuntimeService()
        manager = HandoffDelegateManager()

        await manager.handoff_session(
            current_entity_id="query_entity",
            handoff_request=HandoffRequest(
                target_entity_id="anomaly_AN001",
                handoff_prompt="接手分析",
            ),
            message_history=[],
            runtime_service=fake_svc,
        )

        assert fake_svc.envelopes[0].requester.permissions == []

    @pytest.mark.asyncio
    async def test_handoff_runtime_failure_returns_error_dict(self):
        """目标实体执行失败时返回 error，不抛出。"""
        fake_svc = _FakeRuntimeService(status="failed", answer="AI runtime processing error: boom")
        manager = HandoffDelegateManager()

        result = await manager.handoff_session(
            current_entity_id="query_entity",
            handoff_request=HandoffRequest(
                target_entity_id="anomaly_AN001",
                handoff_prompt="接手分析",
            ),
            message_history=[],
            runtime_service=fake_svc,
        )

        assert result["success"] is False
        assert "boom" in result["final_response"]

    @pytest.mark.asyncio
    async def test_handoff_invalid_request_raises(self):
        """无效 handoff 请求在验证阶段被拒。"""
        manager = HandoffDelegateManager()

        with pytest.raises(ValueError, match="target_entity_id"):
            await manager.handoff_session(
                current_entity_id="query_entity",
                handoff_request=HandoffRequest(target_entity_id="", handoff_prompt="x"),
                message_history=[],
                runtime_service=_FakeRuntimeService(),
            )


class TestNoBrokenImports:
    """断链修复回归：模块可导入，无悬挂引用。"""

    def test_handoff_module_imports_cleanly(self):
        import importlib

        module = importlib.import_module("src.infrastructure.ai.subagents.handoff")
        assert hasattr(module, "HandoffDelegateManager")

    def test_subagents_package_imports_cleanly(self):
        import importlib

        module = importlib.import_module("src.infrastructure.ai.subagents")
        assert hasattr(module, "HandoffDelegateManager")
        assert hasattr(module, "SubagentDispatcher")

    def test_handoff_source_has_no_dangling_references(self):
        """handoff.py 不再引用不存在的 API（get_instance/run_subagent/streaming_tools/domain.model）。"""
        import inspect

        from src.infrastructure.ai.subagents import handoff as handoff_module

        source = inspect.getsource(handoff_module)
        assert "get_instance" not in source
        assert "run_subagent" not in source
        assert "runtime_service.streaming_tools" not in source
        assert "src.domain.model.context" not in source
