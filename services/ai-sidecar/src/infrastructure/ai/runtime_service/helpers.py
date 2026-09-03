"""Helper functions for the AI runtime service.

Contains validation, prompt building, evidence/reasoning assembly, modality
inference, attachment handling, and SSE event sanitization helpers.
"""

from __future__ import annotations

import re
from collections.abc import Iterator
from datetime import UTC, datetime
from typing import Any

from src.infrastructure.ai.context_envelope import ContextEnvelope
from src.infrastructure.ai.evidence_metadata import (
    compute_freshness_seconds,
    generate_object_id,
)
from src.infrastructure.ai.intent_router import IntentCategory
from src.infrastructure.ai.structured_output import (
    AiStructuredOutput,
    OutputEvidence,
    OutputProposal,
    ReasoningStep,
)
from src.infrastructure.ai.templates import get_task_template
from src.infrastructure.ai.tools.read_only_tools import is_read_only_tool

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


def _iter_answer_chunks(answer: str, *, max_chunks: int = 8) -> Iterator[str]:
    """Split heuristic answer into SSE token frames (no provider)."""
    if not answer:
        return
    chunk_size = max(1, (len(answer) + max_chunks - 1) // max_chunks)
    for i in range(0, len(answer), chunk_size):
        yield answer[i : i + chunk_size]


def validate_envelope(envelope: ContextEnvelope) -> list[str]:
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


def build_system_prompt(envelope: ContextEnvelope) -> str:
    """Build system prompt from envelope context.

    Relies on OpenAI server-side prompt caching for repeated prefixes.
    When the envelope's ``task_type`` has a registered task template
    (Tasks A4–A6), the template policy block is appended to the base prompt.
    """
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


def build_evidence(envelope: ContextEnvelope) -> list[OutputEvidence]:
    """Build evidence chain with P1-1-A metadata (as_of, freshness)."""

    now = datetime.now(UTC)

    evidence: list[OutputEvidence] = []
    for item in envelope.context.evidence:
        # Compute freshness if as_of provided
        as_of = getattr(item, "as_of", None)
        freshness_seconds = None
        if as_of is not None:
            try:  # noqa: SIM105 - freshness is best-effort for malformed evidence
                freshness_seconds = compute_freshness_seconds(as_of, now)
            except Exception:  # noqa: BLE001
                pass

        evidence.append(
            OutputEvidence(
                object_type=item.object_type,
                object_id=item.object_id,
                source=item.source,
                field=None,
                as_of=str(as_of) if as_of else None,
                freshness_seconds=freshness_seconds,
            )
        )
    if not evidence and envelope.context.objects:
        for obj in envelope.context.objects:
            # Generate timestamped object ID
            obj_id = generate_object_id(object_type=obj.object_type, identifier=obj.object_id, timestamp=now)
            evidence.append(
                OutputEvidence(
                    object_type=obj.object_type,
                    object_id=obj_id,
                    source=f"ai_query.v_{obj.object_type.lower()}",
                    field=None,
                    as_of=now.isoformat(),
                    freshness_seconds=0,
                )
            )
    if not evidence:
        evidence.append(
            OutputEvidence(
                object_type="system",
                object_id="runtime",
                source="ai_runtime.context_envelope",
                field=None,
                as_of=now.isoformat(),
                freshness_seconds=0,
            )
        )
    return evidence


def build_reasoning_steps(envelope: ContextEnvelope, intent: str) -> list[ReasoningStep]:
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


def build_proposals_from_envelope(envelope: ContextEnvelope, intent: str) -> list[OutputProposal]:
    """Conservative proposal generation — only when message clearly requests a note."""
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
        note_content = _extract_note_content(envelope.task.user_message)
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


def _extract_note_content(user_message: str) -> str:
    text = (user_message or "").strip()
    if not text:
        return "AI runtime note"
    match = re.search(r"(?:备注|note)[:：\s]+(.+)$", text, re.IGNORECASE)
    if match:
        return match.group(1).strip()[:500]
    return text[:500]


def heuristic_answer(envelope: ContextEnvelope, intent: str) -> str:
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


def structured_output_to_response_dict(output: AiStructuredOutput) -> dict[str, Any]:
    token_usage: dict[str, int] | None = None
    if output.token_usage is not None:
        token_usage = {
            "prompt_tokens": output.token_usage.prompt_tokens,
            "completion_tokens": output.token_usage.completion_tokens,
            "total_tokens": output.token_usage.total_tokens,
        }

    payload: dict[str, Any] = {
        "contract_version": output.contract_version,
        "run_id": output.run_id,
        "status": output.status,
        "answer": output.answer,
        "reasoning_steps": [{"step": r.step, "summary": r.summary} for r in output.reasoning_steps],
        "evidence": [
            {
                "object_type": e.object_type,
                "object_id": e.object_id,
                "source": e.source,
                **({"field": e.field} if e.field else {}),
            }
            for e in output.evidence
        ],
        "proposals": [
            {
                "object_type": p.object_type,
                "object_id": p.object_id,
                "action_name": p.action_name,
                "arguments": p.arguments,
                "risk_level": p.risk_level,
                "confidence": p.confidence,
                "reasoning": p.reasoning,
                "requires_approval": p.requires_approval,
                **({"proposal_id": p.proposal_id} if p.proposal_id else {}),
            }
            for p in output.proposals
        ],
        "limitations": list(output.limitations),
        "token_usage": token_usage,
    }

    degraded = bool(
        output.limitations or (output.metrics and output.metrics.model and output.metrics.model.startswith("heuristic"))
    )
    if degraded:
        payload["degraded"] = True

    if output.metrics:
        payload["metrics"] = {
            "model": output.metrics.model,
            "duration_ms": output.metrics.duration_ms,
        }
    else:
        payload["metrics"] = None

    return payload


# Canonical modality order
_MODALITY_ORDER = ["text", "image", "audio", "video", "file"]

_MIME_TO_MODALITY = {
    "image/": "image",
    "audio/": "audio",
    "video/": "video",
}

# Audio MIME → container format accepted by the OpenAI input_audio block.
_AUDIO_MIME_TO_FORMAT = {
    "audio/wav": "wav",
    "audio/x-wav": "wav",
    "audio/wave": "wav",
    "audio/mpeg": "mp3",
    "audio/mp3": "mp3",
}


def _audio_format_from_mime(mime: str) -> str:
    """Map an audio MIME type to an OpenAI input_audio container format.

    Defaults to ``wav`` (the gate's default-allowed audio type) when the subtype
    is unrecognized.
    """
    return _AUDIO_MIME_TO_FORMAT.get((mime or "").lower(), "wav")


def _infer_input_modalities(envelope: ContextEnvelope) -> list[str]:
    """Infer input modalities from a ContextEnvelope's attachments/media/files.

    Returns a sorted list of modality strings following canonical order:
    text, image, audio, video, file.
    """
    modalities: set = {"text"}

    # Check envelope.context for attachments or files
    ctx = getattr(envelope, "context", None)
    if ctx:
        # Check attachments
        attachments = getattr(ctx, "attachments", None) or []
        for att in attachments:
            mime = ""
            if isinstance(att, dict):
                mime = att.get("mime_type", "") or att.get("mimeType", "") or ""
            else:
                mime = getattr(att, "mime_type", "") or getattr(att, "mimeType", "") or ""
            modality = _mime_to_modality(mime)
            if modality:
                modalities.add(modality)

        # Check files
        files = getattr(ctx, "files", None) or []
        for f in files:
            mime = ""
            if isinstance(f, dict):
                mime = f.get("mime_type", "") or f.get("mimeType", "") or ""
            else:
                mime = getattr(f, "mime_type", "") or getattr(f, "mimeType", "") or ""
            modality = _mime_to_modality(mime)
            if modality:
                modalities.add(modality)

        # Check media
        media = getattr(ctx, "media", None) or []
        for m in media:
            mime = ""
            if isinstance(m, dict):
                mime = m.get("mime_type", "") or m.get("mimeType", "") or ""
            else:
                mime = getattr(m, "mime_type", "") or getattr(m, "mimeType", "") or ""
            modality = _mime_to_modality(mime)
            if modality:
                modalities.add(modality)

    # Check top-level attachments on envelope
    envelope_attachments = getattr(envelope, "attachments", None) or []
    for att in envelope_attachments:
        mime = ""
        if isinstance(att, dict):
            mime = att.get("mime_type", "") or att.get("mimeType", "") or ""
        else:
            mime = getattr(att, "mime_type", "") or getattr(att, "mimeType", "") or ""
        modality = _mime_to_modality(mime)
        if modality:
            modalities.add(modality)

    return sorted(modalities, key=lambda m: _MODALITY_ORDER.index(m) if m in _MODALITY_ORDER else 99)


def _mime_to_modality(mime: str) -> str | None:
    """Map a MIME type to a modality string."""
    if not mime:
        return None
    mime_lower = mime.lower()
    for prefix, modality in _MIME_TO_MODALITY.items():
        if mime_lower.startswith(prefix):
            return modality
    # Generic file for non-text, non-image/audio/video
    if not mime_lower.startswith("text/"):
        return "file"
    return None


def _attachment_field(att: Any, *names: str) -> Any:
    """Read the first present field from an attachment (dict or object form).

    Attachments arrive in two shapes across callers — plain dicts (JSON envelopes)
    and lightweight objects — so every accessor must tolerate both, mirroring the
    style used by :func:`_infer_input_modalities`.
    """
    for name in names:
        if isinstance(att, dict):
            value = att.get(name)
        else:
            value = getattr(att, name, None)
        if value is not None and value != "":
            return value
    return None


def _iter_envelope_attachments(envelope: ContextEnvelope) -> Iterator[Any]:
    """Yield every attachment-like item carried by an envelope.

    Covers ``envelope.context.{attachments,files,media}`` and top-level
    ``envelope.attachments`` — the same surfaces inspected by
    :func:`_infer_input_modalities` — so the input gate sees exactly what the
    modality inference saw.
    """
    ctx = getattr(envelope, "context", None)
    if ctx:
        for bucket in ("attachments", "files", "media"):
            for item in getattr(ctx, bucket, None) or []:
                yield item
    for item in getattr(envelope, "attachments", None) or []:
        yield item


def _attachment_size_bytes(att: Any) -> int:
    """Best-effort byte size of an attachment.

    Prefers an explicit ``size``/``size_bytes`` field; otherwise estimates from the
    base64 payload length (3 bytes per 4 base64 chars, ignoring padding).
    """
    explicit = _attachment_field(att, "size_bytes", "size", "bytes")
    if explicit is not None:
        try:
            return int(explicit)
        except (TypeError, ValueError):
            pass
    data_b64 = _attachment_field(att, "data", "data_b64", "base64")
    if isinstance(data_b64, str) and data_b64:
        cleaned = data_b64.split(",", 1)[-1].strip()
        padding = cleaned.count("=")
        return max(0, (len(cleaned) * 3) // 4 - padding)
    return 0


def _sse_event(event: str, data: dict[str, Any]) -> dict[str, Any]:
    return {"event": event, "data": data}


def _sanitize_tool_call_event(tool_call: dict[str, Any]) -> dict[str, Any]:
    """Sanitize tool.call event to include only non-sensitive metadata."""
    return {
        "tool_call_id": tool_call.get("tool_call_id", ""),
        "tool_name": tool_call.get("tool_name", ""),
        "tool_type": "read_only" if is_read_only_tool(tool_call.get("tool_name", "")) else "write_action",
    }


def _sanitize_tool_result_event(payload: dict[str, Any]) -> dict[str, Any]:
    """Sanitize tool.result event to include only non-sensitive metadata."""
    result_status = "succeeded" if "result" in payload else "failed"
    proposal_count = 0
    rejected_count = 0
    if (
        "result" in payload
        and isinstance(payload["result"], dict)
        and payload["result"].get("status") == "proposal_created"
    ):
        result_status = "proposal_created"
        proposal_count = 1
    return {
        "tool_call_id": payload.get("tool_call_id", ""),
        "tool_name": payload.get("tool_name", ""),
        "tool_type": "read_only" if is_read_only_tool(payload.get("tool_name", "")) else "write_action",
        "result_status": result_status,
        "proposal_count": proposal_count,
        "rejected_count": rejected_count,
    }
