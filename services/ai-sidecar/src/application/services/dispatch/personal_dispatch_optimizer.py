"""Personal-level rolling-horizon dispatch optimizer."""

from __future__ import annotations

from collections import Counter, defaultdict
from collections.abc import Iterable
from dataclasses import dataclass, field
from datetime import datetime, timedelta
from typing import Any

from src.domain.models.dispatch import TurnaroundConstraintMode
from src.infrastructure.logging.core import get_logger

from .dispatch_shared import DispatchCalculator, Position
from .qualification_coverage_service import QualificationCoverageService
from .turnaround_constraint_resolver import TurnaroundConstraint

logger = get_logger(__name__)


@dataclass(frozen=True)
class CrewDispatchFreeWindow:
    start_time: datetime
    end_time: datetime
    left_anchor_order_id: str | None = None
    left_anchor_position: Position | None = None
    right_anchor_order_id: str | None = None
    right_anchor_position: Position | None = None


@dataclass(frozen=True)
class CrewDispatchTask:
    id: str
    flight_id: str
    task_type: str
    stand_id: str | None
    stand_position: Position
    planned_start: datetime
    planned_end: datetime
    terminal: str | None
    department_id: str
    department_rule_version: str
    crew_requirement_snapshot: list[dict[str, Any]]
    leg_scope: str | None = None
    turnaround_pair_key: str | None = None
    turnaround_continuity_rules: list[Any] = field(default_factory=list)
    turnaround_context: dict[str, Any] = field(default_factory=dict)
    earliest_start: datetime | None = None
    latest_start: datetime | None = None
    due_start: datetime | None = None
    baseline_members: list[dict[str, Any]] = field(default_factory=list)


@dataclass(frozen=True)
class AvailableEmployee:
    user_id: str
    username: str | None
    source_team_id: str | None
    source_team_name: str | None
    position: Position | None
    schedule_source: str | None = None
    available_from: datetime | None = None
    anchor_position: Position | None = None
    free_windows: list[CrewDispatchFreeWindow] = field(default_factory=list)
    scarcity_cost: float = 0.0


@dataclass(frozen=True)
class CrewMemberAssignment:
    user_id: str
    username: str | None
    source_team_id: str | None
    source_team_name: str | None
    slot_code: str
    qualification_code: str
    qualification_level_code: str | None


@dataclass(frozen=True)
class TaskCrewAssignment:
    task_id: str
    members: list[CrewMemberAssignment]
    qualification_gap: list[dict[str, Any]]
    score_breakdown: dict[str, float]
    source_team_id: str | None
    source_team_name: str | None
    travel_time_minutes: float
    total_distance_meters: float


@dataclass(frozen=True)
class PersonalOptimizationResult:
    success: bool
    assignments: list[TaskCrewAssignment]
    total_cost: float
    unassigned_tasks: list[str]
    solver_time_ms: float
    is_optimal: bool
    unassigned_task_gaps: dict[str, list[dict[str, Any]]] = field(default_factory=dict)


@dataclass(frozen=True)
class _SlotCandidate:
    employee_index: int
    qualification_level_code: str | None


@dataclass(frozen=True)
class _ExpandedSlot:
    slot_id: str
    task_id: str
    task_index: int
    slot_code: str
    qualification_code: str
    min_level_code: str | None
    baseline_user_id: str | None
    candidates: list[_SlotCandidate]


@dataclass(frozen=True)
class _SegmentWindow:
    start_minute: int
    end_minute: int
    left_anchor_order_id: str | None
    left_anchor_position: Position | None
    right_anchor_order_id: str | None
    right_anchor_position: Position | None


@dataclass(frozen=True)
class _ExpandedTurnaroundSlotConstraint:
    inbound_slot_index: int
    outbound_slot_index: int
    constraint: TurnaroundConstraint


class PersonalDispatchOptimizer:
    """Rolling-horizon personal dispatch model close to the OR formulation."""

    SLOT_UNASSIGNED_WEIGHT = 1_000_000_000
    LATENESS_WEIGHT = 1_000_000
    CHANGE_WEIGHT = 10_000
    TRAVEL_WEIGHT = 100
    SCARCITY_WEIGHT = 10
    LOAD_BALANCE_WEIGHT = 1
    TURNAROUND_SOFT_PENALTY_WEIGHT = 50_000

    def __init__(
        self,
        *,
        qualification_coverage_service: QualificationCoverageService | None = None,
        calculator: DispatchCalculator | None = None,
    ) -> None:
        self._qualification_coverage_service = qualification_coverage_service or QualificationCoverageService()
        self._calculator = calculator or DispatchCalculator()

    async def optimize(
        self,
        *,
        tasks: list[CrewDispatchTask],
        employees: list[AvailableEmployee],
        grants_by_user: dict[str, list[Any]],
        levels_by_department: dict[str, Iterable[Any]],
        time_limit_seconds: float = 5.0,
        turnaround_constraints: list[TurnaroundConstraint] | None = None,
    ) -> PersonalOptimizationResult:
        import time

        start_time = time.time()
        if not tasks:
            return PersonalOptimizationResult(
                success=True,
                assignments=[],
                total_cost=0.0,
                unassigned_tasks=[],
                solver_time_ms=0.0,
                is_optimal=True,
            )

        try:
            result = await self._solve_with_cp_sat(
                tasks=tasks,
                employees=employees,
                grants_by_user=grants_by_user,
                levels_by_department=levels_by_department,
                time_limit_seconds=time_limit_seconds,
                turnaround_constraints=list(turnaround_constraints or []),
            )
        except ImportError as exc:
            logger.error("personal optimizer dependency unavailable: %s", exc)
            result = await self._fallback(
                tasks=tasks,
                employees=employees,
                grants_by_user=grants_by_user,
                levels_by_department=levels_by_department,
                turnaround_constraints=list(turnaround_constraints or []),
            )
        except Exception as exc:  # pragma: no cover
            logger.exception("personal optimizer failed, fallback to greedy model: %s", exc)
            result = await self._fallback(
                tasks=tasks,
                employees=employees,
                grants_by_user=grants_by_user,
                levels_by_department=levels_by_department,
                turnaround_constraints=list(turnaround_constraints or []),
            )

        return PersonalOptimizationResult(
            success=result.success,
            assignments=result.assignments,
            total_cost=result.total_cost,
            unassigned_tasks=result.unassigned_tasks,
            solver_time_ms=(time.time() - start_time) * 1000.0,
            is_optimal=result.is_optimal,
            unassigned_task_gaps=result.unassigned_task_gaps,
        )

    async def _solve_with_cp_sat(
        self,
        *,
        tasks: list[CrewDispatchTask],
        employees: list[AvailableEmployee],
        grants_by_user: dict[str, list[Any]],
        levels_by_department: dict[str, Iterable[Any]],
        time_limit_seconds: float,
        turnaround_constraints: list[TurnaroundConstraint],
    ) -> PersonalOptimizationResult:
        from ortools.sat.python import cp_model

        normalized_tasks = [self._normalize_task(task) for task in tasks]
        slot_bundle = self._expand_slots(
            tasks=normalized_tasks,
            employees=employees,
            grants_by_user=grants_by_user,
            levels_by_department=levels_by_department,
        )
        expanded_slots = slot_bundle["slots"]
        if not expanded_slots:
            return PersonalOptimizationResult(
                success=False,
                assignments=[],
                total_cost=float("inf"),
                unassigned_tasks=[task.id for task in normalized_tasks],
                solver_time_ms=0.0,
                is_optimal=False,
                unassigned_task_gaps={task.id: self._build_full_gap_payload(task) for task in normalized_tasks},
            )

        origin = self._resolve_model_origin(normalized_tasks, employees)
        task_offsets = {task.id: self._task_offset_payload(task=task, origin=origin) for task in normalized_tasks}
        horizon_end = max(
            max(payload["latest_start"] + payload["duration"] for payload in task_offsets.values()),
            60,
        )
        segment_payloads = {
            employee.user_id: self._build_segment_payloads(
                employee=employee,
                origin=origin,
                horizon_end=horizon_end,
            )
            for employee in employees
        }
        model_horizon = max(
            [
                horizon_end,
                *(segment.end_minute for segments in segment_payloads.values() for segment in segments),
            ]
        )
        big_m = max(60, model_horizon + 180)

        model = cp_model.CpModel()
        start_vars: dict[str, cp_model.IntVar] = {}
        lateness_vars: dict[str, cp_model.IntVar] = {}
        for task in normalized_tasks:
            payload = task_offsets[task.id]
            start_var = model.NewIntVar(
                payload["earliest_start"],
                payload["latest_start"],
                f"start_{task.id}",
            )
            lateness_var = model.NewIntVar(
                0,
                max(0, payload["latest_start"] + payload["duration"] - payload["due_start"]),
                f"lateness_{task.id}",
            )
            model.Add(lateness_var >= start_var - payload["due_start"])
            start_vars[task.id] = start_var
            lateness_vars[task.id] = lateness_var

        slot_vars: dict[tuple[int, int], cp_model.IntVar] = {}
        slot_uncovered_vars: dict[int, cp_model.IntVar] = {}
        for slot_index, slot in enumerate(expanded_slots):
            for candidate in slot.candidates:
                slot_vars[(slot_index, candidate.employee_index)] = model.NewBoolVar(
                    f"x_{slot_index}_{candidate.employee_index}"
                )
            slot_uncovered_vars[slot_index] = model.NewBoolVar(f"u_{slot_index}")
            model.Add(
                sum(slot_vars[(slot_index, candidate.employee_index)] for candidate in slot.candidates)
                + slot_uncovered_vars[slot_index]
                == 1
            )

        task_employee_vars: dict[tuple[int, int], cp_model.IntVar] = {}
        task_slot_indexes_by_employee: dict[tuple[int, int], list[int]] = defaultdict(list)
        for slot_index, slot in enumerate(expanded_slots):
            for candidate in slot.candidates:
                task_slot_indexes_by_employee[(slot.task_index, candidate.employee_index)].append(slot_index)

        for (task_index, employee_index), slot_indexes in task_slot_indexes_by_employee.items():
            z_var = model.NewBoolVar(f"z_{task_index}_{employee_index}")
            task_employee_vars[(task_index, employee_index)] = z_var
            relevant_vars = [slot_vars[(slot_index, employee_index)] for slot_index in slot_indexes]
            model.Add(sum(relevant_vars) == z_var)

        for task_index, _task in enumerate(normalized_tasks):
            for employee_index, _employee in enumerate(employees):
                relevant_slots = task_slot_indexes_by_employee.get((task_index, employee_index), [])
                if not relevant_slots:
                    continue
                model.Add(sum(slot_vars[(slot_index, employee_index)] for slot_index in relevant_slots) <= 1)

        expanded_turnaround_constraints = self._expand_turnaround_constraints(
            turnaround_constraints=turnaround_constraints,
            tasks=normalized_tasks,
            expanded_slots=expanded_slots,
        )

        segment_task_vars: dict[tuple[int, int, int], cp_model.IntVar] = {}
        first_task_vars: dict[tuple[int, int, int], cp_model.IntVar] = {}
        last_task_vars: dict[tuple[int, int, int], cp_model.IntVar] = {}
        arc_vars: dict[tuple[int, int, int, int], cp_model.IntVar] = {}
        arc_travel_minutes: dict[tuple[int, int, int, int], int] = {}
        left_anchor_minutes: dict[tuple[int, int, int], int] = {}

        for employee_index, employee in enumerate(employees):
            segments = segment_payloads.get(employee.user_id) or []
            for task_index, task in enumerate(normalized_tasks):
                z_var = task_employee_vars.get((task_index, employee_index))
                if z_var is None:
                    continue
                if not segments:
                    model.Add(z_var == 0)
                    continue

                segment_presence_vars: list[cp_model.IntVar] = []
                for segment_index, segment in enumerate(segments):
                    segment_var = model.NewBoolVar(f"z_{task_index}_{employee_index}_{segment_index}")
                    alpha_var = model.NewBoolVar(f"alpha_{task_index}_{employee_index}_{segment_index}")
                    beta_var = model.NewBoolVar(f"beta_{task_index}_{employee_index}_{segment_index}")
                    segment_task_vars[(task_index, employee_index, segment_index)] = segment_var
                    first_task_vars[(task_index, employee_index, segment_index)] = alpha_var
                    last_task_vars[(task_index, employee_index, segment_index)] = beta_var
                    segment_presence_vars.append(segment_var)

                    left_travel = self._travel_minutes(segment.left_anchor_position, task.stand_position)
                    right_travel = self._travel_minutes(task.stand_position, segment.right_anchor_position)
                    left_anchor_minutes[(task_index, employee_index, segment_index)] = left_travel
                    payload = task_offsets[task.id]
                    model.Add(start_vars[task.id] >= segment.start_minute + left_travel - big_m * (1 - alpha_var))
                    model.Add(
                        start_vars[task.id] + payload["duration"] + right_travel
                        <= segment.end_minute + big_m * (1 - beta_var)
                    )

                model.Add(sum(segment_presence_vars) == z_var)

            for segment_index, _segment in enumerate(segments):
                alpha_vars: list[cp_model.IntVar] = []
                beta_vars: list[cp_model.IntVar] = []
                for task_index, task in enumerate(normalized_tasks):
                    segment_var = segment_task_vars.get((task_index, employee_index, segment_index))
                    if segment_var is None:
                        continue
                    alpha_var = first_task_vars[(task_index, employee_index, segment_index)]
                    beta_var = last_task_vars[(task_index, employee_index, segment_index)]
                    alpha_vars.append(alpha_var)
                    beta_vars.append(beta_var)
                    incoming_vars: list[cp_model.IntVar] = []
                    outgoing_vars: list[cp_model.IntVar] = []
                    for other_task_index, other_task in enumerate(normalized_tasks):
                        if other_task_index == task_index:
                            continue
                        if (other_task_index, employee_index) not in task_employee_vars:
                            continue
                        arc_var = model.NewBoolVar(
                            f"v_{task_index}_{other_task_index}_{employee_index}_{segment_index}"
                        )
                        arc_vars[(task_index, other_task_index, employee_index, segment_index)] = arc_var
                        outgoing_vars.append(arc_var)
                        incoming_vars.append(
                            arc_vars.setdefault(
                                (other_task_index, task_index, employee_index, segment_index),
                                model.NewBoolVar(f"v_{other_task_index}_{task_index}_{employee_index}_{segment_index}"),
                            )
                        )
                        travel_minutes = self._travel_minutes(task.stand_position, other_task.stand_position)
                        arc_travel_minutes[(task_index, other_task_index, employee_index, segment_index)] = (
                            travel_minutes
                        )
                        model.Add(
                            start_vars[other_task.id]
                            >= start_vars[task.id]
                            + task_offsets[task.id]["duration"]
                            + travel_minutes
                            - big_m * (1 - arc_var)
                        )
                        model.Add(arc_var <= segment_var)
                        model.Add(arc_var <= segment_task_vars[(other_task_index, employee_index, segment_index)])
                    model.Add(alpha_var + sum(incoming_vars) == segment_var)
                    model.Add(beta_var + sum(outgoing_vars) == segment_var)
                if alpha_vars:
                    model.Add(sum(alpha_vars) <= 1)
                if beta_vars:
                    model.Add(sum(beta_vars) <= 1)

        delta_vars: list[cp_model.IntVar] = []
        for slot_index, slot in enumerate(expanded_slots):
            for candidate in slot.candidates:
                x_var = slot_vars[(slot_index, candidate.employee_index)]
                delta_var = model.NewBoolVar(f"delta_{slot_index}_{candidate.employee_index}")
                baseline_match = (
                    slot.baseline_user_id is not None
                    and slot.baseline_user_id == employees[candidate.employee_index].user_id
                )
                if baseline_match:
                    model.Add(delta_var + x_var == 1)
                else:
                    model.Add(delta_var == x_var)
                delta_vars.append(delta_var)

        turnaround_soft_penalty_terms: list[Any] = []
        for expanded in expanded_turnaround_constraints:
            inbound_slot_index = expanded.inbound_slot_index
            outbound_slot_index = expanded.outbound_slot_index
            constraint = expanded.constraint
            mode = str(constraint.constraint_mode or "").strip()
            if mode == TurnaroundConstraintMode.DISABLED.value:
                continue

            inbound_employee_indexes = {
                candidate.employee_index for candidate in expanded_slots[inbound_slot_index].candidates
            }
            outbound_employee_indexes = {
                candidate.employee_index for candidate in expanded_slots[outbound_slot_index].candidates
            }
            all_employee_indexes = sorted(inbound_employee_indexes | outbound_employee_indexes)

            if mode == TurnaroundConstraintMode.SAME_PERSON.value:
                for employee_index in all_employee_indexes:
                    inbound_var = slot_vars.get((inbound_slot_index, employee_index))
                    outbound_var = slot_vars.get((outbound_slot_index, employee_index))
                    if inbound_var is None and outbound_var is None:
                        continue
                    if inbound_var is None:
                        model.Add(outbound_var == 0)
                        continue
                    if outbound_var is None:
                        model.Add(inbound_var == 0)
                        continue
                    model.Add(inbound_var == outbound_var)
                continue

            if mode == TurnaroundConstraintMode.SOFT_PREFER_SAME_PERSON.value:
                both_covered_var = model.NewBoolVar(
                    f"turnaround_both_covered_{inbound_slot_index}_{outbound_slot_index}"
                )
                model.Add(both_covered_var <= 1 - slot_uncovered_vars[inbound_slot_index])
                model.Add(both_covered_var <= 1 - slot_uncovered_vars[outbound_slot_index])
                model.Add(
                    both_covered_var
                    >= 1 - slot_uncovered_vars[inbound_slot_index] - slot_uncovered_vars[outbound_slot_index]
                )

                common_employee_indexes = sorted(inbound_employee_indexes & outbound_employee_indexes)
                same_employee_match_vars: list[cp_model.IntVar] = []
                for employee_index in common_employee_indexes:
                    inbound_var = slot_vars[(inbound_slot_index, employee_index)]
                    outbound_var = slot_vars[(outbound_slot_index, employee_index)]
                    match_var = model.NewBoolVar(
                        f"turnaround_match_{inbound_slot_index}_{outbound_slot_index}_{employee_index}"
                    )
                    model.Add(match_var <= inbound_var)
                    model.Add(match_var <= outbound_var)
                    model.Add(match_var >= inbound_var + outbound_var - 1)
                    same_employee_match_vars.append(match_var)

                same_employee_var = model.NewBoolVar(f"turnaround_same_{inbound_slot_index}_{outbound_slot_index}")
                if same_employee_match_vars:
                    model.Add(sum(same_employee_match_vars) == same_employee_var)
                else:
                    model.Add(same_employee_var == 0)

                mismatch_var = model.NewBoolVar(f"turnaround_soft_mismatch_{inbound_slot_index}_{outbound_slot_index}")
                model.Add(mismatch_var >= both_covered_var - same_employee_var)
                model.Add(mismatch_var <= both_covered_var)
                model.Add(mismatch_var <= 1 - same_employee_var)
                turnaround_soft_penalty_terms.append(mismatch_var * self._soft_turnaround_penalty(constraint))

        handover_keys: set[tuple[str, str, int]] = set()
        for constraint in turnaround_constraints or []:
            if str(constraint.constraint_mode or "").strip() != TurnaroundConstraintMode.HANDOVER_REQUIRED.value:
                continue
            inbound_payload = task_offsets.get(constraint.inbound_task_id)
            outbound_payload = task_offsets.get(constraint.outbound_task_id)
            if inbound_payload is None or outbound_payload is None:
                continue
            handover_buffer = max(0, int(constraint.tight_threshold_minutes or 0))
            key = (constraint.inbound_task_id, constraint.outbound_task_id, handover_buffer)
            if key in handover_keys:
                continue
            handover_keys.add(key)
            earliest_feasible_outbound = (
                inbound_payload["earliest_start"] + inbound_payload["duration"] + handover_buffer
            )
            if outbound_payload["latest_start"] < earliest_feasible_outbound:
                continue
            model.Add(
                start_vars[constraint.outbound_task_id]
                >= start_vars[constraint.inbound_task_id] + inbound_payload["duration"] + handover_buffer
            )

        employee_slot_feasible_counts = {
            employee_index: max(
                1,
                sum(
                    1
                    for slot in expanded_slots
                    if any(candidate.employee_index == employee_index for candidate in slot.candidates)
                ),
            )
            for employee_index, _employee in enumerate(employees)
        }
        average_workload = round(
            sum(task_offsets[slot.task_id]["duration"] for slot in expanded_slots) / max(1, len(employees))
        )
        eta_vars: list[cp_model.IntVar] = []
        for employee_index, _employee in enumerate(employees):
            workload_expr = sum(
                task_offsets[normalized_tasks[task_index].id]["duration"] * z_var
                for (task_index, emp_index), z_var in task_employee_vars.items()
                if emp_index == employee_index
            )
            eta_var = model.NewIntVar(0, model_horizon + 240, f"eta_{employee_index}")
            model.Add(eta_var >= workload_expr - average_workload)
            model.Add(eta_var >= average_workload - workload_expr)
            eta_vars.append(eta_var)

        objective_terms: list[Any] = []
        objective_terms.extend(
            slot_uncovered_vars[slot_index] * self.SLOT_UNASSIGNED_WEIGHT for slot_index in slot_uncovered_vars
        )
        objective_terms.extend(lateness_var * self.LATENESS_WEIGHT for lateness_var in lateness_vars.values())
        objective_terms.extend(delta_var * self.CHANGE_WEIGHT for delta_var in delta_vars)
        objective_terms.extend(
            first_task_vars[key] * left_anchor_minutes[key] * self.TRAVEL_WEIGHT for key in first_task_vars
        )
        objective_terms.extend(
            arc_var * arc_travel_minutes[key] * self.TRAVEL_WEIGHT for key, arc_var in arc_vars.items()
        )
        objective_terms.extend(
            slot_vars[(slot_index, candidate.employee_index)]
            * round(
                (
                    employees[candidate.employee_index].scarcity_cost
                    or (100.0 / employee_slot_feasible_counts[candidate.employee_index])
                )
                * self.SCARCITY_WEIGHT
            )
            for slot_index, slot in enumerate(expanded_slots)
            for candidate in slot.candidates
        )
        objective_terms.extend(eta_var * self.LOAD_BALANCE_WEIGHT for eta_var in eta_vars)
        objective_terms.extend(turnaround_soft_penalty_terms)
        model.Minimize(sum(objective_terms))

        solver = cp_model.CpSolver()
        solver.parameters.max_time_in_seconds = max(0.1, float(time_limit_seconds or 5.0))
        solver.parameters.num_search_workers = 1
        solver.parameters.random_seed = 7
        status = solver.solve(model)

        feasible_statuses = {cp_model.OPTIMAL, cp_model.FEASIBLE}
        if status not in feasible_statuses:
            return PersonalOptimizationResult(
                success=False,
                assignments=[],
                total_cost=float("inf"),
                unassigned_tasks=[task.id for task in normalized_tasks],
                solver_time_ms=0.0,
                is_optimal=False,
                unassigned_task_gaps={task.id: self._build_full_gap_payload(task) for task in normalized_tasks},
            )

        assignments_by_task: dict[str, list[CrewMemberAssignment]] = defaultdict(list)
        gaps_by_task: dict[str, list[dict[str, Any]]] = defaultdict(list)
        score_by_task: dict[str, dict[str, float]] = defaultdict(
            lambda: {
                "travel_time_minutes": 0.0,
                "total_distance_meters": 0.0,
                "covered_slot_count": 0.0,
                "gap_slot_count": 0.0,
                "lateness_minutes": 0.0,
                "change_count": 0.0,
            }
        )
        for task in normalized_tasks:
            score_by_task[task.id]["lateness_minutes"] = float(solver.Value(lateness_vars[task.id]))

        for slot_index, slot in enumerate(expanded_slots):
            chosen_candidate: _SlotCandidate | None = None
            for candidate in slot.candidates:
                if solver.Value(slot_vars[(slot_index, candidate.employee_index)]) > 0:
                    chosen_candidate = candidate
                    break
            if chosen_candidate is None:
                gaps_by_task[slot.task_id].append(
                    {
                        "slot_code": slot.slot_code,
                        "qualification_code": slot.qualification_code,
                        "min_level_code": slot.min_level_code,
                        "reason": "crew_slot_uncovered",
                    }
                )
                score_by_task[slot.task_id]["gap_slot_count"] += 1.0
                continue

            employee = employees[chosen_candidate.employee_index]
            assignments_by_task[slot.task_id].append(
                CrewMemberAssignment(
                    user_id=employee.user_id,
                    username=employee.username,
                    source_team_id=employee.source_team_id,
                    source_team_name=employee.source_team_name,
                    slot_code=slot.slot_code,
                    qualification_code=slot.qualification_code,
                    qualification_level_code=chosen_candidate.qualification_level_code,
                )
            )
            score_by_task[slot.task_id]["covered_slot_count"] += 1.0
            if slot.baseline_user_id and slot.baseline_user_id != employee.user_id:
                score_by_task[slot.task_id]["change_count"] += 1.0

        for key, alpha_var in first_task_vars.items():
            if solver.Value(alpha_var) <= 0:
                continue
            task_index, employee_index, segment_index = key
            task = normalized_tasks[task_index]
            segment = segment_payloads.get(employees[employee_index].user_id, [])[segment_index]
            score_by_task[task.id]["travel_time_minutes"] += float(left_anchor_minutes.get(key, 0))
            score_by_task[task.id]["total_distance_meters"] += float(
                self._distance_meters(segment.left_anchor_position, task.stand_position)
            )

        for (from_task_index, to_task_index, employee_index, segment_index), arc_var in arc_vars.items():
            if solver.Value(arc_var) <= 0:
                continue
            from_task = normalized_tasks[from_task_index]
            to_task = normalized_tasks[to_task_index]
            score_by_task[to_task.id]["travel_time_minutes"] += float(
                arc_travel_minutes[(from_task_index, to_task_index, employee_index, segment_index)]
            )
            score_by_task[to_task.id]["total_distance_meters"] += float(
                self._distance_meters(from_task.stand_position, to_task.stand_position)
            )

        assignment_rows: list[TaskCrewAssignment] = []
        unassigned_tasks: list[str] = []
        unassigned_task_gaps: dict[str, list[dict[str, Any]]] = {}
        for task in normalized_tasks:
            task_members = sorted(
                assignments_by_task.get(task.id, []),
                key=lambda item: (item.slot_code, item.user_id),
            )
            task_gaps = gaps_by_task.get(task.id, [])
            task_scores = score_by_task.get(task.id, {})
            if task_gaps or not task_members:
                unassigned_tasks.append(task.id)
                unassigned_task_gaps[task.id] = task_gaps or self._build_full_gap_payload(task)
                continue

            dominant_team_id, dominant_team_name = self._resolve_dominant_source_team(task_members)
            covered = task_scores.get("covered_slot_count", 0.0)
            gaps = task_scores.get("gap_slot_count", 0.0)
            assignment_rows.append(
                TaskCrewAssignment(
                    task_id=task.id,
                    members=task_members,
                    qualification_gap=[],
                    score_breakdown={
                        **task_scores,
                        "coverage_ratio": round(covered / max(1.0, covered + gaps), 4),
                    },
                    source_team_id=dominant_team_id,
                    source_team_name=dominant_team_name,
                    travel_time_minutes=task_scores.get("travel_time_minutes", 0.0),
                    total_distance_meters=task_scores.get("total_distance_meters", 0.0),
                )
            )

        return PersonalOptimizationResult(
            success=True,
            assignments=assignment_rows,
            total_cost=float(solver.ObjectiveValue()),
            unassigned_tasks=unassigned_tasks,
            solver_time_ms=0.0,
            is_optimal=status == cp_model.OPTIMAL,
            unassigned_task_gaps=unassigned_task_gaps,
        )

    async def _fallback(
        self,
        *,
        tasks: list[CrewDispatchTask],
        employees: list[AvailableEmployee],
        grants_by_user: dict[str, list[Any]],
        levels_by_department: dict[str, Iterable[Any]],
        turnaround_constraints: list[TurnaroundConstraint],
    ) -> PersonalOptimizationResult:
        normalized_tasks = [self._normalize_task(task) for task in tasks]
        slot_bundle = self._expand_slots(
            tasks=normalized_tasks,
            employees=employees,
            grants_by_user=grants_by_user,
            levels_by_department=levels_by_department,
        )
        expanded_slots = slot_bundle["slots"]
        task_members: dict[str, list[CrewMemberAssignment]] = defaultdict(list)
        task_gaps: dict[str, list[dict[str, Any]]] = defaultdict(list)
        employee_bookings: dict[str, list[tuple[datetime, datetime]]] = defaultdict(list)

        for slot in expanded_slots:
            task = normalized_tasks[slot.task_index]
            chosen_candidate: _SlotCandidate | None = None
            for candidate in slot.candidates:
                employee = employees[candidate.employee_index]
                start_time = task.earliest_start or task.planned_start
                if not self._task_fits_employee_windows(task=task, employee=employee, start_time=start_time):
                    continue
                if self._employee_overlaps_booking(employee.user_id, task, employee_bookings):
                    continue
                chosen_candidate = candidate
                break
            if chosen_candidate is None:
                task_gaps[slot.task_id].append(
                    {
                        "slot_code": slot.slot_code,
                        "qualification_code": slot.qualification_code,
                        "min_level_code": slot.min_level_code,
                        "reason": "crew_slot_uncovered",
                    }
                )
                continue

            employee = employees[chosen_candidate.employee_index]
            task_members[slot.task_id].append(
                CrewMemberAssignment(
                    user_id=employee.user_id,
                    username=employee.username,
                    source_team_id=employee.source_team_id,
                    source_team_name=employee.source_team_name,
                    slot_code=slot.slot_code,
                    qualification_code=slot.qualification_code,
                    qualification_level_code=chosen_candidate.qualification_level_code,
                )
            )
            employee_bookings[employee.user_id].append((task.planned_start, task.planned_end))

        assignments: list[TaskCrewAssignment] = []
        unassigned_tasks: list[str] = []
        unassigned_task_gaps: dict[str, list[dict[str, Any]]] = {}
        for task in normalized_tasks:
            members = sorted(
                task_members.get(task.id, []),
                key=lambda item: (item.slot_code, item.user_id),
            )
            gaps = task_gaps.get(task.id, [])
            if gaps or not members:
                unassigned_tasks.append(task.id)
                unassigned_task_gaps[task.id] = gaps or self._build_full_gap_payload(task)
                continue
            dominant_team_id, dominant_team_name = self._resolve_dominant_source_team(members)
            assignments.append(
                TaskCrewAssignment(
                    task_id=task.id,
                    members=members,
                    qualification_gap=[],
                    score_breakdown={"coverage_ratio": 1.0, "travel_time_minutes": 0.0, "total_distance_meters": 0.0},
                    source_team_id=dominant_team_id,
                    source_team_name=dominant_team_name,
                    travel_time_minutes=0.0,
                    total_distance_meters=0.0,
                )
            )

        return PersonalOptimizationResult(
            success=not unassigned_tasks,
            assignments=assignments,
            total_cost=float(len(unassigned_tasks) * self.SLOT_UNASSIGNED_WEIGHT),
            unassigned_tasks=unassigned_tasks,
            solver_time_ms=0.0,
            is_optimal=False,
            unassigned_task_gaps=unassigned_task_gaps,
        )

    def _expand_slots(
        self,
        *,
        tasks: list[CrewDispatchTask],
        employees: list[AvailableEmployee],
        grants_by_user: dict[str, list[Any]],
        levels_by_department: dict[str, Iterable[Any]],
    ) -> dict[str, Any]:
        slots: list[_ExpandedSlot] = []
        levels_index_by_department = {
            department_id: self._qualification_coverage_service.build_level_index(levels)
            for department_id, levels in levels_by_department.items()
        }
        for task_index, task in enumerate(tasks):
            levels_by_qualification = levels_index_by_department.get(task.department_id, {})
            baseline_assignments = self._expand_baseline_members(task)
            baseline_counters = Counter()
            for requirement in task.crew_requirement_snapshot or []:
                required_count = max(1, int(requirement.get("required_count") or 1))
                base_slot_code = str(requirement.get("slot_code") or "").strip() or "slot"
                qualification_code = str(requirement.get("qualification_code") or "").strip()
                min_level_code = str(requirement.get("min_level_code") or "").strip() or None
                baseline_candidates = baseline_assignments.get(base_slot_code, [])
                for offset in range(required_count):
                    expanded_slot_code = base_slot_code if required_count == 1 else f"{base_slot_code}#{offset + 1}"
                    slot_candidates: list[_SlotCandidate] = []
                    for employee_index, employee in enumerate(employees):
                        matched_grant = self._qualification_coverage_service.best_matching_grant_for_user(
                            user_id=employee.user_id,
                            qualification_code=qualification_code,
                            min_level_code=min_level_code,
                            grants_by_user=grants_by_user,
                            levels_by_qualification=levels_by_qualification,
                            profile={"schedule_source": employee.schedule_source},
                        )
                        if matched_grant is None:
                            continue
                        slot_candidates.append(
                            _SlotCandidate(
                                employee_index=employee_index,
                                qualification_level_code=str(getattr(matched_grant, "level_code", "") or "").strip()
                                or None,
                            )
                        )
                    baseline_index = baseline_counters[base_slot_code]
                    baseline_counters[base_slot_code] += 1
                    baseline_user_id = None
                    if baseline_index < len(baseline_candidates):
                        baseline_user_id = baseline_candidates[baseline_index]
                    slots.append(
                        _ExpandedSlot(
                            slot_id=f"{task.id}:{expanded_slot_code}",
                            task_id=task.id,
                            task_index=task_index,
                            slot_code=expanded_slot_code,
                            qualification_code=qualification_code,
                            min_level_code=min_level_code,
                            baseline_user_id=baseline_user_id,
                            candidates=slot_candidates,
                        )
                    )
        return {"slots": slots}

    @staticmethod
    def _base_slot_code(slot_code: str) -> str:
        return str(slot_code or "").split("#", 1)[0].strip()

    def _expand_turnaround_constraints(
        self,
        *,
        turnaround_constraints: list[TurnaroundConstraint],
        tasks: list[CrewDispatchTask],
        expanded_slots: list[_ExpandedSlot],
    ) -> list[_ExpandedTurnaroundSlotConstraint]:
        task_by_id = {task.id: task for task in tasks}
        slot_indexes_by_base_code: dict[tuple[str, str], list[int]] = defaultdict(list)
        for slot_index, slot in enumerate(expanded_slots):
            slot_indexes_by_base_code[(slot.task_id, self._base_slot_code(slot.slot_code))].append(slot_index)

        resolved: list[_ExpandedTurnaroundSlotConstraint] = []
        for constraint in turnaround_constraints or []:
            if constraint.inbound_task_id not in task_by_id or constraint.outbound_task_id not in task_by_id:
                continue
            inbound_slot_indexes = slot_indexes_by_base_code.get(
                (constraint.inbound_task_id, self._base_slot_code(constraint.inbound_slot_code)),
                [],
            )
            outbound_slot_indexes = slot_indexes_by_base_code.get(
                (constraint.outbound_task_id, self._base_slot_code(constraint.outbound_slot_code)),
                [],
            )
            pair_count = min(len(inbound_slot_indexes), len(outbound_slot_indexes))
            for offset in range(pair_count):
                resolved.append(
                    _ExpandedTurnaroundSlotConstraint(
                        inbound_slot_index=inbound_slot_indexes[offset],
                        outbound_slot_index=outbound_slot_indexes[offset],
                        constraint=constraint,
                    )
                )
        return resolved

    def _soft_turnaround_penalty(self, constraint: TurnaroundConstraint) -> int:
        weight = self.TURNAROUND_SOFT_PENALTY_WEIGHT
        slack_minutes = constraint.slack_minutes
        tight_threshold = constraint.tight_threshold_minutes
        relax_threshold = constraint.relax_threshold_minutes
        if slack_minutes is None:
            return weight
        if tight_threshold is not None and slack_minutes <= tight_threshold:
            return weight * 4
        if relax_threshold is not None and slack_minutes >= relax_threshold:
            return max(1, weight // 4)
        return weight

    def _normalize_task(self, task: CrewDispatchTask) -> CrewDispatchTask:
        planned_start = task.planned_start
        planned_end = task.planned_end or (planned_start + timedelta(minutes=15))
        earliest_start = task.earliest_start or planned_start
        latest_start = task.latest_start or earliest_start
        if latest_start < earliest_start:
            latest_start = earliest_start
        due_start = task.due_start or planned_start
        return CrewDispatchTask(
            id=task.id,
            flight_id=task.flight_id,
            task_type=task.task_type,
            stand_id=task.stand_id,
            stand_position=task.stand_position,
            planned_start=planned_start,
            planned_end=planned_end,
            terminal=task.terminal,
            department_id=task.department_id,
            department_rule_version=task.department_rule_version,
            crew_requirement_snapshot=list(task.crew_requirement_snapshot or []),
            leg_scope=task.leg_scope,
            turnaround_pair_key=task.turnaround_pair_key,
            turnaround_continuity_rules=list(task.turnaround_continuity_rules or []),
            turnaround_context=dict(task.turnaround_context or {}),
            earliest_start=earliest_start,
            latest_start=latest_start,
            due_start=due_start,
            baseline_members=list(task.baseline_members or []),
        )

    def _resolve_model_origin(
        self,
        tasks: list[CrewDispatchTask],
        employees: list[AvailableEmployee],
    ) -> datetime:
        values: list[datetime] = []
        for task in tasks:
            values.append(task.earliest_start or task.planned_start)
            values.append(task.due_start or task.planned_start)
        for employee in employees:
            if employee.available_from is not None:
                values.append(employee.available_from)
            for window in employee.free_windows or []:
                values.append(window.start_time)
        return min(values) if values else min(task.planned_start for task in tasks)

    def _task_offset_payload(
        self,
        *,
        task: CrewDispatchTask,
        origin: datetime,
    ) -> dict[str, int]:
        earliest_start = self._minutes_between(origin, task.earliest_start or task.planned_start)
        latest_start = self._minutes_between(origin, task.latest_start or task.earliest_start or task.planned_start)
        due_start = self._minutes_between(origin, task.due_start or task.planned_start)
        duration = max(
            5,
            round((task.planned_end - task.planned_start).total_seconds() / 60.0),
        )
        if latest_start < earliest_start:
            latest_start = earliest_start
        if due_start < earliest_start:
            due_start = earliest_start
        return {
            "earliest_start": earliest_start,
            "latest_start": latest_start,
            "due_start": due_start,
            "duration": duration,
        }

    def _build_segment_payloads(
        self,
        *,
        employee: AvailableEmployee,
        origin: datetime,
        horizon_end: int,
    ) -> list[_SegmentWindow]:
        windows = [window for window in (employee.free_windows or []) if window.end_time > window.start_time]
        if not windows:
            fallback_start = employee.available_from or origin
            if fallback_start < origin:
                fallback_start = origin
            windows = [
                CrewDispatchFreeWindow(
                    start_time=fallback_start,
                    end_time=origin + timedelta(minutes=horizon_end),
                    left_anchor_position=employee.anchor_position or employee.position,
                )
            ]
        payloads: list[_SegmentWindow] = []
        for window in windows:
            start_minute = self._minutes_between(origin, window.start_time)
            end_minute = self._minutes_between(origin, window.end_time)
            if end_minute <= start_minute:
                continue
            payloads.append(
                _SegmentWindow(
                    start_minute=start_minute,
                    end_minute=end_minute,
                    left_anchor_order_id=window.left_anchor_order_id,
                    left_anchor_position=window.left_anchor_position or employee.anchor_position or employee.position,
                    right_anchor_order_id=window.right_anchor_order_id,
                    right_anchor_position=window.right_anchor_position,
                )
            )
        return payloads

    @staticmethod
    def _expand_baseline_members(task: CrewDispatchTask) -> dict[str, list[str]]:
        baseline_by_slot: dict[str, list[str]] = defaultdict(list)
        for member in task.baseline_members or []:
            user_id = str((member or {}).get("user_id") or "").strip()
            slot_code = str((member or {}).get("slot_code") or "").strip() or "slot"
            if not user_id:
                continue
            baseline_by_slot[slot_code].append(user_id)
        for values in baseline_by_slot.values():
            values.sort()
        return baseline_by_slot

    def _task_fits_employee_windows(
        self,
        *,
        task: CrewDispatchTask,
        employee: AvailableEmployee,
        start_time: datetime,
    ) -> bool:
        end_time = start_time + (task.planned_end - task.planned_start)
        if not employee.free_windows:
            return True
        for window in employee.free_windows:
            if start_time < window.start_time or end_time > window.end_time:
                continue
            left_travel = timedelta(minutes=self._travel_minutes(window.left_anchor_position, task.stand_position))
            right_travel = timedelta(minutes=self._travel_minutes(task.stand_position, window.right_anchor_position))
            if start_time >= window.start_time + left_travel and end_time + right_travel <= window.end_time:
                return True
        return False

    @staticmethod
    def _employee_overlaps_booking(
        user_id: str,
        task: CrewDispatchTask,
        employee_bookings: dict[str, list[tuple[datetime, datetime]]],
    ) -> bool:
        for booked_start, booked_end in employee_bookings.get(user_id, []):
            if booked_start < task.planned_end and task.planned_start < booked_end:
                return True
        return False

    def _travel_minutes(
        self,
        from_position: Position | None,
        to_position: Position | None,
    ) -> int:
        if from_position is None or to_position is None:
            return 0
        if from_position.stand_id and to_position.stand_id and from_position.stand_id == to_position.stand_id:
            return 0
        distance = self._calculator.haversine_distance(
            from_position.lat,
            from_position.lng,
            to_position.lat,
            to_position.lng,
        )
        return max(0, round(self._calculator.estimate_travel_time(distance)))

    def _distance_meters(
        self,
        from_position: Position | None,
        to_position: Position | None,
    ) -> float:
        if from_position is None or to_position is None:
            return 0.0
        return float(
            self._calculator.haversine_distance(
                from_position.lat,
                from_position.lng,
                to_position.lat,
                to_position.lng,
            )
        )

    @staticmethod
    def _minutes_between(origin: datetime, value: datetime) -> int:
        return round((value - origin).total_seconds() / 60.0)

    @staticmethod
    def _resolve_dominant_source_team(
        members: list[CrewMemberAssignment],
    ) -> tuple[str | None, str | None]:
        team_ids = [item.source_team_id for item in members if str(item.source_team_id or "").strip()]
        if not team_ids:
            return None, None
        dominant_team_id = Counter(team_ids).most_common(1)[0][0]
        dominant_team_name = next(
            (item.source_team_name for item in members if str(item.source_team_id or "").strip() == dominant_team_id),
            None,
        )
        return dominant_team_id, dominant_team_name

    @staticmethod
    def _build_full_gap_payload(task: CrewDispatchTask) -> list[dict[str, Any]]:
        gaps: list[dict[str, Any]] = []
        for requirement in task.crew_requirement_snapshot or []:
            required_count = max(1, int(requirement.get("required_count") or 1))
            base_slot_code = str(requirement.get("slot_code") or "").strip() or "slot"
            for offset in range(required_count):
                expanded_slot_code = base_slot_code if required_count == 1 else f"{base_slot_code}#{offset + 1}"
                gaps.append(
                    {
                        "slot_code": expanded_slot_code,
                        "qualification_code": str(requirement.get("qualification_code") or "").strip(),
                        "min_level_code": str(requirement.get("min_level_code") or "").strip() or None,
                        "reason": "crew_slot_uncovered",
                    }
                )
        return gaps
