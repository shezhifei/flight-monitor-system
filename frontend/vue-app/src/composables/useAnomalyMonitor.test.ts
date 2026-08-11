// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { defineComponent, nextTick, ref } from 'vue';

type Recorded = { method: string; url: string; body?: unknown };

const recorded: Recorded[] = [];
const responders: Array<(r: Recorded) => Promise<ApiResult> | ApiResult | null> = [];
const connectMock = vi.fn();
const disconnectMock = vi.fn();
const sseHandlers: Record<string, Array<(event: Event) => void>> = {};
const useSSEOptions: Array<{ url?: string }> = [];

interface ApiResult {
  ok: boolean;
  status: number;
  data: unknown;
  response: Response;
}

function makeResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body ?? null), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function result(ok: boolean, status: number, data: unknown): ApiResult {
  return { ok, status, data, response: makeResponse(status, data) };
}

async function dispatch(method: string, url: string, body?: unknown): Promise<ApiResult> {
  const rec = { method, url, body };
  recorded.push(rec);
  for (let i = responders.length - 1; i >= 0; i -= 1) {
    const out = await responders[i](rec);
    if (out) return out;
  }
  if (method === 'GET' && url.startsWith('/api/v2/anomalies?')) {
    return result(true, 200, { success: true, data: { items: [], total: 0 } });
  }
  if (method === 'GET' && url === '/api/v2/anomalies/stats') {
    return result(true, 200, { success: true, data: {} });
  }
  return result(true, 200, {});
}

vi.mock('./useApi', () => ({
  useApi: () => ({
    get: (url: string) => dispatch('GET', url),
    post: (url: string, body?: unknown) => dispatch('POST', url, body),
  }),
}));

vi.mock('./useToast', () => ({
  useToast: () => ({
    showToast: vi.fn(),
    show: vi.fn(),
  }),
}));

vi.mock('./useSSE', () => ({
  useSSE: (options: { url?: string } = {}) => {
    useSSEOptions.push(options);
    return {
      connect: connectMock,
      disconnect: disconnectMock,
      status: ref('online'),
      on: (eventName: string, handler: (event: Event) => void) => {
        sseHandlers[eventName] = [...(sseHandlers[eventName] || []), handler];
        return () => {};
      },
    };
  },
}));

import { useAnomalyMonitor } from './useAnomalyMonitor';

const openAnomaly = {
  anomaly_id: 'a1',
  detected_at: '2026-06-08T01:00:00Z',
  flight_id: 'CA1501',
  anomaly_type: 'gate_stand_conflict',
  severity: 'critical',
  status: 'open',
  title: '机位冲突',
  description: '冲突描述',
  escalation_level: 1,
  resolved_at: null,
  last_escalated_at: null,
  linked_todo_id: null,
  rule_id: null,
  context_data: {},
  created_at: '2026-06-08T01:00:00Z',
  updated_at: '2026-06-08T01:00:00Z',
};

function envelopeItems(items: unknown[], total = items.length) {
  return { success: true, data: { items, total } };
}

function mountComposable() {
  let api!: ReturnType<typeof useAnomalyMonitor>;
  const Host = defineComponent({
    setup() {
      api = useAnomalyMonitor();
      return () => null;
    },
  });
  return import('vue').then(({ createApp }) => {
    const app = createApp(Host);
    const root = document.createElement('div');
    app.mount(root);
    return { api, unmount: () => app.unmount() };
  });
}

async function flushMountedWork(): Promise<void> {
  await nextTick();
  await Promise.resolve();
  await Promise.resolve();
}

beforeEach(() => {
  recorded.length = 0;
  responders.length = 0;
  connectMock.mockClear();
  disconnectMock.mockClear();
  useSSEOptions.length = 0;
  Object.keys(sseHandlers).forEach((key) => delete sseHandlers[key]);
});

describe('useAnomalyMonitor', () => {
  it('does not inject demo anomalies when list loading fails', async () => {
    responders.push((rec) => {
      if (rec.method === 'GET' && rec.url.startsWith('/api/v2/anomalies?')) {
        return result(false, 503, { message: 'anomaly service unavailable' });
      }
      return null;
    });

    const { api, unmount } = await mountComposable();
    await flushMountedWork();

    expect(api.records.value).toEqual([]);
    expect(api.stats.value.total).toBe(0);
    expect(api.error.value).toBe('anomaly service unavailable');
    expect(connectMock).toHaveBeenCalledOnce();
    expect(useSSEOptions[0]?.url).toBe('/api/v2/anomalies/stream');
    unmount();
  });

  it('unwraps {success,data:{items,total}} using anomaly_id/detected_at/anomaly_type', async () => {
    responders.push((rec) => {
      if (rec.method === 'GET' && rec.url.startsWith('/api/v2/anomalies?')) {
        return result(true, 200, envelopeItems([openAnomaly]));
      }
      if (rec.method === 'GET' && rec.url === '/api/v2/anomalies/stats') {
        return result(true, 200, {
          success: true,
          data: {
            total: 1,
            open: 1,
            acknowledged: 0,
            resolved: 0,
            critical: 1,
            escalated: 1,
          },
        });
      }
      return null;
    });

    const { api, unmount } = await mountComposable();
    await flushMountedWork();

    expect(api.records.value).toHaveLength(1);
    expect(api.records.value[0]?.anomaly_id).toBe('a1');
    expect(api.records.value[0]?.detected_at).toBe('2026-06-08T01:00:00Z');
    expect(api.records.value[0]?.anomaly_type).toBe('gate_stand_conflict');
    expect(api.stats.value.open).toBe(1);
    expect(api.stats.value.critical).toBe(1);
    unmount();
  });

  it('keeps status unchanged when acknowledge fails', async () => {
    responders.push((rec) => {
      if (rec.method === 'GET' && rec.url.startsWith('/api/v2/anomalies?')) {
        return result(true, 200, envelopeItems([openAnomaly]));
      }
      if (rec.method === 'POST' && rec.url === '/api/v2/anomalies/a1/acknowledge') {
        return result(false, 500, { message: 'ack failed' });
      }
      return null;
    });

    const { api, unmount } = await mountComposable();
    await flushMountedWork();

    await expect(api.acknowledge('a1')).rejects.toThrow('ack failed');
    expect(api.stats.value.open).toBe(1);
    expect(api.stats.value.acknowledged).toBe(0);
    expect(api.actionError.value).toBe('ack failed');
    expect(api.isActionBusy('a1')).toBe(false);
    unmount();
  });

  it('updates local status only after successful resolve and posts AnomalyResolveRequest body', async () => {
    responders.push((rec) => {
      if (rec.method === 'GET' && rec.url.startsWith('/api/v2/anomalies?')) {
        return result(true, 200, envelopeItems([openAnomaly]));
      }
      if (rec.method === 'POST' && rec.url === '/api/v2/anomalies/a1/resolve') {
        return result(true, 200, { success: true, data: { anomaly_id: 'a1' } });
      }
      return null;
    });

    const { api, unmount } = await mountComposable();
    await flushMountedWork();

    await api.resolve('a1');

    const resolveCall = recorded.find(
      (r) => r.method === 'POST' && r.url === '/api/v2/anomalies/a1/resolve',
    );
    expect(resolveCall?.body).toEqual({ resolve_todo: true });
    expect(api.stats.value.open).toBe(0);
    expect(api.stats.value.resolved).toBe(1);
    // Default filter status=open hides the resolved row.
    expect(api.records.value).toEqual([]);
    unmount();
  });

  it('subscribes to named anomaly SSE events and patches records', async () => {
    responders.push((rec) => {
      if (rec.method === 'GET' && rec.url.startsWith('/api/v2/anomalies?')) {
        return result(true, 200, envelopeItems([openAnomaly]));
      }
      return null;
    });

    const { api, unmount } = await mountComposable();
    await flushMountedWork();

    expect(Object.keys(sseHandlers).sort()).toEqual([
      'anomaly_acknowledged',
      'anomaly_created',
      'anomaly_resolved',
      'anomaly_updated',
      'initial',
    ].sort());

    const updatedHandlers = sseHandlers.anomaly_updated || [];
    expect(updatedHandlers.length).toBeGreaterThan(0);
    updatedHandlers[0](
      new MessageEvent('anomaly_updated', {
        data: JSON.stringify({
          anomaly_id: 'a1',
          status: 'acknowledged',
          updated_at: '2026-06-08T02:00:00Z',
        }),
      }),
    );
    await nextTick();

    // Filter is status=open, so acknowledged is filtered out of the exposed list.
    expect(api.records.value).toEqual([]);
    unmount();
  });

  it('sends anomaly_type query param when type filter changes', async () => {
    const { api, unmount } = await mountComposable();
    await flushMountedWork();
    recorded.length = 0;

    api.filters.value.type = 'kpi_degradation';
    await flushMountedWork();

    const listCall = recorded.find(
      (r) => r.method === 'GET' && r.url.startsWith('/api/v2/anomalies?'),
    );
    expect(listCall?.url).toContain('anomaly_type=kpi_degradation');
    unmount();
  });
});
