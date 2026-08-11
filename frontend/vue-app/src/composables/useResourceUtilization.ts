import { computed, ref, onMounted, onUnmounted } from 'vue';
import { useApi } from './useApi';
import { useToast } from './useToast';

interface UtilizationRow {
  name?: string;
  dimension?: string;
  utilization?: number;
  [key: string]: unknown;
}

interface ResourceActionSuggestion {
  id: string;
  title: string;
  detail: string;
  severity: 'critical' | 'high';
}

interface ReviewCadence {
  id: string;
  title: string;
  detail: string;
}

interface ApiErrorPayload {
  message?: string;
  detail?: string;
  error?: string | { message?: string };
}

function extractApiError(data: unknown, fallback: string): string {
  if (data && typeof data === 'object') {
    const payload = data as ApiErrorPayload;
    if (typeof payload.message === 'string' && payload.message.trim()) return payload.message;
    if (typeof payload.detail === 'string' && payload.detail.trim()) return payload.detail;
    if (typeof payload.error === 'string' && payload.error.trim()) return payload.error;
    if (payload.error && typeof payload.error === 'object' && typeof payload.error.message === 'string') {
      return payload.error.message;
    }
  }
  return fallback;
}

function unwrapData<T>(payload: unknown): T | null {
  if (payload && typeof payload === 'object' && 'data' in payload && ('success' in payload || 'message' in payload)) {
    return ((payload as { data?: T | null }).data ?? null);
  }
  return payload as T;
}

function unwrapList<T>(payload: unknown): T[] {
  const data = unwrapData<unknown>(payload);
  if (Array.isArray(data)) return data as T[];
  if (data && typeof data === 'object' && Array.isArray((data as { items?: unknown[] }).items)) {
    return (data as { items: T[] }).items;
  }
  return [];
}

function resourceName(row: UtilizationRow, fallback: string): string {
  return String(row.name || row.dimension || fallback);
}

function toPercent(value: number): number {
  return Math.abs(value) <= 1 ? value * 100 : value;
}

export function useResourceUtilization() {
  const api = useApi();
  const toast = useToast();
  const loading = ref(true);
  const snapshot = ref<UtilizationRow[]>([]);
  const trend = ref<UtilizationRow[]>([]);
  const bottlenecks = ref<UtilizationRow[]>([]);
  const error = ref('');
  const refreshTimer = ref<ReturnType<typeof setInterval> | null>(null);
  const refreshInFlight = ref(false);

  const actionSuggestions = computed<ResourceActionSuggestion[]>(() =>
    [...bottlenecks.value]
      .sort((a, b) => (b.utilization ?? 0) - (a.utilization ?? 0))
      .slice(0, 4)
      .map((row, index) => {
        const utilization = toPercent(row.utilization ?? 0);
        const name = resourceName(row, `对象 ${index + 1}`);
        const critical = utilization >= 95;
        return {
          id: `${name}-${index}`,
          title: critical ? `立即分流 ${name}` : `优先关注 ${name}`,
          detail: critical
            ? `当前利用率 ${utilization.toFixed(1)}%，已接近满载。建议立即调拨备用资源、拆分排班或降低该对象新增派工。`
            : `当前利用率 ${utilization.toFixed(1)}%，超过瓶颈阈值。建议在下一轮排班中优先释放该对象负载。`,
          severity: critical ? 'critical' : 'high',
        };
      }),
  );

  const reviewCadence = computed<ReviewCadence[]>(() => {
    if (!snapshot.value.length) {
      return [{ id: 'waiting', title: '等待快照', detail: '加载资源快照后生成复查节奏。' }];
    }
    if (actionSuggestions.value.some((item) => item.severity === 'critical')) {
      return [
        { id: 'five-minutes', title: '5 分钟复查', detail: '存在接近满载对象，建议每 5 分钟复核一次资源释放情况。' },
        { id: 'after-adjustment', title: '调整后复盘', detail: '完成调拨或排班调整后立即刷新，确认瓶颈对象回落到 85% 以下。' },
      ];
    }
    if (actionSuggestions.value.length > 0) {
      return [
        { id: 'fifteen-minutes', title: '15 分钟复查', detail: '存在高负荷对象，建议随下一轮调度窗口复查。' },
      ];
    }
    return [{ id: 'normal-cycle', title: '常规巡检', detail: '当前未发现超阈值对象，保持常规 60 分钟自动刷新。' }];
  });

  async function fetchSnapshot() {
    loading.value = true;
    try {
      const res = await api.get<UtilizationRow[]>('/api/v2/dispatch/analytics/resource-utilization/summary');
      if (!res.ok) {
        error.value = extractApiError(res.data, `资源快照加载失败 (${res.status})`);
        toast.showToast('error', error.value);
        return;
      }
      const list = unwrapList<UtilizationRow>(res.data);
      snapshot.value = list;
      bottlenecks.value = list.filter((r) => toPercent(r.utilization ?? 0) > 85);
      error.value = '';
    } catch (err) {
      error.value = err instanceof Error ? err.message : '资源快照加载失败';
      toast.showToast('error', error.value);
    } finally { loading.value = false; }
  }

  async function fetchTrend() {
    const results = await Promise.allSettled([
      api.get<UtilizationRow[]>('/api/v2/dispatch/analytics/resource-utilization/stands'),
      api.get<UtilizationRow[]>('/api/v2/dispatch/analytics/resource-utilization/teams'),
      api.get<UtilizationRow[]>('/api/v2/dispatch/analytics/resource-utilization/equipment'),
    ]);
    const fulfilled = results
      .filter((result): result is PromiseFulfilledResult<{ ok: boolean; status: number; data: UtilizationRow[] | null; response: Response }> => result.status === 'fulfilled');
    const successful = fulfilled.filter((result) => result.value.ok);
    trend.value = successful.flatMap((result) => unwrapList<UtilizationRow>(result.value.data));
    if (successful.length === 0 && results.length > 0) {
      const firstFailure = fulfilled.find((result) => !result.value.ok);
      error.value = firstFailure
        ? extractApiError(firstFailure.value.data, `资源趋势加载失败 (${firstFailure.value.status})`)
        : '资源趋势加载失败';
      toast.showToast('error', error.value);
    } else if (successful.length < results.length) {
      error.value = '部分资源趋势数据加载失败，当前仅显示已返回的数据。';
      toast.showToast('warning', error.value);
    }
  }

  function startAutoRefresh(intervalMs = 60000) {
    stopAutoRefresh();
    refreshTimer.value = setInterval(() => {
      if (refreshInFlight.value) {
        return;
      }
      refreshInFlight.value = true;
      void fetchSnapshot().finally(() => {
        refreshInFlight.value = false;
      });
    }, intervalMs);
  }

  function stopAutoRefresh() {
    if (refreshTimer.value) { clearInterval(refreshTimer.value); refreshTimer.value = null; }
  }

  onMounted(() => { fetchSnapshot(); fetchTrend(); startAutoRefresh(); });
  onUnmounted(() => stopAutoRefresh());

  return {
    loading,
    snapshot,
    trend,
    bottlenecks,
    error,
    actionSuggestions,
    reviewCadence,
    fetchSnapshot,
    fetchTrend,
    startAutoRefresh,
    stopAutoRefresh,
  };
}
