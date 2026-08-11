export interface KpiTrendPoint {
  label: string;
  value: number;
  targetGap: number;
}

export interface KpiBarPoint {
  hour: string;
  value: number;
}

export interface KpiDistributionPoint {
  label: string;
  value: number;
}

export interface ServiceNodeRow {
  id: string;
  label: string;
  value: number;
  displayValue: string;
  status: 'pass' | 'warning' | 'fail';
}

export interface KpiDashboardState {
  verdictLead: string;
  verdictState: 'pass' | 'warning' | 'fail' | 'pending';
  verdictSupport: string;
  snapshotTime: string;
  scoreDepartureValue: string;
  scoreGapValue: string;
  scoreTurnValue: string;
  scoreServiceValue: string;
  decisionAttainment: string;
  decisionSource: string;
  decisionNextStep: string;
  trendData: KpiTrendPoint[];
  trendBoardLead: string;
  trendBoardSupport: string;
  trendDeltaBadge: string;
  trendMeta: string;
  hourlyData: KpiBarPoint[];
  timePressureLead: string;
  timePressureSupport: string;
  hourlyMeta: string;
  distributionData: KpiDistributionPoint[];
  tailPressureLead: string;
  tailPressureSupport: string;
  distributionMeta: string;
  p90Turnaround: string;
  equipmentRate: string;
  abnormalRatio: string;
  turnaroundInsightText: string;
  equipmentInsightText: string;
  abnormalInsightText: string;
  nodesSummaryText: string;
  nodePressureLead: string;
  nodePressureSupport: string;
}

const TARGET_DEPARTURE_RATE = 90;

const NODE_LABELS: Record<string, string> = {
  cleaning: '清洁',
  loading: '装载',
  boarding: '登机',
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function toNumber(value: unknown): number | null {
  if (typeof value === 'number' && Number.isFinite(value)) return value;
  if (typeof value === 'string' && value.trim()) {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

function ratioToPercent(value: unknown): number | null {
  const numeric = toNumber(value);
  if (numeric === null) return null;
  return Math.abs(numeric) <= 1 ? numeric * 100 : numeric;
}

function formatSignedPercent(value: number | null): string {
  if (value === null) return '-';
  const normalized = Number(value.toFixed(1));
  return `${normalized > 0 ? '+' : ''}${normalized.toFixed(1)}%`;
}

function formatMinutes(value: unknown): string {
  const numeric = toNumber(value);
  if (numeric === null) return '-';
  return `${Number(numeric.toFixed(1))} 分`;
}

function formatDateLabel(value: unknown): string {
  const text = String(value || '').trim();
  const match = text.match(/^(\d{4})-(\d{2})-(\d{2})/);
  return match ? `${match[2]}-${match[3]}` : text;
}

function formatHourLabel(value: unknown): string {
  if (typeof value === 'string' && value.trim()) return value;
  const numeric = toNumber(value);
  return numeric === null ? '' : `${String(Math.trunc(numeric)).padStart(2, '0')}:00`;
}

function statusFromGap(gap: number | null): KpiDashboardState['verdictState'] {
  if (gap === null) return 'pending';
  if (gap >= 0) return 'pass';
  if (gap >= -5) return 'warning';
  return 'fail';
}

function findWeakestNode(nodes: ServiceNodeRow[]): ServiceNodeRow | null {
  return nodes.reduce<ServiceNodeRow | null>((weakest, node) => {
    if (!weakest || node.value < weakest.value) return node;
    return weakest;
  }, null);
}

export function unwrapApiData<T = unknown>(payload: unknown): T {
  if (isRecord(payload) && 'success' in payload && 'data' in payload) {
    return payload.data as T;
  }
  return payload as T;
}

export function formatKpiPercent(value: unknown): string {
  const percent = ratioToPercent(value);
  if (percent === null) return '-';
  return `${percent.toFixed(1)}%`;
}

export function mapKpiTrend(payload: unknown): KpiTrendPoint[] {
  const data = unwrapApiData(payload);
  const items = Array.isArray(data)
    ? data
    : isRecord(data) && Array.isArray(data.items)
      ? data.items
      : [];

  return items
    .filter(isRecord)
    .map((item) => {
      const value = ratioToPercent(item.value) ?? 0;
      return {
        label: formatDateLabel(item.date ?? item.label),
        value: Number(value.toFixed(1)),
        targetGap: Number((value - TARGET_DEPARTURE_RATE).toFixed(1)),
      };
    });
}

export function mapServiceNodes(payload: unknown): ServiceNodeRow[] {
  const data = unwrapApiData(payload);
  const items = isRecord(data) && Array.isArray(data.items) ? data.items : Array.isArray(data) ? data : [];

  return items.filter(isRecord).map((item) => {
    const id = String(item.node || item.id || '').trim() || 'unknown';
    const value = ratioToPercent(item.rate ?? item.value) ?? 0;
    const rounded = Number(value.toFixed(1));
    return {
      id,
      label: NODE_LABELS[id] || id,
      value: rounded,
      displayValue: `${rounded.toFixed(1)}%`,
      status: rounded >= 90 ? 'pass' : rounded >= 75 ? 'warning' : 'fail',
    };
  });
}

export function mapKpiSnapshot(payload: unknown): KpiDashboardState {
  const data = unwrapApiData(payload);
  const snapshot = isRecord(data) ? data : {};
  const departureRate = ratioToPercent(snapshot.on_time_departure_rate);
  const serviceRate = ratioToPercent(snapshot.service_node_compliance_rate);
  const equipmentUtilization = ratioToPercent(snapshot.equipment_utilization_rate);
  const abnormalRatio = ratioToPercent(snapshot.abnormal_flight_ratio);
  const p90TurnaroundMinutes = toNumber(snapshot.turnaround_time_p90_minutes);
  const gap = departureRate === null ? null : departureRate - TARGET_DEPARTURE_RATE;
  const verdictState = statusFromGap(gap);

  const trendData = mapKpiTrend(snapshot.on_time_trend);
  const hourlyRaw = Array.isArray(snapshot.hourly_flight_volume) ? snapshot.hourly_flight_volume.filter(isRecord) : [];
  const maxHourlyCount = Math.max(1, ...hourlyRaw.map((item) => toNumber(item.count) ?? 0));
  const hourlyData = hourlyRaw.map((item) => ({
    hour: formatHourLabel(item.hour_label ?? item.hour),
    value: Number((((toNumber(item.count) ?? 0) / maxHourlyCount) * 100).toFixed(1)),
  }));
  const distributionData = (Array.isArray(snapshot.turnaround_distribution) ? snapshot.turnaround_distribution : [])
    .filter(isRecord)
    .map((item) => ({
      label: String(item.bucket || item.label || ''),
      value: toNumber(item.count ?? item.value) ?? 0,
    }));

  const trendLatest = trendData.at(-1);
  const hourlyPeak = hourlyData.reduce<KpiBarPoint | null>((peak, item) => {
    if (!peak || item.value > peak.value) return item;
    return peak;
  }, null);
  const distributionPeak = distributionData.reduce<KpiDistributionPoint | null>((peak, item) => {
    if (!peak || item.value > peak.value) return item;
    return peak;
  }, null);

  return {
    verdictLead:
      departureRate === null
        ? '等待 KPI 快照'
        : `出港准点率 ${formatKpiPercent(departureRate)}，${gap !== null && gap >= 0 ? '达到' : '低于'}目标 ${Math.abs(gap ?? 0).toFixed(1)} 个百分点`,
    verdictState,
    verdictSupport:
      verdictState === 'pass'
        ? '核心准点指标达到目标，继续关注节点合规率和异常航班占比。'
        : '核心准点指标低于目标，优先回看高峰时段、过站尾差和服务节点合规情况。',
    snapshotTime: String(snapshot.calculated_at || new Date().toLocaleTimeString()),
    scoreDepartureValue: formatKpiPercent(departureRate),
    scoreGapValue: formatSignedPercent(gap),
    scoreTurnValue: formatMinutes(p90TurnaroundMinutes),
    scoreServiceValue: formatKpiPercent(serviceRate),
    decisionAttainment: gap === null ? '待评估' : gap >= 0 ? '达标' : '未达标',
    decisionSource:
      p90TurnaroundMinutes !== null && p90TurnaroundMinutes > 15
        ? '过站尾差'
        : abnormalRatio !== null && abnormalRatio > 5
          ? '异常航班'
          : '节点合规',
    decisionNextStep:
      p90TurnaroundMinutes !== null && p90TurnaroundMinutes > 15
        ? '查过站节点'
        : hourlyPeak
          ? `查 ${hourlyPeak.hour} 时段`
          : '查节点明细',
    trendData,
    trendBoardLead: trendLatest ? `最近一日准点率 ${trendLatest.value.toFixed(1)}%` : '暂无趋势数据',
    trendBoardSupport: '趋势已按出港准点率与目标 90% 的差距归一化。',
    trendDeltaBadge: trendLatest ? `Δ ${formatSignedPercent(trendLatest.targetGap)}` : '等待更新',
    trendMeta: `目标 ${TARGET_DEPARTURE_RATE}%`,
    hourlyData,
    timePressureLead: hourlyPeak ? `${hourlyPeak.hour} 是当前时间窗内最高航班量时段` : '暂无时段数据',
    timePressureSupport: '柱形宽度按当前时间窗最高小时航班量归一化。',
    hourlyMeta: hourlyData.length ? `${hourlyData.length} 个时段` : '暂无数据',
    distributionData,
    tailPressureLead: distributionPeak ? `${distributionPeak.label} 分钟区间航班最多` : '暂无尾部分布数据',
    tailPressureSupport: '分布来自实际进出港过站时长分桶统计。',
    distributionMeta: distributionData.length ? `${distributionData.length} 个分桶` : '暂无数据',
    p90Turnaround: formatMinutes(p90TurnaroundMinutes),
    equipmentRate: formatKpiPercent(equipmentUtilization),
    abnormalRatio: formatKpiPercent(abnormalRatio),
    turnaroundInsightText:
      p90TurnaroundMinutes === null
        ? '暂无数据'
        : p90TurnaroundMinutes > 15
          ? `超出阈值 ${(p90TurnaroundMinutes - 15).toFixed(1)} 分`
          : '低于阈值',
    equipmentInsightText:
      equipmentUtilization === null ? '暂无数据' : equipmentUtilization >= 80 ? '利用率健康' : '利用率偏低',
    abnormalInsightText:
      abnormalRatio === null ? '暂无数据' : abnormalRatio <= 5 ? '在可控范围' : '异常占比偏高',
    nodesSummaryText: '等待节点明细',
    nodePressureLead: '等待节点诊断',
    nodePressureSupport: '节点合规率来自清洁、装载、登机等服务节点的实际里程碑。',
  };
}

export function summarizeServiceNodes(nodes: ServiceNodeRow[]): Pick<KpiDashboardState, 'nodesSummaryText' | 'nodePressureLead' | 'nodePressureSupport'> {
  const weakest = findWeakestNode(nodes);
  if (!weakest) {
    return {
      nodesSummaryText: '暂无节点明细',
      nodePressureLead: '当前时间窗暂无服务节点样本',
      nodePressureSupport: '请切换日期或等待航班里程碑数据写入后再查看。',
    };
  }

  return {
    nodesSummaryText: `${nodes.length} 个服务节点`,
    nodePressureLead: `${weakest.label}合规率最低：${weakest.displayValue}`,
    nodePressureSupport:
      weakest.status === 'pass'
        ? '所有服务节点均处于健康区间。'
        : '优先回看低合规节点对应的保障里程碑和资源安排。',
  };
}
