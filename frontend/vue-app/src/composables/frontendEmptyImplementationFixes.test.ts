// @vitest-environment jsdom
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { defineComponent, nextTick } from 'vue';

type Recorded = { method: string; url: string; body?: unknown };
type MockResult = { ok: boolean; status: number; data: unknown; response: Response };
type Responder = (record: Recorded) => MockResult | Promise<MockResult> | null;

const recorded: Recorded[] = [];
const responders: Responder[] = [];
const toastCalls: Array<{ type: string; message: string }> = [];

function makeResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body ?? null), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function result(ok: boolean, status: number, data: unknown): MockResult {
  return { ok, status, data, response: makeResponse(status, data) };
}

async function dispatch(method: string, url: string, body?: unknown): Promise<MockResult> {
  const record: Recorded = { method, url, body };
  recorded.push(record);
  for (let i = responders.length - 1; i >= 0; i--) {
    const response = await responders[i](record);
    if (response) return response;
  }
  if (method === 'GET' && url.startsWith('/api/v2/kpi/baseline-compare')) {
    return result(true, 200, {
      success: true,
      data: {
        target_date: '2026-06-09',
        weather_category: 'normal',
        items: [
          { hour: '08:00', actual_volume: 12, baseline_volume: 10, actual_on_time_rate: 78, baseline_on_time_rate: 92, is_abnormal: true },
        ],
      },
    });
  }
  if (method === 'GET' && url.startsWith('/api/v2/kpi/trend-with-anomalies')) {
    return result(true, 200, {
      success: true,
      data: { metric: 'on_time_rate', days: 7, items: [{ date: '2026-06-09', value: 0.9, anomaly_count: 0 }] },
    });
  }
  if (method === 'GET' && url.startsWith('/api/v2/kpi/compare')) {
    return result(true, 200, {
      success: true,
      data: {
        metrics: {
          on_time_departure_rate: { base: 0.91, compare: 0.88, delta: -0.03, change_rate: -0.032 },
        },
      },
    });
  }
  if (method === 'GET') return result(true, 200, []);
  return result(true, 200, null);
}

vi.mock('@/composables/useApi', () => ({
  useApi: () => ({
    raw: vi.fn(),
    request: vi.fn(),
    get: (url: string) => dispatch('GET', url),
    post: (url: string, body?: unknown) => dispatch('POST', url, body),
    put: (url: string, body?: unknown) => dispatch('PUT', url, body),
    patch: (url: string, body?: unknown) => dispatch('PATCH', url, body),
    delete: (url: string) => dispatch('DELETE', url),
  }),
}));

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    showToast: (type: string, message: unknown) => {
      toastCalls.push({ type, message: String(message) });
    },
    show: (type: string, message: unknown) => {
      toastCalls.push({ type, message: String(message) });
    },
    toasts: { value: [] },
  }),
}));

import { useFlightImports } from '@/composables/useFlightImports';
import { useOpsReview } from '@/composables/useOpsReview';
import { usePendingActions } from '@/composables/usePendingActions';
import { useResourceUtilization } from '@/composables/useResourceUtilization';
import { useSystemFlags } from '@/composables/useSystemFlags';

async function flushAsync() {
  await Promise.resolve();
  await Promise.resolve();
  await nextTick();
}

async function mountComposable<T>(factory: () => T) {
  let api!: T;
  const Host = defineComponent({
    setup() {
      api = factory();
      return () => null;
    },
  });
  const { createApp } = await import('vue');
  const app = createApp(Host);
  const root = document.createElement('div');
  app.mount(root);
  await flushAsync();
  return { api, unmount: () => app.unmount() };
}

beforeEach(() => {
  recorded.length = 0;
  responders.length = 0;
  toastCalls.length = 0;
});

describe('frontend empty/fake implementation fixes', () => {
  it('loads system flags from the backend envelope shape', async () => {
    responders.push((record) => {
      if (record.method === 'GET' && record.url === '/api/v2/system/flags') {
        return result(true, 200, {
          success: true,
          data: {
            flags: [
              {
                path: 'dispatch.auto',
                label: 'Auto',
                value: true,
                type: 'boolean',
                category: 'dispatch',
                description: '自动派工',
                masked: false,
              },
            ],
          },
        });
      }
      return null;
    });

    const { api, unmount } = await mountComposable(useSystemFlags);

    expect(api.flags.value).toHaveLength(1);
    expect(api.flags.value[0]?.path).toBe('dispatch.auto');
    // Preferred taxonomy only (legacy sidebar). Unknown keys still filter under `all`.
    expect(api.categories.value).toEqual(['all']);
    expect(api.categoryCounts.value.all).toBe(1);
    expect(api.categoryCounts.value.dispatch).toBe(1);
    unmount();
  });

  it('does not show a fake success when a system flag update fails', async () => {
    const { api, unmount } = await mountComposable(useSystemFlags);
    recorded.length = 0;
    responders.push((record) => {
      if (record.method === 'PATCH') return result(false, 500, { message: '写入失败' });
      return null;
    });

    await api.updateFlag('dispatch.auto', true);

    expect(recorded).toEqual([
      { method: 'PATCH', url: '/api/v2/system/flags', body: { path: 'dispatch.auto', value: true } },
    ]);
    expect(toastCalls).toContainEqual({ type: 'error', message: '写入失败' });
    expect(toastCalls.some((call) => call.type === 'success')).toBe(false);
    unmount();
  });

  it('keeps flight import preview state intact when preview or commit fails', async () => {
    const api = useFlightImports();
    responders.push((record) => {
      if (record.url === '/api/v2/system/flight-imports/preview') {
        return result(false, 400, { message: '文件格式错误' });
      }
      return null;
    });

    await api.preview(new File(['bad'], 'bad.csv', { type: 'text/csv' }));

    expect(api.fileSelected.value).toBe(false);
    expect(api.previewData.value).toEqual([]);
    expect(toastCalls).toContainEqual({ type: 'error', message: '文件格式错误' });

    responders.length = 0;
    toastCalls.length = 0;
    api.previewId.value = 'preview_1';
    api.fileSelected.value = true;
    api.previewData.value = [{ flight_no: 'CA100' }];
    responders.push((record) => {
      if (record.url === '/api/v2/system/flight-imports/preview_1/commit') {
        return result(false, 500, { message: '导入事务失败' });
      }
      return null;
    });

    await api.commitImport();

    expect(api.importProgress.value).toBe(0);
    expect(api.fileSelected.value).toBe(true);
    expect(api.previewData.value).toEqual([{ flight_no: 'CA100' }]);
    expect(toastCalls).toContainEqual({ type: 'error', message: '导入事务失败' });
    expect(toastCalls.some((call) => call.type === 'success')).toBe(false);
  });

  it('shows errors and keeps pending actions when approve/reject fails', async () => {
    const api = usePendingActions();
    api.actions.value = [
      { actionId: 'action/1', toolName: 'dispatch', status: 'pending' },
    ];
    responders.push((record) => {
      if (record.method === 'POST') return result(false, 409, { message: '动作已过期' });
      return null;
    });

    await api.approve('action/1');

    expect(recorded[0].url).toBe('/api/v2/ai/pending-actions/action%2F1/approve');
    expect(api.actions.value).toHaveLength(1);
    expect(toastCalls).toContainEqual({ type: 'error', message: '动作已过期' });
  });

  it('surfaces resource utilization failures and derives action suggestions from bottlenecks', async () => {
    responders.push((record) => {
      if (record.url === '/api/v2/dispatch/analytics/resource-utilization/summary') {
        return result(false, 500, { message: '资源服务不可用' });
      }
      return null;
    });

    const { api, unmount } = await mountComposable(useResourceUtilization);

    expect(api.error.value).toBe('资源服务不可用');
    api.snapshot.value = [{ name: '机位 A12', utilization: 0.96 }];
    api.bottlenecks.value = [{ name: '机位 A12', utilization: 0.96 }];
    expect(api.actionSuggestions.value[0].title).toBe('立即分流 机位 A12');
    expect(api.reviewCadence.value[0].title).toBe('5 分钟复查');
    unmount();
  });

  it('uses real KPI endpoints for ops review report and replay flows', async () => {
    const { api, unmount } = await mountComposable(useOpsReview);
    recorded.length = 0;

    await api.fetchKpiCompare({
      baseStartDate: '2026-06-01',
      baseEndDate: '2026-06-03',
      compareStartDate: '2026-06-04',
      compareEndDate: '2026-06-06',
    });
    await api.runReplay('2026-06-09', 'rain');
    await api.generateReport();

    const urls = recorded.map((record) => record.url);
    expect(urls.some((url) => url.startsWith('/api/v2/kpi/compare?'))).toBe(true);
    expect(urls.some((url) => url.includes('base_start_date=2026-06-01'))).toBe(true);
    expect(urls.some((url) => url.startsWith('/api/v2/kpi/baseline-compare?'))).toBe(true);
    expect(urls.some((url) => url.includes('/api/v2/kpi/generate-report'))).toBe(false);
    expect(urls.some((url) => url.includes('/api/v2/kpi/replay'))).toBe(false);
    expect(api.kpiComparison.value).toHaveLength(1);
    expect(api.replayEvents.value[0].title).toContain('基线偏离');
    expect(api.aiReport.value).toContain('# 运行复盘报告');
    unmount();
  });
});
