import { computed, getCurrentInstance, onBeforeUnmount, ref } from 'vue';
import { useApi } from './useApi';
import { useSSE } from './useSSE';
import { useToast } from './useToast';

/** Details payload for schedule overrun warnings. */
export interface DispatchOverrunWarningDetails {
  shared_personnel?: unknown;
  countdown_minutes?: number | null;
  lead_minutes?: number | null;
  lead_source?: string | null;
  eta_missing?: boolean | null;
  predicted_conflict_minutes?: number | null;
  [key: string]: unknown;
}

/**
 * Non-blocking dispatch overrun warning alert.
 * acknowledge = seen only; resolve = close. Never gates publish/replan/dispatch.
 */
export interface DispatchOverrunWarning {
  id: string;
  flight_id?: string | null;
  alert_type?: string | null;
  severity?: string | null;
  message?: string | null;
  is_resolved?: boolean | null;
  dedupe_key: string;
  current_order_id?: string | null;
  next_order_id?: string | null;
  occurrence_count?: number | null;
  acknowledged_at?: string | null;
  acknowledged_by?: string | null;
  details?: DispatchOverrunWarningDetails | null;
}

const SSE_URL = '/api/v2/sse/stream?topics=dispatch_alerts';
const EVENT_NAME = 'dispatch_overrun_warning';
const ALERTS_URL = '/api/v2/dispatch/alerts';

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
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

/** Unwrap list payloads: array | {items} | {data} | {data:{items}}. */
export function unwrapAlertList(payload: unknown): unknown[] {
  if (Array.isArray(payload)) {
    return payload;
  }
  if (!isRecord(payload)) {
    return [];
  }
  if (Array.isArray(payload.items)) {
    return payload.items;
  }
  if (Array.isArray(payload.data)) {
    return payload.data;
  }
  if (isRecord(payload.data)) {
    if (Array.isArray(payload.data.items)) {
      return payload.data.items;
    }
    if (Array.isArray(payload.data.alerts)) {
      return payload.data.alerts;
    }
  }
  if (Array.isArray(payload.alerts)) {
    return payload.alerts;
  }
  return [];
}

export function normalizeOverrunWarning(value: unknown): DispatchOverrunWarning | null {
  if (!isRecord(value)) {
    return null;
  }
  const id = typeof value.id === 'string' ? value.id.trim() : '';
  const dedupeKeyRaw = value.dedupe_key ?? value.dedupeKey;
  const dedupe_key = typeof dedupeKeyRaw === 'string' ? dedupeKeyRaw.trim() : '';
  if (!id || !dedupe_key) {
    return null;
  }
  const details = isRecord(value.details)
    ? (value.details as DispatchOverrunWarningDetails)
    : null;
  return {
    id,
    flight_id: value.flight_id == null ? null : String(value.flight_id),
    alert_type: value.alert_type == null ? null : String(value.alert_type),
    severity: value.severity == null ? null : String(value.severity),
    message: value.message == null ? null : String(value.message),
    is_resolved: Boolean(value.is_resolved),
    dedupe_key,
    current_order_id: value.current_order_id == null ? null : String(value.current_order_id),
    next_order_id: value.next_order_id == null ? null : String(value.next_order_id),
    occurrence_count:
      typeof value.occurrence_count === 'number' && Number.isFinite(value.occurrence_count)
        ? value.occurrence_count
        : Number(value.occurrence_count) || 0,
    acknowledged_at: value.acknowledged_at == null ? null : String(value.acknowledged_at),
    acknowledged_by: value.acknowledged_by == null ? null : String(value.acknowledged_by),
    details,
  };
}

/** Format shared_personnel for display (string[] or objects with name/username). */
export function formatSharedPersonnel(raw: unknown): string {
  if (raw == null) {
    return '';
  }
  if (typeof raw === 'string') {
    return raw.trim();
  }
  if (!Array.isArray(raw)) {
    return '';
  }
  return raw
    .map((item) => {
      if (typeof item === 'string') {
        return item.trim();
      }
      if (isRecord(item)) {
        return String(
          item.name
            ?? item.username
            ?? item.user_display_name
            ?? item.user_id
            ?? item.id
            ?? '',
        ).trim();
      }
      return '';
    })
    .filter(Boolean)
    .join('、');
}

/**
 * Pure upsert by dedupe_key. Resolved alerts are removed from the list.
 * Exported for unit tests.
 */
export function upsertOverrunWarning(
  list: readonly DispatchOverrunWarning[],
  next: DispatchOverrunWarning,
): DispatchOverrunWarning[] {
  if (next.is_resolved) {
    return list.filter((item) => item.dedupe_key !== next.dedupe_key && item.id !== next.id);
  }
  const byDedupe = list.findIndex((item) => item.dedupe_key === next.dedupe_key);
  if (byDedupe >= 0) {
    const copy = list.slice();
    copy[byDedupe] = { ...copy[byDedupe], ...next };
    return copy;
  }
  const byId = list.findIndex((item) => item.id === next.id);
  if (byId >= 0) {
    const copy = list.slice();
    copy[byId] = { ...copy[byId], ...next };
    return copy;
  }
  return [next, ...list];
}

export function useDispatchOverrunWarnings() {
  const api = useApi();
  const toast = useToast();

  const warnings = ref<DispatchOverrunWarning[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const actionBusyIds = ref<Set<string>>(new Set());
  const started = ref(false);

  const { connect, disconnect, on } = useSSE({
    url: SSE_URL,
    autoConnect: false,
  });

  const unresolvedWarnings = computed(() =>
    warnings.value.filter((w) => !w.is_resolved),
  );

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

  function applyWarning(next: DispatchOverrunWarning): void {
    warnings.value = upsertOverrunWarning(warnings.value, next);
  }

  function handleSsePayload(payload: unknown): void {
    // Envelope may be the alert itself or {data: alert}
    let candidate = payload;
    if (isRecord(payload) && payload.data !== undefined && !payload.dedupe_key && !payload.id) {
      candidate = payload.data;
    }
    if (Array.isArray(candidate)) {
      for (const item of candidate) {
        const normalized = normalizeOverrunWarning(item);
        if (normalized) {
          applyWarning(normalized);
        }
      }
      return;
    }
    const normalized = normalizeOverrunWarning(candidate);
    if (normalized) {
      applyWarning(normalized);
    }
  }

  async function fetchUnresolved(flightId?: string | null): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      const params = new URLSearchParams();
      params.set('unresolved', 'true');
      if (flightId) {
        params.set('flight_id', flightId);
      }
      const res = await api.get(`${ALERTS_URL}?${params.toString()}`);
      if (!res.ok) {
        throw new Error(readErrorMessage(res.data, `预排冲突预警加载失败: HTTP ${res.status}`));
      }
      const list = unwrapAlertList(res.data)
        .map((item) => normalizeOverrunWarning(item))
        .filter((item): item is DispatchOverrunWarning => item != null)
        .filter((item) => !item.is_resolved);
      warnings.value = list;
    } catch (err) {
      console.error('Failed to fetch dispatch overrun warnings:', err);
      error.value = err instanceof Error ? err.message : '预排冲突预警加载失败';
      // Fail-closed: do not inject demo warnings.
    } finally {
      loading.value = false;
    }
  }

  /** Acknowledge = seen only; does not resolve the conflict. */
  async function acknowledge(id: string): Promise<void> {
    if (!id || isActionBusy(id)) {
      return;
    }
    setActionBusy(id, true);
    try {
      const res = await api.post(`${ALERTS_URL}/${encodeURIComponent(id)}/acknowledge`);
      if (!res.ok) {
        throw new Error(readErrorMessage(res.data, `确认预警失败: HTTP ${res.status}`));
      }
      const existing = warnings.value.find((w) => w.id === id);
      if (existing) {
        applyWarning({
          ...existing,
          acknowledged_at: new Date().toISOString(),
        });
      }
    } catch (err) {
      console.error('Failed to acknowledge overrun warning:', err);
      const message = err instanceof Error ? err.message : '确认预警失败';
      toast.showToast('error', message);
      throw err;
    } finally {
      setActionBusy(id, false);
    }
  }

  /** Resolve = close the warning (optional notes). */
  async function resolve(id: string, notes?: string): Promise<void> {
    if (!id || isActionBusy(id)) {
      return;
    }
    setActionBusy(id, true);
    try {
      const body = notes !== undefined && notes !== '' ? { notes } : {};
      const res = await api.post(`${ALERTS_URL}/${encodeURIComponent(id)}/resolve`, body);
      if (!res.ok) {
        throw new Error(readErrorMessage(res.data, `关闭预警失败: HTTP ${res.status}`));
      }
      const existing = warnings.value.find((w) => w.id === id);
      if (existing) {
        applyWarning({ ...existing, is_resolved: true });
      } else {
        warnings.value = warnings.value.filter((w) => w.id !== id);
      }
    } catch (err) {
      console.error('Failed to resolve overrun warning:', err);
      const message = err instanceof Error ? err.message : '关闭预警失败';
      toast.showToast('error', message);
      throw err;
    } finally {
      setActionBusy(id, false);
    }
  }

  let offSse: (() => void) | null = null;

  async function start(options?: { flightId?: string | null }): Promise<void> {
    if (started.value) {
      await fetchUnresolved(options?.flightId);
      return;
    }
    started.value = true;
    if (!offSse) {
      offSse = on(EVENT_NAME, (event) => {
        if (!(event instanceof MessageEvent)) {
          return;
        }
        try {
          const parsed = JSON.parse(event.data);
          handleSsePayload(parsed);
        } catch {
          // ignore non-JSON heartbeat frames
        }
      });
    }
    await fetchUnresolved(options?.flightId);
    await connect();
  }

  function stop(): void {
    if (offSse) {
      offSse();
      offSse = null;
    }
    disconnect();
    started.value = false;
  }

  if (getCurrentInstance()) {
    onBeforeUnmount(() => {
      stop();
    });
  }

  return {
    warnings: unresolvedWarnings,
    rawWarnings: warnings,
    loading,
    error,
    actionBusyIds,
    started,
    fetchUnresolved,
    acknowledge,
    resolve,
    isActionBusy,
    start,
    stop,
    /** Test/helper: apply a single SSE-style payload. */
    handleSsePayload,
    applyWarning,
  };
}
