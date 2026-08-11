export interface CommandCenterKpis {
  decisionCount: number;
  riskFlights: number;
  openAnomalies: number;
  dispatchBlockers: number;
  delayPressure: number;
}

export interface CommandCenterVerdict {
  title: string;
  detail: string;
  severity: 'ok' | 'warn' | 'critical';
  window: string;
}

export interface CommandQueueItem {
  id: string;
  name: string;
  meta: string;
  severity: 'ok' | 'warn' | 'critical';
}

export interface CommandDistributionItem {
  id: string;
  label: string;
  value: number;
  detail: string;
  severity: 'ok' | 'warn' | 'critical';
}

export interface CommandCenterSnapshot {
  kpis: CommandCenterKpis;
  verdict: CommandCenterVerdict;
  priorityQueue: CommandQueueItem[];
  windowPressure: CommandDistributionItem[];
  heatmapData: CommandDistributionItem[];
  dispatchLoad: CommandDistributionItem[];
  terminalLoad: CommandDistributionItem[];
}

type RawRecord = Record<string, unknown>;

const EMPTY_KPIS: CommandCenterKpis = {
  decisionCount: 0,
  riskFlights: 0,
  openAnomalies: 0,
  dispatchBlockers: 0,
  delayPressure: 0,
};

export function unwrapList(payload: unknown): RawRecord[] {
  if (Array.isArray(payload)) {
    return payload.filter(isRecord);
  }
  if (!isRecord(payload)) {
    return [];
  }

  for (const key of ['data', 'items', 'records', 'orders', 'rows']) {
    const value = payload[key];
    if (Array.isArray(value)) {
      return value.filter(isRecord);
    }
    if (isRecord(value)) {
      const nested = unwrapList(value);
      if (nested.length > 0) {
        return nested;
      }
    }
  }
  return [];
}

export function buildCommandCenterSnapshot(
  flightsPayload: unknown,
  anomaliesPayload: unknown,
  dispatchPayload: unknown,
  windowHours = 2,
  now: Date = new Date(),
): CommandCenterSnapshot {
  const flights = unwrapList(flightsPayload);
  const anomalies = unwrapList(anomaliesPayload);
  const dispatchOrders = unwrapList(dispatchPayload);
  const windowFlights = flights.filter((flight) => isInFutureWindow(flight, windowHours, now));
  const scopedFlights = windowFlights.length > 0 ? windowFlights : flights;
  const openAnomalies = anomalies.filter((item) => isOpenStatus(textValue(item.status)));
  const pendingDispatch = dispatchOrders.filter((item) => isPendingDispatchStatus(textValue(item.status)));
  const delayValues = scopedFlights.map(delayMinutes).filter((value): value is number => value !== null);
  const riskFlights = scopedFlights.filter((flight) => (delayMinutes(flight) ?? 0) > 30);
  const blockedOrders = pendingDispatch.filter(isBlockedDispatch);
  const kpis: CommandCenterKpis = {
    decisionCount: pendingDispatch.length,
    riskFlights: riskFlights.length,
    openAnomalies: openAnomalies.length,
    dispatchBlockers: blockedOrders.length,
    delayPressure: Math.round(sum(delayValues) / Math.max(delayValues.length, 1)),
  };

  return {
    kpis,
    verdict: calculateVerdict(kpis, windowHours),
    priorityQueue: buildPriorityQueue(riskFlights, openAnomalies, blockedOrders),
    windowPressure: buildWindowPressure(scopedFlights, now),
    heatmapData: buildHeatmap(openAnomalies),
    dispatchLoad: buildDispatchLoad(pendingDispatch),
    terminalLoad: buildTerminalLoad(scopedFlights),
  };
}

export function calculateVerdict(kpis: CommandCenterKpis = EMPTY_KPIS, windowHours = 2): CommandCenterVerdict {
  const windowText = `当前 ${windowHours}h 窗口`;
  if (kpis.openAnomalies > 5 || kpis.riskFlights > 10 || kpis.dispatchBlockers > 5) {
    return {
      title: '高压力态势',
      detail: `${kpis.openAnomalies} 个异常，${kpis.riskFlights} 个风险航班，${kpis.dispatchBlockers} 个派工阻塞`,
      severity: 'critical',
      window: windowText,
    };
  }
  if (kpis.openAnomalies > 2 || kpis.riskFlights > 5 || kpis.dispatchBlockers > 0) {
    return {
      title: '需关注',
      detail: `${kpis.openAnomalies} 个异常待处理，${kpis.dispatchBlockers} 个派工阻塞`,
      severity: 'warn',
      window: windowText,
    };
  }
  return {
    title: '运行正常',
    detail: '无重大风险',
    severity: 'ok',
    window: windowText,
  };
}

function buildPriorityQueue(
  riskFlights: RawRecord[],
  anomalies: RawRecord[],
  blockedOrders: RawRecord[],
): CommandQueueItem[] {
  const flightItems = riskFlights.map((flight, index) => ({
    id: stableId(flight, 'risk-flight', index),
    name: `${flightLabel(flight)} 延误 ${delayMinutes(flight) ?? 0} 分钟`,
    meta: `${textValue(flight.terminal) || '未知航站楼'} / ${textValue(flight.stand) || '未分配机位'}`,
    severity: severityFromDelay(delayMinutes(flight) ?? 0),
    score: 100 + (delayMinutes(flight) ?? 0),
  }));
  const anomalyItems = anomalies.map((anomaly, index) => ({
    id: stableId(anomaly, 'anomaly', index),
    name: `${flightLabel(anomaly)} ${textValue(anomaly.message) || textValue(anomaly.description) || textValue(anomaly.anomaly_type) || '异常待处理'}`,
    meta: `${textValue(anomaly.terminal) || '未知航站楼'} / ${textValue(anomaly.stand) || '未分配机位'}`,
    severity: severityFromText(textValue(anomaly.severity)),
    score: severityScore(textValue(anomaly.severity)) + 80,
  }));
  const dispatchItems = blockedOrders.map((order, index) => ({
    id: stableId(order, 'dispatch', index),
    name: `${flightLabel(order)} ${textValue(order.task_name) || textValue(order.task_type) || '派工任务'} 受阻`,
    meta: `${textValue(order.team_name) || textValue(order.assignee_name) || '未分配资源'} / ${textValue(order.status) || 'pending'}`,
    severity: 'critical' as const,
    score: 160,
  }));

  return [...dispatchItems, ...anomalyItems, ...flightItems]
    .sort((left, right) => right.score - left.score || left.name.localeCompare(right.name, 'zh-CN'))
    .slice(0, 12)
    .map((item) => ({
      id: item.id,
      name: item.name,
      meta: item.meta,
      severity: item.severity,
    }));
}

function buildWindowPressure(flights: RawRecord[], now: Date): CommandDistributionItem[] {
  const buckets = [
    { id: '0-60', label: '0-60 分钟', min: 0, max: 60 },
    { id: '60-120', label: '60-120 分钟', min: 60, max: 120 },
    { id: '120+', label: '120 分钟以上', min: 120, max: Number.POSITIVE_INFINITY },
  ];
  return buckets.map((bucket) => {
    const items = flights.filter((flight) => {
      const minutes = minutesUntilDeparture(flight, now);
      return minutes !== null && minutes >= bucket.min && minutes < bucket.max;
    });
    const delayed = items.filter((flight) => (delayMinutes(flight) ?? 0) > 30).length;
    return {
      id: bucket.id,
      label: bucket.label,
      value: items.length,
      detail: `${delayed} 个高风险离港`,
      severity: delayed > 3 ? 'critical' : delayed > 0 ? 'warn' : 'ok',
    };
  });
}

function buildHeatmap(anomalies: RawRecord[]): CommandDistributionItem[] {
  return topGroups(anomalies, (item) => textValue(item.stand) || textValue(item.stand_id) || '未分配机位')
    .map(([label, value]) => ({
      id: `stand-${label}`,
      label,
      value,
      detail: `${value} 个未闭环异常`,
      severity: value > 3 ? 'critical' : value > 1 ? 'warn' : 'ok',
    }));
}

function buildDispatchLoad(orders: RawRecord[]): CommandDistributionItem[] {
  return topGroups(orders, (item) => textValue(item.team_name) || textValue(item.assignee_name) || textValue(item.task_type) || '未分配资源')
    .map(([label, value]) => ({
      id: `dispatch-${label}`,
      label,
      value,
      detail: `${value} 个待执行任务`,
      severity: value > 6 ? 'critical' : value > 3 ? 'warn' : 'ok',
    }));
}

function buildTerminalLoad(flights: RawRecord[]): CommandDistributionItem[] {
  return topGroups(flights, (item) => textValue(item.terminal) || '未知航站楼')
    .map(([label, value]) => ({
      id: `terminal-${label}`,
      label,
      value,
      detail: `${value} 个窗口内航班`,
      severity: value > 20 ? 'critical' : value > 10 ? 'warn' : 'ok',
    }));
}

function topGroups(items: RawRecord[], getLabel: (item: RawRecord) => string): [string, number][] {
  const groups = new Map<string, number>();
  for (const item of items) {
    const label = getLabel(item);
    groups.set(label, (groups.get(label) ?? 0) + 1);
  }
  return [...groups.entries()]
    .sort((left, right) => right[1] - left[1] || left[0].localeCompare(right[0], 'zh-CN'))
    .slice(0, 8);
}

function isRecord(value: unknown): value is RawRecord {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function textValue(value: unknown): string {
  return typeof value === 'string' ? value.trim() : '';
}

function numericValue(value: unknown): number | null {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === 'string' && value.trim()) {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

function delayMinutes(flight: RawRecord): number | null {
  const direct = numericValue(flight.delay_minutes)
    ?? numericValue(flight.departure_delay_minutes)
    ?? numericValue(flight.delayMinutes);
  if (direct !== null) {
    return Math.max(0, Math.round(direct));
  }
  const scheduled = parseTime(flight.scheduled_departure);
  const estimated = parseTime(flight.estimated_departure);
  if (scheduled && estimated) {
    return Math.max(0, Math.round((estimated.getTime() - scheduled.getTime()) / 60000));
  }
  return null;
}

function minutesUntilDeparture(flight: RawRecord, now: Date): number | null {
  const departure = parseTime(flight.estimated_departure) ?? parseTime(flight.scheduled_departure);
  if (!departure) {
    return null;
  }
  return Math.round((departure.getTime() - now.getTime()) / 60000);
}

function isInFutureWindow(flight: RawRecord, hours: number, now: Date): boolean {
  const minutes = minutesUntilDeparture(flight, now);
  return minutes !== null && minutes >= 0 && minutes <= hours * 60;
}

function parseTime(value: unknown): Date | null {
  if (typeof value !== 'string' || !value.trim()) {
    return null;
  }
  const timestamp = Date.parse(value);
  return Number.isNaN(timestamp) ? null : new Date(timestamp);
}

function isOpenStatus(status: string): boolean {
  return !status || ['open', 'pending', 'new', 'active', 'acknowledged'].includes(status.toLowerCase());
}

function isPendingDispatchStatus(status: string): boolean {
  return !status || ['pending', 'draft', 'published', 'assigned', 'accepted', 'in_progress'].includes(status.toLowerCase());
}

function isBlockedDispatch(order: RawRecord): boolean {
  const status = textValue(order.status).toLowerCase();
  return order.blocked === true
    || textValue(order.safety_gate).toLowerCase() === 'blocked'
    || status === 'blocked'
    || status === 'conflict';
}

function flightLabel(item: RawRecord): string {
  return textValue(item.flight_number)
    || textValue(item.flight_no)
    || textValue(item.flight_id)
    || textValue(item.id)
    || '未关联航班';
}

function stableId(item: RawRecord, prefix: string, index: number): string {
  return textValue(item.id)
    || textValue(item.order_id)
    || textValue(item.anomaly_id)
    || textValue(item.flight_id)
    || `${prefix}-${index}`;
}

function severityScore(severity: string): number {
  const normalized = severity.toLowerCase();
  if (['critical', 'high', 'error'].includes(normalized)) {
    return 80;
  }
  if (['warning', 'warn', 'medium'].includes(normalized)) {
    return 45;
  }
  return 20;
}

function severityFromText(severity: string): CommandQueueItem['severity'] {
  const score = severityScore(severity);
  return score >= 80 ? 'critical' : score >= 45 ? 'warn' : 'ok';
}

function severityFromDelay(minutes: number): CommandQueueItem['severity'] {
  if (minutes >= 60) {
    return 'critical';
  }
  if (minutes > 30) {
    return 'warn';
  }
  return 'ok';
}

function sum(values: number[]): number {
  return values.reduce((total, value) => total + value, 0);
}
