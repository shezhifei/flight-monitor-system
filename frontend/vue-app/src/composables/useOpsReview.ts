import { ref, onMounted } from 'vue';
import { useApi } from './useApi';
import { useToast } from './useToast';

interface ApiEnvelope<T> {
  success?: boolean;
  data?: T | null;
  message?: string | null;
}

interface ApiErrorPayload {
  message?: string;
  detail?: string;
  error?: string | { message?: string };
}

interface BaselineItem {
  hour?: string;
  actual_volume?: number;
  actual_on_time_rate?: number;
  baseline_volume?: number;
  baseline_on_time_rate?: number;
  threshold_margin?: number;
  is_abnormal?: boolean;
  [key: string]: unknown;
}

interface BaselinePayload {
  target_date?: string;
  weather_category?: string;
  items?: BaselineItem[];
  [key: string]: unknown;
}

export interface BaselineResponse extends BaselinePayload {
  items: BaselineItem[];
  totalEvents: number;
  flightEvents: number;
  anomalyEvents: number;
  dispatchConflicts: number;
}

export interface KpiComparisonRow {
  metric?: string;
  name?: string;
  baseline?: number | string;
  compare?: number | string;
  current?: number | string;
  change?: number;
  changeRate?: number | null;
  [key: string]: unknown;
}

export interface TrendPoint {
  label?: string;
  metric?: string;
  value?: number | string;
  anomaly?: boolean;
  anomalyCount?: number;
  [key: string]: unknown;
}

export interface ReplayEvent {
  title: string;
  subtitle: string;
  description: string;
  level: 'INFO' | 'WARN' | 'CRITICAL';
  payload: Record<string, unknown>;
}

interface KpiCompareParams {
  baseStartDate?: string;
  baseEndDate?: string;
  compareStartDate?: string;
  compareEndDate?: string;
}

const METRIC_LABELS: Record<string, string> = {
  avg_turnaround_minutes: '平均过站时长',
  p90_turnaround_minutes: 'P90 过站时长',
  on_time_departure_rate: '出港准点率',
  on_time_arrival_rate: '进港准点率',
  service_node_compliance_rate: '服务节点合规率',
  abnormal_ratio: '异常占比',
};

function todayIso(): string {
  return new Date().toISOString().slice(0, 10);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function toNumber(value: unknown): number {
  if (typeof value === 'number' && Number.isFinite(value)) return value;
  if (typeof value === 'string' && value.trim()) {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : 0;
  }
  return 0;
}

function unwrapData<T>(payload: unknown): T | null {
  if (isRecord(payload) && 'data' in payload && ('success' in payload || 'message' in payload)) {
    return (payload as ApiEnvelope<T>).data ?? null;
  }
  return payload as T;
}

function extractApiError(data: unknown, fallback: string): string {
  if (isRecord(data)) {
    const payload = data as ApiErrorPayload;
    if (typeof payload.message === 'string' && payload.message.trim()) return payload.message;
    if (typeof payload.detail === 'string' && payload.detail.trim()) return payload.detail;
    if (typeof payload.error === 'string' && payload.error.trim()) return payload.error;
    if (isRecord(payload.error) && typeof payload.error.message === 'string') {
      return payload.error.message;
    }
  }
  return fallback;
}

function normalizeBaseline(payload: unknown): BaselineResponse | null {
  const data = unwrapData<BaselinePayload>(payload);
  if (!isRecord(data)) return null;
  const items = Array.isArray(data.items) ? data.items.filter(isRecord) as BaselineItem[] : [];
  const totalEvents = items.reduce((sum, item) => sum + toNumber(item.actual_volume), 0);
  const anomalyEvents = items.filter((item) => Boolean(item.is_abnormal)).length;
  return {
    ...data,
    items,
    totalEvents,
    flightEvents: totalEvents,
    anomalyEvents,
    dispatchConflicts: anomalyEvents,
  };
}

function normalizeTrend(payload: unknown): TrendPoint[] {
  const data = unwrapData<unknown>(payload);
  const metric = isRecord(data) ? String(data.metric || '') : '';
  const items = isRecord(data) && Array.isArray(data.items)
    ? data.items
    : Array.isArray(data)
      ? data
      : [];
  return items.filter(isRecord).map((item) => {
    const anomalyCount = toNumber(item.anomaly_count);
    return {
      label: String(item.date || item.label || item.hour || ''),
      metric,
      value: item.value as number | string | undefined,
      anomaly: Boolean(item.anomaly ?? item.is_abnormal ?? anomalyCount > 0),
      anomalyCount,
      ...item,
    };
  });
}

function normalizeKpiComparison(payload: unknown): KpiComparisonRow[] {
  const data = unwrapData<unknown>(payload);
  if (!isRecord(data)) return [];
  if (Array.isArray(data.comparison)) {
    return data.comparison.filter(isRecord) as KpiComparisonRow[];
  }
  const metrics = isRecord(data.metrics) ? data.metrics : {};
  return Object.entries(metrics)
    .filter((entry): entry is [string, Record<string, unknown>] => isRecord(entry[1]))
    .map(([key, value]) => ({
      metric: key,
      name: METRIC_LABELS[key] || key,
      baseline: value.base as number | string | undefined,
      compare: value.compare as number | string | undefined,
      change: toNumber(value.delta),
      changeRate: value.change_rate === null || value.change_rate === undefined ? null : toNumber(value.change_rate),
    }));
}

function formatValue(value: unknown): string {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return Number(value.toFixed(2)).toString();
  }
  if (typeof value === 'string' && value.trim()) return value;
  return '-';
}

function buildReport(
  baseline: BaselineResponse | null,
  comparison: KpiComparisonRow[],
  trend: TrendPoint[],
): string {
  const lines = ['# 运行复盘报告', '', `生成时间：${new Date().toLocaleString()}`, ''];
  if (baseline) {
    const abnormalHours = baseline.items.filter((item) => item.is_abnormal);
    lines.push(
      '## 基线偏离',
      '',
      `- 日期：${baseline.target_date || '-'}`,
      `- 天气类别：${baseline.weather_category || '-'}`,
      `- 航班事件总量：${baseline.totalEvents}`,
      `- 异常时段：${abnormalHours.length}`,
      '',
    );
    if (abnormalHours.length > 0) {
      lines.push('重点时段：');
      for (const item of abnormalHours.slice(0, 6)) {
        lines.push(`- ${item.hour || '-'}：实际准点率 ${formatValue(item.actual_on_time_rate)}，基线 ${formatValue(item.baseline_on_time_rate)}`);
      }
      lines.push('');
    }
  }
  if (comparison.length > 0) {
    lines.push('## KPI 对比', '');
    for (const row of comparison) {
      lines.push(`- ${row.name || row.metric || '-'}：基线 ${formatValue(row.baseline)}，对比 ${formatValue(row.compare ?? row.current)}，变化 ${formatValue(row.change)}`);
    }
    lines.push('');
  }
  if (trend.length > 0) {
    const anomalyPoints = trend.filter((item) => item.anomaly);
    lines.push(
      '## 趋势与异常',
      '',
      `- 趋势点数量：${trend.length}`,
      `- 含异常叠加的点：${anomalyPoints.length}`,
      '',
    );
  }
  lines.push(
    '## 建议',
    '',
    baseline && baseline.anomalyEvents > 0
      ? '- 优先复核异常时段的航班量、准点率和保障节点瓶颈，确认是否需要调整资源排班。'
      : '- 当前加载数据未显示明显基线偏离，建议继续按常规节奏观察趋势。'
  );
  return lines.join('\n');
}

function buildReplayEvents(baseline: BaselineResponse): ReplayEvent[] {
  const abnormalEvents = baseline.items
    .filter((item) => item.is_abnormal)
    .map((item) => {
      const actualRate = toNumber(item.actual_on_time_rate);
      const baselineRate = toNumber(item.baseline_on_time_rate);
      const gap = actualRate - baselineRate;
      return {
        title: `${item.hour || '-'} 基线偏离`,
        subtitle: `实际准点率较基线 ${gap.toFixed(2)} 个百分点`,
        description: `实际航班量 ${toNumber(item.actual_volume)}，基线航班量 ${toNumber(item.baseline_volume)}。`,
        level: gap < -10 ? 'CRITICAL' : 'WARN',
        payload: { ...item, gap },
      } satisfies ReplayEvent;
    });

  if (abnormalEvents.length > 0) return abnormalEvents;
  return [{
    title: '基线偏离检查完成',
    subtitle: `${baseline.target_date || '-'} 未发现异常时段`,
    description: '当前日期的实际准点率未低于天气基线阈值。',
    level: 'INFO',
    payload: {
      target_date: baseline.target_date,
      weather_category: baseline.weather_category,
      total_events: baseline.totalEvents,
    },
  }];
}

export function useOpsReview() {
  const api = useApi();
  const toast = useToast();
  const loading = ref(true);
  const error = ref('');
  const baselineData = ref<BaselineResponse | null>(null);
  const trendData = ref<TrendPoint[]>([]);
  const kpiComparison = ref<KpiComparisonRow[]>([]);
  const aiReport = ref('');
  const generatingReport = ref(false);
  const replayDate = ref('');
  const replayRunning = ref(false);
  const replayEvents = ref<ReplayEvent[]>([]);

  async function fetchBaselineCompare(params?: { date?: string; weather?: string }) {
    loading.value = true;
    try {
      const query = new URLSearchParams({
        date: params?.date || todayIso(),
        weather: params?.weather || 'normal',
      });
      const res = await api.get<ApiEnvelope<BaselinePayload> | BaselinePayload>(`/api/v2/kpi/baseline-compare?${query.toString()}`);
      if (!res.ok) {
        error.value = extractApiError(res.data, `基线对比加载失败 (${res.status})`);
        toast.showToast('error', error.value);
        return null;
      }
      const normalized = normalizeBaseline(res.data);
      baselineData.value = normalized;
      error.value = '';
      return normalized;
    } finally { loading.value = false; }
  }

  async function fetchKpiCompare(params: KpiCompareParams) {
    const baseStartDate = params.baseStartDate;
    const baseEndDate = params.baseEndDate;
    const compareStartDate = params.compareStartDate;
    const compareEndDate = params.compareEndDate;
    const missing = !baseStartDate || !baseEndDate || !compareStartDate || !compareEndDate;
    if (missing) {
      const message = '请先选择完整的基线与对比日期范围';
      error.value = message;
      toast.showToast('warning', message);
      return [];
    }
    loading.value = true;
    try {
      const query = new URLSearchParams({
        base_start_date: baseStartDate,
        base_end_date: baseEndDate,
        compare_start_date: compareStartDate,
        compare_end_date: compareEndDate,
      });
      const res = await api.get(`/api/v2/kpi/compare?${query.toString()}`);
      if (!res.ok) {
        error.value = extractApiError(res.data, `KPI 对比加载失败 (${res.status})`);
        toast.showToast('error', error.value);
        return [];
      }
      const rows = normalizeKpiComparison(res.data);
      kpiComparison.value = rows;
      error.value = '';
      return rows;
    } finally { loading.value = false; }
  }

  async function fetchTrendWithAnomalies() {
    const query = new URLSearchParams({ metric: 'on_time_rate', days: '7' });
    const res = await api.get(`/api/v2/kpi/trend-with-anomalies?${query.toString()}`);
    if (!res.ok) {
      error.value = extractApiError(res.data, `趋势叠加加载失败 (${res.status})`);
      toast.showToast('error', error.value);
      return [];
    }
    const rows = normalizeTrend(res.data);
    trendData.value = rows;
    return rows;
  }

  async function generateReport() {
    generatingReport.value = true;
    try {
      if (!baselineData.value && trendData.value.length === 0 && kpiComparison.value.length === 0) {
        const message = '请先加载基线、趋势或 KPI 对比数据后再生成报告';
        toast.showToast('warning', message);
        return false;
      }
      aiReport.value = buildReport(baselineData.value, kpiComparison.value, trendData.value);
      return true;
    } finally { generatingReport.value = false; }
  }

  async function runReplay(date: string, weather = 'normal') {
    replayRunning.value = true;
    replayDate.value = date;
    try {
      const baseline = await fetchBaselineCompare({ date, weather });
      if (!baseline) {
        replayEvents.value = [];
        return [];
      }
      const events = buildReplayEvents(baseline);
      replayEvents.value = events;
      return events;
    } finally { replayRunning.value = false; }
  }

  onMounted(() => { fetchBaselineCompare(); fetchTrendWithAnomalies(); });

  return {
    loading,
    error,
    baselineData,
    trendData,
    kpiComparison,
    aiReport,
    generatingReport,
    replayDate,
    replayRunning,
    replayEvents,
    fetchBaselineCompare,
    fetchKpiCompare,
    fetchTrendWithAnomalies,
    generateReport,
    runReplay,
  };
}
