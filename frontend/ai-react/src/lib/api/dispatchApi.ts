import { requestEnvelope } from '@/lib/http/apiClient';

export interface DispatchReplanRequest {
  strategy: string;
  max_suggestions: number;
  window_start?: string;
  window_end?: string;
  apply_changes?: boolean;
  scope?: Record<string, unknown>;
}

function readScopeString(scope: Record<string, unknown> | undefined, keys: string[]): string | undefined {
  if (!scope) {
    return undefined;
  }
  for (const key of keys) {
    const value = scope[key];
    if (typeof value === 'string' && value.trim()) {
      return value.trim();
    }
  }
  return undefined;
}

function defaultWindow(): { window_start: string; window_end: string } {
  const now = Date.now();
  return {
    window_start: new Date(now - 2 * 60 * 60 * 1000).toISOString(),
    window_end: new Date(now + 4 * 60 * 60 * 1000).toISOString(),
  };
}

function buildReplanPayload(input: DispatchReplanRequest, applyChanges: boolean): Record<string, unknown> {
  const fallback = defaultWindow();
  const windowStart =
    input.window_start ||
    readScopeString(input.scope, ['window_start', 'windowStart', 'windowStartIso']) ||
    fallback.window_start;
  const windowEnd =
    input.window_end ||
    readScopeString(input.scope, ['window_end', 'windowEnd', 'windowEndIso']) ||
    fallback.window_end;

  return {
    window_start: windowStart,
    window_end: windowEnd,
    strategy: input.strategy || 'balanced',
    max_suggestions: Math.max(1, Math.trunc(input.max_suggestions || 20)),
    apply_changes: applyChanges,
  };
}

export async function loadDispatchConflicts(params: {
  start_time?: string;
  end_time?: string;
  severity?: string;
  type?: string;
  query?: string;
} = {}): Promise<Array<Record<string, unknown>>> {
  const search = new URLSearchParams();
  Object.entries(params).forEach(([key, value]) => {
    if (value) {
      const normalizedKey = key === 'start_time' ? 'window_start' : key === 'end_time' ? 'window_end' : key;
      search.set(normalizedKey, value);
    }
  });
  const payload = await requestEnvelope<{ conflicts: Array<Record<string, unknown>> }>(
    `/api/v2/dispatch-orders/conflicts?${search.toString()}`,
  );
  return payload.conflicts || [];
}

export async function previewReplan(payload: DispatchReplanRequest): Promise<Record<string, unknown>> {
  return requestEnvelope<Record<string, unknown>>('/api/v2/dispatch-orders/replan', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(buildReplanPayload(payload, false)),
  });
}

export async function applyReplan(payload: DispatchReplanRequest): Promise<Record<string, unknown>> {
  return requestEnvelope<Record<string, unknown>>('/api/v2/dispatch-orders/replan', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(buildReplanPayload(payload, true)),
  });
}
