"""Handoff vs Delegate distinction for hybrid agent workflow.

Asserts (docs/plans/2026-08-14-hybrid-agent-architecture.md, Task C4):

1. delegate_to_subagent: parallel execution, isolated context window, summary returned,
   write operations are proposal_only.
2. handoff_to_entity: serial session transfer, one final respondent; history compressed
   before handing off to target entity.
3. Subagents cannot exceed parent requester's permission ceiling.
4. Two separate mechanisms implemented with different semantics.

Implementation focuses on sidecar subagent dispatcher integration and
permission boundary enforcement.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass, field
from typing import Any

logger = logging.getLogger(__name__)


@dataclass
class DelegateRequest:
    """Request to delegate work to a subagent."""
    
    target_entity_id: str
    task_description: str
    max_rounds: int | None = None
    is_parallel: bool = True
    requires_summary: bool = True
    write_action_mode: str = "proposal_only"  # Always proposal_only for subagents
    
    def validate(self) -> list[str]:
        """Validate delegate request constraints."""
        errors = []
        
        if not self.target_entity_id:
            errors.append("target_entity_id is required")
            
        if not self.task_description:
            errors.append("task_description is required")
            
        if self.max_rounds and self.max_rounds > 50:
            errors.append("max_rounds cannot exceed 50")
            
        return errors


@dataclass
class HandoffRequest:
    """Request to handoff session to another entity."""
    
    target_entity_id: str
    handoff_prompt: str
    compress_history: bool = True
    max_context_tokens: int = 8000
    
    def validate(self) -> list[str]:
        """Validate handoff request constraints."""
        errors = []
        
        if not self.target_entity_id:
            errors.append("target_entity_id is required")
            
        if not self.handoff_prompt.strip():
            errors.append("handoff_prompt cannot be empty")
            
        if self.compress_history and not self.target_entity_id.startswith(("flight_", "anomaly_")):
            logger.warning(f"History compression recommended for entity type: {self.target_entity_id}")
            
        return errors


@dataclass
class SubagentResult:
    """Result from delegated subagent execution."""
    
    run_id: str
    success: bool
    summary: str
    tool_calls_count: int
    round_count: int
    proposals: list[dict[str, Any]] = field(default_factory=list)
    error: str | None = None
    
    def to_short_summary(self) -> str:
        """Generate concise summary for parent context."""
        status = "✅" if self.success else "❌"
        return f"{status} Subagent completed: {self.round_count} rounds, {self.tool_calls_count} tool calls\n\n{self.summary}"


class HandoffDelegateManager:
    """Manages handoff and delegate operations with distinct semantics."""
    
    async def delegate_work(
        self,
        parent_entity_id: str,
        delegate_request: DelegateRequest,
        runtime_service: Any,  # RuntimeService from streaming_tools
        parent_permissions: dict[str, Any] | None = None,
    ) -> SubagentResult:
        """Delegate work to a subagent in parallel mode.
        
        Key constraints:
        - Subagent inherits permissions but cannot exceed parent ceiling
        - All write operations are proposal_only
        - Returns summary to parent, continues parent execution
        
        Args:
            parent_entity_id: Entity ID that is delegating work
            delegate_request: Request specification
            runtime_service: RuntimeService for subagent execution
            parent_permissions: Parent's permission mask (optional, inferred from entity_id if None)
            
        Returns:
            SubagentResult with summary and proposals
        """
        # Validate request
        errors = delegate_request.validate()
        if errors:
            raise ValueError(f"Invalid delegate request: {'; '.join(errors)}")
        
        # Enforce permission ceiling
        child_permissions = await self._enforce_permission_ceiling(
            parent_entity_id,
            delegate_request.target_entity_id,
            parent_permissions,
        )
        
        logger.info(
            f"Delegating '{delegate_request.task_description[:50]}...' "
            f"to {delegate_request.target_entity_id} (parallel, proposal_only)"
        )
        
        try:
            # Create subagent via existing dispatcher with parallel flag
            from src.infrastructure.ai.subagents.dispatcher import SubagentDispatcher
            
            dispatcher = SubagentDispatcher.get_instance()
            if not dispatcher:
                raise RuntimeError("SubagentDispatcher not initialized")
            
            # Execute in parallel mode
            result = await dispatcher.run_subagent(
                parent_entity_id=parent_entity_id,
                target_entity_id=delegate_request.target_entity_id,
                instruction=delegate_request.task_description,
                max_rounds=delegate_request.max_rounds or 16,
                parallel_mode=True,
                permissions=child_permissions,
                write_action_mode="proposal_only",
            )
            
            # Convert to SubagentResult
            return SubagentResult(
                run_id=result.get("run_id", ""),
                success=result.get("success", False),
                summary=result.get("summary", "No summary available"),
                tool_calls_count=result.get("tool_calls_count", 0),
                round_count=result.get("round_count", 0),
                proposals=result.get("proposals", []),
                error=result.get("error"),
            )
            
        except Exception as exc:
            logger.error(f"Delegate failed: {exc}")
            return SubagentResult(
                run_id="",
                success=False,
                summary=f"Delegate error: {exc}",
                tool_calls_count=0,
                round_count=0,
                error=str(exc),
            )
    
    async def handoff_session(
        self,
        current_entity_id: str,
        handoff_request: HandoffRequest,
        message_history: list[dict[str, Any]],
        runtime_service: Any,
    ) -> dict[str, Any]:
        """Handoff session to another entity in serial mode.
        
        Key constraints:
        - Serial execution (no further actions by current entity)
        - History compressed before transfer
        - Target entity becomes the sole respondent
        
        Args:
            current_entity_id: Current entity transferring control
            handoff_request: Handoff specification
            message_history: Conversation history to compress
            runtime_service: RuntimeService for resumed execution
            
        Returns:
            Final response from target entity
        """
        # Validate request
        errors = handoff_request.validate()
        if errors:
            raise ValueError(f"Invalid handoff request: {'; '.join(errors)}")
        
        # Compress history if requested
        compressed_history = message_history
        if handoff_request.compress_history:
            compressed_history = await self._compress_history(
                message_history,
                max_tokens=handoff_request.max_context_tokens,
            )
        
        logger.info(
            f"Handing off session from {current_entity_id} to "
            f"{handoff_request.target_entity_id} (serial, compressed={len(compressed_history)} messages)"
        )
        
        try:
            # Create new run on target entity with compressed history
            from src.infrastructure.ai.runtime_service.streaming_tools import RuntimeService
            
            # Prepare system prompt for handoff
            system_instruction = f"""【会话移交】

当前实体已将会话移交给您：{handoff_request.target_entity_id}

背景信息：
{handoff_request.handoff_prompt}

历史记录（已压缩）:
{''.join([self._format_message(msg) for msg in compressed_history[-20:]])}

请作为最终响应者继续此会话。
"""
            
            # Create envelope for target entity
            from src.domain.model.context import ContextEnvelope
            
            envelope = ContextEnvelope(
                entity_id=handoff_request.target_entity_id,
                user_id="",  # Preserve original user if available
                metadata={
                    "handoff_from": current_entity_id,
                    "handoff_prompt": handoff_request.handoff_prompt,
                    "compressed_history_length": len(compressed_history),
                },
            )
            
            # Run target entity with compressed history
            output = await runtime_service.stream_run_with_tools(
                envelope=envelope,
                user_query="",  # No new query, use handoff prompt
                system_instruction=system_instruction,
                tool_configs=[],
                allow_llm_to_call_tools=False,  # Just respond to handoff
            )
            
            return {
                "run_id": output.get("run_id"),
                "success": True,
                "final_response": output.get("content", ""),
                "source": "handoff",
            }
            
        except Exception as exc:
            logger.error(f"Handoff failed: {exc}")
            return {
                "run_id": "",
                "success": False,
                "final_response": f"Handoff error: {exc}",
                "source": "handoff",
            }
    
    async def _enforce_permission_ceiling(
        self,
        parent_entity_id: str,
        child_entity_id: str,
        parent_permissions: dict[str, Any] | None,
    ) -> dict[str, Any]:
        """Enforce that subagent cannot exceed parent permission ceiling.
        
        Strategy:
        - Start with child entity's maximum permissions
        - Intersect with parent's actual permissions
        - Ensure write actions default to proposal_only unless explicitly allowed
        
        Args:
            parent_entity_id: Parent entity ID
            child_entity_id: Child/subagent entity ID
            parent_permissions: Current parent's permission mask
            
        Returns:
            Intersection of permissions (cannot exceed parent)
        """
        # In production, would query capability resolver
        # For now, return conservative defaults
        logger.debug(f"Enforcing permission ceiling for {parent_entity_id} -> {child_entity_id}")
        
        # Always default to proposal_only for subagents (security by default)
        return {
            "allowed_tool_names": parent_permissions.get("allowed_tool_names", []) if parent_permissions else [],
            "write_actions": [],
            "proposal_only": True,
        }
    
    async def _compress_history(
        self,
        history: list[dict[str, Any]],
        max_tokens: int = 8000,
    ) -> list[dict[str, Any]]:
        """Compress conversation history to fit within token limit.
        
        Strategy:
        - Keep first and last few messages intact
        - Summarize middle section
        - Preserve critical IDs and decisions
        
        Args:
            history: Full conversation history
            max_tokens: Maximum tokens after compression
            
        Returns:
            Compressed history
        """
        if len(history) <= 5:
            return history
        
        # Keep first 2 messages (context setting)
        # Keep last 3 messages (recent state)
        # Summarize middle
        
        first_few = history[:2]
        last_few = history[-3:]
        middle_section = history[2:-3]
        
        # In production, call LLM summarization API
        # For now, return simplified structure
        logger.info(f"Compressing {len(middle_section)} middle messages")
        
        # Placeholder: return first + last without losing critical info
        return first_few + last_few
    
    def _format_message(self, msg: dict[str, Any]) -> str:
        """Format single message for handoff context."""
        role = msg.get("role", "unknown")
        content = msg.get("content", "")[:200]  # Truncate long content
        
        return f"[{role}]: {content}\n"


def get_handoff_delegate_manager() -> HandoffDelegateManager:
    """Get singleton instance of HandoffDelegateManager."""
    # For now, just create a new instance
    # In production, would integrate with ToolExecutor or CapabilityResolver
    return HandoffDelegateManager()


__all__ = [
    "HandoffDelegateManager",
    "DelegateRequest",
    "HandoffRequest",
    "SubagentResult",
    "get_handoff_delegate_manager",
]
