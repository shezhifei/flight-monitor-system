const ACTIVE_MANIFEST_URL = '/frontend/vendor/ortools/active-manifest.json';
const DEFAULT_RUNTIME_MANIFEST_URL = '/frontend/vendor/ortools/runtime-manifest.json';

let solverRuntimePromise = null;

async function fetchJson(url, label) {
    const response = await fetch(url, {
        cache: 'no-store',
        credentials: 'same-origin'
    });
    if (!response.ok) {
        throw new Error(`${label} load failed: ${response.status}`);
    }
    return response.json();
}

function normalizeStaticUrl(value) {
    const text = String(value || '').trim();
    if (!text) {
        return '';
    }
    if (text.startsWith('http://') || text.startsWith('https://') || text.startsWith('/')) {
        return text;
    }
    return `/${text.replace(/^\/+/, '').replace(/\\/g, '/')}`;
}

function ensureText(value, label) {
    const text = String(value || '').trim();
    if (!text) {
        throw new Error(`${label} is required`);
    }
    return text;
}

function normalizeId(value) {
    const text = String(value || '').trim();
    return text || null;
}

function normalizeOrder(order = {}) {
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
        earliest_start_time: order?.earliest_start_time || null,
        latest_start_time: order?.latest_start_time || null,
        sla_deadline_time: order?.sla_deadline_time || null,
        duration_minutes: Number(order?.duration_minutes || 0) || 0,
        required_start_time: order?.required_start_time || null,
        actual_start_time: order?.actual_start_time || null,
        actual_end_time: order?.actual_end_time || null,
        estimated_completion_time: order?.estimated_completion_time || null,
        effective_start_time: order?.effective_start_time || null,
        effective_end_time: order?.effective_end_time || null,
        turnaround_pair_key: normalizeId(order?.turnaround_pair_key),
        turnaround_constraint_mode: normalizeId(order?.turnaround_constraint_mode),
        personnel_slots: Array.isArray(order?.personnel_slots) ? order.personnel_slots : [],
        equipment_slots: Array.isArray(order?.equipment_slots) ? order.equipment_slots : [],
        baseline_assignment: order?.baseline_assignment && typeof order.baseline_assignment === 'object'
            ? order.baseline_assignment
            : {},
        current_assignment: order?.current_assignment && typeof order.current_assignment === 'object'
            ? order.current_assignment
            : {},
        is_optimizable: order?.is_optimizable !== false,
        is_fixed_anchor: Boolean(order?.is_fixed_anchor),
        is_locked: Boolean(order?.is_locked),
        has_conflict: Boolean(order?.has_conflict)
    };
}

function normalizeAnchorState(anchorState = {}) {
    const freeWindows = Array.isArray(anchorState?.free_windows) ? anchorState.free_windows : [];
    return {
        resource_type: ensureText(anchorState?.resource_type || '', 'resource_type'),
        resource_id: ensureText(anchorState?.resource_id || '', 'resource_id'),
        anchor_order_id: normalizeId(anchorState?.anchor_order_id),
        location_stand_id: normalizeId(anchorState?.location_stand_id),
        available_from: anchorState?.available_from || null,
        free_windows
    };
}

function normalizeSnapshot(snapshot = {}) {
    return {
        cluster_id: normalizeId(snapshot?.cluster_id),
        snapshot_id: normalizeId(snapshot?.snapshot_id),
        solver_version: ensureText(snapshot?.solver_version || '', 'solver_version'),
        model_version: normalizeId(snapshot?.model_version) || 'dispatch_wasm_pdf_full_model_v2',
        travel_time_mode: String(snapshot?.travel_time_mode || 'zero_matrix_forbidden').trim(),
        objective_config: snapshot?.objective_config && typeof snapshot.objective_config === 'object'
            ? snapshot.objective_config
            : {},
        unsupported_features: Array.isArray(snapshot?.unsupported_features) ? snapshot.unsupported_features : [],
        optimizable_orders: (Array.isArray(snapshot?.optimizable_orders) ? snapshot.optimizable_orders : (Array.isArray(snapshot?.orders) ? snapshot.orders : []))
            .map(normalizeOrder),
        fixed_anchor_orders: (Array.isArray(snapshot?.fixed_anchor_orders) ? snapshot.fixed_anchor_orders : (Array.isArray(snapshot?.fixed_orders) ? snapshot.fixed_orders : []))
            .map(normalizeOrder),
        employee_anchor_states: (Array.isArray(snapshot?.employee_anchor_states) ? snapshot.employee_anchor_states : [])
            .map(normalizeAnchorState),
        equipment_anchor_states: (Array.isArray(snapshot?.equipment_anchor_states) ? snapshot.equipment_anchor_states : [])
            .map(normalizeAnchorState),
        employee_free_windows: Array.isArray(snapshot?.employee_free_windows) ? snapshot.employee_free_windows : [],
        equipment_free_windows: Array.isArray(snapshot?.equipment_free_windows) ? snapshot.equipment_free_windows : [],
        employee_unavailable_blocks: Array.isArray(snapshot?.employee_unavailable_blocks) ? snapshot.employee_unavailable_blocks : [],
        equipment_unavailable_blocks: Array.isArray(snapshot?.equipment_unavailable_blocks) ? snapshot.equipment_unavailable_blocks : [],
        resource_travel_edges: Array.isArray(snapshot?.resource_travel_edges) ? snapshot.resource_travel_edges : [],
        turnaround_pairs: Array.isArray(snapshot?.turnaround_pairs) ? snapshot.turnaround_pairs : [],
        max_suggestions: Math.max(1, Number(snapshot?.max_suggestions || 20) || 20)
    };
}

async function ensureSolverReady() {
    if (!solverRuntimePromise) {
        solverRuntimePromise = (async () => {
            const activeManifest = await fetchJson(ACTIVE_MANIFEST_URL, 'OR-Tools active manifest');
            const runtimeManifestPath = normalizeStaticUrl(activeManifest?.runtime_manifest_path) || DEFAULT_RUNTIME_MANIFEST_URL;
            if (!runtimeManifestPath) {
                throw new Error('OR-Tools active manifest missing runtime_manifest_path');
            }
            let runtimeManifest;
            try {
                runtimeManifest = await fetchJson(runtimeManifestPath, 'OR-Tools runtime manifest');
            } catch (error) {
                const detail = error?.message || String(error || 'runtime manifest missing');
                throw new Error(
                    `OR-Tools runtime missing; install local release via scripts/ortools/install_local_release.py or fetch published assets (${detail})`
                );
            }
            const jsUrl = normalizeStaticUrl(runtimeManifest?.js_url);
            const wasmUrl = normalizeStaticUrl(runtimeManifest?.wasm_url);
            if (!jsUrl || !wasmUrl) {
                throw new Error('OR-Tools runtime manifest missing js_url or wasm_url');
            }
            const moduleNamespace = await import(jsUrl);
            const initModule = typeof moduleNamespace?.default === 'function'
                ? moduleNamespace.default
                : null;
            if (!initModule) {
                throw new Error('OR-Tools solver module missing default initializer');
            }
            const initializedModule = await initModule({
                locateFile(path) {
                    if (String(path || '').endsWith('.wasm')) {
                        return wasmUrl;
                    }
                    return path;
                }
            });
            const solveCluster = typeof initializedModule?.solve_cluster === 'function'
                ? initializedModule.solve_cluster.bind(initializedModule)
                : (typeof moduleNamespace?.solve_cluster === 'function'
                    ? moduleNamespace.solve_cluster
                    : null);
            if (!solveCluster) {
                throw new Error('OR-Tools solver export solve_cluster not found');
            }
            return {
                solve_cluster: solveCluster,
                runtime_manifest: runtimeManifest,
                active_manifest: activeManifest
            };
        })().catch((error) => {
            solverRuntimePromise = null;
            throw error;
        });
    }
    return solverRuntimePromise;
}

function buildOrderResourceKeys(order) {
    const keys = new Set();
    const baselineAssignment = order?.baseline_assignment && typeof order.baseline_assignment === 'object'
        ? order.baseline_assignment
        : {};
    const currentAssignment = order?.current_assignment && typeof order.current_assignment === 'object'
        ? order.current_assignment
        : {};

    const pushUser = (userId) => {
        const normalized = normalizeId(userId);
        if (normalized) {
            keys.add(`employee:${normalized}`);
        }
    };
    const pushEquipment = (equipmentId) => {
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
    (Array.isArray(baselineAssignment?.personnel_slot_assignments) ? baselineAssignment.personnel_slot_assignments : []).forEach((item) => pushUser(item?.user_id));
    (Array.isArray(baselineAssignment?.equipment_slot_assignments) ? baselineAssignment.equipment_slot_assignments : []).forEach((item) => pushEquipment(item?.equipment_id));
    (Array.isArray(currentAssignment?.member_user_ids) ? currentAssignment.member_user_ids : []).forEach(pushUser);
    (Array.isArray(currentAssignment?.equipment_ids) ? currentAssignment.equipment_ids : []).forEach(pushEquipment);
    pushUser(currentAssignment?.individual_user_id);

    return Array.from(keys);
}

function buildWasmClusters(snapshot) {
    const normalized = normalizeSnapshot(snapshot);
    if (normalized.unsupported_features.length > 0) {
        throw new Error(`snapshot has unsupported_features: ${normalized.unsupported_features.join(', ')}`);
    }
    const orders = normalized.optimizable_orders.filter((order) => !order.is_locked);
    if (orders.length === 0) {
        const fixedOnlyConflict = findFixedOnlyHardConflict(normalized);
        if (fixedOnlyConflict) {
            throw new Error(fixedOnlyConflict);
        }
        return [];
    }

    const parent = new Map();
    const find = (id) => {
        const current = parent.get(id) || id;
        if (current === id) {
            parent.set(id, id);
            return id;
        }
        const root = find(current);
        parent.set(id, root);
        return root;
    };
    const union = (left, right) => {
        const leftRoot = find(left);
        const rightRoot = find(right);
        if (leftRoot !== rightRoot) {
            parent.set(rightRoot, leftRoot);
        }
    };

    const resourceOwners = new Map();
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

    const grouped = new Map();
    orders.forEach((order) => {
        const root = find(order.order_id);
        if (!grouped.has(root)) {
            grouped.set(root, []);
        }
        grouped.get(root).push(order);
    });

    const anchorKey = (anchorState) => {
        const resourceType = String(anchorState?.resource_type || '').trim();
        const resourceId = String(anchorState?.resource_id || '').trim();
        return resourceType && resourceId ? `${resourceType}:${resourceId}` : '';
    };

    return Array.from(grouped.entries()).map(([clusterId, clusterOrders]) => {
        const orderIdSet = new Set(clusterOrders.map((item) => item.order_id));
        const resourceKeySet = new Set();
        clusterOrders.forEach((order) => {
            buildOrderResourceKeys(order).forEach((resourceKey) => resourceKeySet.add(resourceKey));
        });
        let clusterFixedAnchorOrders = normalized.fixed_anchor_orders.filter((order) => {
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
            const fixedOrder = normalized.fixed_anchor_orders.find((order) => {
                return order.order_id && !orderIdSet.has(order.order_id)
                    && (order.order_id === inboundOrderId || order.order_id === outboundOrderId);
            });
            const otherOrderId = fixedOrder && fixedOrder.order_id === inboundOrderId ? outboundOrderId : inboundOrderId;
            if (fixedOrder && otherOrderId && orderIdSet.has(otherOrderId) && !fixedOrderIdSet.has(fixedOrder.order_id)) {
                clusterFixedAnchorOrders = [...clusterFixedAnchorOrders, fixedOrder];
                fixedOrderIdSet.add(fixedOrder.order_id);
            }
        });
        const clusterTurnaroundPairs = turnaroundPairs.filter((pair) => {
            const inboundOrderId = normalizeId(pair?.inbound_order_id);
            const outboundOrderId = normalizeId(pair?.outbound_order_id);
            return inboundOrderId && outboundOrderId
                && (orderIdSet.has(inboundOrderId) || fixedOrderIdSet.has(inboundOrderId))
                && (orderIdSet.has(outboundOrderId) || fixedOrderIdSet.has(outboundOrderId));
        });
        const clusterOrderNodeSet = new Set([
            ...clusterOrders.map((item) => `order:${item.order_id}`),
            ...clusterFixedAnchorOrders.map((item) => `order:${item.order_id}`)
        ]);
        return {
            cluster_id: String(clusterId || ''),
            solver_version: normalized.solver_version,
            model_version: normalized.model_version,
            objective_config: normalized.objective_config,
            optimizable_orders: clusterOrders,
            fixed_anchor_orders: clusterFixedAnchorOrders,
            employee_anchor_states: normalized.employee_anchor_states.filter((anchorState) => resourceKeySet.has(anchorKey(anchorState))),
            equipment_anchor_states: normalized.equipment_anchor_states.filter((anchorState) => resourceKeySet.has(anchorKey(anchorState))),
            employee_free_windows: normalized.employee_free_windows.filter((window) => resourceKeySet.has(`employee:${String(window?.resource_id || '').trim()}`)),
            equipment_free_windows: normalized.equipment_free_windows.filter((window) => resourceKeySet.has(`equipment:${String(window?.resource_id || '').trim()}`)),
            employee_unavailable_blocks: normalized.employee_unavailable_blocks.filter((block) => resourceKeySet.has(`employee:${String(block?.resource_id || '').trim()}`)),
            equipment_unavailable_blocks: normalized.equipment_unavailable_blocks.filter((block) => resourceKeySet.has(`equipment:${String(block?.resource_id || '').trim()}`)),
            resource_travel_edges: normalized.resource_travel_edges.filter((edge) => {
                const resourceKey = `${String(edge?.resource_type || '').trim()}:${String(edge?.resource_id || '').trim()}`;
                return resourceKeySet.has(resourceKey)
                    && (
                        clusterOrderNodeSet.has(String(edge?.from_node || '').trim())
                        || String(edge?.from_node || '').startsWith('anchor:')
                    )
                    && (
                        clusterOrderNodeSet.has(String(edge?.to_node || '').trim())
                        || String(edge?.to_node || '').startsWith('anchor:')
                    );
            }),
            turnaround_pairs: clusterTurnaroundPairs
        };
    });
}

function fixedSlotUserId(order, slotCode) {
    const assignments = Array.isArray(order?.baseline_assignment?.personnel_slot_assignments)
        ? order.baseline_assignment.personnel_slot_assignments
        : [];
    const match = assignments.find((item) => String(item?.slot_code || '').trim() === String(slotCode || '').trim());
    return normalizeId(match?.user_id);
}

function findFixedOnlyHardConflict(snapshot) {
    const fixedById = new Map(snapshot.fixed_anchor_orders.map((order) => [order.order_id, order]));
    for (const pair of snapshot.turnaround_pairs) {
        if (!pair?.hard_continuity_required) {
            continue;
        }
        const inbound = fixedById.get(normalizeId(pair?.inbound_order_id));
        const outbound = fixedById.get(normalizeId(pair?.outbound_order_id));
        if (!inbound || !outbound) {
            continue;
        }
        const slotPairs = Array.isArray(pair?.slot_pairs) && pair.slot_pairs.length > 0
            ? pair.slot_pairs
            : [{ inbound_slot_code: pair?.inbound_slot_code, outbound_slot_code: pair?.outbound_slot_code }];
        for (const slotPair of slotPairs) {
            const inboundUserId = fixedSlotUserId(inbound, slotPair?.inbound_slot_code);
            const outboundUserId = fixedSlotUserId(outbound, slotPair?.outbound_slot_code);
            if (inboundUserId && outboundUserId && inboundUserId !== outboundUserId) {
                return `fixed-anchor hard continuity conflict for pair ${String(pair?.pair_key || '').trim() || 'unknown'}`;
            }
        }
    }
    return null;
}

function mergeSolverResults(snapshot, clusterResults) {
    const maxSuggestions = Math.max(1, Number(snapshot?.max_suggestions || 20) || 20);
    const allOrderResults = clusterResults
        .flatMap((item) => Array.isArray(item?.order_results) ? item.order_results : [])
        .sort((left, right) => {
            return Number(left?.gap_count || left?.gap_summary?.slot_gap_count || 0)
                - Number(right?.gap_count || right?.gap_summary?.slot_gap_count || 0)
                || Number(left?.lateness_minutes || left?.lateness?.minutes || 0)
                - Number(right?.lateness_minutes || right?.lateness?.minutes || 0)
                || Number(left?.continuity_break_count || left?.continuity_summary?.break_count || 0)
                - Number(right?.continuity_break_count || right?.continuity_summary?.break_count || 0)
                || Number(left?.baseline_change_count || left?.change_summary?.baseline_change_count || 0)
                - Number(right?.baseline_change_count || right?.change_summary?.baseline_change_count || 0)
                || Number(left?.travel_minutes || left?.travel_summary?.minutes || 0)
                - Number(right?.travel_minutes || right?.travel_summary?.minutes || 0)
                || Number(left?.objective_breakdown?.scarcity_cost || 0)
                - Number(right?.objective_breakdown?.scarcity_cost || 0)
                || Number(left?.objective_breakdown?.load_deviation || 0)
                - Number(right?.objective_breakdown?.load_deviation || 0)
                || String(left?.dispatch_order_id || '').localeCompare(String(right?.dispatch_order_id || ''));
        });
    const orderResults = allOrderResults.slice(0, maxSuggestions);

    const mergeNumericFields = (field) => clusterResults.reduce((sum, item) => sum + Number(item?.objective_breakdown?.[field] || 0), 0);
    const allStageResults = clusterResults.flatMap((item) => Array.isArray(item?.solver_run_metadata?.objective_stage_results) ? item.solver_run_metadata.objective_stage_results : []);
    const solverRunMetadata = {
        solver: clusterResults
            .map((item) => String(item?.solver_run_metadata?.solver || '').trim())
            .find(Boolean) || 'dispatch_solver_ortools_wasm_strict_pdf_v3',
        solver_mode: 'frontend_wasm',
        solver_backend: 'ortools_cp_sat_wasm',
        solver_version: snapshot?.solver_version || '',
        worker_count: 1,
        cluster_count: clusterResults.length,
        solve_status: clusterResults.every((item) => String(item?.solver_run_metadata?.solve_status || '').trim().toUpperCase() === 'OPTIMAL')
            ? 'OPTIMAL'
            : (clusterResults.every((item) => ['OPTIMAL', 'FEASIBLE'].includes(String(item?.solver_run_metadata?.solve_status || '').trim().toUpperCase()))
                ? 'FEASIBLE'
                : 'FAILED'),
        feasible: clusterResults.every((item) => item?.solver_run_metadata?.feasible !== false),
        timed_out: clusterResults.some((item) => Boolean(item?.solver_run_metadata?.timed_out)),
        wall_time_ms: clusterResults.reduce((sum, item) => sum + Number(item?.solver_run_metadata?.wall_time_ms || 0), 0),
        conflicts: clusterResults.reduce((sum, item) => sum + Number(item?.solver_run_metadata?.conflicts || 0), 0),
        branches: clusterResults.reduce((sum, item) => sum + Number(item?.solver_run_metadata?.branches || 0), 0),
        best_bound: clusterResults.reduce((sum, item) => sum + Number(item?.solver_run_metadata?.best_bound || 0), 0),
        total_lateness_minutes: clusterResults.reduce((sum, item) => sum + Number(item?.solver_run_metadata?.total_lateness_minutes || 0), 0),
        unresolved_assigned_conflict_order_ids: Array.from(new Set(clusterResults.flatMap((item) => Array.isArray(item?.solver_run_metadata?.unresolved_assigned_conflict_order_ids) ? item.solver_run_metadata.unresolved_assigned_conflict_order_ids : []))),
        unassigned_unplanned_order_ids: Array.from(new Set(clusterResults.flatMap((item) => Array.isArray(item?.solver_run_metadata?.unassigned_unplanned_order_ids) ? item.solver_run_metadata.unassigned_unplanned_order_ids : []))),
        objective_stage_results: allStageResults,
        objective_values: {
            slot_gap: mergeNumericFields('slot_gap'),
            total_lateness_minutes: mergeNumericFields('total_lateness_minutes'),
            continuity_break: mergeNumericFields('continuity_break'),
            continuity_penalty: mergeNumericFields('continuity_penalty'),
            baseline_change: mergeNumericFields('baseline_change'),
            travel_cost: mergeNumericFields('travel_cost'),
            scarcity_cost: mergeNumericFields('scarcity_cost'),
            load_deviation: mergeNumericFields('load_deviation')
        }
    };

    return {
        order_results: orderResults,
        suggestions: orderResults,
        personnel_slot_assignments: clusterResults.flatMap((item) => Array.isArray(item?.personnel_slot_assignments) ? item.personnel_slot_assignments : []),
        equipment_slot_assignments: clusterResults.flatMap((item) => Array.isArray(item?.equipment_slot_assignments) ? item.equipment_slot_assignments : []),
        continuity_decisions: clusterResults.flatMap((item) => Array.isArray(item?.continuity_decisions) ? item.continuity_decisions : []),
        gap_summary: {
            slot_gap_count: mergeNumericFields('slot_gap'),
            unresolved_assigned_conflict_order_ids: Array.from(new Set(clusterResults.flatMap((item) => Array.isArray(item?.solver_run_metadata?.unresolved_assigned_conflict_order_ids) ? item.solver_run_metadata.unresolved_assigned_conflict_order_ids : []))),
            unassigned_unplanned_order_ids: Array.from(new Set(clusterResults.flatMap((item) => Array.isArray(item?.solver_run_metadata?.unassigned_unplanned_order_ids) ? item.solver_run_metadata.unassigned_unplanned_order_ids : [])))
        },
        continuity_summary: {
            break_count: mergeNumericFields('continuity_break'),
            penalty: mergeNumericFields('continuity_penalty'),
            decisions: clusterResults.flatMap((item) => Array.isArray(item?.continuity_decisions) ? item.continuity_decisions : [])
        },
        change_summary: {
            baseline_change_count: mergeNumericFields('baseline_change'),
            changed_order_count: allOrderResults.length
        },
        travel_summary: {
            minutes: mergeNumericFields('travel_cost')
        },
        objective_breakdown: {
            slot_gap: mergeNumericFields('slot_gap'),
            total_lateness_minutes: mergeNumericFields('total_lateness_minutes'),
            continuity_break: mergeNumericFields('continuity_break'),
            continuity_penalty: mergeNumericFields('continuity_penalty'),
            baseline_change: mergeNumericFields('baseline_change'),
            travel_cost: mergeNumericFields('travel_cost'),
            scarcity_cost: mergeNumericFields('scarcity_cost'),
            load_deviation: mergeNumericFields('load_deviation')
        },
        solver_run_metadata: solverRunMetadata,
        solver_metadata: solverRunMetadata
    };
}

async function solveSnapshot(snapshotPayload) {
    const solver = await ensureSolverReady();
    const normalizedSnapshot = normalizeSnapshot(snapshotPayload);
    const clusters = buildWasmClusters(normalizedSnapshot);
    if (clusters.length === 0) {
        return {
            order_results: [],
            suggestions: [],
            personnel_slot_assignments: [],
            equipment_slot_assignments: [],
            continuity_decisions: [],
            gap_summary: {
                slot_gap_count: 0,
                unresolved_assigned_conflict_order_ids: [],
                unassigned_unplanned_order_ids: []
            },
            continuity_summary: {
                break_count: 0,
                penalty: 0,
                decisions: []
            },
            change_summary: {
                baseline_change_count: 0,
                changed_order_count: 0
            },
            travel_summary: {
                minutes: 0
            },
            objective_breakdown: {
                slot_gap: 0,
                total_lateness_minutes: 0,
                continuity_break: 0,
                continuity_penalty: 0,
                baseline_change: 0,
                travel_cost: 0,
                scarcity_cost: 0,
                load_deviation: 0
            },
            solver_run_metadata: {
                solver: 'dispatch_solver_ortools_wasm_strict_pdf_v3',
                solver_mode: 'frontend_wasm',
                solver_backend: 'ortools_cp_sat_wasm',
                solver_version: normalizedSnapshot.solver_version,
                worker_count: 1,
                cluster_count: 0,
                solve_status: 'OPTIMAL',
                feasible: true,
                timed_out: false,
                wall_time_ms: 0,
                conflicts: 0,
                branches: 0,
                best_bound: 0,
                total_lateness_minutes: 0,
                unresolved_assigned_conflict_order_ids: [],
                unassigned_unplanned_order_ids: [],
                objective_stage_results: [],
                objective_values: {
                    slot_gap: 0,
                    total_lateness_minutes: 0,
                    continuity_break: 0,
                    continuity_penalty: 0,
                    baseline_change: 0,
                    travel_cost: 0,
                    scarcity_cost: 0,
                    load_deviation: 0
                }
            },
            solver_metadata: {
                solver: 'dispatch_solver_ortools_wasm_strict_pdf_v3',
                solver_mode: 'frontend_wasm',
                solver_backend: 'ortools_cp_sat_wasm',
                solver_version: normalizedSnapshot.solver_version,
                worker_count: 1,
                cluster_count: 0,
                solve_status: 'OPTIMAL',
                feasible: true,
                timed_out: false,
                wall_time_ms: 0,
                conflicts: 0,
                branches: 0,
                best_bound: 0,
                total_lateness_minutes: 0,
                unresolved_assigned_conflict_order_ids: [],
                unassigned_unplanned_order_ids: [],
                objective_stage_results: [],
                objective_values: {
                    slot_gap: 0,
                    total_lateness_minutes: 0,
                    continuity_break: 0,
                    continuity_penalty: 0,
                    baseline_change: 0,
                    travel_cost: 0,
                    scarcity_cost: 0,
                    load_deviation: 0
                }
            }
        };
    }

    const clusterResults = [];
    for (const cluster of clusters) {
        const responseText = solver.solve_cluster(JSON.stringify(cluster));
        const clusterResult = JSON.parse(String(responseText || '{}'));
        const metadata = clusterResult?.solver_run_metadata && typeof clusterResult.solver_run_metadata === 'object'
            ? clusterResult.solver_run_metadata
            : {};
        const solveStatus = String(metadata?.solve_status || '').trim().toUpperCase();
        if (metadata?.timed_out || metadata?.feasible === false || !['OPTIMAL', 'FEASIBLE'].includes(solveStatus)) {
            const detail = String(metadata?.error || clusterResult?.error || solveStatus || 'solver failure').trim();
            throw new Error(`OR-Tools cluster ${cluster.cluster_id || 'dispatch-cluster'} failed: ${detail}`);
        }
        clusterResults.push(clusterResult);
    }
    return mergeSolverResults(normalizedSnapshot, clusterResults);
}

self.onmessage = async (event) => {
    try {
        const payload = await solveSnapshot(event?.data || {});
        self.postMessage({ ok: true, payload });
    } catch (error) {
        self.postMessage({
            ok: false,
            error: error?.message || String(error || 'dispatch worker failed')
        });
    }
};
