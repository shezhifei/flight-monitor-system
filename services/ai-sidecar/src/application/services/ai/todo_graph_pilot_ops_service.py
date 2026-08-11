"""Operational services for the Todo graph pilot."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timedelta
from typing import Any
from zoneinfo import ZoneInfo

from src.application.services.ai.feature_flags import is_ai_feature_enabled
from src.application.services.ai.todo_graph_pilot_metrics import build_todo_graph_pilot_snapshot
from src.application.services.anomaly.alert_service import AlertChannel, AlertLevel
from src.domain.ai.todo_graph_pilot import (
    DEFAULT_TODO_GRAPH_PILOT_ALERT_DEDUPE_SECONDS,
    DEFAULT_TODO_GRAPH_PILOT_DAILY_REVIEW_HOUR,
    DEFAULT_TODO_GRAPH_PILOT_DAILY_REVIEW_MINUTE,
    DEFAULT_TODO_GRAPH_PILOT_ENTITY_ID,
    DEFAULT_TODO_GRAPH_PILOT_PENDING_STALE_AFTER_MINUTES,
    DEFAULT_TODO_GRAPH_PILOT_REVIEW_INTERVAL_SECONDS,
    DEFAULT_TODO_GRAPH_PILOT_ROLLBACK_VERIFY_DELAY_MINUTES,
    DEFAULT_TODO_GRAPH_PILOT_TIMEZONE,
    DEFAULT_TODO_GRAPH_PILOT_WINDOW_HOURS,
    TODO_GRAPH_PILOT_RUNBOOK_REF,
    is_todo_graph_pilot_rollout_enabled,
)
from src.domain.utils.time_utils import utc_now
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


@dataclass
class TodoGraphPilotSnapshotBundle:
    snapshot: dict[str, Any]
    executions: list[dict[str, Any]]
    steps_by_run_id: dict[str, list[dict[str, Any]]]
    pending_actions: list[dict[str, Any]]
    pending_total: int
    window_started_at: datetime
    window_ended_at: datetime


@dataclass
class _AlertState:
    last_sent_at: datetime
    payload: dict[str, Any]


@dataclass
class _RollbackVerificationState:
    started_at: datetime
    verify_after: datetime
    baseline_graph_actual_total: int
    baseline_stale_pending_total: int
    baseline_duplicate_total: int


class TodoGraphPilotSnapshotService:
    """Collects a pilot snapshot from the current application facts."""

    def __init__(
        self,
        *,
        agent_service: Any,
        tool_registry: Any,
    ) -> None:
        self._agent_service = agent_service
        self._tool_registry = tool_registry

    @staticmethod
    def _is_graph_requested_execution(execution: dict[str, Any]) -> bool:
        runtime = execution.get("runtime") if isinstance(execution.get("runtime"), dict) else {}
        requested_path = str(runtime.get("requested_path") or "").strip().lower()
        if requested_path:
            return requested_path == "graph"
        metadata = execution.get("metadata") if isinstance(execution.get("metadata"), dict) else {}
        return str(metadata.get("runtime_path_requested") or "").strip().lower() == "graph"

    async def list_executions(
        self,
        *,
        entity_id: str | None,
        started_after: datetime | None,
        limit: int,
    ) -> list[dict[str, Any]]:
        return await self._agent_service.get_entity_executions(
            entity_id,
            None,
            limit,
            started_after=started_after,
        )

    async def collect_snapshot(
        self,
        *,
        entity_id: str | None = DEFAULT_TODO_GRAPH_PILOT_ENTITY_ID,
        window_hours: int = DEFAULT_TODO_GRAPH_PILOT_WINDOW_HOURS,
        sample_limit: int = 200,
        pending_stale_after_minutes: int = DEFAULT_TODO_GRAPH_PILOT_PENDING_STALE_AFTER_MINUTES,
        window_ended_at: datetime | None = None,
    ) -> TodoGraphPilotSnapshotBundle:
        resolved_window_ended_at = window_ended_at or utc_now()
        resolved_window_started_at = resolved_window_ended_at - timedelta(
            hours=max(1, int(window_hours or DEFAULT_TODO_GRAPH_PILOT_WINDOW_HOURS))
        )
        normalized_entity_id = str(entity_id or "").strip() or None

        executions = await self.list_executions(
            entity_id=normalized_entity_id,
            started_after=resolved_window_started_at,
            limit=sample_limit,
        )
        graph_requested_run_ids = [
            str(execution.get("run_id") or "").strip()
            for execution in executions
            if isinstance(execution, dict)
            and str(execution.get("run_id") or "").strip()
            and self._is_graph_requested_execution(execution)
        ]

        steps_by_run_id: dict[str, list[dict[str, Any]]] = {}
        get_execution_steps_batch = getattr(self._agent_service, "get_execution_steps_batch", None)
        if callable(get_execution_steps_batch):
            try:
                steps_by_run_id = await get_execution_steps_batch(graph_requested_run_ids)
            except Exception as exc:  # noqa: BLE001 - agent service batch fetch must not break snapshot
                logger.warning("failed to load execution steps batch for todo graph pilot snapshot: %s", exc)
        else:
            get_execution_steps = getattr(self._agent_service, "get_execution_steps", None)
            if callable(get_execution_steps):
                for run_id in graph_requested_run_ids:
                    try:
                        steps_by_run_id[run_id] = await get_execution_steps(run_id)
                    except Exception as exc:  # noqa: BLE001 - per-run step fetch must not break snapshot
                        logger.warning(
                            "failed to load execution steps for todo graph pilot snapshot run_id=%s: %s",
                            run_id,
                            exc,
                        )

        pending_total = int(
            await self._tool_registry.count_pending_actions(
                status="pending",
                entity_id=normalized_entity_id,
            )
            or 0
        )
        pending_fetch_limit = min(max(int(sample_limit), pending_total), 1000)
        recent_actions = list(
            await self._tool_registry.list_pending_actions(
                entity_id=normalized_entity_id,
                created_after=resolved_window_started_at,
                limit=sample_limit,
                offset=0,
            )
            or []
        )
        pending_actions = (
            list(
                await self._tool_registry.list_pending_actions(
                    status="pending",
                    entity_id=normalized_entity_id,
                    limit=pending_fetch_limit,
                    offset=0,
                )
                or []
            )
            if pending_fetch_limit > 0
            else []
        )

        actions_by_id: dict[str, dict[str, Any]] = {}
        for item in recent_actions + pending_actions:
            if not isinstance(item, dict):
                continue
            action_id = str(item.get("action_id") or "").strip()
            if not action_id:
                continue
            actions_by_id[action_id] = item

        snapshot = build_todo_graph_pilot_snapshot(
            executions=executions,
            steps_by_run_id=steps_by_run_id,
            pending_actions=list(actions_by_id.values()),
            pending_total=pending_total,
            pending_stale_after_minutes=pending_stale_after_minutes,
            entity_id=normalized_entity_id,
            window_hours=window_hours,
            window_started_at=resolved_window_started_at,
            window_ended_at=resolved_window_ended_at,
        )

        return TodoGraphPilotSnapshotBundle(
            snapshot=snapshot,
            executions=executions,
            steps_by_run_id=steps_by_run_id,
            pending_actions=list(actions_by_id.values()),
            pending_total=pending_total,
            window_started_at=resolved_window_started_at,
            window_ended_at=resolved_window_ended_at,
        )


class TodoGraphPilotOpsService:
    """Runs the pilot watcher, digest generation, and rollback verification."""

    def __init__(
        self,
        *,
        snapshot_service: TodoGraphPilotSnapshotService,
        config_store: Any,
        alert_service: Any,
        sse_hub: Any,
        config_manager: Any = None,
        entity_id: str = DEFAULT_TODO_GRAPH_PILOT_ENTITY_ID,
        window_hours: int = DEFAULT_TODO_GRAPH_PILOT_WINDOW_HOURS,
        pending_stale_after_minutes: int = DEFAULT_TODO_GRAPH_PILOT_PENDING_STALE_AFTER_MINUTES,
        dedupe_seconds: int = DEFAULT_TODO_GRAPH_PILOT_ALERT_DEDUPE_SECONDS,
        rollback_verify_delay_minutes: int = DEFAULT_TODO_GRAPH_PILOT_ROLLBACK_VERIFY_DELAY_MINUTES,
        review_hour: int = DEFAULT_TODO_GRAPH_PILOT_DAILY_REVIEW_HOUR,
        review_minute: int = DEFAULT_TODO_GRAPH_PILOT_DAILY_REVIEW_MINUTE,
        review_timezone: str = DEFAULT_TODO_GRAPH_PILOT_TIMEZONE,
    ) -> None:
        self._snapshot_service = snapshot_service
        self._config_store = config_store
        self._alert_service = alert_service
        self._sse_hub = sse_hub
        self._config_manager = config_manager
        self._entity_id = entity_id
        self._window_hours = max(1, int(window_hours or DEFAULT_TODO_GRAPH_PILOT_WINDOW_HOURS))
        self._pending_stale_after_minutes = max(
            1,
            int(pending_stale_after_minutes or DEFAULT_TODO_GRAPH_PILOT_PENDING_STALE_AFTER_MINUTES),
        )
        self._dedupe_seconds = max(60, int(dedupe_seconds or DEFAULT_TODO_GRAPH_PILOT_ALERT_DEDUPE_SECONDS))
        self._rollback_verify_delay_minutes = max(
            1,
            int(rollback_verify_delay_minutes or DEFAULT_TODO_GRAPH_PILOT_ROLLBACK_VERIFY_DELAY_MINUTES),
        )
        self._review_hour = max(0, min(23, int(review_hour or DEFAULT_TODO_GRAPH_PILOT_DAILY_REVIEW_HOUR)))
        self._review_minute = max(0, min(59, int(review_minute or DEFAULT_TODO_GRAPH_PILOT_DAILY_REVIEW_MINUTE)))
        self._review_timezone = review_timezone or DEFAULT_TODO_GRAPH_PILOT_TIMEZONE
        self._active_alerts: dict[str, _AlertState] = {}
        self._rollback_verification: _RollbackVerificationState | None = None
        self._previous_rollout_enabled: bool | None = None
        self._last_daily_review_key: str | None = None
        self._rollback_alerts_since_last_review = 0
        self._consecutive_ready_reviews = 0

    async def build_snapshot(
        self,
        *,
        entity_id: str | None = None,
        window_hours: int | None = None,
        sample_limit: int = 200,
        pending_stale_after_minutes: int | None = None,
        window_ended_at: datetime | None = None,
    ) -> dict[str, Any]:
        """Build the externally exposed pilot snapshot from the shared fact source."""
        bundle = await self._snapshot_service.collect_snapshot(
            entity_id=entity_id,
            window_hours=window_hours or self._window_hours,
            sample_limit=sample_limit,
            pending_stale_after_minutes=(pending_stale_after_minutes or self._pending_stale_after_minutes),
            window_ended_at=window_ended_at,
        )
        return bundle.snapshot

    def resolve_review_interval_seconds(self) -> int:
        manager = self._config_manager
        if manager is None:
            return DEFAULT_TODO_GRAPH_PILOT_REVIEW_INTERVAL_SECONDS
        try:
            return max(
                60,
                int(
                    manager.get_int(
                        "ai.todo_graph_pilot.review_interval_seconds",
                        DEFAULT_TODO_GRAPH_PILOT_REVIEW_INTERVAL_SECONDS,
                    )
                ),
            )
        except Exception as exc:  # noqa: BLE001 - config read may fail in various ways
            logger.warning("review_interval_seconds config read failed; using default: %s", exc)
            return DEFAULT_TODO_GRAPH_PILOT_REVIEW_INTERVAL_SECONDS

    async def run_review(self, *, now: datetime | None = None) -> dict[str, Any]:
        resolved_now = now or utc_now()
        bundle = await self._snapshot_service.collect_snapshot(
            entity_id=self._entity_id,
            window_hours=self._window_hours,
            pending_stale_after_minutes=self._pending_stale_after_minutes,
            window_ended_at=resolved_now,
        )
        snapshot = bundle.snapshot
        entity_config = await self._config_store.get(self._entity_id)
        entity_rollout_enabled = is_todo_graph_pilot_rollout_enabled(entity_config)
        global_rollout_enabled = is_ai_feature_enabled(
            "AI_TODO_AGENT_GRAPH_V1",
            config_manager=self._config_manager,
            default=False,
        )

        alerts_sent = await self._process_operational_alerts(
            snapshot=snapshot,
            now=resolved_now,
            entity_rollout_enabled=entity_rollout_enabled,
            global_rollout_enabled=global_rollout_enabled,
        )
        recoveries_sent = await self._resolve_recovered_alerts(
            snapshot=snapshot,
            now=resolved_now,
            entity_rollout_enabled=entity_rollout_enabled,
            global_rollout_enabled=global_rollout_enabled,
        )
        rollback_status = await self._process_rollback_verification(
            bundle=bundle,
            now=resolved_now,
            entity_rollout_enabled=entity_rollout_enabled,
            global_rollout_enabled=global_rollout_enabled,
        )
        daily_summary = await self._maybe_emit_daily_review(
            bundle=bundle,
            now=resolved_now,
            entity_rollout_enabled=entity_rollout_enabled,
            global_rollout_enabled=global_rollout_enabled,
        )

        self._previous_rollout_enabled = entity_rollout_enabled
        return {
            "entity_id": self._entity_id,
            "snapshot": snapshot,
            "alerts_sent": alerts_sent,
            "recoveries_sent": recoveries_sent,
            "rollback_status": rollback_status,
            "daily_summary": daily_summary,
        }

    def _build_alert_payloads(
        self,
        *,
        snapshot: dict[str, Any],
        entity_rollout_enabled: bool,
        global_rollout_enabled: bool,
    ) -> list[dict[str, Any]]:
        verdict = snapshot.get("verdict") if isinstance(snapshot.get("verdict"), dict) else {}
        approvals = snapshot.get("approvals") if isinstance(snapshot.get("approvals"), dict) else {}
        guardrails = snapshot.get("guardrails") if isinstance(snapshot.get("guardrails"), dict) else {}
        window = snapshot.get("window") if isinstance(snapshot.get("window"), dict) else {}

        base_payload = {
            "entity_id": self._entity_id,
            "window_hours": snapshot.get("scope", {}).get("window_hours", self._window_hours),
            "pending_stale_after_minutes": window.get(
                "pending_stale_after_minutes",
                self._pending_stale_after_minutes,
            ),
            "runbook_ref": TODO_GRAPH_PILOT_RUNBOOK_REF,
            "entity_rollout_enabled": entity_rollout_enabled,
            "global_rollout_enabled": global_rollout_enabled,
        }

        alerts: list[dict[str, Any]] = []
        if verdict.get("status") == "rollback_recommended":
            reasons = list(verdict.get("reasons") or [])
            alerts.append(
                {
                    **base_payload,
                    "alert_key": f"rollback_recommended:{'|'.join(reasons)}",
                    "alert_type": "rollback_recommended",
                    "event": "todo_graph_pilot_alert",
                    "title": "Todo graph pilot rollback recommended",
                    "message": (
                        "Pilot verdict switched to rollback_recommended; disable entity rollout first "
                        "and escalate to the global flag only if the issue escapes pilot scope."
                    ),
                    "level": AlertLevel.ERROR,
                    "verdict": verdict,
                    "reasons": reasons,
                    "recommended_action": "disable_entity_rollout",
                }
            )

        stale_pending_total = int(approvals.get("stale_pending_total") or 0)
        if stale_pending_total > 0:
            alerts.append(
                {
                    **base_payload,
                    "alert_key": "stale_pending_total",
                    "alert_type": "stale_pending_total",
                    "event": "todo_graph_pilot_alert",
                    "title": "Todo graph pilot stale approvals detected",
                    "message": (
                        f"Pilot approvals breached the {base_payload['pending_stale_after_minutes']}-minute SLA "
                        f"with {stale_pending_total} stale pending actions."
                    ),
                    "level": AlertLevel.WARNING,
                    "stale_pending_total": stale_pending_total,
                    "recommended_action": "clear_pending_approvals",
                }
            )

        duplicate_total = int(guardrails.get("duplicate_tool_execution_total") or 0)
        if duplicate_total > 0:
            alerts.append(
                {
                    **base_payload,
                    "alert_key": "duplicate_tool_execution_total",
                    "alert_type": "duplicate_tool_execution_total",
                    "event": "todo_graph_pilot_alert",
                    "title": "Todo graph pilot duplicate tool execution detected",
                    "message": (
                        f"Pilot guardrails recorded {duplicate_total} duplicate tool executions; "
                        "stop expansion and verify rollback readiness immediately."
                    ),
                    "level": AlertLevel.CRITICAL,
                    "duplicate_tool_execution_total": duplicate_total,
                    "recommended_action": "stop_and_investigate_duplicates",
                }
            )

        return alerts

    async def _process_operational_alerts(
        self,
        *,
        snapshot: dict[str, Any],
        now: datetime,
        entity_rollout_enabled: bool,
        global_rollout_enabled: bool,
    ) -> list[dict[str, Any]]:
        sent: list[dict[str, Any]] = []
        for payload in self._build_alert_payloads(
            snapshot=snapshot,
            entity_rollout_enabled=entity_rollout_enabled,
            global_rollout_enabled=global_rollout_enabled,
        ):
            state = self._active_alerts.get(payload["alert_key"])
            should_send = state is None or (now - state.last_sent_at).total_seconds() >= self._dedupe_seconds
            if not should_send:
                continue
            await self._emit_alert(payload)
            self._active_alerts[payload["alert_key"]] = _AlertState(last_sent_at=now, payload=payload)
            if payload["alert_type"] == "rollback_recommended":
                self._rollback_alerts_since_last_review += 1
            sent.append(payload)
        return sent

    async def _resolve_recovered_alerts(
        self,
        *,
        snapshot: dict[str, Any],
        now: datetime,
        entity_rollout_enabled: bool,
        global_rollout_enabled: bool,
    ) -> list[dict[str, Any]]:
        current_keys = {
            payload["alert_key"]
            for payload in self._build_alert_payloads(
                snapshot=snapshot,
                entity_rollout_enabled=entity_rollout_enabled,
                global_rollout_enabled=global_rollout_enabled,
            )
        }
        recovered: list[dict[str, Any]] = []
        for alert_key in list(self._active_alerts.keys()):
            if alert_key in current_keys:
                continue
            state = self._active_alerts.pop(alert_key)
            payload = dict(state.payload)
            payload["event"] = "todo_graph_pilot_alert_recovered"
            payload["title"] = f"{payload['title']} recovered"
            payload["message"] = f"{payload['alert_type']} is no longer active for the pilot entity."
            await self._emit_alert(payload, is_recovery=True)
            recovered.append(payload)
        return recovered

    async def _process_rollback_verification(
        self,
        *,
        bundle: TodoGraphPilotSnapshotBundle,
        now: datetime,
        entity_rollout_enabled: bool,
        global_rollout_enabled: bool,
    ) -> dict[str, Any] | None:
        snapshot = bundle.snapshot
        executions = snapshot.get("executions") if isinstance(snapshot.get("executions"), dict) else {}
        approvals = snapshot.get("approvals") if isinstance(snapshot.get("approvals"), dict) else {}
        guardrails = snapshot.get("guardrails") if isinstance(snapshot.get("guardrails"), dict) else {}

        if self._previous_rollout_enabled is True and not entity_rollout_enabled:
            self._rollback_verification = _RollbackVerificationState(
                started_at=now,
                verify_after=now + timedelta(minutes=self._rollback_verify_delay_minutes),
                baseline_graph_actual_total=int(executions.get("graph_actual_total") or 0),
                baseline_stale_pending_total=int(approvals.get("stale_pending_total") or 0),
                baseline_duplicate_total=int(guardrails.get("duplicate_tool_execution_total") or 0),
            )

        if entity_rollout_enabled:
            self._rollback_verification = None
            return None

        if self._rollback_verification is None or now < self._rollback_verification.verify_after:
            return None

        post_rollback_executions = await self._snapshot_service.list_executions(
            entity_id=self._entity_id,
            started_after=self._rollback_verification.started_at,
            limit=200,
        )
        new_graph_total = sum(
            1
            for execution in post_rollback_executions
            if str((execution.get("runtime") or {}).get("path") or "").strip().lower() == "graph"
        )
        new_legacy_total = sum(
            1
            for execution in post_rollback_executions
            if str((execution.get("runtime") or {}).get("path") or "").strip().lower() == "legacy"
        )

        baseline = self._rollback_verification
        verification_payload = {
            "event": "todo_graph_pilot_rollback_verified",
            "entity_id": self._entity_id,
            "window_hours": self._window_hours,
            "runbook_ref": TODO_GRAPH_PILOT_RUNBOOK_REF,
            "entity_rollout_enabled": entity_rollout_enabled,
            "global_rollout_enabled": global_rollout_enabled,
            "verify_after_minutes": self._rollback_verify_delay_minutes,
            "started_at": baseline.started_at.isoformat(),
            "graph_actual_total_baseline": baseline.baseline_graph_actual_total,
            "graph_actual_total_current": int(executions.get("graph_actual_total") or 0),
            "stale_pending_total_baseline": baseline.baseline_stale_pending_total,
            "stale_pending_total_current": int(approvals.get("stale_pending_total") or 0),
            "duplicate_tool_execution_total_baseline": baseline.baseline_duplicate_total,
            "duplicate_tool_execution_total_current": int(guardrails.get("duplicate_tool_execution_total") or 0),
            "new_graph_total": new_graph_total,
            "new_legacy_total": new_legacy_total,
        }
        success = (
            new_graph_total == 0
            and int(executions.get("graph_actual_total") or 0) <= baseline.baseline_graph_actual_total
            and int(approvals.get("stale_pending_total") or 0) <= baseline.baseline_stale_pending_total
            and int(guardrails.get("duplicate_tool_execution_total") or 0) <= baseline.baseline_duplicate_total
        )

        if success:
            verification_payload.update(
                {
                    "title": "Todo graph pilot rollback verified",
                    "message": "Pilot rollback held for 15 minutes and new pilot executions stayed on legacy.",
                    "level": AlertLevel.INFO.value,
                }
            )
            await self._emit_system_alert_event(verification_payload)
        else:
            verification_payload.update(
                {
                    "event": "todo_graph_pilot_rollback_verification_failed",
                    "title": "Todo graph pilot rollback verification failed",
                    "message": (
                        "Pilot rollback did not hold cleanly; verify entity rollout fields, then consider "
                        "the global AI_TODO_AGENT_GRAPH_V1 kill switch."
                    ),
                    "level": AlertLevel.ERROR.value,
                    "recommended_action": "consider_global_kill_switch",
                }
            )
            await self._emit_alert(
                {
                    **verification_payload,
                    "level": AlertLevel.ERROR,
                    "alert_type": "rollback_verification_failed",
                    "alert_key": "rollback_verification_failed",
                }
            )

        self._rollback_verification = None
        return verification_payload

    async def _maybe_emit_daily_review(
        self,
        *,
        bundle: TodoGraphPilotSnapshotBundle,
        now: datetime,
        entity_rollout_enabled: bool,
        global_rollout_enabled: bool,
    ) -> dict[str, Any] | None:
        local_now = now.astimezone(ZoneInfo(self._review_timezone))
        review_key = local_now.date().isoformat()
        review_due = (local_now.hour, local_now.minute) >= (self._review_hour, self._review_minute)
        if not review_due or self._last_daily_review_key == review_key:
            return None

        snapshot = bundle.snapshot
        verdict = snapshot.get("verdict") if isinstance(snapshot.get("verdict"), dict) else {}
        ready_without_rollbacks = (
            verdict.get("status") == "ready_to_expand" and self._rollback_alerts_since_last_review == 0
        )
        if ready_without_rollbacks:
            self._consecutive_ready_reviews += 1
        else:
            self._consecutive_ready_reviews = 0

        executions = snapshot.get("executions") if isinstance(snapshot.get("executions"), dict) else {}
        approvals = snapshot.get("approvals") if isinstance(snapshot.get("approvals"), dict) else {}
        guardrails = snapshot.get("guardrails") if isinstance(snapshot.get("guardrails"), dict) else {}
        value_metrics = snapshot.get("value_metrics") if isinstance(snapshot.get("value_metrics"), dict) else {}

        summary = {
            "event": "todo_graph_pilot_daily_review",
            "entity_id": self._entity_id,
            "window_hours": self._window_hours,
            "review_timezone": self._review_timezone,
            "review_key": review_key,
            "runbook_ref": TODO_GRAPH_PILOT_RUNBOOK_REF,
            "entity_rollout_enabled": entity_rollout_enabled,
            "global_rollout_enabled": global_rollout_enabled,
            "verdict": verdict,
            "reasons": list(verdict.get("reasons") or []),
            "graph_requested_total": int(executions.get("graph_requested_total") or 0),
            "completion_rate": float(executions.get("completion_rate") or 0.0),
            "graph_fallback_rate": float(executions.get("graph_fallback_rate") or 0.0),
            "graph_resume_success_rate": float(approvals.get("graph_resume_success_rate") or 0.0),
            "stale_pending_total": int(approvals.get("stale_pending_total") or 0),
            "duplicate_tool_execution_total": int(guardrails.get("duplicate_tool_execution_total") or 0),
            "top_fallback_reasons": list(executions.get("top_fallback_reasons") or []),
            "value_metrics": value_metrics,
            "expansion_gate": {
                "consecutive_ready_reviews": self._consecutive_ready_reviews,
                "rollback_recommended_alerts_since_last_review": self._rollback_alerts_since_last_review,
                "eligible_to_expand": self._consecutive_ready_reviews >= 2 and ready_without_rollbacks,
            },
        }

        await self._emit_system_alert_event(summary)
        if self._alert_service is not None and callable(getattr(self._alert_service, "send_alert_async", None)):
            await self._alert_service.send_alert_async(
                title="Todo graph pilot daily review",
                message=(
                    f"Daily review emitted for {self._entity_id}: verdict={verdict.get('status') or 'n/a'}, "
                    f"graph_requested_total={summary['graph_requested_total']}."
                ),
                level=AlertLevel.INFO,
                channels=[AlertChannel.LOG],
                metadata=summary,
            )

        self._last_daily_review_key = review_key
        self._rollback_alerts_since_last_review = 0
        return summary

    async def _emit_alert(self, payload: dict[str, Any], *, is_recovery: bool = False) -> None:
        await self._emit_system_alert_event(
            {
                **payload,
                "level": getattr(payload.get("level"), "value", payload.get("level")),
                "is_recovery": bool(is_recovery),
            }
        )
        if self._alert_service is None:
            return
        send_alert_async = getattr(self._alert_service, "send_alert_async", None)
        if not callable(send_alert_async):
            return
        await send_alert_async(
            title=str(payload.get("title") or "Todo graph pilot alert"),
            message=str(payload.get("message") or "todo graph pilot operational alert"),
            level=payload.get("level") if isinstance(payload.get("level"), AlertLevel) else AlertLevel.WARNING,
            channels=[AlertChannel.LOG],
            metadata={
                key: getattr(value, "value", value) for key, value in payload.items() if key not in {"title", "message"}
            }
            | {"is_recovery": bool(is_recovery)},
        )

    async def _emit_system_alert_event(self, payload: dict[str, Any]) -> None:
        if self._sse_hub is None:
            return
        try:
            await self._sse_hub.broadcast_to_topic("system_alerts", payload)
        except Exception as exc:  # noqa: BLE001 - SSE broadcast must not break alert pipeline
            logger.warning("failed to broadcast todo graph pilot system alert: %s", exc)


__all__ = [
    "TodoGraphPilotOpsService",
    "TodoGraphPilotSnapshotBundle",
    "TodoGraphPilotSnapshotService",
]
