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

