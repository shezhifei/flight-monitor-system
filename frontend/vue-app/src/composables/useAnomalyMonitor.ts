import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import { useApi } from './useApi';
import { useSSE } from './useSSE';
import { useToast } from './useToast';

/** Mirrors Rust `AnomalyResponse` field names. */
export interface Anomaly {
  anomaly_id: string;
  flight_id: string;
  anomaly_type: string;
  severity: string;
  status: string;
  title: string;
  description?: string | null;
  detected_at: string;
  resolved_at?: string | null;
  escalation_level: number;
  last_escalated_at?: string | null;
  linked_todo_id?: string | null;
  rule_id?: string | null;
  context_data?: Record<string, unknown>;
  created_at?: string;
  updated_at?: string;
}

export interface AnomalyStats {
  total: number;
  open: number;
  acknowledged: number;
  resolved: number;
  critical: number;
  escalated: number;
}

export interface AnomalyResolveBody {
  resolve_todo: boolean;
  _note?: string;
}

const ANOMALY_SSE_EVENTS = [
  'initial',
  'anomaly_created',
  'anomaly_updated',
  'anomaly_acknowledged',
  'anomaly_resolved',
] as const;

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function asAnomaly(value: unknown): Anomaly | null {
  if (!isRecord(value)) {
    return null;
  }
  const anomalyId = value.anomaly_id;
  if (typeof anomalyId !== 'string' || !anomalyId.trim()) {
    return null;
  }
  return value as unknown as Anomaly;
}

export function useAnomalyMonitor() {
  const api = useApi();
  const toast = useToast();
  const loading = ref(true);
  const records = ref<Anomaly[]>([]);
  const filters = ref({ status: 'open', type: '', limit: 100 });
  const streamState = ref<'connecting' | 'online' | 'offline'>('connecting');
  const error = ref<string | null>(null);
  const actionError = ref<string | null>(null);
  const actionBusyIds = ref<Set<string>>(new Set());
  const lastUpdatedAt = ref<string | null>(null);
  const statsSnapshot = ref<AnomalyStats | null>(null);

  function computeLocalStats(list: Anomaly[]): AnomalyStats {
    return {
      total: list.length,
      open: list.filter((r) => r.status === 'open').length,
      acknowledged: list.filter((r) => r.status === 'acknowledged').length,
      resolved: list.filter((r) => r.status === 'resolved').length,
      critical: list.filter((r) => r.severity === 'critical').length,
      escalated: list.filter((r) => Number(r.escalation_level) > 0).length,
    };
  }

  const stats = computed(() => {
    return statsSnapshot.value ?? computeLocalStats(records.value || []);
  });

  const filteredRecords = computed(() => {
    let result = records.value || [];
    if (filters.value.status) {
      result = result.filter((r) => r.status === filters.value.status);
    }
    if (filters.value.type) {
      result = result.filter((r) => r.anomaly_type === filters.value.type);
    }
    return result.slice(0, Number(filters.value.limit));
  });

  /** Unwrap list payloads: array | {items} | {data:{items,total}} | {data:[]}. */
  function unwrapList(payload: unknown): Anomaly[] | null {
    if (Array.isArray(payload)) {
      return payload as Anomaly[];
    }
    if (!isRecord(payload)) {
      return null;
    }
    if (Array.isArray(payload.items)) {
      return payload.items as Anomaly[];
    }
    if (Array.isArray(payload.records)) {
      return payload.records as Anomaly[];
    }
    if (Array.isArray(payload.list)) {
      return payload.list as Anomaly[];
    }
    if (payload.data !== undefined) {
      return unwrapList(payload.data);
    }
    return null;
  }

  function unwrapObject(payload: unknown): Record<string, unknown> | null {
    if (!isRecord(payload)) {
      return null;
    }
    if (isRecord(payload.data)) {
      return payload.data;
    }
    return payload;
  }

  function readNumber(record: Record<string, unknown>, key: keyof AnomalyStats): number {
    const value = record[key];
    const numeric = typeof value === 'number' ? value : Number(value);
    return Number.isFinite(numeric) ? numeric : 0;
  }

  function hasStatsPayload(record: Record<string, unknown>): boolean {
    return ['total', 'open', 'acknowledged', 'resolved', 'critical', 'escalated']
      .some((key) => record[key] !== undefined && record[key] !== null);
  }

  function readErrorMessage(payload: unknown, fallback: string): string {
    if (typeof payload === 'string' && payload.trim()) {
      return payload;
    }
    if (isRecord(payload)) {
      const detail = payload.detail;
      if (typeof detail === 'string' && detail.trim()) {
        return detail;
      }
      const message = payload.message ?? payload.error;
      if (typeof message === 'string' && message.trim()) {
        return message;
      }
    }
    return fallback;
  }

  function setActionBusy(id: string, busy: boolean): void {
    const next = new Set(actionBusyIds.value);
    if (busy) {
      next.add(id);
    } else {
      next.delete(id);
    }
    actionBusyIds.value = next;
  }

  function isActionBusy(id: string): boolean {
    return actionBusyIds.value.has(id);
  }

  function markUpdated(): void {
    lastUpdatedAt.value = new Date().toISOString();
  }

  function upsertAnomaly(next: Anomaly): void {
    const idx = records.value.findIndex((r) => r.anomaly_id === next.anomaly_id);
    if (idx >= 0) {
      const copy = records.value.slice();
      copy[idx] = { ...copy[idx], ...next };
      records.value = copy;
    } else {
      records.value = [next, ...records.value];
    }
    statsSnapshot.value = computeLocalStats(records.value);
    markUpdated();
  }

  function patchAnomaly(anomalyId: string, patch: Record<string, unknown>): void {
    const idx = records.value.findIndex((r) => r.anomaly_id === anomalyId);
    if (idx < 0) {
      return;
    }
    const copy = records.value.slice();
    copy[idx] = { ...copy[idx], ...patch } as Anomaly;
    records.value = copy;
    statsSnapshot.value = computeLocalStats(records.value);
    markUpdated();
  }

  function handleSsePayload(eventName: string, payload: unknown): void {
    if (eventName === 'initial') {
      const list = unwrapList(payload);
      if (list) {
        records.value = list;
        statsSnapshot.value = computeLocalStats(list);
        markUpdated();
      }
      return;
    }

    if (!isRecord(payload)) {
      return;
    }

    // Full anomaly object (created / push)
    const full = asAnomaly(payload);
    if (full && (eventName === 'anomaly_created' || eventName === 'anomaly')) {
      upsertAnomaly(full);
      return;
    }

    const anomalyId = typeof payload.anomaly_id === 'string' ? payload.anomaly_id : '';
    if (!anomalyId) {
      return;
    }

    if (eventName === 'anomaly_acknowledged') {
      patchAnomaly(anomalyId, { ...payload, status: 'acknowledged' });
      return;
    }
    if (eventName === 'anomaly_resolved') {
      patchAnomaly(anomalyId, { ...payload, status: 'resolved' });
      return;
    }
    if (eventName === 'anomaly_updated' || eventName === 'anomaly_created') {
      if (full) {
        upsertAnomaly(full);
      } else {
        patchAnomaly(anomalyId, payload);
      }
    }
  }

  async function fetchRecords() {
    loading.value = true;
    error.value = null;
    try {
      const params = new URLSearchParams();
      if (filters.value.status) params.set('status', filters.value.status);
      if (filters.value.type) params.set('anomaly_type', filters.value.type);
      params.set('limit', String(filters.value.limit));
      const res = await api.get(`/api/v2/anomalies?${params.toString()}`);
      const list = unwrapList(res.data);
      if (!res.ok || !list) {
        throw new Error(readErrorMessage(res.data, `异常记录加载失败: HTTP ${res.status}`));
      }
      records.value = list;
      statsSnapshot.value = computeLocalStats(list);
      markUpdated();
    } catch (err) {
      console.error('Failed to fetch anomalies:', err);
      error.value = err instanceof Error ? err.message : '异常记录加载失败';
      toast.showToast('error', error.value);
      // Fail-closed: do not inject demo anomalies.
    } finally {
      loading.value = false;
    }
  }

  async function fetchStats() {
    try {
      const res = await api.get('/api/v2/anomalies/stats');
      const payload = unwrapObject(res.data);
      if (!res.ok || !payload) {
        throw new Error(readErrorMessage(res.data, `异常统计加载失败: HTTP ${res.status}`));
      }
      if (!hasStatsPayload(payload)) {
        return;
      }
      statsSnapshot.value = {
        total: readNumber(payload, 'total'),
        open: readNumber(payload, 'open'),
        acknowledged: readNumber(payload, 'acknowledged'),
        resolved: readNumber(payload, 'resolved'),
        critical: readNumber(payload, 'critical'),
        escalated: readNumber(payload, 'escalated'),
      };
    } catch (err) {
      console.error('Failed to fetch anomaly stats:', err);
      error.value = err instanceof Error ? err.message : '异常统计加载失败';
      toast.showToast('error', error.value);
    }
  }

  async function updateStatus(
    id: string,
    path: string,
    nextStatus: string,
    body?: AnomalyResolveBody,
  ) {
    actionError.value = null;
    setActionBusy(id, true);
    try {
      const res = body === undefined
        ? await api.post(`/api/v2/anomalies/${id}/${path}`)
        : await api.post(`/api/v2/anomalies/${id}/${path}`, body);
      if (!res.ok) {
        throw new Error(readErrorMessage(res.data, `异常处置失败: HTTP ${res.status}`));
      }
      const r = records.value.find((item) => item.anomaly_id === id);
      if (r) r.status = nextStatus;
      statsSnapshot.value = computeLocalStats(records.value || []);
      await fetchStats();
    } catch (err) {
      console.error(`Failed to ${path} anomaly:`, err);
      actionError.value = err instanceof Error ? err.message : '异常处置失败';
      throw err;
    } finally {
      setActionBusy(id, false);
    }
  }

  async function acknowledge(id: string) {
    await updateStatus(id, 'acknowledge', 'acknowledged');
  }

  async function resolve(id: string, options?: Partial<AnomalyResolveBody>) {
    const body: AnomalyResolveBody = {
      resolve_todo: options?.resolve_todo ?? true,
    };
    if (options?._note !== undefined) {
      body._note = options._note;
    }
    await updateStatus(id, 'resolve', 'resolved', body);
  }

  const { connect, disconnect, on, status: sseStatus } = useSSE({
    url: '/api/v2/anomalies/stream',
  });

  watch(sseStatus, (next) => {
    if (next === 'online' || next === 'offline' || next === 'connecting') {
      streamState.value = next;
    } else if (next === 'reconnecting') {
      streamState.value = 'connecting';
    }
  }, { immediate: true });

  for (const eventName of ANOMALY_SSE_EVENTS) {
    on(eventName, (event) => {
      if (!(event instanceof MessageEvent)) return;
      try {
        const parsed = JSON.parse(event.data);
        handleSsePayload(eventName, parsed);
      } catch {
        // ignore non-JSON heartbeat frames
      }
    });
  }

  // Filter changes should re-query the server with Rust query params.
  watch(
    () => ({
      status: filters.value.status,
      type: filters.value.type,
      limit: filters.value.limit,
    }),
    () => {
      void fetchRecords();
    },
  );

  onMounted(() => {
    fetchRecords();
    fetchStats();
    connect();
  });

  onUnmounted(() => {
    disconnect();
  });

  return {
    loading,
    records: filteredRecords,
    filters,
    stats,
    streamState,
    error,
    actionError,
    actionBusyIds,
    lastUpdatedAt,
    fetchRecords,
    acknowledge,
    resolve,
    isActionBusy,
  };
}
