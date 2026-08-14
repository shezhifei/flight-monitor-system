import { computed, ref, watch } from 'vue';
import type { ComputedRef, Ref } from 'vue';
import { useApi } from '@/composables/useApi';
import {
  fetchAnalytics as fetchDispatchAnalytics,
  type AnalyticsBreakdownItem,
  type AnalyticsTrendPoint,
  type ConflictItem,
  type DispatchOrder,
  type TimelineData,
} from '@/composables/useDispatchBoardData';
import { unwrapApiData } from '@/shared/apiEnvelope';
import {
  type EmployeeAnalyticsBucket,
  type EmployeeAnalyticsItem,
  toTimestamp,
} from './useDispatchBoardPageAiTypes';
import { renderTrendChartInto } from './useTrendChart';

export interface UseDispatchBoardPageAiAnalyticsOptions {
  timelineData: Readonly<Ref<TimelineData | null>>;
  windowStartMs: Readonly<Ref<number>>;
  windowEndMs: Readonly<Ref<number>>;
  visibleTimelineItems: ComputedRef<readonly DispatchOrder[]>;
  analyticsMode: Ref<'team' | 'employee'>;
  impactedOrderIds: Ref<string[]>;
}

export interface UseDispatchBoardPageAiAnalyticsReturn {
  analyticsData: Ref<unknown | null>;
  analyticsMetrics: Ref<{ conflictRate: string; replanRate: string; responseMinutes: string; balanceScore: string; idleRate: string; ontimeRate: string }>;
  analyticsBreakdownList: Ref<Array<{ id: string; label: string; value: string; orderCount?: number; completedOrderCount?: number; occupiedMinutes?: number; teamLabels?: string[]; resourceId?: string; orderIds?: string[]; representativeOrderId?: string }>>;
  conflictRawList: Ref<ConflictItem[]>;
  conflictList: Ref<ConflictItem[]>;
  conflictSeverityFilter: Ref<string>;
  conflictTypeFilter: Ref<string>;
  conflictQueryInput: Ref<string>;
  conflictMetrics: Ref<{ total: number; high: number; orders: number }>;
  availableConflictTypes: ComputedRef<string[]>;
  trendChartRef: Ref<HTMLElement | null>;
  fetchConflicts: () => Promise<void>;
  updateConflictList: () => void;
  fetchAnalytics: () => Promise<void>;
  buildEmployeeAnalyticsBreakdown: (items: readonly DispatchOrder[]) => EmployeeAnalyticsItem[];
}

export function useDispatchBoardPageAiAnalytics(options: UseDispatchBoardPageAiAnalyticsOptions): UseDispatchBoardPageAiAnalyticsReturn {
  const { timelineData: _timelineData, windowStartMs, windowEndMs, visibleTimelineItems, analyticsMode, impactedOrderIds: _impactedOrderIds } = options;
  const api = useApi();

  const analyticsData = ref<unknown | null>(null);
  const analyticsMetrics = ref({
    conflictRate: '-',
    replanRate: '-',
    responseMinutes: '-',
    balanceScore: '-',
    idleRate: '-',
    ontimeRate: '-',
  });
  const analyticsBreakdownList = ref<Array<{ id: string; label: string; value: string; orderCount?: number; completedOrderCount?: number; occupiedMinutes?: number; teamLabels?: string[]; resourceId?: string; orderIds?: string[]; representativeOrderId?: string }>>([]);

  const conflictRawList = ref<ConflictItem[]>([]);
  const conflictList = ref<ConflictItem[]>([]);
  const conflictSeverityFilter = ref('all');
  const conflictTypeFilter = ref('all');
  const conflictQueryInput = ref('');
  const conflictMetrics = ref({ total: 0, high: 0, orders: 0 });
  const availableConflictTypes = computed(() => {
    const types = new Set<string>();
    conflictRawList.value.forEach((c) => {
      if (c.conflict_type) types.add(c.conflict_type);
    });
    return Array.from(types);
  });

  const trendChartRef = ref<HTMLElement | null>(null);

  async function fetchConflicts() {
    try {
      const params = new URLSearchParams();
      params.set('window_start', new Date(windowStartMs.value).toISOString());
      params.set('window_end', new Date(windowEndMs.value).toISOString());
      params.set('limit', '200');
      const res = await api.get<unknown>(`/api/v2/dispatch-orders/conflicts?${params.toString()}`);
      const payload = unwrapApiData<{ conflicts?: ConflictItem[] }>(res.data);
      if (res.ok && payload) {
        conflictRawList.value = (payload.conflicts || []).map((c) => ({
          ...c,
          description: String(c.description || c.message || c.conflict_type || '').trim(),
        }));
        updateConflictList();
      }
    } catch (e) {
      console.warn('Failed to fetch conflicts:', e);
    }
  }

  function updateConflictList() {
    const query = conflictQueryInput.value.trim().toLowerCase();
    conflictList.value = conflictRawList.value.filter((c) => {
      if (conflictSeverityFilter.value !== 'all' && String(c.severity || '').trim().toLowerCase() !== conflictSeverityFilter.value) return false;
      if (conflictTypeFilter.value !== 'all' && String(c.conflict_type || '').trim() !== conflictTypeFilter.value) return false;
      if (query) {
        const searchText = [c.resource_name, c.resource_id, c.conflict_type, c.description, c.message, ...(Array.isArray(c.related_dispatch_order_ids) ? c.related_dispatch_order_ids : [])]
          .map((v) => String(v || '').trim().toLowerCase())
          .filter(Boolean)
          .join(' ');
        if (!searchText.includes(query)) return false;
      }
      return true;
    });
    const related = new Set<string>();
    conflictList.value.forEach((c) =>
      (c.related_dispatch_order_ids || []).forEach((id) => {
        const n = String(id || '').trim();
        if (n) related.add(n);
      }),
    );
    conflictMetrics.value = {
      total: conflictList.value.length,
      high: conflictList.value.filter((c) => ['critical', 'high'].includes(String(c.severity || '').toLowerCase())).length,
      orders: related.size,
    };
  }

  watch([() => conflictSeverityFilter.value, () => conflictTypeFilter.value, () => conflictQueryInput.value], updateConflictList);

  async function fetchAnalytics() {
    try {
      const res = await fetchDispatchAnalytics({ windowStartMs: windowStartMs.value, windowEndMs: windowEndMs.value });
      analyticsData.value = res;
      if (res) {
        const summary = (res as { summary?: Record<string, unknown> }).summary || {};
        analyticsMetrics.value = {
          conflictRate: String(summary.conflict_rate ?? '-'),
          replanRate: String(summary.replan_rate ?? '-'),
          responseMinutes: String(summary.avg_dispatch_response_minutes ?? '-'),
          balanceScore: String(summary.team_load_balance_score ?? '-'),
          idleRate: String(summary.equipment_idle_rate ?? '-'),
          ontimeRate: String(summary.key_order_ontime_rate ?? '-'),
        };
        analyticsBreakdownList.value =
          analyticsMode.value === 'employee'
            ? buildEmployeeAnalyticsBreakdown(visibleTimelineItems.value)
            : (((res as unknown as { breakdown?: readonly AnalyticsBreakdownItem[] }).breakdown || []) as readonly AnalyticsBreakdownItem[]).map((b: AnalyticsBreakdownItem, i: number) => ({
                id: `breakdown-${i}`,
                label: String((b as unknown as Record<string, unknown>).group_label ?? (b as unknown as Record<string, unknown>).label ?? (b as unknown as Record<string, unknown>).name ?? (b as unknown as Record<string, unknown>).group_key ?? `项 ${i + 1}`),
                value: String((b as unknown as Record<string, unknown>).order_count ?? (b as unknown as Record<string, unknown>).value ?? (b as unknown as Record<string, unknown>).score ?? ''),
              }));
        const trend = (res as unknown as { trend?: readonly AnalyticsTrendPoint[] }).trend;
        if (Array.isArray(trend) && trend.length > 0) {
          renderTrendChart(trend);
        }
      }
    } catch (e) {
      console.warn('Failed to fetch analytics:', e);
    }
  }

  function renderTrendChart(data: ReadonlyArray<AnalyticsTrendPoint>) {
    const chartEl = trendChartRef.value;
    if (!chartEl || !data || data.length === 0) return;
    const option = {
      tooltip: { trigger: 'axis' },
      grid: { left: '3%', right: '4%', bottom: '3%', containLabel: true },
      xAxis: {
        type: 'category',
        data: data.map((d) => {
          const date = new Date(String(d.timestamp || d.time || d.date || ''));
          return date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });
        }),
      },
      yAxis: { type: 'value' },
      series: [
        {
          data: data.map((d) => Number((d as unknown as Record<string, unknown>).value ?? (d as unknown as Record<string, unknown>).count ?? (d as unknown as Record<string, unknown>).amount ?? 0)),
          type: 'line',
          smooth: true,
          areaStyle: { opacity: 0.3 },
        },
      ],
    };
    renderTrendChartInto(chartEl, option);
  }

  function buildEmployeeAnalyticsBreakdown(items: readonly DispatchOrder[]): EmployeeAnalyticsItem[] {
    const grouped = new Map<string, EmployeeAnalyticsBucket>();
    for (const item of items) {
      if (!item || item.is_flight_summary) continue;
      const members = Array.isArray(item.members) ? item.members : [];
      const primary = members[0] || null;
      const id = String(item.individual_user_id || item.focus_user_id || primary?.user_id || '').trim();
      const label = String(item.individual_username || item.focus_user_name || primary?.username || primary?.user_display_name || primary?.name || '').trim();
      if (!id && !label) continue;
      const key = id || label;
      const bucket = grouped.get(key) || {
        label: label || id,
        orderCount: 0,
        completedOrderCount: 0,
        occupiedMinutes: 0,
        teamLabels: new Set<string>(),
        resourceId: id,
        orderIds: new Set<string>(),
        representativeOrderId: '',
      };
      bucket.orderCount += 1;
      if (String(item.status || '').trim() === 'completed') bucket.completedOrderCount += 1;
      if (item.order_id) {
        bucket.orderIds.add(item.order_id);
        if (!bucket.representativeOrderId) bucket.representativeOrderId = item.order_id;
      }
      const start = toTimestamp(item.actual_start_time || item.planned_start_time || item.start_time);
      const end = toTimestamp(item.actual_end_time || item.effective_end_time || item.planned_end_time || item.end_time);
      if (start > 0 && end > start) {
        bucket.occupiedMinutes += Math.round((end - start) / 60000);
        bucket.representativeOrderId = item.order_id || '';
      }
      const team = String(item.team_name || '').trim();
      if (team) bucket.teamLabels.add(team);
      grouped.set(key, bucket);
    }
    return Array.from(grouped.entries())
      .map(([k, v]) => ({
        id: `employee-${k}`,
        label: v.label,
        value: `${v.orderCount} 单 / ${v.occupiedMinutes} 分钟`,
        orderCount: v.orderCount,
        completedOrderCount: v.completedOrderCount,
        occupiedMinutes: v.occupiedMinutes,
        teamLabels: Array.from(v.teamLabels),
        resourceId: v.resourceId,
        orderIds: Array.from(v.orderIds),
        representativeOrderId: v.representativeOrderId,
      }))
      .sort((a, b) => b.orderCount - a.orderCount || b.occupiedMinutes - a.occupiedMinutes || a.label.localeCompare(b.label, 'zh-CN'));
  }

  return {
    analyticsData,
    analyticsMetrics,
    analyticsBreakdownList,
    conflictRawList,
    conflictList,
    conflictSeverityFilter,
    conflictTypeFilter,
    conflictQueryInput,
    conflictMetrics,
    availableConflictTypes,
    trendChartRef,
    fetchConflicts,
    updateConflictList,
    fetchAnalytics,
    buildEmployeeAnalyticsBreakdown,
  };
}
