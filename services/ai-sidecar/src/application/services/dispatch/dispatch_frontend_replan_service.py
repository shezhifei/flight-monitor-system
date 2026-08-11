"""Frontend joint-replan snapshot/build/apply service."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
from typing import Any

from src.domain.exceptions.base import BusinessRuleException
from src.domain.utils.time_utils import utc_now
from src.shared.id_generator import generate_id

from .dispatch_frontend_replan_collaborators import (
    DispatchFrontendReplanApplyCoordinator,
    DispatchFrontendReplanCollaborationCoordinator,
    DispatchFrontendReplanNotificationCoordinator,
    DispatchFrontendReplanSnapshotBuilder,
    _empty_notification_summary,
    _iso,
)


@dataclass(frozen=True)
class ZeroTravelTimeProvider:
    """Placeholder travel-time provider; currently all edges cost 0."""

    def minutes(self, *_args: Any, **_kwargs: Any) -> int:
        return 0


class DispatchFrontendReplanService:
    SNAPSHOT_TTL_SECONDS = 300
    MODEL_VERSION = "dispatch_wasm_pdf_full_model_v2"
    SOLVER_VERSION = "dispatch_solver_ortools_wasm_strict_pdf_v3"
    MAX_CANDIDATE_USERS = 8
    MAX_CANDIDATE_TEAMS = 8
    MAX_CANDIDATE_EQUIPMENTS = 8

    def __init__(
        self,
        *,
        order_repo: Any,
        order_member_repo: Any | None = None,
        cache_service: Any,
        travel_time_provider: Any | None = None,
        team_type_repo: Any | None = None,
        team_repo: Any | None = None,
        equipment_repo: Any | None = None,
        team_member_repo: Any | None = None,
        notification_service: Any | None = None,
        collaboration_recorder: Any | None = None,
        travel_stats_repo: Any | None = None,
        shift_instance_repo: Any | None = None,
        schedule_exception_repo: Any | None = None,
    ) -> None:
        self._cache_service = cache_service
        self._travel_stats_repo = travel_stats_repo
        effective_travel_time_provider = travel_time_provider or ZeroTravelTimeProvider()
        self._snapshot_builder = DispatchFrontendReplanSnapshotBuilder(
            order_repo=order_repo,
            travel_time_provider=effective_travel_time_provider,
            team_type_repo=team_type_repo,
            team_repo=team_repo,
            equipment_repo=equipment_repo,
            travel_stats_repo=travel_stats_repo,
            shift_instance_repo=shift_instance_repo,
            schedule_exception_repo=schedule_exception_repo,
            max_candidate_users=self.MAX_CANDIDATE_USERS,
            max_candidate_teams=self.MAX_CANDIDATE_TEAMS,
            max_candidate_equipments=self.MAX_CANDIDATE_EQUIPMENTS,
        )
        self._apply_coordinator = DispatchFrontendReplanApplyCoordinator(
            order_repo=order_repo,
            order_member_repo=order_member_repo,
        )
        self._notification_coordinator = DispatchFrontendReplanNotificationCoordinator(
            notification_service=notification_service,
            team_member_repo=team_member_repo,
        )
        self._collaboration_coordinator = DispatchFrontendReplanCollaborationCoordinator(
            collaboration_recorder=collaboration_recorder,
        )

    async def build_snapshot(
        self,
        *,
        window_start: datetime,
        window_end: datetime,
        strategy: str,
        max_suggestions: int,
        include_cancelled: bool = False,
    ) -> dict[str, Any]:
        artifacts = await self._snapshot_builder.build(
            window_start=window_start,
            window_end=window_end,
            include_cancelled=include_cancelled,
        )

        snapshot_id = generate_id()
        payload = {
            "snapshot_id": snapshot_id,
            "model_version": self.MODEL_VERSION,
            "solver_version": self.SOLVER_VERSION,
            "generated_at": _iso(utc_now()),
            "window_start": _iso(window_start),
            "window_end": _iso(window_end),
            "strategy": strategy,
            "max_suggestions": max(1, int(max_suggestions or 20)),
            "travel_time_mode": "historical_matrix" if self._travel_stats_repo is not None else "zero_matrix_forbidden",
            "objective_config": {
                "staged_lexicographic": True,
                "objective_priority": [
                    "minimize_slot_gap",
                    "minimize_sla_lateness",
                    "minimize_turnaround_break",
                    "minimize_personnel_baseline_change",
                    "minimize_travel_cost",
                    "minimize_scarcity_cost",
                    "minimize_employee_load_deviation",
                ],
                "objective_stage_keys": [
                    "slot_gap",
                    "sla_lateness",
                    "continuity_break",
                    "personnel_baseline_change",
                    "travel_cost",
                    "scarcity_cost",
                    "employee_load_deviation",
                ],
                "timeout_ms": 10000,
                "travel_time_mode": "historical_matrix"
                if self._travel_stats_repo is not None
                else "zero_matrix_forbidden",
                "average_workload_target": artifacts.average_workload_target,
            },
            "unsupported_features": list(artifacts.unsupported_features),
            "optimizable_orders": artifacts.optimizable_orders,
            "fixed_anchor_orders": artifacts.fixed_anchor_orders,
            "orders": artifacts.optimizable_orders,
            "fixed_orders": artifacts.fixed_anchor_orders,
            "employee_anchor_states": artifacts.employee_anchor_states,
            "equipment_anchor_states": artifacts.equipment_anchor_states,
            "employee_free_windows": artifacts.employee_free_windows,
            "equipment_free_windows": artifacts.equipment_free_windows,
            "employee_unavailable_blocks": artifacts.employee_unavailable_blocks,
            "equipment_unavailable_blocks": artifacts.equipment_unavailable_blocks,
            "resource_travel_edges": artifacts.resource_travel_edges,
            "turnaround_pairs": artifacts.turnaround_pairs,
            "impact_summary": self._build_snapshot_impact_summary(artifacts.optimizable_orders),
            "changed_orders": [],
            "risk_level": self._build_snapshot_risk_level(artifacts.optimizable_orders),
            "requires_manual_confirmation": any(
                str(item.get("conflict_state") or "").strip() in {"resource_conflict", "gap"}
                or bool(((item.get("baseline_assignment") or {}).get("qualification_gap")) or [])
                for item in artifacts.optimizable_orders
            ),
        }
        self._cache_service.set(
            self._cache_key(snapshot_id),
            payload,
            ttl=self.SNAPSHOT_TTL_SECONDS,
        )
        return payload

    async def apply_snapshot(
        self,
        *,
        snapshot_id: str,
        solver_version: str,
        strategy: str,
        order_results: list[dict[str, Any]] | None = None,
        personnel_slot_assignments: list[dict[str, Any]] | None = None,
        equipment_slot_assignments: list[dict[str, Any]] | None = None,
        continuity_decisions: list[dict[str, Any]] | None = None,
        objective_breakdown: dict[str, Any] | None = None,
        solver_run_metadata: dict[str, Any] | None = None,
        suggestions: list[dict[str, Any]] | None = None,
        solver_metadata: dict[str, Any] | None = None,
        actor_id: str | None = None,
        actor_name: str | None = None,
    ) -> dict[str, Any]:
        normalized_order_results = list(order_results or [])
        if not normalized_order_results and suggestions:
            normalized_order_results = list(suggestions or [])
        normalized_personnel_slot_assignments = list(personnel_slot_assignments or [])
        normalized_equipment_slot_assignments = list(equipment_slot_assignments or [])
        normalized_continuity_decisions = list(continuity_decisions or [])
        normalized_solver_run_metadata = solver_run_metadata or solver_metadata or {}
        payload = self._cache_service.get(self._cache_key(snapshot_id))
        if not isinstance(payload, dict):
            raise BusinessRuleException(message="重排快照已过期，请重新预览")
        if str(payload.get("solver_version") or "") != str(solver_version or ""):
            raise BusinessRuleException(message="求解器版本不匹配，请重新预览")

        snapshot_orders = {
            str(item.get("order_id") or ""): item
            for item in [
                *(payload.get("optimizable_orders") or []),
                *(payload.get("fixed_anchor_orders") or []),
                *(payload.get("orders") or []),
                *(payload.get("fixed_orders") or []),
            ]
            if str(item.get("order_id") or "").strip()
        }
        if not normalized_order_results:
            return {
                "snapshot_id": snapshot_id,
                "applied": False,
                "order_results": [],
                "personnel_slot_assignments": [],
                "equipment_slot_assignments": [],
                "continuity_decisions": [],
                "objective_breakdown": objective_breakdown or {},
                "solver_run_metadata": normalized_solver_run_metadata,
                "solver_metadata": normalized_solver_run_metadata,
                "notification_summary": self._empty_notification_summary(),
                "message": "无可应用的重排建议",
            }

        apply_result = await self._apply_coordinator.apply(
            snapshot_id=snapshot_id,
            solver_version=solver_version,
            strategy=strategy,
            order_results=normalized_order_results,
            personnel_slot_assignments=normalized_personnel_slot_assignments,
            equipment_slot_assignments=normalized_equipment_slot_assignments,
            continuity_decisions=normalized_continuity_decisions,
            snapshot_orders=snapshot_orders,
            objective_breakdown=objective_breakdown,
            solver_run_metadata=normalized_solver_run_metadata,
            actor_id=actor_id,
        )
        notification_summary = await self._notification_coordinator.send(
            suggestions=apply_result.order_results,
            snapshot_orders=snapshot_orders,
            actor_id=actor_id,
            actor_name=actor_name,
        )
        await self._collaboration_coordinator.record(
            snapshot_id=snapshot_id,
            suggestions=apply_result.order_results,
            snapshot_orders=snapshot_orders,
            notification_summary=notification_summary,
            actor_id=actor_id,
            actor_name=actor_name,
        )

        return {
            "snapshot_id": snapshot_id,
            "applied": True,
            "order_results": apply_result.order_results,
            "personnel_slot_assignments": apply_result.personnel_slot_assignments,
            "equipment_slot_assignments": apply_result.equipment_slot_assignments,
            "continuity_decisions": apply_result.continuity_decisions,
            "objective_breakdown": apply_result.objective_breakdown,
            "solver_run_metadata": normalized_solver_run_metadata,
            "solver_metadata": normalized_solver_run_metadata,
            "suggestions": apply_result.order_results,
            "notification_summary": notification_summary,
            "impact_summary": self._build_apply_impact_summary(apply_result.order_results, snapshot_orders),
            "changed_orders": [
                item.get("dispatch_order_id") for item in apply_result.order_results if item.get("dispatch_order_id")
            ],
            "risk_level": self._build_apply_risk_level(apply_result.order_results),
            "requires_manual_confirmation": any(
                bool(item.get("requires_manual_confirmation"))
                or str(item.get("suggestion_type") or "")
                in {"assigned_conflict_resolution", "unassigned_late_assignment"}
                for item in apply_result.order_results
            ),
            "message": f"已应用重排（{len(apply_result.order_results)}条）",
        }

    @staticmethod
    def _empty_notification_summary() -> dict[str, Any]:
        return _empty_notification_summary()

    @staticmethod
    def _cache_key(snapshot_id: str) -> str:
        return f"dispatch:frontend_replan_snapshot:{snapshot_id}"

    @staticmethod
    def _build_snapshot_impact_summary(orders: list[dict[str, Any]]) -> dict[str, Any]:
        affected_flights = {
            str(item.get("flight_id") or "").strip() for item in orders if str(item.get("flight_id") or "").strip()
        }
        conflict_orders = [
            item for item in orders if str(item.get("conflict_state") or "").strip() == "resource_conflict"
        ]
        unassigned_orders = [item for item in orders if str(item.get("conflict_state") or "").strip() == "gap"]
        return {
            "affected_flights": len(affected_flights),
            "changed_orders": 0,
            "reassigned_orders": len(conflict_orders),
            "delayed_orders": len(unassigned_orders),
            "added_delay_minutes": 0.0,
            "replaced_member_count": 0,
            "qualification_gap_count": sum(
                len((item.get("baseline_assignment") or {}).get("qualification_gap") or []) for item in orders
            ),
        }

    @staticmethod
    def _build_snapshot_risk_level(orders: list[dict[str, Any]]) -> str:
        conflict_count = sum(
            1 for item in orders if str(item.get("conflict_state") or "").strip() in {"resource_conflict", "gap"}
        )
        if conflict_count >= 8:
            return "critical"
        if conflict_count >= 4:
            return "high"
        if conflict_count >= 1:
            return "medium"
        return "low"

    @staticmethod
    def _build_apply_impact_summary(
        suggestions: list[dict[str, Any]],
        snapshot_orders: dict[str, dict[str, Any]],
    ) -> dict[str, Any]:
        affected_flights = set()
        delayed_orders = 0
        reassigned_orders = 0
        added_delay_minutes = 0.0
        replaced_member_count = 0
        qualification_gap_count = 0
        for item in suggestions:
            order_id = str(item.get("dispatch_order_id") or "").strip()
            snapshot_order = snapshot_orders.get(order_id) or {}
            flight_id = str(snapshot_order.get("flight_id") or "").strip()
            if flight_id:
                affected_flights.add(flight_id)
            if (item.get("current_assignment") or {}) != (item.get("suggested_assignment") or {}):
                reassigned_orders += 1
            replaced_member_count += int((item.get("member_change_summary") or {}).get("changed_member_count") or 0)
            qualification_gap_count += len(item.get("qualification_gap") or [])
            original_start = item.get("original_start_time")
            suggested_start = item.get("suggested_start_time")
            if original_start and suggested_start and suggested_start > original_start:
                delayed_orders += 1
            added_delay_minutes += float(item.get("lateness_minutes") or 0.0)
        return {
            "affected_flights": len(affected_flights),
            "changed_orders": len(suggestions),
            "reassigned_orders": reassigned_orders,
            "delayed_orders": delayed_orders,
            "added_delay_minutes": round(added_delay_minutes, 2),
            "replaced_member_count": replaced_member_count,
            "qualification_gap_count": qualification_gap_count,
        }

    @staticmethod
    def _build_apply_risk_level(suggestions: list[dict[str, Any]]) -> str:
        if any(item.get("qualification_gap") for item in suggestions):
            return "critical"
        if any(bool(item.get("requires_manual_confirmation")) for item in suggestions):
            return "high"
        if any(float(item.get("impact_score") or 0.0) >= 30 for item in suggestions):
            return "critical"
        if any(float(item.get("impact_score") or 0.0) >= 15 for item in suggestions):
            return "high"
        if suggestions:
            return "medium"
        return "low"
