export interface DispatchOrder {
    order_id: string;
    flight_id?: string;
    status: string;
    conflict_state?: string;
    order_class?: string;
    stand_id?: string;
    planned_start_time?: string;
    planned_end_time?: string;
    earliest_start_time?: string;
    latest_start_time?: string;
    duration_minutes: number;
    required_start_time?: string;
    actual_start_time?: string;
    actual_end_time?: string;
    estimated_completion_time?: string;
    effective_start_time?: string;
    effective_end_time?: string;
    turnaround_pair_key?: string;
    turnaround_constraint_mode?: string;
    personnel_slots: PersonnelSlot[];
    equipment_slots: EquipmentSlot[];
    baseline_assignment?: Assignment;
    current_assignment?: Assignment;
    is_optimizable: boolean;
    is_fixed_anchor: boolean;
    is_locked: boolean;
    has_conflict: boolean;
}

export interface PersonnelSlot {
    slot_code: string;
    candidate_user_ids?: string[];
    baseline_user_id?: string;
}

export interface EquipmentSlot {
    slot_code: string;
    candidate_equipment_ids?: string[];
    baseline_equipment_id?: string;
}

export interface Assignment {
    personnel_slot_assignments?: SlotAssignment[];
    equipment_slot_assignments?: SlotAssignment[];
    member_user_ids?: string[];
    equipment_ids?: string[];
    individual_user_id?: string;
}

export interface SlotAssignment {
    slot_code: string;
    user_id?: string;
    equipment_id?: string;
}

export interface AnchorState {
    resource_type: string;
    resource_id: string;
    anchor_order_id?: string;
    location_stand_id?: string;
    available_from?: string;
    free_windows: TimeWindow[];
}

export interface TimeWindow {
    start_time: string;
    end_time: string;
}

export interface UnavailableBlock {
    resource_id: string;
    resource_type: string;
    start_time: string;
    end_time: string;
}

export interface TravelEdge {
    resource_type: string;
    resource_id: string;
    from_node: string;
    to_node: string;
    travel_time_minutes: number;
}

export interface TurnaroundPair {
    pair_key?: string;
    inbound_order_id?: string;
    outbound_order_id?: string;
    hard_continuity_required?: boolean;
    slot_pairs?: SlotPair[];
    inbound_slot_code?: string;
    outbound_slot_code?: string;
}

export interface SlotPair {
    inbound_slot_code: string;
    outbound_slot_code: string;
}

export interface DispatchSnapshot {
    cluster_id?: string;
    snapshot_id?: string;
    solver_version: string;
    model_version?: string;
    travel_time_mode?: string;
    objective_config?: ObjectiveConfig;
    unsupported_features?: string[];
    optimizable_orders: DispatchOrder[];
    fixed_anchor_orders: DispatchOrder[];
    employee_anchor_states: AnchorState[];
    equipment_anchor_states: AnchorState[];
    employee_free_windows: TimeWindow[];
    equipment_free_windows: TimeWindow[];
    employee_unavailable_blocks: UnavailableBlock[];
    equipment_unavailable_blocks: UnavailableBlock[];
    resource_travel_edges: TravelEdge[];
    turnaround_pairs: TurnaroundPair[];
    max_suggestions?: number;
}

export interface ObjectiveConfig {
    weights?: Record<string, number>;
    constraints?: ObjectiveConstraint[];
}

export interface ObjectiveConstraint {
    type: string;
    params?: Record<string, unknown>;
}

export interface OrderResult {
    dispatch_order_id: string;
    gap_count?: number;
    gap_summary?: GapSummary;
    lateness_minutes?: number;
    lateness?: Lateness;
    continuity_break_count?: number;
    continuity_summary?: ContinuitySummary;
    baseline_change_count?: number;
    change_summary?: ChangeSummary;
    travel_minutes?: number;
    travel_summary?: TravelSummary;
    objective_breakdown?: ObjectiveBreakdown;
}

export interface GapSummary {
    slot_gap_count: number;
    /**
     * 每个人员/设备槽位都排上了才为 true。见 SolverRunMetadata.plan_complete ——
     * 这里是同一判定的按单/按聚合副本，旧 solver 产物不带此字段。
     */
    plan_complete?: boolean;
    unresolved_assigned_conflict_order_ids: string[];
    unassigned_unplanned_order_ids: string[];
}

export interface Lateness {
    minutes: number;
}

export interface ContinuitySummary {
    break_count: number;
    penalty: number;
    decisions: ContinuityDecision[];
}

export interface ContinuityDecision {
    pair_key: string;
    inbound_order_id: string;
    outbound_order_id: string;
    maintained: boolean;
}

export interface ChangeSummary {
    baseline_change_count: number;
    changed_order_count: number;
}

export interface TravelSummary {
    minutes: number;
}

export interface ObjectiveBreakdown {
    slot_gap: number;
    total_lateness_minutes: number;
    continuity_break: number;
    continuity_penalty: number;
    baseline_change: number;
    travel_cost: number;
    scarcity_cost: number;
    load_deviation: number;
}

export interface SolverRunMetadata {
    solver: string;
    solver_mode: string;
    solver_backend: string;
    solver_version: string;
    worker_count: number;
    cluster_count: number;
    solve_status: 'OPTIMAL' | 'FEASIBLE' | 'FAILED';
    feasible: boolean;
    /**
     * 每个人员/设备槽位都排上了才为 true。
     *
     * 与 `feasible` 是两回事：后者只说明 CP-SAT 找到了解。候选为空的槽位会被
     * `AddExactlyOne({candidates…, gap})` 按构造钉成 gap，所以"一个人都没排上"
     * 同样是可行且最优的。判断"这个方案能直接执行吗"要看这个字段。
     * 旧 solver 产物不带此字段，缺省按 false 处理。
     */
    plan_complete?: boolean;
    timed_out: boolean;
    wall_time_ms: number;
    conflicts: number;
    branches: number;
    best_bound: number;
    total_lateness_minutes: number;
    unresolved_assigned_conflict_order_ids: string[];
    unassigned_unplanned_order_ids: string[];
    objective_stage_results?: ObjectiveStageResult[];
    objective_values?: Record<string, number>;
}

export interface ObjectiveStageResult {
    stage: number;
    objective_value: number;
    wall_time_ms: number;
}

export interface SolverResult {
    order_results: OrderResult[];
    suggestions: OrderResult[];
    personnel_slot_assignments: SlotAssignment[];
    equipment_slot_assignments: SlotAssignment[];
    continuity_decisions: ContinuityDecision[];
    gap_summary: GapSummary;
    continuity_summary: ContinuitySummary;
    change_summary: ChangeSummary;
    travel_summary: TravelSummary;
    objective_breakdown: ObjectiveBreakdown;
    solver_run_metadata: SolverRunMetadata;
    solver_metadata: SolverRunMetadata;
}
