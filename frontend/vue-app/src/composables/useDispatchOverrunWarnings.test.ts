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
  if (method === 'GET' && url.startsWith('/api/v2/dispatch/alerts')) {
    return result(true, 200, { success: true, data: { items: [] } });
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
        return () => {
          sseHandlers[eventName] = (sseHandlers[eventName] || []).filter((h) => h !== handler);
        };
      },
    };
  },
}));

import {
  formatSharedPersonnel,
  normalizeOverrunWarning,
  upsertOverrunWarning,
  useDispatchOverrunWarnings,
  unwrapAlertList,
  type DispatchOverrunWarning,
} from './useDispatchOverrunWarnings';

const sampleWarning: DispatchOverrunWarning = {
  id: 'alert-1',
  flight_id: 'CA1501',
  alert_type: 'dispatch_schedule_overrun',
  severity: 'warning',
  message: '共享人员可能延误下一单',
  is_resolved: false,
  dedupe_key: 'dispatch_schedule_overrun:ord-cur:ord-next',
  current_order_id: 'ord-cur',
  next_order_id: 'ord-next',
  occurrence_count: 1,
  acknowledged_at: null,
  acknowledged_by: null,
  details: {
    shared_personnel: ['张三', '李四'],
    countdown_minutes: 8,
    lead_minutes: 5,
    lead_source: 'system_default',
    eta_missing: false,
    predicted_conflict_minutes: 12,
  },
};

function envelopeItems(items: unknown[]) {
  return { success: true, data: { items } };
}

function mountComposable() {
  let api!: ReturnType<typeof useDispatchOverrunWarnings>;
  const Host = defineComponent({
    setup() {
      api = useDispatchOverrunWarnings();
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

async function flush(): Promise<void> {
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

describe('unwrapAlertList / normalizeOverrunWarning', () => {
  it('unwraps nested data.items envelopes', () => {
    expect(unwrapAlertList(envelopeItems([sampleWarning]))).toHaveLength(1);
    expect(unwrapAlertList({ data: [sampleWarning] })).toHaveLength(1);
    expect(unwrapAlertList([sampleWarning])).toHaveLength(1);
    expect(unwrapAlertList(null)).toEqual([]);
  });

  it('requires id and dedupe_key', () => {
    expect(normalizeOverrunWarning({ id: 'x' })).toBeNull();
    expect(normalizeOverrunWarning({ dedupe_key: 'k' })).toBeNull();
    expect(normalizeOverrunWarning(sampleWarning)?.dedupe_key).toBe(sampleWarning.dedupe_key);
  });
});

describe('upsertOverrunWarning', () => {
  it('inserts new warnings and updates by dedupe_key', () => {
    let list: DispatchOverrunWarning[] = [];
    list = upsertOverrunWarning(list, sampleWarning);
    expect(list).toHaveLength(1);

    list = upsertOverrunWarning(list, {
      ...sampleWarning,
      message: 'updated',
      occurrence_count: 2,
      details: { ...sampleWarning.details, countdown_minutes: 3 },
    });
    expect(list).toHaveLength(1);
    expect(list[0]?.message).toBe('updated');
    expect(list[0]?.occurrence_count).toBe(2);
    expect(list[0]?.details?.countdown_minutes).toBe(3);
  });

  it('removes warning when is_resolved is true', () => {
    const list = upsertOverrunWarning([sampleWarning], {
      ...sampleWarning,
      is_resolved: true,
    });
    expect(list).toEqual([]);
  });

  it('does not create duplicates for the same dedupe_key', () => {
    const other: DispatchOverrunWarning = {
      ...sampleWarning,
      id: 'alert-2',
      dedupe_key: 'dispatch_schedule_overrun:other:next',
    };
    let list = upsertOverrunWarning([], sampleWarning);
    list = upsertOverrunWarning(list, other);
    list = upsertOverrunWarning(list, { ...sampleWarning, message: 'again' });
    expect(list).toHaveLength(2);
    expect(list.find((w) => w.dedupe_key === sampleWarning.dedupe_key)?.message).toBe('again');
  });
});

describe('formatSharedPersonnel', () => {
  it('joins string arrays and object names', () => {
    expect(formatSharedPersonnel(['甲', '乙'])).toBe('甲、乙');
    expect(formatSharedPersonnel([{ username: 'u1' }, { name: '王五' }])).toBe('u1、王五');
    expect(formatSharedPersonnel(null)).toBe('');
  });
});

describe('useDispatchOverrunWarnings', () => {
  it('fetches unresolved alerts on start and connects SSE with dispatch_alerts topic', async () => {
    responders.push((rec) => {
      if (rec.method === 'GET' && rec.url.startsWith('/api/v2/dispatch/alerts')) {
        return result(true, 200, envelopeItems([sampleWarning]));
      }
      return null;
    });

    const { api, unmount } = await mountComposable();
    await api.start();
    await flush();

    expect(api.warnings.value).toHaveLength(1);
    expect(api.warnings.value[0]?.id).toBe('alert-1');
    expect(connectMock).toHaveBeenCalledOnce();
    expect(useSSEOptions[0]?.url).toContain('topics=dispatch_alerts');
    expect(Object.keys(sseHandlers)).toContain('dispatch_overrun_warning');

    const listCall = recorded.find(
      (r) => r.method === 'GET' && r.url.startsWith('/api/v2/dispatch/alerts'),
    );
    expect(listCall?.url).toContain('unresolved=true');
    unmount();
  });

  it('upserts SSE events by dedupe_key and removes resolved', async () => {
    responders.push((rec) => {
      if (rec.method === 'GET' && rec.url.startsWith('/api/v2/dispatch/alerts')) {
        return result(true, 200, envelopeItems([sampleWarning]));
      }
      return null;
    });

    const { api, unmount } = await mountComposable();
    await api.start();
    await flush();

    const handlers = sseHandlers.dispatch_overrun_warning || [];
    expect(handlers.length).toBeGreaterThan(0);

    handlers[0](
      new MessageEvent('dispatch_overrun_warning', {
        data: JSON.stringify({
          ...sampleWarning,
          message: 'SSE updated',
          details: { ...sampleWarning.details, countdown_minutes: 1 },
        }),
      }),
    );
    await nextTick();
    expect(api.warnings.value).toHaveLength(1);
    expect(api.warnings.value[0]?.message).toBe('SSE updated');

    handlers[0](
      new MessageEvent('dispatch_overrun_warning', {
        data: JSON.stringify({ ...sampleWarning, is_resolved: true }),
      }),
    );
    await nextTick();
    expect(api.warnings.value).toEqual([]);
    unmount();
  });

  it('acknowledge posts and marks local acknowledged_at without removing', async () => {
    responders.push((rec) => {
      if (rec.method === 'GET' && rec.url.startsWith('/api/v2/dispatch/alerts')) {
        return result(true, 200, envelopeItems([sampleWarning]));
      }
      if (rec.method === 'POST' && rec.url === '/api/v2/dispatch/alerts/alert-1/acknowledge') {
        return result(true, 200, { success: true });
      }
      return null;
    });

    const { api, unmount } = await mountComposable();
    await api.start();
    await flush();

    await api.acknowledge('alert-1');
    expect(api.warnings.value).toHaveLength(1);
    expect(api.warnings.value[0]?.acknowledged_at).toBeTruthy();
    expect(api.isActionBusy('alert-1')).toBe(false);
    unmount();
  });

  it('resolve posts body and removes warning from list', async () => {
    responders.push((rec) => {
      if (rec.method === 'GET' && rec.url.startsWith('/api/v2/dispatch/alerts')) {
        return result(true, 200, envelopeItems([sampleWarning]));
      }
      if (rec.method === 'POST' && rec.url === '/api/v2/dispatch/alerts/alert-1/resolve') {
        return result(true, 200, { success: true });
      }
      return null;
    });

    const { api, unmount } = await mountComposable();
    await api.start();
    await flush();

    await api.resolve('alert-1', 'manual close');
    const resolveCall = recorded.find(
      (r) => r.method === 'POST' && r.url === '/api/v2/dispatch/alerts/alert-1/resolve',
    );
    expect(resolveCall?.body).toEqual({ notes: 'manual close' });
    expect(api.warnings.value).toEqual([]);
    unmount();
  });

  it('keeps list unchanged when acknowledge fails', async () => {
    responders.push((rec) => {
      if (rec.method === 'GET' && rec.url.startsWith('/api/v2/dispatch/alerts')) {
        return result(true, 200, envelopeItems([sampleWarning]));
      }
      if (rec.method === 'POST' && rec.url === '/api/v2/dispatch/alerts/alert-1/acknowledge') {
        return result(false, 500, { message: 'ack failed' });
      }
      return null;
    });

    const { api, unmount } = await mountComposable();
    await api.start();
    await flush();

    await expect(api.acknowledge('alert-1')).rejects.toThrow('ack failed');
    expect(api.warnings.value).toHaveLength(1);
    expect(api.warnings.value[0]?.acknowledged_at).toBeNull();
    unmount();
  });

  it('stop disconnects SSE', async () => {
    const { api, unmount } = await mountComposable();
    await api.start();
    await flush();
    api.stop();
    expect(disconnectMock).toHaveBeenCalled();
    unmount();
  });
});
