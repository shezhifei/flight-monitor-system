"""Read-only LangGraph orchestration adapter for AI Runtime (P2.1).

This module provides a **read-only** LangGraph-based orchestration layer that:
- Validates envelope, classifies intent, enriches context (read-only), assembles prompt,
  generates model answer, and assembles structured output.
- Does NOT execute tools, write to databases, call external business APIs, or mutate state.
- When LangGraph is unavailable or graph execution fails, falls back gracefully to
  the existing linear runtime path.

Design constraints:
- No tool execution (no DomainActionExecutor, no AIP action nodes).
- No database writes (no agent_execution_repository, no checkpointer writes).
- No mutation of Rust-controlled state (no proposal execution, no run status changes).
- user_message is kept in memory only — never written to logs.

State contract:
- run_id, intent, envelope_summary, enrichment_evidence, limitations,
  answer, proposals, token_usage.
- Does NOT store API keys, full JWTs, or complete service identity tokens.
"""

from __future__ import annotations

import os
import re
import time
from dataclasses import dataclass, field
from typing import Any

from src.infrastructure.ai.context_envelope import ContextEnvelope
from src.infrastructure.ai.intent_router import IntentCategory, classify_intent
from src.infrastructure.ai.runtime_context import enhance_context
from src.infrastructure.ai.runtime_llm import (
    LlmClient,
    LlmUnavailableError,
    OpenAiLlmClient,
    sanitize_provider_error,
)
from src.infrastructure.ai.structured_output import (
    AiStructuredOutput,
    OutputEvidence,
    OutputMetrics,
    OutputProposal,
    ReasoningStep,
    TokenUsage,
)
from src.infrastructure.ai.templates import get_task_template

_LANGGRAPH_AVAILABLE: bool = False
try:
    from langgraph.graph import END, StateGraph
    from typing_extensions import TypedDict

    _LANGGRAPH_AVAILABLE = True
except ImportError:
    TypedDict = dict  # type: ignore[assignment]  # fallback when langgraph is absent
    END = "__end__"
    StateGraph = type  # type: ignore[assignment]  # fallback when langgraph is absent

_INTENT_LABELS: dict[str, str] = {
    IntentCategory.QUERY_FLIGHT: "航班查询",
    IntentCategory.QUERY_ANOMALY: "异常/告警查询",
    IntentCategory.QUERY_STATS: "统计查询",
    IntentCategory.DISPATCH_OPS: "调度操作",
    IntentCategory.TODO_MGMT: "待办管理",
    IntentCategory.REPORT: "报告生成",
    IntentCategory.ADVISOR: "处置建议",
    IntentCategory.BUSINESS_CASE: "业务事项",
    IntentCategory.QUERY_TEAM: "班组查询",
    IntentCategory.QUERY_EQUIPMENT: "设备查询",
    IntentCategory.QUERY_STAND: "机位查询",
    IntentCategory.QUERY_DISPATCH: "派工查询",
    IntentCategory.GENERAL: "通用",
}


# ── Graph state contract (read-only, no secrets) ──────────────────────────

if _LANGGRAPH_AVAILABLE:

    class RuntimeGraphState(TypedDict):
        run_id: str
        intent: str
        envelope_summary: str
        enrichment_evidence: list[OutputEvidence]
        limitations: list[str]
        user_message: str
        answer: str
        proposals: list[OutputProposal]
        reasoning_steps: list[ReasoningStep]
        prompt_tokens: int
        completion_tokens: int
        total_tokens: int
        model_name: str
        provider_available: bool

else:

    class RuntimeGraphState(dict):  # type: ignore[no-redef]  # fallback when langgraph is absent
        pass


# ── Graph result contract ────────────────────────────────────────────────


@dataclass
class RuntimeGraphResult:
    answer: str
    reasoning_steps: list[ReasoningStep]
    evidence: list[OutputEvidence]
    proposals: list[OutputProposal]
    limitations: list[str]
    model_name: str
    token_usage: TokenUsage
    duration_ms: int


@dataclass
class StreamingGraphContext:
    """Prepared graph state for streaming — returned by run_streaming().

    Holds everything from the validate→enrich→assemble_prompt stages
    so the caller can feed the system_prompt + user_message directly
    into a StreamingLlmClient without waiting for the full answer.
    """

    intent: str
    system_prompt: str
    user_message: str
    reasoning_steps: list[ReasoningStep] = field(default_factory=list)
    evidence: list[OutputEvidence] = field(default_factory=list)
    proposals: list[OutputProposal] = field(default_factory=list)
    limitations: list[str] = field(default_factory=list)
    run_id: str = ""
    _started: float = 0.0

    def build_heuristic_result(self, answer: str) -> RuntimeGraphResult:
        return RuntimeGraphResult(
            answer=answer,
            reasoning_steps=list(self.reasoning_steps),
            evidence=list(self.evidence),
            proposals=list(self.proposals),
            limitations=list(self.limitations),
            model_name="heuristic-runtime-v1",
            token_usage=TokenUsage(),
            duration_ms=_elapsed_ms(self._started),
        )

    def build_streamed_result(
        self,
        answer: str,
        model_name: str,
        token_usage: TokenUsage,
        extra_steps: list[ReasoningStep] | None = None,
    ) -> RuntimeGraphResult:
        steps = list(self.reasoning_steps)
        if extra_steps:
            steps.extend(extra_steps)
        return RuntimeGraphResult(
            answer=answer,
            reasoning_steps=steps,
            evidence=list(self.evidence),
            proposals=list(self.proposals),
            limitations=list(self.limitations),
            model_name=model_name,
            token_usage=token_usage,
            duration_ms=_elapsed_ms(self._started),
        )


# ── Reusable utility functions (mirrored from runtime_service to avoid circular import) ──


def _graph_validate_envelope(envelope: ContextEnvelope) -> list[str]:
    errors: list[str] = []
    if not (envelope.job_id or "").strip():
        errors.append("job_id is required")
    if not (envelope.run_id or "").strip():
        errors.append("run_id is required")
    if not (envelope.requester.user_id or "").strip():
        errors.append("requester.user_id is required")
    if not (envelope.task.task_type or "").strip():
        errors.append("task.task_type is required")
    return errors


def _graph_build_system_prompt(envelope: ContextEnvelope) -> str:
    # Mirror of runtime_service.helpers.build_system_prompt (kept in sync;
    # separate copy to avoid a circular import).
    object_summary = ", ".join(f"{obj.object_type}:{obj.object_id}" for obj in envelope.context.objects[:10]) or "none"
    allowed = ", ".join(envelope.ontology.allowed_actions[:20]) or "none"
    prompt = (
        "You are the flight operations AI runtime assistant. "
        "Answer in Chinese unless the user writes in another language. "
        "Be concise; cite context objects when relevant. "
        "Do not claim to have executed actions — only propose actions via the Rust control plane.\n"
        f"Task type: {envelope.task.task_type}\n"
        f"Context objects: {object_summary}\n"
        f"Allowed actions: {allowed}\n"
        f"Risk ceiling: {envelope.ontology.risk_ceiling}"
    )
    template = get_task_template(envelope.task.task_type)
    if template is not None:
        prompt = f"{prompt}\n\n{template.system_prompt_addendum}"
    return prompt


def _graph_build_evidence(envelope: ContextEnvelope) -> list[OutputEvidence]:
    evidence: list[OutputEvidence] = []
    for item in envelope.context.evidence:
        evidence.append(
            OutputEvidence(
                object_type=item.object_type,
                object_id=item.object_id,
                source=item.source,
                field=None,
            )
        )
    if not evidence and envelope.context.objects:
        for obj in envelope.context.objects:
            evidence.append(
                OutputEvidence(
                    object_type=obj.object_type,
                    object_id=obj.object_id,
                    source=f"ai_query.v_{obj.object_type.lower()}",
                    field=None,
                )
            )
    if not evidence:
        evidence.append(
            OutputEvidence(
                object_type="system",
                object_id="runtime",
                source="ai_runtime.context_envelope",
                field=None,
            )
        )
    return evidence


def _graph_build_reasoning_steps(envelope: ContextEnvelope, intent: str) -> list[ReasoningStep]:
    intent_label = _INTENT_LABELS.get(intent, intent)
    return [
        ReasoningStep(
            step="validate_envelope",
            summary=(
                f"Validated job_id={envelope.job_id}, run_id={envelope.run_id}, user={envelope.requester.user_id}"
            ),
        ),
        ReasoningStep(
            step="classify_intent",
            summary=f"Classified intent as {intent} ({intent_label})",
        ),
        ReasoningStep(
            step="assemble_context",
            summary=(
                f"Loaded {len(envelope.context.objects)} object(s), "
                f"{len(envelope.ontology.allowed_actions)} allowed action(s)"
            ),
        ),
    ]


def _graph_build_proposals(envelope: ContextEnvelope, intent: str) -> list[OutputProposal]:
    message = (envelope.task.user_message or "").lower()
    wants_note = any(token in message for token in ("note", "备注", "annotate", "注释"))
    if not wants_note:
        return []

    proposals: list[OutputProposal] = []
    for obj in envelope.context.objects:
        if obj.object_type != "Flight":
            continue
        action_key = "Flight.add_note"
        if action_key not in envelope.ontology.allowed_actions:
            continue
        text = (envelope.task.user_message or "").strip()
        match = re.search(r"(?:备注|note)[:：\s]+(.+)$", text, re.IGNORECASE)
        note_content = match.group(1).strip()[:500] if match else text[:500]
        proposals.append(
            OutputProposal(
                object_type="Flight",
                object_id=obj.object_id,
                action_name="add_note",
                arguments={"note_content": note_content},
                risk_level="low",
                confidence=0.85,
                reasoning="User message requests adding a note to the flight",
                requires_approval=False,
            )
        )
    return proposals


def _graph_heuristic_answer(envelope: ContextEnvelope, intent: str) -> str:
    user_message = (envelope.task.user_message or "").strip()
    if not user_message:
        return "我已收到您的请求。请说明需要查询或处理的航班/保障对象。"

    object_types = [obj.object_type for obj in envelope.context.objects]
    intent_label = _INTENT_LABELS.get(intent, intent)

    if "Flight" in object_types:
        flight_ids = [obj.object_id for obj in envelope.context.objects if obj.object_type == "Flight"]
        return (
            f"（启发式运行时）已按「{intent_label}」理解您的请求：{user_message}。"
            f" 当前上下文包含航班 {', '.join(flight_ids)}。"
            " 如需执行写操作，将通过 Rust proposal ingest 流程提交审批。"
        )

    return (
        f"（启发式运行时）已按「{intent_label}」理解您的请求：{user_message}。"
        f" 可访问对象类型：{', '.join(object_types) if object_types else '通用航班运行数据'}。"
    )


# ── Read-only graph nodes ────────────────────────────────────────────────


def _node_validate(state: RuntimeGraphState) -> RuntimeGraphState:
    state["reasoning_steps"] = list(state.get("reasoning_steps") or [])
    state["limitations"] = list(state.get("limitations") or [])
    state["enrichment_evidence"] = list(state.get("enrichment_evidence") or [])
    state["proposals"] = list(state.get("proposals") or [])
    return state


def _node_enrich(state: RuntimeGraphState, envelope: ContextEnvelope) -> RuntimeGraphState:
    intent = state.get("intent", "general")
    reasoning = _graph_build_reasoning_steps(envelope, intent)
    evidence = _graph_build_evidence(envelope)
    proposals = _graph_build_proposals(envelope, intent)

    enrichment = enhance_context(envelope)
    reasoning.extend(enrichment.reasoning_steps)
    evidence.extend(enrichment.evidence)
    limitations = list(state.get("limitations", []))
    limitations.extend(enrichment.limitations)

    state["reasoning_steps"] = reasoning
    state["enrichment_evidence"] = evidence
    state["proposals"] = proposals
    state["limitations"] = limitations
    return state


def _node_assemble_prompt(state: RuntimeGraphState, envelope: ContextEnvelope) -> RuntimeGraphState:
    state["envelope_summary"] = _graph_build_system_prompt(envelope)
    return state


def _node_generate_answer(
    state: RuntimeGraphState,
    envelope: ContextEnvelope,
    llm: LlmClient | None,
) -> RuntimeGraphState:
    intent = state.get("intent", "general")

    if llm is not None:
        try:
            completion = llm.complete(
                state.get("envelope_summary", ""),
                state.get("user_message", ""),
            )
            state["answer"] = completion.content
            state["model_name"] = completion.model
            state["prompt_tokens"] = completion.prompt_tokens
            state["completion_tokens"] = completion.completion_tokens
            state["total_tokens"] = completion.prompt_tokens + completion.completion_tokens
            state["provider_available"] = True
            state["reasoning_steps"] = [
                *list(state.get("reasoning_steps", [])),
                ReasoningStep(step="llm_complete", summary=f"Generated answer via {completion.model}"),
            ]
            return state
        except (LlmUnavailableError, Exception):  # noqa: BLE001 - LLM fallback to heuristic must catch all
            limitations = list(state.get("limitations", []))
            limitations.append("LLM configured but unavailable; fell back to heuristic")
            state["limitations"] = limitations

    state["answer"] = _graph_heuristic_answer(envelope, intent)
    state["model_name"] = "heuristic-runtime-v1"
    state["prompt_tokens"] = 0
    state["completion_tokens"] = 0
    state["total_tokens"] = 0
    state["provider_available"] = False
    if llm is None:
        limitations = list(state.get("limitations", []))
        if not any("LLM" in x for x in limitations):
            limitations.append("LLM not configured (set OPENAI_API_KEY for full model-backed answers)")
        state["limitations"] = limitations
    state["reasoning_steps"] = [
        *list(state.get("reasoning_steps", [])),
        ReasoningStep(
            step="heuristic_only", summary="No LLM configured or provider unavailable; produced heuristic answer"
        ),
    ]
    return state


def _node_assemble_output(state: RuntimeGraphState) -> RuntimeGraphState:
    return state


# ── Graph builder ────────────────────────────────────────────────────────


def _build_readonly_graph() -> Any:
    if not _LANGGRAPH_AVAILABLE:
        raise RuntimeError("langgraph is not installed")

    workflow = StateGraph(RuntimeGraphState)

    workflow.add_node("validate", lambda s: s)
    workflow.add_node("enrich", lambda s: s)
    workflow.add_node("assemble_prompt", lambda s: s)
    workflow.add_node("generate_answer", lambda s: s)
    workflow.add_node("assemble_output", lambda s: s)

    workflow.set_entry_point("validate")
    workflow.add_edge("validate", "enrich")
    workflow.add_edge("enrich", "assemble_prompt")
    workflow.add_edge("assemble_prompt", "generate_answer")
    workflow.add_edge("generate_answer", "assemble_output")
    workflow.add_edge("assemble_output", END)

    return workflow.compile()


# ── RuntimeGraphRunner ───────────────────────────────────────────────────


@dataclass
class RuntimeGraphRunner:
    _llm_override: LlmClient | None = None

    @staticmethod
    def is_langgraph_available() -> bool:
        return _LANGGRAPH_AVAILABLE

    @staticmethod
    def is_enabled() -> bool:
        return os.getenv("AI_RUNTIME_USE_LANGGRAPH", "0").strip() in ("1", "true", "yes")

    def run(
        self,
        envelope: ContextEnvelope,
        llm: LlmClient | None = None,
    ) -> tuple[RuntimeGraphResult, str | None]:
        started = time.monotonic()
        if not _LANGGRAPH_AVAILABLE:
            result = self._linear_fallback(envelope, llm, started)
            return result, "langgraph not installed; used linear fallback"

        if not self.is_enabled():
            result = self._linear_fallback(envelope, llm, started)
            return result, "AI_RUNTIME_USE_LANGGRAPH not enabled; used linear fallback"

        try:
            return self._execute_graph(envelope, llm, started)
        except Exception as exc:  # noqa: BLE001 - graph execution fallback must catch all
            sanitized = sanitize_provider_error(exc)
            result = self._linear_fallback(envelope, llm, started)
            return result, f"graph execution failed ({sanitized}); used linear fallback"

    def run_streaming(
        self,
        envelope: ContextEnvelope,
    ) -> tuple[StreamingGraphContext, str | None]:
        """Execute validate→enrich→assemble_prompt eagerly, return context for streaming.

        Returns (StreamingGraphContext, optional fallback reason).
        If graph setup fails, returns a context with heuristic-level info
        plus a fallback reason string.
        """
        started = time.monotonic()
        intent = classify_intent(envelope.task.user_message)

        if not _LANGGRAPH_AVAILABLE or not self.is_enabled():
            reason = "langgraph not installed" if not _LANGGRAPH_AVAILABLE else "AI_RUNTIME_USE_LANGGRAPH not enabled"
            return self._build_streaming_context(
                envelope,
                intent,
                started,
            ), reason

        try:
            return self._prepare_graph_streaming(envelope, intent, started)
        except Exception as exc:  # noqa: BLE001 - graph setup fallback must catch all
            sanitized = sanitize_provider_error(exc)
            ctx = self._build_streaming_context(envelope, intent, started)
            ctx.limitations.append(f"graph setup failed ({sanitized}); fell back")
            return ctx, f"graph setup failed ({sanitized})"

    def _prepare_graph_streaming(
        self,
        envelope: ContextEnvelope,
        intent: str,
        started: float,
    ) -> tuple[StreamingGraphContext, None]:
        """Run the non-LLM graph stages and return a streaming context."""
        state: RuntimeGraphState = {
            "run_id": envelope.run_id or "",
            "intent": intent,
            "envelope_summary": "",
            "enrichment_evidence": [],
            "limitations": [],
            "user_message": envelope.task.user_message,
            "answer": "",
            "proposals": [],
            "reasoning_steps": [],
            "prompt_tokens": 0,
            "completion_tokens": 0,
            "total_tokens": 0,
            "model_name": "",
            "provider_available": False,
        }

        state = _node_validate(state)

        validation_errors = _graph_validate_envelope(envelope)
        if validation_errors:
            ctx = StreamingGraphContext(
                intent=intent,
                system_prompt="",
                user_message=envelope.task.user_message,
                limitations=validation_errors,
                run_id=state.get("run_id", ""),
                _started=started,
            )
            return ctx, None

        state = _node_enrich(state, envelope)
        state = _node_assemble_prompt(state, envelope)

        ctx = StreamingGraphContext(
            intent=intent,
            system_prompt=state.get("envelope_summary", ""),
            user_message=envelope.task.user_message,
            reasoning_steps=list(state.get("reasoning_steps", [])),
            evidence=list(state.get("enrichment_evidence", [])),
            proposals=list(state.get("proposals", [])),
            limitations=list(state.get("limitations", [])),
            run_id=state.get("run_id", ""),
            _started=started,
        )
        return ctx, None

    @staticmethod
    def _build_streaming_context(
        envelope: ContextEnvelope,
        intent: str,
        started: float,
    ) -> StreamingGraphContext:
        """Build a StreamingGraphContext using the linear (non-graph) path."""
        reasoning_steps = _graph_build_reasoning_steps(envelope, intent)
        evidence = _graph_build_evidence(envelope)
        proposals = _graph_build_proposals(envelope, intent)
        enrichment = enhance_context(envelope)
        reasoning_steps.extend(enrichment.reasoning_steps)
        evidence.extend(enrichment.evidence)
        limitations = list(enrichment.limitations)

        return StreamingGraphContext(
            intent=intent,
            system_prompt=_graph_build_system_prompt(envelope),
            user_message=envelope.task.user_message,
            reasoning_steps=reasoning_steps,
            evidence=evidence,
            proposals=proposals,
            limitations=limitations,
            run_id=(envelope.run_id or "").strip() or "graph_fallback",
            _started=started,
        )

    def _execute_graph(
        self,
        envelope: ContextEnvelope,
        llm: LlmClient | None,
        started: float,
    ) -> tuple[RuntimeGraphResult, None]:
        resolved_llm = llm or self._resolve_llm()
        intent = classify_intent(envelope.task.user_message)

        state: RuntimeGraphState = {
            "run_id": envelope.run_id or "",
            "intent": intent,
            "envelope_summary": "",
            "enrichment_evidence": [],
            "limitations": [],
            "user_message": envelope.task.user_message,
            "answer": "",
            "proposals": [],
            "reasoning_steps": [],
            "prompt_tokens": 0,
            "completion_tokens": 0,
            "total_tokens": 0,
            "model_name": "",
            "provider_available": False,
        }

        state = _node_validate(state)

        validation_errors = _graph_validate_envelope(envelope)
        if validation_errors:
            output = _build_result_from_state(
                state=state,
                envelope=envelope,
                started=started,
                answer="; ".join(validation_errors),
                model_name="runtime-validator",
                token_usage=TokenUsage(),
            )
            return output, None

        state = _node_enrich(state, envelope)
        state = _node_assemble_prompt(state, envelope)
        state = _node_generate_answer(state, envelope, resolved_llm)
        state = _node_assemble_output(state)

        output = _build_result_from_state(
            state=state,
            envelope=envelope,
            started=started,
            answer=state.get("answer", ""),
            model_name=state.get("model_name", "heuristic-runtime-v1"),
            token_usage=TokenUsage(
                prompt_tokens=state.get("prompt_tokens", 0),
                completion_tokens=state.get("completion_tokens", 0),
                total_tokens=state.get("total_tokens", 0),
            ),
        )
        return output, None

    def _linear_fallback(
        self,
        envelope: ContextEnvelope,
        llm: LlmClient | None,
        started: float,
    ) -> RuntimeGraphResult:
        resolved_llm = llm or self._resolve_llm()
        intent = classify_intent(envelope.task.user_message)
        reasoning_steps = _graph_build_reasoning_steps(envelope, intent)
        evidence = _graph_build_evidence(envelope)
        proposals = _graph_build_proposals(envelope, intent)
        enrichment = enhance_context(envelope)
        reasoning_steps.extend(enrichment.reasoning_steps)
        evidence.extend(enrichment.evidence)
        limitations = list(enrichment.limitations)

        if resolved_llm is not None:
            try:
                completion = resolved_llm.complete(
                    _graph_build_system_prompt(envelope),
                    envelope.task.user_message,
                )
                reasoning_steps.append(
                    ReasoningStep(
                        step="llm_complete",
                        summary=f"Generated answer via {completion.model}",
                    )
                )
                return RuntimeGraphResult(
                    answer=completion.content,
                    reasoning_steps=reasoning_steps,
                    evidence=evidence,
                    proposals=proposals,
                    limitations=limitations,
                    model_name=completion.model,
                    token_usage=TokenUsage(
                        prompt_tokens=completion.prompt_tokens,
                        completion_tokens=completion.completion_tokens,
                        total_tokens=completion.prompt_tokens + completion.completion_tokens,
                    ),
                    duration_ms=_elapsed_ms(started),
                )
            except (LlmUnavailableError, Exception) as exc:  # noqa: BLE001 - LLM fallback must catch all
                limitations.append(sanitize_provider_error(exc) if isinstance(exc, Exception) else str(exc))

        if not limitations:
            limitations.append("LLM not configured (set OPENAI_API_KEY for full model-backed answers)")
        reasoning_steps.append(
            ReasoningStep(
                step="heuristic_only",
                summary="No LLM configured or provider unavailable; produced heuristic answer",
            )
        )
        return RuntimeGraphResult(
            answer=_graph_heuristic_answer(envelope, intent),
            reasoning_steps=reasoning_steps,
            evidence=evidence,
            proposals=proposals,
            limitations=limitations,
            model_name="heuristic-runtime-v1",
            token_usage=TokenUsage(),
            duration_ms=_elapsed_ms(started),
        )

    @staticmethod
    def _resolve_llm() -> LlmClient | None:
        openai_client = OpenAiLlmClient()
        return openai_client if openai_client.is_configured() else None


def _build_result_from_state(
    *,
    state: RuntimeGraphState,
    envelope: ContextEnvelope,
    started: float,
    answer: str,
    model_name: str,
    token_usage: TokenUsage,
) -> RuntimeGraphResult:
    evidence = list(state.get("enrichment_evidence", []))
    if not evidence:
        evidence = _graph_build_evidence(envelope)

    return RuntimeGraphResult(
        answer=answer,
        reasoning_steps=list(state.get("reasoning_steps", [])),
        evidence=evidence,
        proposals=list(state.get("proposals", [])),
        limitations=list(state.get("limitations", [])),
        model_name=model_name,
        token_usage=token_usage,
        duration_ms=_elapsed_ms(started),
    )


def _elapsed_ms(started: float) -> int:
    return max(1, int((time.monotonic() - started) * 1000))


def graph_result_to_output(
    result: RuntimeGraphResult,
    envelope: ContextEnvelope,
    *,
    graph_error: str | None = None,
) -> AiStructuredOutput:
    limitations = list(result.limitations)
    if graph_error:
        limitations.append(f"graph orchestration: {graph_error}")

    evidence = list(result.evidence)
    if not evidence:
        evidence = _graph_build_evidence(envelope)

    return AiStructuredOutput(
        contract_version="ai-structured-output.v1",
        run_id=(envelope.run_id or "").strip() or "graph_fallback",
        status="succeeded",
        answer=result.answer,
        reasoning_steps=result.reasoning_steps,
        evidence=evidence,
        proposals=result.proposals,
        limitations=limitations,
        metrics=OutputMetrics(model=result.model_name, duration_ms=result.duration_ms),
        token_usage=result.token_usage,
    )


__all__ = [
    "RuntimeGraphResult",
    "RuntimeGraphRunner",
    "RuntimeGraphState",
    "StreamingGraphContext",
    "graph_result_to_output",
]
