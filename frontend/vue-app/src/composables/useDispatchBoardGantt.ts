import { useApi } from '@/composables/useApi';
import { unwrapApiData, toStringArray } from './useDispatchBoardApi';
import type { DispatchOrder } from './useDispatchBoardOrders';
import type { SafetyProgressMap } from './useDispatchBoardChecklist';

export type ViewMode = 'flight' | 'team' | 'employee' | 'equipment';

export type SafetyGateFilter = 'all' | 'blocked' | 'pending' | 'ready';

export type ConflictSeverity = 'critical' | 'high' | 'medium' | 'low';

export type ConflictType = 'team_overlap' | 'individual_overlap' | 'stand_overlap' | 'equipment_overlap';

export type ReplanStrategy = 'stability' | 'balanced' | 'efficiency';

export type AnalyticsBreakdownMode = 'team' | 'employee';

export interface TimelineLane {
  id: string;
  label?: string | null;
  resource_type?: string | null;
  resource_id?: string | null;
  resource_label?: string | null;
  [key: string]: unknown;
}


export interface TimelineData {
  items: ReadonlyArray<DispatchOrder>;
  lanes?: ReadonlyArray<TimelineLane>;
  window_start?: string | null;
  window_end?: string | null;
  view_mode?: ViewMode | null;
  terminal?: string | null;
  [key: string]: unknown;
}


export interface AnalyticsSummary {
  [key: string]: unknown;
}


export interface AnalyticsBreakdownItem {
  [key: string]: unknown;
}


export interface AnalyticsTrendPoint {
  [key: string]: unknown;
}


export interface AnalyticsData {
  summary: AnalyticsSummary | null;
  breakdown: ReadonlyArray<AnalyticsBreakdownItem>;
  trend: ReadonlyArray<AnalyticsTrendPoint>;
}


export interface ConflictItem {
  conflict_type?: ConflictType | null;
  severity?: ConflictSeverity | null;
  resource_id?: string | null;
  resource_name?: string | null;
  resource_type?: string | null;
  related_dispatch_order_ids?: string[] | null;
  description?: string | null;
  [key: string]: unknown;
}


export const CONFLICT_SEVERITY_ORDER: readonly ConflictSeverity[] = Object.freeze([
  'critical',
  'high',
  'medium',
  'low',
]);


export const CONFLICT_TYPE_LABELS: Record<ConflictType, string> = Object.freeze({
  team_overlap: '班组冲突',
  individual_overlap: '人员冲突',
  stand_overlap: '机位冲突',
  equipment_overlap: '设备冲突',
});


export const REPLAN_REASON_LABELS: Record<string, string> = Object.freeze({
  resource_time_overlap: '资源时间重叠',
  assigned_conflict_repair: '冲突修复',
  unassigned_assignment: '未指派排班',
});


export const SAFETY_GATE_COLORS = Object.freeze({
  ready: '#34C759',
  pending: '#FF9500',
  blocked: '#D64545',
});


export const VIEW_LABELS: Record<ViewMode, string> = Object.freeze({
  flight: '航班视角',
  team: '班组视角',
  employee: '员工视角',
  equipment: '设备视角',
});


export const DEFAULT_REFRESH_INTERVAL_MS = 15_000;

export const DEFAULT_PAST_MINUTES = 60;

export const DEFAULT_FUTURE_MINUTES = 360;

export const STATUS_LIST_LIMIT = 200;

export const CONFLICT_LIST_LIMIT = 180;

export function normalizeTerminalList(rawTerminals: unknown[]): string[] {
  const seen = new Set<string>();
  const terminals: string[] = [];

  for (const terminal of rawTerminals) {
    const value = String(terminal ?? '').trim();
    if (!value) continue;
    const key = value.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    terminals.push(value);
  }

  terminals.sort((a, b) => a.localeCompare(b, 'zh-CN', { numeric: true, sensitivity: 'base' }));
  return terminals;
}

/** Collect terminal strings from timeline items. */

export function collectTerminalsFromTimeline(timeline: TimelineData | null | undefined): string[] {
  if (!timeline?.items) return [];
  return timeline.items
    .map((item) => String(item?.terminal ?? '').trim())
    .filter(Boolean);
}

/** Normalise a member's user id from a TimelineMember. */

export interface FetchTimelineOptions {
  viewMode?: ViewMode;
  windowStartMs: number;
  windowEndMs: number;
  terminal?: string;
}


export interface FetchAnalyticsOptions {
  windowStartMs: number;
  windowEndMs: number;
}

/** Fetch timeline data from the dispatch orders API. */

export async function fetchTimeline(
  options: FetchTimelineOptions,
): Promise<TimelineData | null> {
  const { get } = useApi();
  const params = new URLSearchParams();
  params.set('view_mode', options.viewMode || 'flight');
  params.set('window_start', new Date(options.windowStartMs).toISOString());
  params.set('window_end', new Date(options.windowEndMs).toISOString());
  if (options.terminal && options.terminal !== 'all') {
    params.set('terminal', options.terminal);
  }

  const result = await get<unknown>(`/api/v2/dispatch-orders/timeline?${params.toString()}`);
  if (!result.ok || !result.data) {
    throw new Error(`时间线加载失败 (HTTP ${result.status})`);
  }
  const payload = unwrapApiData(result.data);
  return payload && typeof payload === 'object' ? (payload as unknown as TimelineData) : null;
}

/** Fetch safety progress for all orders in a timeline. */

export async function fetchTimelineSafetyProgress(
  timeline: TimelineData | null,
): Promise<SafetyProgressMap> {
  const { post } = useApi();
  const requestItems: Array<{ dispatch_order_id: string; task_type: string }> = [];
  const uniqueByOrder = new Map<string, string>();
  const timelineItems = Array.isArray(timeline?.items) ? timeline.items : [];

  for (const item of timelineItems) {
    if (!item || item.is_flight_summary) continue;
    const orderId = String(item.order_id ?? '').trim();
    const stepCode = String(item.task_type ?? '').trim();
    if (!orderId || !stepCode) continue;
    if (!uniqueByOrder.has(orderId)) {
      uniqueByOrder.set(orderId, stepCode);
    }
  }

  uniqueByOrder.forEach((stepCode, orderId) => {
    requestItems.push({ dispatch_order_id: orderId, task_type: stepCode });
  });

  if (requestItems.length === 0) return {};

  const result = await post<unknown>('/api/v2/dispatch-orders/safety-checklist/progress', {
    orders: requestItems,
  });
  if (!result.ok || !result.data) return {};

  const payload = unwrapApiData(result.data) as { items?: unknown[] } | null;
  const items = Array.isArray(payload?.items) ? payload.items : [];
  const nextMap: SafetyProgressMap = {};

  for (const item of items) {
    if (!item || typeof item !== 'object') continue;
    const rec = item as Record<string, unknown>;
    const orderId = String(rec.dispatch_order_id ?? '').trim();
    if (!orderId) continue;
    nextMap[orderId] = {
      dispatch_order_id: orderId,
      task_type: String(rec.task_type ?? '').trim(),
      enforced: Boolean(rec.enforced),
      ready: Boolean(rec.ready),
      required_total: Number(rec.required_total ?? 0),
      completed_required: Number(rec.completed_required ?? 0),
      pending_required_count: Number(rec.pending_required_count ?? 0),
      failed_required_count: Number(rec.failed_required_count ?? 0),
      template_version: (rec.template_version as string | null) ?? null,
      blocking_issues: toStringArray(rec.blocking_issues),
      soft_missing_count: Number(rec.soft_missing_count ?? 0),
      can_soft_complete: Boolean(rec.can_soft_complete ?? true),
    };
  }

  return nextMap;
}

/** Fetch analytics summary, breakdown, and trend. */

export async function fetchAnalytics(
  options: FetchAnalyticsOptions,
): Promise<AnalyticsData> {
  const { get } = useApi();
  const params = new URLSearchParams();
  params.set('window_start', new Date(options.windowStartMs).toISOString());
  params.set('window_end', new Date(options.windowEndMs).toISOString());

  const [summaryResult, breakdownResult, trendResult] = await Promise.all([
    get<unknown>(`/api/v2/dispatch/analytics/summary?${params.toString()}`),
    get<unknown>(`/api/v2/dispatch/analytics/breakdown?group_by=team&${params.toString()}`),
    get<unknown>(`/api/v2/dispatch/analytics/trend?bucket=hour&${params.toString()}`),
  ]);

  const failed = [
    ['summary', summaryResult] as const,
    ['breakdown', breakdownResult] as const,
    ['trend', trendResult] as const,
  ].filter(([, result]) => !result.ok);
  if (failed.length > 0) {
    throw new Error(failed.map(([name, result]) => `${name} HTTP ${result.status}`).join('; '));
  }

  const summary = unwrapApiData(summaryResult.data);
  const breakdown = unwrapApiData(breakdownResult.data);
  const trend = unwrapApiData(trendResult.data);

  return {
    summary: summary && typeof summary === 'object' ? (summary as AnalyticsSummary) : null,
    breakdown: Array.isArray(breakdown) ? (breakdown as AnalyticsBreakdownItem[]) : [],
    trend: Array.isArray(trend) ? (trend as AnalyticsTrendPoint[]) : [],
  };
}

/** Fetch a single dispatch order by ID. */

export async function loadConfiguredTerminals(timeline?: TimelineData | null): Promise<string[]> {
  const { get } = useApi();
  let fetchedTerminals: string[] = [];

  try {
    const result = await get<unknown>('/api/v2/stands?include_inactive=false');
    if (result.ok && result.data) {
      const stands = unwrapApiData(result.data);
      if (Array.isArray(stands)) {
        fetchedTerminals = (stands as Array<Record<string, unknown>>)
          .map((stand) => String(stand?.terminal ?? '').trim())
          .filter(Boolean);
      }
    }
  } catch (error) {
    console.warn('Failed to load configured terminals:', error);
  }

  if (fetchedTerminals.length === 0) {
    fetchedTerminals = collectTerminalsFromTimeline(timeline);
  }

  return normalizeTerminalList(fetchedTerminals);
}

