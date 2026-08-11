import { computed, onBeforeUnmount, readonly, ref, watch } from 'vue';
import type { Ref } from 'vue';
import { useAuth } from './useAuth';
import { useProtobuf } from './useProtobuf';
import { type SSEStatus, useSSE } from './useSSE';
import { useToast } from './useToast';
import { useNotification } from './useNotification';
import {
  normalizeFlightId,
  type Flight,
  type LegType,
  type UseFlightDataReturn,
} from './useFlightData';
import type { FlightStreamFrame, AnomalyAlert, UserNotification } from '@/types/bindings';
export type { UserNotification };
import { SSE_TOPICS as TOPICS } from '../types/backend';

export type FlightConnectionStatusKey = 'connecting' | 'online' | 'reconnecting' | 'offline';

export interface FlightConnectionStatusMeta {
  text: string;
  cls: FlightConnectionStatusKey;
}

export interface FlightStreamOptions {
  flightData: {
    originalFlights: Readonly<Ref<readonly unknown[]>>;
    setFlights: UseFlightDataReturn['setFlights'];
    loadAirportContext: UseFlightDataReturn['loadAirportContext'];
    loadFlightsPaged: UseFlightDataReturn['loadFlightsPaged'];
  };
  announce?: (message: string) => void;
  reconnectMs?: number;
  anomalyReconnectMs?: number;
  notificationReconnectMs?: number;
}

const DEFAULT_RECONNECT_MS = 5000;
const PROTO_SCHEMA_PATH = '/frontend/proto/transport_v1.proto';
const PROTO_FLIGHT_STREAM_FRAME = 'transport.v1.FlightStreamFrame';
const MESSAGE_LIMIT = 100;
const UNIFIED_STREAM_URL = '/api/v2/sse/stream?topics=flights,flight_status_changes,anomaly_alerts,business_cases';
const BUSINESS_CASE_EVENT_NAMES = [
  TOPICS.BUSINESS_CASES,
  'business_case.created',
  'business_case.updated',
  'business_case.deleted',
] as const;

interface QueuedFlightUpdate {
  partial: Partial<Flight>;
  changedFields: string[];
}

export interface FlightFlashEvent {
  flightId: string;
  tone: 'warn' | 'ok';
}

let flightUpdateQueue: QueuedFlightUpdate[] = [];
let isFlushScheduled = false;

const NOTIFICATION_ID_CACHE_LIMIT = 500;

function extractEventPayload(event: Event): unknown {
  const messageEvent = event as MessageEvent & { __parsedJson?: unknown };
  return messageEvent.__parsedJson !== undefined ? messageEvent.__parsedJson : messageEvent.data;
}

export function coerceRecord(rawPayload: unknown): Record<string, unknown> | null {
  if (typeof rawPayload === 'string') {
    try {
      return JSON.parse(rawPayload) as Record<string, unknown>;
    } catch {
      return null;
    }
  }
  if (rawPayload && typeof rawPayload === 'object') {
    return rawPayload as Record<string, unknown>;
  }
  return null;
}

const CONNECTION_STATUS: Record<FlightConnectionStatusKey, FlightConnectionStatusMeta> = {
  connecting: { text: '实时连接中', cls: 'connecting' },
  online: { text: '实时连接正常', cls: 'online' },
  reconnecting: { text: '实时连接重试中', cls: 'reconnecting' },
  offline: { text: '实时连接中断', cls: 'offline' },
};

const FIELD_LABELS: Record<string, string> = {
  status: '状态',
  inbound_leg: '进港航段',
  outbound_leg: '出港航段',
  stand: '机位',
  gate: '登机口',
  flight_number: '主航班号',
  scheduled_departure: '计划起飞',
  scheduled_arrival: '计划到达',
  estimated_departure: '预计起飞',
  estimated_arrival: '预计到达',
  actual_departure: '实际起飞',
  actual_arrival: '实际到达',
  start_boarding_time: '开始登机',
  end_boarding_time: '结束登机',
  aircraft_type_detail: '机型',
  cobt_time: 'COBT',
  on_blocks_time: '上轮挡',
  cabin_door_open_time: '开舱门',
  deboarding_complete_time: '下客完成',
  cleaning_start_time: '清洁开始',
  cleaning_end_time: '清洁结束',
  cabin_door_close_time: '关客舱门',
  cargo_door_close_time: '关货舱门',
  loading_complete_time: '装载完成',
  off_blocks_time: '撤轮挡',
  passenger_ready_time: '人齐',
  boarding_allowed_time: '允许登机',
};

function isBinaryPayload(value: unknown): value is ArrayBuffer | Uint8Array {
  return value instanceof ArrayBuffer || value instanceof Uint8Array;
}

function formatValue(value: unknown): string {
  if (value === null || value === undefined || value === '') {
    return '--';
  }

  if (typeof value === 'boolean') {
    return value ? '是' : '否';
  }

  if (Array.isArray(value)) {
    return value.map((entry) => formatValue(entry)).join('、') || '--';
  }

  if (typeof value === 'string') {
    if (/^\d{4}-\d{2}-\d{2}T/.test(value)) {
      const timestamp = new Date(value);
      if (!Number.isNaN(timestamp.getTime())) {
        return timestamp.toLocaleString('zh-CN', { hour12: false });
      }
    }
    return value.trim() || '--';
  }

  if (typeof value === 'object') {
    const record = value as Record<string, unknown>;
    if (typeof record.flight_no === 'string' && record.flight_no.trim()) {
      return record.flight_no.trim().toUpperCase();
    }
    if (typeof record.code === 'string' && record.code.trim()) {
      return record.code.trim().toUpperCase();
    }
    try {
      return JSON.stringify(value);
    } catch {
      return '[object]';
    }
  }

  return String(value);
}

function getFlightLabel(flight: Flight | null | undefined, fallbackId?: string): string {
  const candidates = [
    flight?.flight_number,
    flight?.outbound_leg?.flight_no,
    flight?.inbound_leg?.flight_no,
    fallbackId,
  ];

  for (const candidate of candidates) {
    const normalized = String(candidate ?? '').trim();
    if (normalized) {
      return normalized.toUpperCase();
    }
  }

  return '未知航班';
}

function normalizeNotificationList(payload: unknown): UserNotification[] {
  if (Array.isArray(payload)) {
    return payload as UserNotification[];
  }

  if (!payload || typeof payload !== 'object') {
    return [];
  }

  const record = payload as Record<string, unknown>;
  const candidates = [record.items, record.data, record.notifications, record.entries];
  for (const candidate of candidates) {
    if (Array.isArray(candidate)) {
      return candidate as UserNotification[];
    }
  }

  return [];
}

function normalizeSeverityLabel(severity: unknown): string {
  const normalized = String(severity ?? '').trim().toLowerCase();
  switch (normalized) {
    case 'critical':
      return '紧急';
    case 'warning':
      return '提醒';
    case 'success':
      return '成功';
    case 'info':
      return '通知';
    default:
      return '消息';
  }
}

function mapSSEStatus(status: SSEStatus): FlightConnectionStatusKey {
  switch (status) {
    case 'online':
      return 'online';
    case 'reconnecting':
      return 'reconnecting';
    case 'offline':
      return 'offline';
    case 'connecting':
    case 'idle':
    default:
      return 'connecting';
  }
}

export function useFlightStream(options: FlightStreamOptions) {
  const auth = useAuth();
  const protobuf = useProtobuf(PROTO_SCHEMA_PATH);
  const { showToast } = useToast();
  const { sentReceiptReminderQueue, updateUnreadCount } = useNotification();

  const unifiedSSE = useSSE({
    url: UNIFIED_STREAM_URL,
    authenticated: true,
    autoReconnect: true,
    reconnectMs: options.reconnectMs ?? DEFAULT_RECONNECT_MS,
    clientScope: 'flight_monitor_unified',
  });

  const connectionStatusKey = ref<FlightConnectionStatusKey>('connecting');
  const updateMessages = ref<string[]>([]);
  const lastUpdatedAt = ref(new Date());
  const initialized = ref(false);
  const isRefreshing = ref(false);
  const initializationError = ref<string | null>(null);
  const usingFallbackData = ref(false);
  const hasLoadedSnapshot = ref(false);
  const anomalyRegistry = ref<Record<string, AnomalyAlert[]>>({});
  const notificationIds = new Set<string>();
  const flightFlashEvents = ref<FlightFlashEvent[]>([]);

  function publishFlashEvents(events: FlightFlashEvent[]): void {
    if (events.length === 0) return;
    flightFlashEvents.value = events;
  }

  function resolveFlashTone(previousFlight: Flight | null, nextFlight: Flight): 'warn' | 'ok' {
    const nextStatus = String(nextFlight.status ?? '').toLowerCase();
    const prevOpenAnomalies = Number(previousFlight?.anomaly_summary?.open_count ?? 0);
    const nextOpenAnomalies = Number(nextFlight.anomaly_summary?.open_count ?? 0);
    if (/delay|cancel|abnormal|anomaly|divert/.test(nextStatus) || nextOpenAnomalies > prevOpenAnomalies) {
      return 'warn';
    }
    return 'ok';
  }

  const criticalNotificationQueue = ref<UserNotification[]>([]);
  const parseErrorCount = ref(0);
  let fullSyncTimer: number | null = null;
  const FULL_SYNC_MIN_INTERVAL_MS = 5000;
  let lastFullSyncAt = 0;

  function announce(message: string): void {
    options.announce?.(message);
  }

  function setConnectionStatus(status: FlightConnectionStatusKey): void {
    if (connectionStatusKey.value === status) {
      return;
    }
    connectionStatusKey.value = status;
    announce(CONNECTION_STATUS[status].text);
  }

  function touchLastUpdated(): void {
    lastUpdatedAt.value = new Date();
  }

  function pushUpdateMessage(message: string): void {
    const normalized = String(message ?? '').trim();
    if (!normalized) {
      return;
    }
    updateMessages.value = [normalized, ...updateMessages.value].slice(0, MESSAGE_LIMIT);
  }

  function getCanonicalFlights(): Flight[] {
    return Array.from(options.flightData.originalFlights.value) as Flight[];
  }

  function commitFlights(nextFlights: Flight[]): Flight[] {
    touchLastUpdated();
    return options.flightData.setFlights(nextFlights, true);
  }

  function syncAnomalySummaries(affectedFlightIds?: ReadonlySet<string>): void {
    const flights = getCanonicalFlights();
    const nextFlights = [...flights];
    const flashEvents: FlightFlashEvent[] = [];
    let changed = false;

    for (let i = 0; i < flights.length; i += 1) {
      const flight = flights[i];
      const flightId = normalizeFlightId(flight.flight_id);
      if (!flightId) continue;
      if (affectedFlightIds && !affectedFlightIds.has(flightId)) continue;
      const anomalies = anomalyRegistry.value[flightId] ?? [];
      const prevOpen = Number(flight.anomaly_summary?.open_count ?? 0);
      const nextOpen = anomalies.length;
      nextFlights[i] = {
        ...flight,
        anomaly_summary: {
          ...(flight.anomaly_summary ?? {}),
          has_open_anomaly: anomalies.length > 0,
          open_count: anomalies.length,
          acknowledged_count: Number(flight.anomaly_summary?.acknowledged_count ?? 0),
        },
      } satisfies Flight;
      changed = true;
      if (nextOpen !== prevOpen) {
        flashEvents.push({ flightId, tone: nextOpen > prevOpen ? 'warn' : 'ok' });
      }
    }

    if (!changed) return;
    commitFlights(nextFlights);
    publishFlashEvents(flashEvents);
  }

  function describeFieldUpdate(
    oldFlight: Flight | null | undefined,
    nextFlight: Flight,
    changedFields: string[],
    fallbackFlightId: string,
  ): void {
    const flightLabel = getFlightLabel(nextFlight, fallbackFlightId);
    changedFields.forEach((field) => {
      const previousValue = formatValue(oldFlight ? oldFlight[field] : undefined);
      const nextValue = formatValue(nextFlight[field]);
      if (previousValue === nextValue) {
        return;
      }
      const fieldLabel = FIELD_LABELS[field] ?? field;
      pushUpdateMessage(`${flightLabel} ${fieldLabel}：${previousValue} → ${nextValue}`);
    });
  }

  function scheduleFullSync(): void {
    if (fullSyncTimer !== null) {
      window.clearTimeout(fullSyncTimer);
    }
    const now = Date.now();
    const elapsed = now - lastFullSyncAt;
    const delay = Math.max(0, FULL_SYNC_MIN_INTERVAL_MS - elapsed);
    fullSyncTimer = window.setTimeout(() => {
      fullSyncTimer = null;
      lastFullSyncAt = Date.now();
      void refreshFlights(false).catch((error) => {
        console.warn('Flight stream full sync failed:', error);
      });
    }, delay);
  }

  async function decodeFlightFrame(payload: ArrayBuffer | Uint8Array): Promise<FlightStreamFrame> {
    if (!protobuf.isReady.value) {
      await protobuf.load();
    }
    return protobuf.decode<FlightStreamFrame>(PROTO_FLIGHT_STREAM_FRAME, payload);
  }

  async function normalizeFlightFrame(payload: unknown): Promise<FlightStreamFrame | null> {
    if (!payload) {
      return null;
    }

    if (typeof payload === 'string') {
      try {
        return JSON.parse(payload) as FlightStreamFrame;
      } catch {
        return null;
      }
    }

    if (isBinaryPayload(payload)) {
      return decodeFlightFrame(payload);
    }

    if (typeof payload === 'object') {
      return payload as FlightStreamFrame;
    }

    return null;
  }

  function flushFlightUpdates(): void {
    isFlushScheduled = false;

    if (flightUpdateQueue.length === 0) {
      return;
    }

    // Deduplicate: later updates for same flight_id override earlier ones
    const mergedMap = new Map<string, Partial<Flight>>();
    const changedFieldsMap = new Map<string, string[]>();
    const flightOrder: string[] = [];

    for (const item of flightUpdateQueue) {
      const partial = item.partial;
      const fid = normalizeFlightId(partial.flight_id);
      if (!fid) continue;
      if (!mergedMap.has(fid)) {
        flightOrder.push(fid);
      }
      const existing = mergedMap.get(fid) ?? {};
      mergedMap.set(fid, { ...existing, ...partial, flight_id: fid });
      const currentFields = changedFieldsMap.get(fid) ?? [];
      const nextFields = item.changedFields.filter((field) => !currentFields.includes(field));
      changedFieldsMap.set(fid, [...currentFields, ...nextFields]);
    }

    // Single-pass merge with an id index (O(n + m) instead of O(m·n))
    const currentFlights = getCanonicalFlights();
    const indexById = new Map<string, number>();
    for (let i = 0; i < currentFlights.length; i += 1) {
      const id = normalizeFlightId(currentFlights[i].flight_id);
      if (id) indexById.set(id, i);
    }

    const nextFlights = [...currentFlights];
    const flashEvents: FlightFlashEvent[] = [];
    for (const fid of flightOrder) {
      const partial = mergedMap.get(fid)!;
      const currentIndex = indexById.get(fid) ?? -1;
      const currentFlight = currentIndex >= 0 ? nextFlights[currentIndex] : null;
      const mergedFlight = buildMergedFlight(currentFlight, partial as Flight, fid);
      if (currentIndex >= 0) {
        nextFlights[currentIndex] = mergedFlight;
      } else {
        indexById.set(fid, nextFlights.length);
        nextFlights.push(mergedFlight);
      }
      describeFieldUpdate(currentFlight, mergedFlight, changedFieldsMap.get(fid) ?? [], fid);
      flashEvents.push({ flightId: fid, tone: resolveFlashTone(currentFlight, mergedFlight) });
    }

    commitFlights(nextFlights);
    flightUpdateQueue = [];
    publishFlashEvents(flashEvents);
  }

  function queueFlightUpdate(partial: Partial<Flight>, changedFields: string[] = []): void {
    flightUpdateQueue.push({ partial, changedFields });

    if (!isFlushScheduled) {
      isFlushScheduled = true;
      requestAnimationFrame(flushFlightUpdates);
    }
  }

  function buildMergedFlight(
    currentFlight: Flight | null,
    partialFlight: Flight,
    flightId: string,
  ): Flight {
    const hasPartialField = (field: keyof Flight): boolean =>
      Object.prototype.hasOwnProperty.call(partialFlight, field);
    return {
      ...(currentFlight ?? {}),
      ...partialFlight,
      flight_id: partialFlight.flight_id ?? currentFlight?.flight_id ?? flightId,
      inbound_leg: hasPartialField('inbound_leg')
        ? (partialFlight.inbound_leg ?? null)
        : (currentFlight?.inbound_leg ?? null),
      outbound_leg: hasPartialField('outbound_leg')
        ? (partialFlight.outbound_leg ?? null)
        : (currentFlight?.outbound_leg ?? null),
      anomaly_summary: hasPartialField('anomaly_summary')
        ? (partialFlight.anomaly_summary ?? null)
        : (currentFlight?.anomaly_summary ?? null),
    } satisfies Flight;
  }

  async function handleFlightPayload(rawPayload: unknown): Promise<void> {
    const payload = await normalizeFlightFrame(rawPayload);
    if (!payload || typeof payload !== 'object') {
      return;
    }

    if (payload.type === 'heartbeat') {
      setConnectionStatus('online');
      return;
    }

    if (payload.type === 'initial_data' && Array.isArray(payload.flights)) {
      commitFlights(payload.flights as unknown as Flight[]);
      setConnectionStatus('online');
      announce(`已加载 ${payload.flights.length} 条航班数据`);
      scheduleFullSync();
      return;
    }

    const incrementalPayload =
      (payload.patch as Partial<Flight> | undefined) ??
      (payload.data as Flight | undefined) ??
      payload.flight_data ??
      payload.flight;
    const normalizedId = normalizeFlightId(payload.flight_id ?? incrementalPayload?.flight_id);
    if (!normalizedId || !incrementalPayload) {
      return;
    }

    const changedFields = Array.isArray(payload.changed_fields)
      ? payload.changed_fields.map((field) => String(field))
      : Object.keys(incrementalPayload).filter((field) => !['flight_id', 'version', 'updated_at'].includes(field));
    queueFlightUpdate(
      { ...(incrementalPayload as Partial<Flight>), flight_id: normalizedId },
      changedFields,
    );
    setConnectionStatus('online');
  }

  function handleBusinessCasePayload(rawPayload: unknown): void {
    const payload = coerceRecord(rawPayload);
    if (!payload) {
      return;
    }

    const eventName = String(payload.event || payload.type || 'business_case.updated').trim();
    const caseId = String(payload.case_id || '').trim();
    const flightId = String(payload.flight_id || '').trim();
    if (!caseId && !flightId) {
      return;
    }

    scheduleFullSync();
    setConnectionStatus('online');

    if (caseId) {
      const action = eventName.endsWith('.created')
        ? '创建'
        : eventName.endsWith('.deleted')
          ? '删除'
          : '更新';
      pushUpdateMessage(`业务事项已${action}，正在同步航班详情`);
    }
  }

  function setInitialAnomalies(items: AnomalyAlert[]): void {
    const nextRegistry: Record<string, AnomalyAlert[]> = {};
    items.forEach((item) => {
      const flightId = normalizeFlightId(item?.flight_id);
      if (!flightId) {
        return;
      }
      nextRegistry[flightId] ??= [];
      nextRegistry[flightId].push(item);
    });
    anomalyRegistry.value = nextRegistry;
    syncAnomalySummaries();
  }

  function upsertAnomaly(item: AnomalyAlert, resolved: boolean): void {
    const flightId = normalizeFlightId(item?.flight_id);
    if (!flightId) {
      return;
    }

    const nextRegistry = { ...anomalyRegistry.value };
    const bucket = [...(nextRegistry[flightId] ?? [])];
    const anomalyId = String(item.anomaly_id ?? item.id ?? '').trim();
    const existingIndex = bucket.findIndex((entry) => String(entry.anomaly_id ?? entry.id ?? '').trim() === anomalyId);

    if (resolved) {
      if (existingIndex >= 0) {
        bucket.splice(existingIndex, 1);
      }
    } else if (existingIndex >= 0) {
      bucket[existingIndex] = item;
    } else {
      bucket.push(item);
    }

    if (bucket.length > 0) {
      nextRegistry[flightId] = bucket;
    } else {
      delete nextRegistry[flightId];
    }

    anomalyRegistry.value = nextRegistry;
    syncAnomalySummaries(new Set([flightId]));
  }

  function handleAnomalyPayload(rawPayload: unknown): void {
    const payload = coerceRecord(rawPayload);
    if (!payload) {
      return;
    }

    if (payload.type === 'heartbeat') {
      return;
    }

    if (payload.type === 'initial_data') {
      const items = Array.isArray(payload.data) ? payload.data as AnomalyAlert[] : Array.isArray(payload.items) ? payload.items as AnomalyAlert[] : [];
      setInitialAnomalies(items);
      return;
    }

    const anomaly = (payload.data as AnomalyAlert | undefined) ?? (payload.anomaly as AnomalyAlert | undefined);
    if (!anomaly) {
      return;
    }

    const resolved = payload.status === 'resolved' || anomaly.status === 'resolved';
    upsertAnomaly(anomaly, resolved);
  }

  function pushNotificationMessage(item: UserNotification, initial = false): void {
    const notificationId = String(item.notification_id ?? item.id ?? '').trim();
    if (notificationId) {
      if (notificationIds.has(notificationId)) {
        return;
      }
      notificationIds.add(notificationId);
      if (notificationIds.size > NOTIFICATION_ID_CACHE_LIMIT) {
        const oldest = notificationIds.values().next().value;
        if (oldest !== undefined) notificationIds.delete(oldest);
      }
    }

    const title = String(item.title ?? '').trim();
    const body = String(item.body ?? item.message ?? item.content ?? '').trim();
    const relatedFlight = String(item.related_flight_label ?? item.related_flight_no ?? item.flight_no ?? '').trim();
    const severityLabel = normalizeSeverityLabel(item.severity);
    const parts = [title || body || '收到通知'];
    if (body && body !== title) {
      parts.push(body);
    }
    if (relatedFlight) {
      parts.push(`航班 ${relatedFlight}`);
    }
    
    // Global interception for Critical un-acked notifications
    if (item.severity === 'critical' && item.receipt_required && item.ack_status === 'pending') {
      const exists = criticalNotificationQueue.value.find(n => n.notification_id === notificationId);
      if (!exists) {
        criticalNotificationQueue.value.push(item);
      }
    } else {
      pushUpdateMessage(`${severityLabel}：${parts.join(' · ')}`);
      if (!initial) {
        showToast(item.severity === 'warning' ? 'warning' : 'info', parts.join(' · '));
      }
    }

    if (!initial) {
      void updateUnreadCount();
    }
  }

  function shiftCriticalNotification() {
    return criticalNotificationQueue.value.shift();
  }

  function handleNotificationPayload(rawPayload: unknown, initial = false): void {
    const payload = coerceRecord(rawPayload) as (UserNotification & Record<string, unknown>) | null;
    if (!payload) {
      return;
    }

    if (initial) {
      const items = normalizeNotificationList(payload);
      [...items].reverse().forEach((item) => {
        pushNotificationMessage(item, true);
      });
      return;
    }

    if ('type' in payload && payload.type === 'sender_receipt_update') {
      handleSenderReceiptUpdatePayload(payload);
      return;
    }

    if ('type' in payload && payload.type === 'user_notification') {
      const wrappedNotification = (payload as Record<string, unknown>).notification;
      pushNotificationMessage((wrappedNotification || payload) as UserNotification, false);
      return;
    }

    pushNotificationMessage(payload as UserNotification, false);
  }

  function handleSenderReceiptUpdatePayload(rawPayload: unknown): void {
    const payload = rawPayload && typeof rawPayload === 'object'
      ? rawPayload as Record<string, unknown>
      : null;
    if (!payload) {
      return;
    }

    const receiptGroupId = String(payload.receipt_group_id || '').trim();
    if (!receiptGroupId) {
      return;
    }

    const recipientLabel = String(payload.recipient_username || payload.recipient_user_id || '对方').trim() || '对方';
    const title = String(payload.title || '通知').trim() || '通知';
    const ackStatus = String(payload.ack_status || '').trim().toLowerCase();

    if (ackStatus === 'acknowledged') {
      showToast('success', `${recipientLabel} 已确认《${title}》`, { duration: 3200 });
    } else if (ackStatus === 'rejected') {
      const suffix = payload.ack_note ? `：${String(payload.ack_note).trim()}` : '';
      showToast('warning', `${recipientLabel} 已拒绝《${title}》${suffix}`, { duration: 4200 });
    }

    const summary = payload.summary && typeof payload.summary === 'object'
      ? payload.summary as Record<string, unknown>
      : {};
    const pendingCount = Number(summary.pending_count ?? 0);
    if (Boolean(summary.is_overdue) && pendingCount > 0 && !sentReceiptReminderQueue.value.includes(receiptGroupId)) {
      sentReceiptReminderQueue.value.push(receiptGroupId);
    }
  }

  async function ensureStreamAuth(): Promise<boolean> {
    if (!await auth.requireAuthAsync()) {
      setConnectionStatus('offline');
      return false;
    }

    await auth.refreshSSEToken();
    return true;
  }

  async function connectFlightStream(): Promise<void> {
    if (!await ensureStreamAuth()) {
      return;
    }
    setConnectionStatus('connecting');
    await unifiedSSE.connect();
  }

    // Unified SSE: anomaly-alerts / business_cases topics are multiplexed on the same connection.
  async function connectAnomalyStream(): Promise<void> {
    if (!await ensureStreamAuth()) {
      return;
    }
    await unifiedSSE.connect();
  }

  // Unified SSE: user_notifications topic is multiplexed on the same connection.
  async function connectNotificationStream(): Promise<void> {
    if (!await ensureStreamAuth()) {
      return;
    }
    await unifiedSSE.connect();
  }

  async function connectAll(): Promise<void> {
    await connectFlightStream();
  }

  function disconnectAll(): void {
    unifiedSSE.disconnect();
    setConnectionStatus('offline');
    if (fullSyncTimer !== null) {
      window.clearTimeout(fullSyncTimer);
      fullSyncTimer = null;
    }
    // Drain pending rAF updates on disconnect
    flightUpdateQueue = [];
    isFlushScheduled = false;
  }

  async function loadFlightsSnapshot(announceSuccess = true): Promise<Flight[]> {
    const flights = await options.flightData.loadFlightsPaged();
    touchLastUpdated();
    hasLoadedSnapshot.value = true;
    usingFallbackData.value = false;
    initializationError.value = null;
    if (announceSuccess) {
      announce(`已刷新 ${flights.length} 条航班数据`);
    }
    return flights;
  }

  async function refreshFlights(announceSuccess = true): Promise<Flight[]> {
    if (isRefreshing.value) {
      return getCanonicalFlights();
    }

    isRefreshing.value = true;
    try {
      return await loadFlightsSnapshot(announceSuccess);
    } catch (error) {
      const message = error instanceof Error ? error.message : '航班数据加载失败';
      if (!hasLoadedSnapshot.value) {
        initializationError.value = message;
      } else {
        initializationError.value = `${message}，当前显示的是上次成功加载的数据。`;
        pushUpdateMessage('航班快照刷新失败，当前数据可能已过期。');
        showToast('warning', `${message}，当前数据可能已过期`, { duration: 5000 });
      }
      throw error;
    } finally {
      isRefreshing.value = false;
    }
  }

  async function initialize(force = false): Promise<void> {
    if (isRefreshing.value && !force) return;
    if (initialized.value && !force) {
      return;
    }

    isRefreshing.value = true;
    initializationError.value = null;
    try {
      const [airportResult, flightsResult] = await Promise.allSettled([
        options.flightData.loadAirportContext(),
        loadFlightsSnapshot(false),
      ]);

      if (airportResult.status === 'rejected') {
        console.warn('Failed to load airport context:', airportResult.reason);
      }

      if (flightsResult.status === 'rejected') {
        console.warn('Failed to load flights snapshot:', flightsResult.reason);
        const message = flightsResult.reason instanceof Error
          ? flightsResult.reason.message
          : '实时快照加载失败';
        initializationError.value = message;
        pushUpdateMessage('实时快照加载失败，请稍后重试。');
        showToast('warning', `${message}，请稍后重试`, { duration: 5000 });
      }

      try {
        await connectAll();
      } catch (error) {
        console.warn('Failed to initialize flight streams:', error);
        pushUpdateMessage('实时连接初始化失败，页面将继续显示最近一次快照。');
        setConnectionStatus('offline');
      }
    } finally {
      initialized.value = true;
      isRefreshing.value = false;
    }
  }

  function handleVisibilityChange(): void {
    if (document.visibilityState === 'hidden') {
      disconnectAll();
      return;
    }

    void connectAll().catch((error) => {
      console.warn('Failed to resume flight streams:', error);
      pushUpdateMessage('实时连接恢复失败，请手动刷新或稍后重试。');
      showToast('warning', `实时连接恢复失败: ${error instanceof Error ? error.message : String(error)}`, { duration: 5000 });
      setConnectionStatus('offline');
    });
  }

  unifiedSSE.on(TOPICS.FLIGHTS, (event) => {
    void handleFlightPayload(extractEventPayload(event)).catch((error) => {
      console.error('Flight stream payload parse error:', error);
      parseErrorCount.value++;
      if (parseErrorCount.value === 1) pushUpdateMessage('航班数据解析异常，当前显示最后已知快照。');
    });
  });
  unifiedSSE.on(TOPICS.FLIGHT_STATUS_CHANGES, (event) => {
    void handleFlightPayload(extractEventPayload(event)).catch((error) => {
      console.error('Flight status change payload parse error:', error);
      parseErrorCount.value++;
      if (parseErrorCount.value === 1) pushUpdateMessage('航班数据解析异常，当前显示最后已知快照。');
    });
  });
  unifiedSSE.on('flight_updated', (event) => {
    void handleFlightPayload(extractEventPayload(event)).catch((error) => {
      console.error('Flight update payload parse error:', error);
      parseErrorCount.value++;
      if (parseErrorCount.value === 1) pushUpdateMessage('航班数据解析异常，当前显示最后已知快照。');
    });
  });
  unifiedSSE.on('flight_status_changed', (event) => {
    void handleFlightPayload(extractEventPayload(event)).catch((error) => {
      console.error('Flight status payload parse error:', error);
      parseErrorCount.value++;
      if (parseErrorCount.value === 1) pushUpdateMessage('航班数据解析异常，当前显示最后已知快照。');
    });
  });
  unifiedSSE.on(TOPICS.ANOMALY_ALERTS, (event) => {
    handleAnomalyPayload(extractEventPayload(event));
  });
  BUSINESS_CASE_EVENT_NAMES.forEach((eventName) => {
    unifiedSSE.on(eventName, (event) => {
      handleBusinessCasePayload(extractEventPayload(event));
    });
  });
  unifiedSSE.on('user_notifications', (event) => {
    handleNotificationPayload(extractEventPayload(event));
  });
  unifiedSSE.on('user_notification', (event) => {
    handleNotificationPayload(extractEventPayload(event));
  });
  unifiedSSE.on('sender_receipt_update', (event) => {
    handleNotificationPayload(extractEventPayload(event));
  });
  unifiedSSE.on('initial', (event) => {
    handleNotificationPayload(extractEventPayload(event), true);
  });
  function handleFlightLabelsChangedPayload(rawPayload: unknown): void {
    const payload = coerceRecord(rawPayload);
    if (!payload) {
      return;
    }

    const flightId = normalizeFlightId(String(payload.flight_id ?? '').trim());
    if (!flightId) {
      return;
    }

    const action = String(payload.action ?? '').trim();
    const code = String(payload.code ?? '').trim();
    const legType = payload.leg_type as LegType | null;
    const labels = Array.isArray(payload.labels) ? payload.labels as string[] : [];

    let batchFlights = getCanonicalFlights();
    const flightIndex = batchFlights.findIndex((f) => normalizeFlightId(f.flight_id) === flightId);

    if (flightIndex >= 0) {
      const flight = { ...batchFlights[flightIndex] };
      if (legType === 'inbound') {
        flight.inbound_leg = { ...(flight.inbound_leg ?? {}), labels };
      } else if (legType === 'outbound') {
        flight.outbound_leg = { ...(flight.outbound_leg ?? {}), labels };
      } else {
        flight.labels = labels;
      }
      batchFlights = [...batchFlights];
      batchFlights[flightIndex] = flight;

      const flightLabel = getFlightLabel(flight, flightId);
      if (action === 'attach') {
        pushUpdateMessage(`${flightLabel} 已添加标签"${code}"`);
      } else if (action === 'detach') {
        pushUpdateMessage(`${flightLabel} 已移除标签"${code}"`);
      }

      commitFlights(batchFlights);
    }
  }

  unifiedSSE.on('heartbeat', () => {
    setConnectionStatus('online');
  });
  unifiedSSE.on('flight_labels_changed', (event) => {
    handleFlightLabelsChangedPayload(extractEventPayload(event));
  });

  watch(unifiedSSE.status, (status) => {
    setConnectionStatus(mapSSEStatus(status));
  }, { immediate: true });

  document.addEventListener('visibilitychange', handleVisibilityChange);
  window.addEventListener('beforeunload', disconnectAll);

  onBeforeUnmount(() => {
    document.removeEventListener('visibilitychange', handleVisibilityChange);
    window.removeEventListener('beforeunload', disconnectAll);
    disconnectAll();
  });

  return {
    connectionStatusKey: readonly(connectionStatusKey),
    connectionStatusMeta: computed(() => CONNECTION_STATUS[connectionStatusKey.value]),
    connectionStatusText: computed(() => CONNECTION_STATUS[connectionStatusKey.value].text),
    connectionStatusClass: computed(() => CONNECTION_STATUS[connectionStatusKey.value].cls),
    updateMessages: readonly(updateMessages),
    flightFlashEvents: readonly(flightFlashEvents),
    lastUpdatedAt: readonly(lastUpdatedAt),
    anomalyRegistry: readonly(anomalyRegistry),
    initialized: readonly(initialized),
    isRefreshing: readonly(isRefreshing),
    initializationError: readonly(initializationError),
    usingFallbackData: readonly(usingFallbackData),
    hasLoadedSnapshot: readonly(hasLoadedSnapshot),
    connectFlightStream,
    connectAnomalyStream,
    connectNotificationStream,
    connectAll,
    disconnectAll,
    refreshFlights,
    initialize,
    handleFlightPayload,
    criticalNotificationQueue: readonly(criticalNotificationQueue),
    shiftCriticalNotification
  };
}

export function buildDebouncedScheduler(
  fn: () => void,
  minIntervalMs: number,
): () => void {
  let timer: ReturnType<typeof setTimeout> | null = null;
  let lastRunAt = 0;

  return function schedule(): void {
    if (timer !== null) {
      clearTimeout(timer);
    }
    const now = Date.now();
    const elapsed = now - lastRunAt;
    const delay = Math.max(0, minIntervalMs - elapsed);
    timer = setTimeout(() => {
      timer = null;
      lastRunAt = Date.now();
      fn();
    }, delay);
  };
}
