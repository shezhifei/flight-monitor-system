/**
 * useDispatchReplan.ts - Composable for dispatch replanning logic.
 * 
 * Manages the full replan lifecycle:
 * 1. Fetch Snapshot (from backend)
 * 2. Solve (via Web Worker / OR-Tools)
 * 3. Preview (suggestions)
 * 4. Apply (to backend)
 */

import { computed, ref } from 'vue';
import { useApi } from '@/composables/useApi';
import { useToast } from '@/composables/useToast';
import type { ReplanStrategy } from '@/composables/useDispatchBoardData';

export type ReplanMode = 'idle' | 'snapshotting' | 'solving' | 'previewing' | 'applying' | 'error';

export interface SolverMetadata {
  solve_status?: string;
  wall_time_ms?: number;
  conflicts?: number;
  feasible?: boolean;
  /** 槽位全部排满才为 true；`feasible` 只表示求解器找到了解。见 SolverRunMetadata。 */
  plan_complete?: boolean;
  lexicographic_degraded?: boolean;
  degraded_stages?: string[];
  [key: string]: unknown;
}

export interface ReplanSuggestion {
  id: string;
  orderId: string;
  description: string;
  changes?: string[];
  suggestionType?: string;
  [key: string]: unknown;
}

interface ReplanWindowOptions {
  windowStartMs?: number | null;
  windowEndMs?: number | null;
}

function unwrapEnvelope<T>(payload: unknown): T | null {
  if (!payload || typeof payload !== 'object') {
    return null;
  }

  const record = payload as Record<string, unknown>;
  if ('data' in record) {
    return (record.data ?? null) as T | null;
  }

  return payload as T;
}

// Simple Worker Pool manager
class ReplanWorkerPool {
  private worker: Worker | null = null;
  private busy = false;

  get isBusy() { return this.busy; }

  async solve(payload: Record<string, unknown>): Promise<Record<string, unknown> | null> {
    if (this.busy) throw new Error('Solver is busy');
    this.busy = true;

    if (!this.worker) {
      this.worker = new Worker(
        new URL('@/workers/dispatchReplanWorker.ts', import.meta.url),
        { type: 'module' }
      );
    }

    return new Promise((resolve, reject) => {
      const cleanup = () => {
        this.worker!.removeEventListener('message', handleMessage);
        this.worker!.removeEventListener('error', handleError);
        this.busy = false;
      };

      const handleMessage = (e: MessageEvent) => {
        cleanup();
        if (e.data.ok) resolve(e.data.payload);
        else reject(new Error(e.data.error || 'Solver failed'));
      };

      const handleError = (e: ErrorEvent) => {
        cleanup();
        reject(new Error(e.message || 'Worker crashed'));
        this.worker = null; // Kill crashed worker
      };

      this.worker!.addEventListener('message', handleMessage);
      this.worker!.addEventListener('error', handleError);
      this.worker!.postMessage(payload);
    });
  }

  terminate() {
    this.worker?.terminate();
    this.worker = null;
    this.busy = false;
  }
}

const workerPool = new ReplanWorkerPool();

export function useDispatchReplan() {
  const api = useApi();
  const toast = useToast();

  const mode = ref<ReplanMode>('idle');
  const suggestions = ref<ReplanSuggestion[]>([]);
  const solverMetadata = ref<SolverMetadata | null>(null);
  const error = ref<string | null>(null);
  const canApply = computed(() =>
    mode.value === 'previewing'
      && suggestions.value.length > 0
      && solverMetadata.value?.feasible === true
      && solverMetadata.value?.plan_complete === true,
  );
  const snapshot = ref<Record<string, unknown> | null>(null);
  const previewResult = ref<Record<string, unknown> | null>(null);

  async function runReplan(
    strategy: ReplanStrategy,
    maxSuggestions: number,
    windowOptions: ReplanWindowOptions = {},
  ) {
    if (mode.value !== 'idle' && mode.value !== 'previewing' && mode.value !== 'error') return;

    mode.value = 'snapshotting';
    error.value = null;

    try {
      const windowStartMs = Number(windowOptions.windowStartMs ?? NaN);
      const windowEndMs = Number(windowOptions.windowEndMs ?? NaN);
      if (!Number.isFinite(windowStartMs) || !Number.isFinite(windowEndMs)) {
        throw new Error('缺少有效的重排时间窗');
      }

      const params = new URLSearchParams({
        strategy,
        max_suggestions: String(Math.max(1, Math.trunc(maxSuggestions || 20))),
        window_start: new Date(windowStartMs).toISOString(),
        window_end: new Date(windowEndMs).toISOString(),
      });

      // 1. Get Snapshot from backend
      const snapRes = await api.get<Record<string, unknown>>(
        `/api/v2/dispatch-orders/replan-snapshot?${params.toString()}`,
      );
      const snapPayload = unwrapEnvelope<Record<string, unknown>>(snapRes.data);
      if (!snapRes.ok || !snapPayload) throw new Error('Failed to fetch replan snapshot');
      
      snapshot.value = snapPayload;
      mode.value = 'solving';

      // 2. Run Worker Solver
      const result = await workerPool.solve(snapshot.value);
      previewResult.value = result && typeof result === 'object' ? result : null;
      
      // 3. Process Result
      const resultRecord = result ?? {};
      solverMetadata.value = (resultRecord.solver_metadata || resultRecord.solver_run_metadata || {}) as SolverMetadata;
      const rawSuggestions = (resultRecord.order_results || resultRecord.suggestions || []) as Record<string, unknown>[];
      
      suggestions.value = rawSuggestions.map((s: Record<string, unknown>, i: number) => ({
        id: `replan-${i}`,
        orderId: String(s.dispatch_order_id || s.order_id || ''),
        description: String(s.description || s.action || `工单 ${s.dispatch_order_id} 建议调整`),
        changes: (s.changes as string[]) || [],
        suggestionType: String(s.suggestion_type || 'unknown')
      }));

      mode.value = 'previewing';
    } catch (e: unknown) {
      mode.value = 'error';
      const errMsg = (e as { message?: string }).message;
      error.value = errMsg ?? null;
      toast.show('error', `重排失败: ${errMsg ?? ''}`);
    }
  }

  async function applyReplan(strategy: ReplanStrategy) {
    if (!canApply.value) {
      toast.show('warning', '当前重排方案未完整填满人员和设备槽位，不能应用');
      return false;
    }

    mode.value = 'applying';
    try {
      const solverPayload = previewResult.value ?? {};
      const solverRunMetadata = solverPayload.solver_run_metadata as Record<string, unknown> | undefined;
      const res = await api.post('/api/v2/dispatch-orders/replan-apply', {
        snapshot_id: snapshot.value?.snapshot_id,
        solver_version: (snapshot.value?.solver_version as string | undefined) || (solverRunMetadata?.solver_version as string | undefined),
        strategy,
        order_results: Array.isArray(solverPayload.order_results) ? solverPayload.order_results : [],
        personnel_slot_assignments: Array.isArray(solverPayload.personnel_slot_assignments)
          ? solverPayload.personnel_slot_assignments
          : [],
        equipment_slot_assignments: Array.isArray(solverPayload.equipment_slot_assignments)
          ? solverPayload.equipment_slot_assignments
          : [],
        continuity_decisions: Array.isArray(solverPayload.continuity_decisions)
          ? solverPayload.continuity_decisions
          : [],
        objective_breakdown: solverPayload.objective_breakdown || {},
        solver_run_metadata: solverPayload.solver_run_metadata || solverPayload.solver_metadata || {},
      });
      const applyPayload = unwrapEnvelope<Record<string, unknown>>(res.data);

      if (!res.ok || !applyPayload) throw new Error('Apply failed');

      toast.show('success', '重排方案已成功应用');
      clearReplan();
      return true;
    } catch (e: unknown) {
      mode.value = 'previewing'; // Stay in preview on failure
      toast.show('error', `应用失败: ${(e as { message?: string }).message}`);
      return false;
    }
  }

  function clearReplan() {
    mode.value = 'idle';
    suggestions.value = [];
    solverMetadata.value = null;
    error.value = null;
    snapshot.value = null;
    previewResult.value = null;
  }

  return {
    mode,
    suggestions,
    solverMetadata,
    canApply,
    error,
    runReplan,
    applyReplan,
    clearReplan
  };
}
