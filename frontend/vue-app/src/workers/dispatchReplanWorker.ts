/**
 * Dispatch Replanning Worker
 * 
 * Web Worker that loads OR-Tools WASM solver dynamically and processes
 * dispatch optimization requests via postMessage communication.
 */

const ACTIVE_MANIFEST_URL = '/frontend/vendor/ortools/active-manifest.json';
const DEFAULT_RUNTIME_MANIFEST_URL = '/frontend/vendor/ortools/runtime-manifest.json';
const ORTOOLS_ASSET_BASE = '/frontend/vendor/ortools/';

type SolverRuntime = {
    solve_cluster: (clusterJson: string) => string;
    runtime_manifest: ORToolsRuntimeManifest;
    active_manifest: ORToolsActiveManifest;
};

type SolverRuntimeLoader = () => Promise<SolverRuntime>;

let solverRuntimePromise: Promise<SolverRuntime> | null = null;

// ============================================================================
// Type Definitions
// ============================================================================

interface ORToolsActiveManifest {
    runtime_manifest_path?: string;
    [key: string]: unknown;
}

interface ORToolsRuntimeManifest {
    js_url?: string;
    wasm_url?: string;
    [key: string]: unknown;
}

export interface PersonnelSlot {
    slot_code?: string;
    candidate_user_ids?: string[];
    baseline_user_id?: string;
    [key: string]: unknown;
}

export interface EquipmentSlot {
    slot_code?: string;
    candidate_equipment_ids?: string[];
    baseline_equipment_id?: string;
    [key: string]: unknown;
}

export interface SlotAssignment {
    slot_code?: string;
    user_id?: string;
    equipment_id?: string;
    [key: string]: unknown;
}

export interface Assignment {
    personnel_slot_assignments?: SlotAssignment[];
    equipment_slot_assignments?: SlotAssignment[];
    member_user_ids?: string[];
    equipment_ids?: string[];
    individual_user_id?: string;
    [key: string]: unknown;
}

export interface Order {
    order_id: string;
    flight_id: string;
    status: string;
    conflict_state: string;
    order_class: string | null;
    stand_id: string | null;
    planned_start_time: string | null;
    planned_end_time: string | null;
    completion_time_mode: string | null;
    completion_target_time: string | null;
    earliest_start_time: string | null;
    latest_start_time: string | null;
    duration_minutes: number;
    required_start_time: string | null;
    actual_start_time: string | null;
    actual_end_time: string | null;
    estimated_completion_time: string | null;
    effective_start_time: string | null;
    effective_end_time: string | null;
    turnaround_pair_key: string | null;
    turnaround_constraint_mode: string | null;
    /**
     * 稠密的 `人数 -> 分钟` 表，下标 k 是排满 k 个人员槽位时的作业时长。
     * 由后端按部门配置展开；null 表示该部门未配置，solver 用 duration_minutes 常量。
     */
    duration_by_crew_size: number[] | null;
    personnel_slots: PersonnelSlot[];
    equipment_slots: EquipmentSlot[];
    baseline_assignment: Assignment;
    current_assignment: Assignment;
    is_optimizable: boolean;
    is_fixed_anchor: boolean;
    is_locked: boolean;
    has_conflict: boolean;
    [key: string]: unknown;
}

export interface AnchorState {
    resource_type: string;
    resource_id: string;
    anchor_order_id: string | null;
    location_stand_id: string | null;
    available_from: string | null;
    free_windows: FreeWindow[];
    [key: string]: unknown;
}

export interface FreeWindow {
    resource_id?: string;
    resource_type?: string;
    start_time?: string;
    end_time?: string;
    [key: string]: unknown;
}

export interface UnavailableBlock {
    resource_id?: string;
    resource_type?: string;
    start_time?: string;
    end_time?: string;
    reason?: string;
    [key: string]: unknown;
}

export interface TravelEdge {
    resource_type?: string;
    resource_id?: string;
    from_node?: string;
    to_node?: string;
    travel_time_minutes?: number;
    [key: string]: unknown;
}

export interface TurnaroundPair {
    pair_key?: string;
    inbound_order_id?: string;
    outbound_order_id?: string;
    hard_continuity_required?: boolean;
    slot_pairs?: TurnaroundSlotPair[];
    inbound_slot_code?: string;
    outbound_slot_code?: string;
    [key: string]: unknown;
}

export interface TurnaroundSlotPair {
    inbound_slot_code?: string;
    outbound_slot_code?: string;
    [key: string]: unknown;
}

export interface ObjectiveConfig {
    [key: string]: unknown;
}

export interface Snapshot {
    cluster_id?: string | null;
    snapshot_id?: string | null;
    solver_version: string;
    model_version?: string | null;
    travel_time_mode?: string;
    objective_config?: ObjectiveConfig;
    unsupported_features?: string[];
    optimizable_orders?: Order[];
    fixed_anchor_orders?: Order[];
    fixed_orders?: Order[];
    orders?: Order[];
    employee_anchor_states?: AnchorState[];
    equipment_anchor_states?: AnchorState[];
    employee_free_windows?: FreeWindow[];
    equipment_free_windows?: FreeWindow[];
    employee_unavailable_blocks?: UnavailableBlock[];
    equipment_unavailable_blocks?: UnavailableBlock[];
    resource_travel_edges?: TravelEdge[];
    turnaround_pairs?: TurnaroundPair[];
    max_suggestions?: number;
    [key: string]: unknown;
}

export interface Cluster {
    cluster_id: string;
    solver_version: string;
    model_version: string;
    objective_config: ObjectiveConfig;
    optimizable_orders: Order[];
    fixed_anchor_orders: Order[];
    employee_anchor_states: AnchorState[];
    equipment_anchor_states: AnchorState[];
    employee_free_windows: FreeWindow[];
    equipment_free_windows: FreeWindow[];
    employee_unavailable_blocks: UnavailableBlock[];
    equipment_unavailable_blocks: UnavailableBlock[];
    resource_travel_edges: TravelEdge[];
    turnaround_pairs: TurnaroundPair[];
    [key: string]: unknown;
}

export interface ObjectiveStageResult {
    stage_name?: string;
    objective_value?: number;
    [key: string]: unknown;
}

export interface SolverRunMetadata {
    solver?: string;
    solver_mode?: string;
    solver_backend?: string;
    solver_version?: string;
    worker_count?: number;
    cluster_count?: number;
    solve_status?: string;
    feasible?: boolean;
    /**
     * True only when every personnel and equipment slot got filled.
     *
     * `feasible` answers a narrower question — whether CP-SAT found a solution.
     * A slot with no candidates is pinned to a gap by construction, so a plan
     * that staffs nobody is feasible and optimal. Treat this, not `feasible`,
     * as "the plan can be worked as-is".
     */
    plan_complete?: boolean;
    timed_out?: boolean;
    /**
     * True when at least one lexicographic stage exhausted its budget without
     * proving optimality. The plan is still feasible and still respects every
     * hard constraint, but the staged objective order was only approximated.
     */
    lexicographic_degraded?: boolean;
    degraded_stages?: string[];
    wall_time_ms?: number;
    conflicts?: number;
    branches?: number;
    best_bound?: number;
    total_lateness_minutes?: number;
    unresolved_assigned_conflict_order_ids?: string[];
    unassigned_unplanned_order_ids?: string[];
    objective_stage_results?: ObjectiveStageResult[];
    error?: string;
    [key: string]: unknown;
}

export interface OrderResult {
    dispatch_order_id?: string;
    gap_count?: number;
    gap_summary?: {
        slot_gap_count?: number;
        [key: string]: unknown;
    };
    lateness_minutes?: number;
    lateness?: {
        minutes?: number;
        [key: string]: unknown;
    };
    continuity_break_count?: number;
    continuity_summary?: {
        break_count?: number;
        [key: string]: unknown;
    };
    baseline_change_count?: number;
    change_summary?: {
        baseline_change_count?: number;
        [key: string]: unknown;
    };
    travel_minutes?: number;
    travel_summary?: {
        minutes?: number;
        [key: string]: unknown;
    };
    objective_breakdown?: {
        scarcity_cost?: number;
        load_deviation?: number;
        [key: string]: unknown;
    };
    [key: string]: unknown;
}

export interface ContinuityDecision {
    [key: string]: unknown;
}

export interface MergeResult {
    order_results: OrderResult[];
    suggestions: OrderResult[];
    personnel_slot_assignments: SlotAssignment[];
    equipment_slot_assignments: SlotAssignment[];
    continuity_decisions: ContinuityDecision[];
    gap_summary: {
        slot_gap_count: number;
        plan_complete?: boolean;
        unresolved_assigned_conflict_order_ids: string[];
        unassigned_unplanned_order_ids: string[];
    };
    continuity_summary: {
        break_count: number;
        penalty: number;
        decisions: ContinuityDecision[];
    };
    change_summary: {
        baseline_change_count: number;
        changed_order_count: number;
    };
    travel_summary: {
        minutes: number;
    };
    objective_breakdown: {
        slot_gap: number;
        total_lateness_minutes: number;
        continuity_break: number;
        continuity_penalty: number;
        baseline_change: number;
        travel_cost: number;
        scarcity_cost: number;
        load_deviation: number;
    };
    solver_run_metadata: SolverRunMetadata;
    solver_metadata: SolverRunMetadata;
    [key: string]: unknown;
}

export interface WorkerResponseSuccess {
    ok: true;
    payload: MergeResult | Record<string, never>;
}

export interface WorkerResponseError {
    ok: false;
    error: string;
}

export type WorkerResponse = WorkerResponseSuccess | WorkerResponseError;

// ============================================================================
// Helper Functions
// ============================================================================

async function fetchJson<T>(url: string, label: string): Promise<T> {
    const response = await fetch(url, {
        cache: 'no-store',
        credentials: 'same-origin',
    });
    if (!response.ok) {
        throw new Error(`${label} load failed: ${response.status}`);
    }
    return response.json() as Promise<T>;
}

function normalizeStaticUrl(value: unknown): string {
    const text = String(value || '').trim().replace(/\\/g, '/');
    if (!text) {
        return '';
    }
    if (/^[a-z][a-z0-9+.-]*:/i.test(text) || text.startsWith('//')) {
        throw new Error('OR-Tools runtime asset path must be same-origin');
    }

    const candidate = text.startsWith('/')
        ? text
        : text.startsWith(ORTOOLS_ASSET_BASE.replace(/^\//, ''))
            ? `/${text}`
            : `${ORTOOLS_ASSET_BASE}${text.replace(/^\/+/, '')}`;
    const locationOrigin = typeof location !== 'undefined' ? location.origin : 'http://localhost';
    const resolved = new URL(candidate, locationOrigin);
    if (resolved.origin !== locationOrigin || !resolved.pathname.startsWith(ORTOOLS_ASSET_BASE)) {
        throw new Error('OR-Tools runtime asset path must stay under /frontend/vendor/ortools/');
    }

    return `${resolved.pathname}${resolved.search}${resolved.hash}`;
}

function ensureText(value: unknown, label: string): string {
    const text = String(value || '').trim();
    if (!text) {
        throw new Error(`${label} is required`);
    }
    return text;
}

function normalizeId(value: unknown): string | null {
    const text = String(value || '').trim();
    return text || null;
}

function normalizeOrder(order: Partial<Order> = {}): Order {
    return {
        ...order,
        order_id: ensureText(order?.order_id || '', 'order_id'),
        flight_id: String(order?.flight_id || '').trim(),
        status: String(order?.status || '').trim(),
        conflict_state: String(order?.conflict_state || '').trim() || 'none',
        order_class: String(order?.order_class || '').trim() || null,
        stand_id: normalizeId(order?.stand_id),
        planned_start_time: order?.planned_start_time || null,
        planned_end_time: order?.planned_end_time || null,
        completion_time_mode: normalizeId(order?.completion_time_mode),
        completion_target_time: order?.completion_target_time || null,
        earliest_start_time: order?.earliest_start_time || null,
        latest_start_time: order?.latest_start_time || null,
        duration_minutes: Number(order?.duration_minutes || 0) || 0,
        required_start_time: order?.required_start_time || null,
        actual_start_time: order?.actual_start_time || null,
        actual_end_time: order?.actual_end_time || null,
        estimated_completion_time: order?.estimated_completion_time || null,
        effective_start_time: order?.effective_start_time || null,
        effective_end_time: order?.effective_end_time || null,
        turnaround_pair_key: normalizeId(order?.turnaround_pair_key),
        turnaround_constraint_mode: normalizeId(order?.turnaround_constraint_mode),
        // 原样透传：整数校验与长度校验都在 solver 的 NormalizeDurationTable 里，
        // 形状不对时它整表退回常量，这里再判一次只会多一处口径。
        duration_by_crew_size: Array.isArray(order?.duration_by_crew_size)
            ? (order.duration_by_crew_size as number[])
            : null,
        personnel_slots: Array.isArray(order?.personnel_slots) ? order.personnel_slots : [],
        equipment_slots: Array.isArray(order?.equipment_slots) ? order.equipment_slots : [],
        baseline_assignment:
            order?.baseline_assignment && typeof order.baseline_assignment === 'object'
                ? (order.baseline_assignment as Assignment)
                : {},
        current_assignment:
            order?.current_assignment && typeof order.current_assignment === 'object'
                ? (order.current_assignment as Assignment)
                : {},
        is_optimizable: order?.is_optimizable !== false,
        is_fixed_anchor: Boolean(order?.is_fixed_anchor),
        is_locked: Boolean(order?.is_locked),
        has_conflict: Boolean(order?.has_conflict),
    };
}

function normalizeAnchorState(anchorState: Partial<AnchorState> = {}): AnchorState {
    const freeWindows = Array.isArray(anchorState?.free_windows) ? anchorState.free_windows : [];
    return {
        resource_type: ensureText(anchorState?.resource_type || '', 'resource_type'),
        resource_id: ensureText(anchorState?.resource_id || '', 'resource_id'),
        anchor_order_id: normalizeId(anchorState?.anchor_order_id),
        location_stand_id: normalizeId(anchorState?.location_stand_id),
        available_from: anchorState?.available_from || null,
        free_windows: freeWindows,
    };
}

function normalizeSnapshot(snapshot: Partial<Snapshot> = {}): Snapshot {
    return {
        cluster_id: normalizeId(snapshot?.cluster_id),
        snapshot_id: normalizeId(snapshot?.snapshot_id),
        solver_version: ensureText(snapshot?.solver_version || '', 'solver_version'),
        model_version: normalizeId(snapshot?.model_version) || 'dispatch_wasm_pdf_full_model_v2',
        travel_time_mode: String(snapshot?.travel_time_mode || 'zero_matrix_forbidden').trim(),
        objective_config:
            snapshot?.objective_config && typeof snapshot.objective_config === 'object'
                ? (snapshot.objective_config as ObjectiveConfig)
                : {},
        unsupported_features: Array.isArray(snapshot?.unsupported_features) ? snapshot.unsupported_features : [],
        optimizable_orders: (
            Array.isArray(snapshot?.optimizable_orders)
                ? snapshot.optimizable_orders
                : Array.isArray(snapshot?.orders)
                ? snapshot.orders
                : []
        ).map(normalizeOrder),
        fixed_anchor_orders: (
            Array.isArray(snapshot?.fixed_anchor_orders)
                ? snapshot.fixed_anchor_orders
                : Array.isArray(snapshot?.fixed_orders)
                ? snapshot.fixed_orders
                : []
        ).map(normalizeOrder),
        employee_anchor_states: (Array.isArray(snapshot?.employee_anchor_states) ? snapshot.employee_anchor_states : []).map(
            normalizeAnchorState,
        ),
        equipment_anchor_states: (Array.isArray(snapshot?.equipment_anchor_states) ? snapshot.equipment_anchor_states : []).map(
            normalizeAnchorState,
        ),
        employee_free_windows: Array.isArray(snapshot?.employee_free_windows) ? snapshot.employee_free_windows : [],
        equipment_free_windows: Array.isArray(snapshot?.equipment_free_windows) ? snapshot.equipment_free_windows : [],
        employee_unavailable_blocks: Array.isArray(snapshot?.employee_unavailable_blocks)
            ? snapshot.employee_unavailable_blocks
            : [],
        equipment_unavailable_blocks: Array.isArray(snapshot?.equipment_unavailable_blocks)
            ? snapshot.equipment_unavailable_blocks
            : [],
        resource_travel_edges: Array.isArray(snapshot?.resource_travel_edges) ? snapshot.resource_travel_edges : [],
        turnaround_pairs: Array.isArray(snapshot?.turnaround_pairs) ? snapshot.turnaround_pairs : [],
        max_suggestions: Math.max(1, Number(snapshot?.max_suggestions || 20) || 20),
    };
}

async function ensureSolverReady(): Promise<SolverRuntime> {
    if (!solverRuntimePromise) {
        solverRuntimePromise = (async () => {
            const activeManifest = await fetchJson<ORToolsActiveManifest>(ACTIVE_MANIFEST_URL, 'OR-Tools active manifest');
            const runtimeManifestPath = normalizeStaticUrl(activeManifest?.runtime_manifest_path) || DEFAULT_RUNTIME_MANIFEST_URL;
            if (!runtimeManifestPath) {
                throw new Error('OR-Tools active manifest missing runtime_manifest_path');
            }

            let runtimeManifest: ORToolsRuntimeManifest;
            try {
                runtimeManifest = await fetchJson<ORToolsRuntimeManifest>(runtimeManifestPath, 'OR-Tools runtime manifest');
            } catch (error) {
                const detail = error instanceof Error ? error.message : String(error || 'runtime manifest missing');
                throw new Error(
                    `OR-Tools runtime missing; install local release via scripts/ortools/install_local_release.py or fetch published assets (${detail})`,
                );
            }

            const jsUrl = normalizeStaticUrl(runtimeManifest?.js_url);
            const wasmUrl = normalizeStaticUrl(runtimeManifest?.wasm_url);
            if (!jsUrl || !wasmUrl) {
                throw new Error('OR-Tools runtime manifest missing js_url or wasm_url');
            }

            // Dynamic import for WASM solver - works in Vite worker with module type
            const moduleNamespace = await import(/* @vite-ignore */ jsUrl);
            const initModule =
                typeof moduleNamespace?.default === 'function' ? (moduleNamespace.default as (opts: { locateFile: (path: string) => string }) => Promise<unknown>) : null;
            if (!initModule) {
                throw new Error('OR-Tools solver module missing default initializer');
            }

            const initializedModule = await initModule({
                locateFile(path: string) {
                    if (String(path || '').endsWith('.wasm')) {
                        return wasmUrl;
                    }
                    return path;
                },
            });

            const solveCluster =
                typeof (initializedModule as Record<string, unknown>)?.solve_cluster === 'function'
                    ? ((initializedModule as Record<string, unknown>).solve_cluster as (clusterJson: string) => string).bind(initializedModule)
                    : typeof moduleNamespace?.solve_cluster === 'function'
                    ? (moduleNamespace.solve_cluster as (clusterJson: string) => string)
                    : null;

            if (!solveCluster) {
                throw new Error('OR-Tools solver export solve_cluster not found');
            }

            return {
                solve_cluster: solveCluster,
                runtime_manifest: runtimeManifest,
                active_manifest: activeManifest,
            };
        })().catch((error) => {
            solverRuntimePromise = null;
            throw error;
        });
    }
    return solverRuntimePromise;
}

function buildOrderResourceKeys(order: Order): string[] {
    const keys = new Set<string>();

    const baselineAssignment =
        order?.baseline_assignment && typeof order.baseline_assignment === 'object'
            ? order.baseline_assignment
            : {};
    const currentAssignment =
        order?.current_assignment && typeof order.current_assignment === 'object'
            ? order.current_assignment
            : {};

    const pushUser = (userId: unknown) => {
        const normalized = normalizeId(userId);
        if (normalized) {
            keys.add(`employee:${normalized}`);
        }
    };
    const pushEquipment = (equipmentId: unknown) => {
        const normalized = normalizeId(equipmentId);
        if (normalized) {
            keys.add(`equipment:${normalized}`);
        }
    };

    (Array.isArray(order?.personnel_slots) ? order.personnel_slots : []).forEach((slot) => {
        (Array.isArray(slot?.candidate_user_ids) ? slot.candidate_user_ids : []).forEach(pushUser);
        pushUser(slot?.baseline_user_id);
    });
    (Array.isArray(order?.equipment_slots) ? order.equipment_slots : []).forEach((slot) => {
        (Array.isArray(slot?.candidate_equipment_ids) ? slot.candidate_equipment_ids : []).forEach(pushEquipment);
        pushEquipment(slot?.baseline_equipment_id);
    });
    (Array.isArray(baselineAssignment?.personnel_slot_assignments) ? baselineAssignment.personnel_slot_assignments : []).forEach(
        (item) => pushUser(item?.user_id),
    );
    (Array.isArray(baselineAssignment?.equipment_slot_assignments) ? baselineAssignment.equipment_slot_assignments : []).forEach(
        (item) => pushEquipment(item?.equipment_id),
    );
    (Array.isArray(currentAssignment?.member_user_ids) ? currentAssignment.member_user_ids : []).forEach(pushUser);
    (Array.isArray(currentAssignment?.equipment_ids) ? currentAssignment.equipment_ids : []).forEach(pushEquipment);
    pushUser(currentAssignment?.individual_user_id);

    return Array.from(keys);
}

function findFixedOnlyHardConflict(snapshot: Snapshot): string | null {
    const fixedById = new Map(snapshot.fixed_anchor_orders?.map((order) => [order.order_id, order]) ?? []);
    for (const pair of snapshot.turnaround_pairs ?? []) {
        if (!pair?.hard_continuity_required) {
            continue;
        }
        const inbound = fixedById.get(normalizeId(pair?.inbound_order_id) ?? '');
        const outbound = fixedById.get(normalizeId(pair?.outbound_order_id) ?? '');
        if (!inbound || !outbound) {
            continue;
        }
        const slotPairs =
            Array.isArray(pair?.slot_pairs) && pair.slot_pairs.length > 0
                ? pair.slot_pairs
                : [{ inbound_slot_code: pair?.inbound_slot_code, outbound_slot_code: pair?.outbound_slot_code }];
        for (const slotPair of slotPairs) {
            const inboundUserId = fixedSlotUserId(inbound, slotPair?.inbound_slot_code ?? '');
            const outboundUserId = fixedSlotUserId(outbound, slotPair?.outbound_slot_code ?? '');
            if (inboundUserId && outboundUserId && inboundUserId !== outboundUserId) {
                return `fixed-anchor hard continuity conflict for pair ${String(pair?.pair_key || '').trim() || 'unknown'}`;
            }
        }
    }
    return null;
}

function fixedSlotUserId(order: Order, slotCode: string): string | null {
    const assignments = Array.isArray(order?.baseline_assignment?.personnel_slot_assignments)
        ? order.baseline_assignment.personnel_slot_assignments
        : [];
    const match = assignments.find((item) => String(item?.slot_code || '').trim() === String(slotCode || '').trim());
    return normalizeId(match?.user_id);
}

function buildWasmClusters(snapshot: Snapshot): Cluster[] {
    const normalized = normalizeSnapshot(snapshot);
    if ((normalized.unsupported_features ?? []).length > 0) {
        throw new Error(`snapshot has unsupported_features: ${(normalized.unsupported_features ?? []).join(', ')}`);
    }

    const orders = (normalized.optimizable_orders ?? []).filter((order) => !order.is_locked);
    if (orders.length === 0) {
        const fixedOnlyConflict = findFixedOnlyHardConflict(normalized);
        if (fixedOnlyConflict) {
            throw new Error(fixedOnlyConflict);
        }
        return [];
    }

    // Union-Find data structure
    const parent = new Map<string, string>();
    const find = (id: string): string => {
        const current = parent.get(id) || id;
        if (current === id) {
            parent.set(id, id);
            return id;
        }
        const root = find(current);
        parent.set(id, root);
        return root;
    };
    const union = (left: string, right: string) => {
        const leftRoot = find(left);
        const rightRoot = find(right);
        if (leftRoot !== rightRoot) {
            parent.set(rightRoot, leftRoot);
        }
    };

    const resourceOwners = new Map<string, string>();
    orders.forEach((order) => {
        const orderId = order.order_id;
        parent.set(orderId, orderId);
        buildOrderResourceKeys(order).forEach((resourceKey) => {
            const existing = resourceOwners.get(resourceKey);
            if (existing) {
                union(existing, orderId);
            } else {
                resourceOwners.set(resourceKey, orderId);
            }
        });
    });

    const turnaroundPairs = Array.isArray(normalized.turnaround_pairs) ? normalized.turnaround_pairs : [];
    turnaroundPairs.forEach((pair) => {
        const inboundOrderId = normalizeId(pair?.inbound_order_id);
        const outboundOrderId = normalizeId(pair?.outbound_order_id);
        if (inboundOrderId && outboundOrderId && parent.has(inboundOrderId) && parent.has(outboundOrderId)) {
            union(inboundOrderId, outboundOrderId);
        }
    });

    const grouped = new Map<string, Order[]>();
    orders.forEach((order) => {
        const root = find(order.order_id);
        if (!grouped.has(root)) {
            grouped.set(root, []);
        }
        grouped.get(root)!.push(order);
    });

    const anchorKey = (anchorState: AnchorState): string => {
        const resourceType = String(anchorState?.resource_type || '').trim();
        const resourceId = String(anchorState?.resource_id || '').trim();
        return resourceType && resourceId ? `${resourceType}:${resourceId}` : '';
    };

    return Array.from(grouped.entries()).map(([clusterId, clusterOrders]) => {
        const orderIdSet = new Set(clusterOrders.map((item) => item.order_id));
        const resourceKeySet = new Set<string>();
        clusterOrders.forEach((order) => {
            buildOrderResourceKeys(order).forEach((resourceKey) => resourceKeySet.add(resourceKey));
        });

        let clusterFixedAnchorOrders = (normalized.fixed_anchor_orders ?? []).filter((order) => {
            const orderId = order.order_id;
            if (!orderId || orderIdSet.has(orderId)) {
                return false;
            }
            return buildOrderResourceKeys(order).some((resourceKey) => resourceKeySet.has(resourceKey));
        });

        const fixedOrderIdSet = new Set(clusterFixedAnchorOrders.map((order) => order.order_id));
        turnaroundPairs.forEach((pair) => {
            const inboundOrderId = normalizeId(pair?.inbound_order_id);
            const outboundOrderId = normalizeId(pair?.outbound_order_id);
            const fixedOrder = (normalized.fixed_anchor_orders ?? []).find(
                (order) =>
                    order.order_id &&
                    !orderIdSet.has(order.order_id) &&
                    (order.order_id === inboundOrderId || order.order_id === outboundOrderId),
            );
            const otherOrderId =
                fixedOrder && fixedOrder.order_id === inboundOrderId ? outboundOrderId : inboundOrderId;
            if (fixedOrder && otherOrderId && orderIdSet.has(otherOrderId) && !fixedOrderIdSet.has(fixedOrder.order_id)) {
                clusterFixedAnchorOrders = [...clusterFixedAnchorOrders, fixedOrder];
                fixedOrderIdSet.add(fixedOrder.order_id);
            }
        });

        const clusterTurnaroundPairs = turnaroundPairs.filter((pair) => {
            const inboundOrderId = normalizeId(pair?.inbound_order_id);
            const outboundOrderId = normalizeId(pair?.outbound_order_id);
            return (
                inboundOrderId &&
                outboundOrderId &&
                (orderIdSet.has(inboundOrderId) || fixedOrderIdSet.has(inboundOrderId)) &&
                (orderIdSet.has(outboundOrderId) || fixedOrderIdSet.has(outboundOrderId))
            );
        });

        const clusterOrderNodeSet = new Set(
            [
                ...clusterOrders.map((item) => `order:${item.order_id}`),
                ...clusterFixedAnchorOrders.map((item) => `order:${item.order_id}`),
            ],
        );

        return {
            cluster_id: String(clusterId || ''),
            solver_version: normalized.solver_version,
            model_version: normalized.model_version ?? 'dispatch_wasm_pdf_full_model_v2',
            objective_config: normalized.objective_config ?? {},
            optimizable_orders: clusterOrders,
            fixed_anchor_orders: clusterFixedAnchorOrders,
            employee_anchor_states: (normalized.employee_anchor_states ?? []).filter((anchorState) =>
                resourceKeySet.has(anchorKey(anchorState)),
            ),
            equipment_anchor_states: (normalized.equipment_anchor_states ?? []).filter((anchorState) =>
                resourceKeySet.has(anchorKey(anchorState)),
            ),
            employee_free_windows: (normalized.employee_free_windows ?? []).filter((window) =>
                resourceKeySet.has(`employee:${String(window?.resource_id || '').trim()}`),
            ),
            equipment_free_windows: (normalized.equipment_free_windows ?? []).filter((window) =>
                resourceKeySet.has(`equipment:${String(window?.resource_id || '').trim()}`),
            ),
            employee_unavailable_blocks: (normalized.employee_unavailable_blocks ?? []).filter((block) =>
                resourceKeySet.has(`employee:${String(block?.resource_id || '').trim()}`),
            ),
            equipment_unavailable_blocks: (normalized.equipment_unavailable_blocks ?? []).filter((block) =>
                resourceKeySet.has(`equipment:${String(block?.resource_id || '').trim()}`),
            ),
            resource_travel_edges: (normalized.resource_travel_edges ?? []).filter((edge) => {
                const resourceKey = `${String(edge?.resource_type || '').trim()}:${String(edge?.resource_id || '').trim()}`;
                return (
                    resourceKeySet.has(resourceKey) &&
                    (clusterOrderNodeSet.has(String(edge?.from_node || '').trim()) ||
                        String(edge?.from_node || '').startsWith('anchor:')) &&
                    (clusterOrderNodeSet.has(String(edge?.to_node || '').trim()) ||
                        String(edge?.to_node || '').startsWith('anchor:'))
                );
            }),
            turnaround_pairs: clusterTurnaroundPairs,
        };
    });
}

function mergeSolverResults(snapshot: Snapshot, clusterResults: unknown[]): MergeResult {
    const maxSuggestions = Math.max(1, Number(snapshot?.max_suggestions || 20) || 20);

    const allOrderResults: OrderResult[] = clusterResults
        .flatMap<OrderResult>((item) => (Array.isArray((item as Record<string, unknown>)?.order_results) ? (item as Record<string, unknown>).order_results as OrderResult[] : []))
        .sort((left, right) => {
            return (
                Number((left as OrderResult)?.gap_count || (left as OrderResult)?.gap_summary?.slot_gap_count || 0) -
                    Number((right as OrderResult)?.gap_count || (right as OrderResult)?.gap_summary?.slot_gap_count || 0) ||
                Number((left as OrderResult)?.lateness_minutes || (left as OrderResult)?.lateness?.minutes || 0) -
                    Number((right as OrderResult)?.lateness_minutes || (right as OrderResult)?.lateness?.minutes || 0) ||
                Number((left as OrderResult)?.continuity_break_count || (left as OrderResult)?.continuity_summary?.break_count || 0) -
                    Number((right as OrderResult)?.continuity_break_count || (right as OrderResult)?.continuity_summary?.break_count || 0) ||
                Number((left as OrderResult)?.baseline_change_count || (left as OrderResult)?.change_summary?.baseline_change_count || 0) -
                    Number((right as OrderResult)?.baseline_change_count || (right as OrderResult)?.change_summary?.baseline_change_count || 0) ||
                Number((left as OrderResult)?.travel_minutes || (left as OrderResult)?.travel_summary?.minutes || 0) -
                    Number((right as OrderResult)?.travel_minutes || (right as OrderResult)?.travel_summary?.minutes || 0) ||
                Number((left as OrderResult)?.objective_breakdown?.scarcity_cost || 0) -
                    Number((right as OrderResult)?.objective_breakdown?.scarcity_cost || 0) ||
                Number((left as OrderResult)?.objective_breakdown?.load_deviation || 0) -
                    Number((right as OrderResult)?.objective_breakdown?.load_deviation || 0) ||
                String((left as OrderResult)?.dispatch_order_id || '').localeCompare(String((right as OrderResult)?.dispatch_order_id || ''))
            );
        });

    const orderResults = allOrderResults.slice(0, maxSuggestions);

    const mergeNumericFields = (field: string): number =>
        clusterResults.reduce<number>((sum, item) => sum + Number(((item as Record<string, unknown>)?.objective_breakdown as Record<string, unknown>)?.[field] || 0), 0);

    const allStageResults: ObjectiveStageResult[] = clusterResults.flatMap((item) =>
        Array.isArray(((item as Record<string, unknown>)?.solver_run_metadata as Record<string, unknown>)?.objective_stage_results)
            ? ((item as Record<string, unknown>)?.solver_run_metadata as Record<string, unknown>).objective_stage_results as ObjectiveStageResult[]
            : [],
    );

    const solverRunMetadata: SolverRunMetadata = {
        solver:
            clusterResults
                .map((item) => String(((item as Record<string, unknown>)?.solver_run_metadata as Record<string, unknown>)?.solver || '').trim())
                .find(Boolean) || 'dispatch_solver_ortools_wasm_strict_pdf_v3',
        solver_mode: 'frontend_wasm',
        solver_backend: 'ortools_cp_sat_wasm',
        solver_version: snapshot?.solver_version || '',
        worker_count: 1,
        cluster_count: clusterResults.length,
        solve_status: clusterResults.every(
            (item) =>
                String(((item as Record<string, unknown>)?.solver_run_metadata as Record<string, unknown>)?.solve_status || '')
                    .trim()
                    .toUpperCase() === 'OPTIMAL',
        )
            ? 'OPTIMAL'
            : clusterResults.every((item) =>
                  ['OPTIMAL', 'FEASIBLE'].includes(
                      String(((item as Record<string, unknown>)?.solver_run_metadata as Record<string, unknown>)?.solve_status || '').trim().toUpperCase(),
                  ),
              )
            ? 'FEASIBLE'
            : 'FAILED',
        feasible: clusterResults.every((item) => ((item as Record<string, unknown>)?.solver_run_metadata as Record<string, unknown>)?.feasible !== false),
        // `=== true`, not `!== false`: a cluster solved by a solver build that
        // predates this flag reports nothing, and "didn't say" must not read as
        // "complete". Unlike `feasible`, the safe default here is the pessimistic one.
        plan_complete: clusterResults.every(
            (item) => ((item as Record<string, unknown>)?.solver_run_metadata as Record<string, unknown>)?.plan_complete === true,
        ),
        timed_out: clusterResults.some((item) => Boolean(((item as Record<string, unknown>)?.solver_run_metadata as Record<string, unknown>)?.timed_out)),
        lexicographic_degraded: clusterResults.some((item) =>
            Boolean(((item as Record<string, unknown>)?.solver_run_metadata as Record<string, unknown>)?.lexicographic_degraded),
        ),
        degraded_stages: Array.from(
            new Set(
                clusterResults.flatMap((item) => {
                    const stages = ((item as Record<string, unknown>)?.solver_run_metadata as Record<string, unknown>)
                        ?.degraded_stages;
                    return Array.isArray(stages) ? stages.map((stage) => String(stage)) : [];
                }),
            ),
        ),
        wall_time_ms: clusterResults.reduce<number>(
            (sum, item) => sum + Number(((item as Record<string, unknown>)?.solver_run_metadata as Record<string, unknown>)?.wall_time_ms || 0),
            0,
        ),
        conflicts: clusterResults.reduce<number>(
            (sum, item) => sum + Number(((item as Record<string, unknown>)?.solver_run_metadata as Record<string, unknown>)?.conflicts || 0),
            0,
        ),
        branches: clusterResults.reduce<number>(
            (sum, item) => sum + Number(((item as Record<string, unknown>)?.solver_run_metadata as Record<string, unknown>)?.branches || 0),
            0,
        ),
        best_bound: clusterResults.reduce<number>(
            (sum, item) => sum + Number(((item as Record<string, unknown>)?.solver_run_metadata as Record<string, unknown>)?.best_bound || 0),
            0,
        ),
        total_lateness_minutes: clusterResults.reduce<number>(
            (sum, item) => sum + Number(((item as Record<string, unknown>)?.solver_run_metadata as Record<string, unknown>)?.total_lateness_minutes || 0),
            0,
        ),
        unresolved_assigned_conflict_order_ids: Array.from<string>(
            new Set(
                clusterResults.flatMap((item) =>
                    Array.isArray(((item as Record<string, unknown>)?.solver_run_metadata as Record<string, unknown>)?.unresolved_assigned_conflict_order_ids)
                        ? ((item as Record<string, unknown>)?.solver_run_metadata as Record<string, unknown>).unresolved_assigned_conflict_order_ids as string[]
                        : [],
                ),
            ),
        ),
        unassigned_unplanned_order_ids: Array.from<string>(
            new Set(
                clusterResults.flatMap((item) =>
                    Array.isArray(((item as Record<string, unknown>)?.solver_run_metadata as Record<string, unknown>)?.unassigned_unplanned_order_ids)
                        ? ((item as Record<string, unknown>)?.solver_run_metadata as Record<string, unknown>).unassigned_unplanned_order_ids as string[]
                        : [],
                ),
            ),
        ),
        objective_stage_results: allStageResults as ObjectiveStageResult[],
        objective_values: {
            slot_gap: mergeNumericFields('slot_gap'),
            total_lateness_minutes: mergeNumericFields('total_lateness_minutes'),
            continuity_break: mergeNumericFields('continuity_break'),
            continuity_penalty: mergeNumericFields('continuity_penalty'),
            baseline_change: mergeNumericFields('baseline_change'),
            travel_cost: mergeNumericFields('travel_cost'),
            scarcity_cost: mergeNumericFields('scarcity_cost'),
            load_deviation: mergeNumericFields('load_deviation'),
        },
    };

    return {
        order_results: orderResults,
        suggestions: orderResults,
        personnel_slot_assignments: clusterResults.flatMap<SlotAssignment>((item) =>
            Array.isArray((item as Record<string, unknown>)?.personnel_slot_assignments)
                ? (item as Record<string, unknown>).personnel_slot_assignments as SlotAssignment[]
                : [],
        ),
        equipment_slot_assignments: clusterResults.flatMap<SlotAssignment>((item) =>
            Array.isArray((item as Record<string, unknown>)?.equipment_slot_assignments)
                ? (item as Record<string, unknown>).equipment_slot_assignments as SlotAssignment[]
                : [],
        ),
        continuity_decisions: clusterResults.flatMap<ContinuityDecision>((item) =>
            Array.isArray((item as Record<string, unknown>)?.continuity_decisions)
                ? (item as Record<string, unknown>).continuity_decisions as ContinuityDecision[]
                : [],
        ),
        gap_summary: {
            slot_gap_count: mergeNumericFields('slot_gap'),
            // 与 solver_run_metadata.plan_complete 同源，避免两份摘要各算各的。
            plan_complete: solverRunMetadata.plan_complete,
            unresolved_assigned_conflict_order_ids: Array.from<string>(
                new Set(
                    clusterResults.flatMap<string>((item) =>
                        Array.isArray(
                            ((item as Record<string, unknown>)?.solver_run_metadata as Record<string, unknown>)
                                ?.unresolved_assigned_conflict_order_ids,
                        )
                            ? ((item as Record<string, unknown>)?.solver_run_metadata as Record<string, unknown>)
                                  .unresolved_assigned_conflict_order_ids as string[]
                            : [],
                    ),
                ),
            ),
            unassigned_unplanned_order_ids: Array.from<string>(
                new Set(
                    clusterResults.flatMap<string>((item) =>
                        Array.isArray(
                            ((item as Record<string, unknown>)?.solver_run_metadata as Record<string, unknown>)?.unassigned_unplanned_order_ids,
                        )
                            ? ((item as Record<string, unknown>)?.solver_run_metadata as Record<string, unknown>).unassigned_unplanned_order_ids as string[]
                            : [],
                    ),
                ),
            ),
        },
        continuity_summary: {
            break_count: mergeNumericFields('continuity_break'),
            penalty: mergeNumericFields('continuity_penalty'),
            decisions: clusterResults.flatMap<ContinuityDecision>((item) =>
                Array.isArray((item as Record<string, unknown>)?.continuity_decisions)
                    ? (item as Record<string, unknown>).continuity_decisions as ContinuityDecision[]
                    : [],
            ),
        },
        change_summary: {
            baseline_change_count: mergeNumericFields('baseline_change'),
            changed_order_count: allOrderResults.length,
        },
        travel_summary: {
            minutes: mergeNumericFields('travel_cost'),
        },
        objective_breakdown: {
            slot_gap: mergeNumericFields('slot_gap'),
            total_lateness_minutes: mergeNumericFields('total_lateness_minutes'),
            continuity_break: mergeNumericFields('continuity_break'),
            continuity_penalty: mergeNumericFields('continuity_penalty'),
            baseline_change: mergeNumericFields('baseline_change'),
            travel_cost: mergeNumericFields('travel_cost'),
            scarcity_cost: mergeNumericFields('scarcity_cost'),
            load_deviation: mergeNumericFields('load_deviation'),
        },
        solver_run_metadata: solverRunMetadata,
        solver_metadata: solverRunMetadata,
    };
}

async function solveSnapshot(
    snapshotPayload: Partial<Snapshot>,
    solverRuntimeLoader: SolverRuntimeLoader = ensureSolverReady,
): Promise<MergeResult | Record<string, never>> {
    const normalizedSnapshot = normalizeSnapshot(snapshotPayload);
    const clusters = buildWasmClusters(normalizedSnapshot);

    if (clusters.length === 0) {
        return {};
    }

    const solver = await solverRuntimeLoader();
    const clusterResults: unknown[] = [];
    for (const cluster of clusters) {
        const responseText = solver.solve_cluster(JSON.stringify(cluster));
        const clusterResult = JSON.parse(String(responseText || '{}'));
        const metadata =
            (clusterResult as Record<string, unknown>)?.solver_run_metadata && typeof (clusterResult as Record<string, unknown>).solver_run_metadata === 'object'
                ? (clusterResult as Record<string, unknown>).solver_run_metadata
                : {};
        const solveStatus = String((metadata as Record<string, unknown>)?.solve_status || '').trim().toUpperCase();
        if (
            (metadata as Record<string, unknown>)?.feasible === false ||
            !['OPTIMAL', 'FEASIBLE'].includes(solveStatus)
        ) {
            const detail = String(
                (metadata as Record<string, unknown>)?.error ||
                    (clusterResult as Record<string, unknown>)?.error ||
                    solveStatus ||
                    'solver failure',
            ).trim();
            throw new Error(`OR-Tools cluster ${cluster.cluster_id || 'dispatch-cluster'} failed: ${detail}`);
        }
        // A stage that ran out of budget still yields a feasible plan; flag it
        // instead of throwing, so the caller can render the plan with a warning.
        const degradedStages = (metadata as Record<string, unknown>)?.degraded_stages;
        if (
            (metadata as Record<string, unknown>)?.lexicographic_degraded ||
            (Array.isArray(degradedStages) && degradedStages.length > 0)
        ) {
            console.warn(
                `OR-Tools cluster ${cluster.cluster_id || 'dispatch-cluster'} lexicographic degradation:`,
                Array.isArray(degradedStages) ? degradedStages.join(', ') : 'unknown stages',
            );
        }
        clusterResults.push(clusterResult);
    }

    return mergeSolverResults(normalizedSnapshot, clusterResults);
}

// ============================================================================
// Worker Message Handler
// ============================================================================

// TypeScript typing for self in Web Worker context
declare const self: Worker;

export async function handleDispatchReplanMessage(
    data: Partial<Snapshot>,
    postMessage: (message: WorkerResponse) => void,
    solverRuntimeLoader: SolverRuntimeLoader = ensureSolverReady,
): Promise<void> {
    try {
        const payload = await solveSnapshot(data, solverRuntimeLoader);
        postMessage({ ok: true, payload } as WorkerResponseSuccess);
    } catch (error) {
        postMessage({
            ok: false,
            error: error instanceof Error ? error.message : String(error || 'dispatch worker failed'),
        } as WorkerResponseError);
    }
}

// Handle incoming messages
self.onmessage = async (event: MessageEvent<Snapshot>) => {
    await handleDispatchReplanMessage(event?.data || {}, (message) => self.postMessage(message));
};
