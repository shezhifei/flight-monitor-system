// 搬运自 frontend/ai-react/src/lib/api/dispatchApi.test.ts。
// 适配点：源测试用 vi.mock('@/lib/http/apiClient') mock requestEnvelope 并断言
// (url, init) 调用形态；这里改为 mock 注入的 ApiLike，断言 api.get(url) /
// api.post(url, body)。URL 与请求体内容与源断言逐一对应（body 由
// requestEnvelope/useApi.post 负责 JSON 序列化）。
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createDispatchApi } from './api';
import type { ApiLike } from './envelope';

function createMockApi() {
  return {
    get: vi.fn(),
    post: vi.fn(),
    put: vi.fn(),
    delete: vi.fn(),
    raw: vi.fn(),
  };
}

let api: ReturnType<typeof createMockApi>;
let dispatchApi: ReturnType<typeof createDispatchApi>;

beforeEach(() => {
  api = createMockApi();
  dispatchApi = createDispatchApi(api as unknown as ApiLike);
  vi.useRealTimers();
});

describe('dispatchApi', () => {
  it('loads conflicts from the v2 dispatch conflicts endpoint', async () => {
    api.get.mockResolvedValueOnce({
      ok: true,
      status: 200,
      data: { conflicts: [{ conflict_id: 'c1' }] },
      response: new Response(),
    });

    const rows = await dispatchApi.loadDispatchConflicts({
      start_time: '2026-06-08T00:00:00.000Z',
      end_time: '2026-06-08T06:00:00.000Z',
      severity: 'high',
    });

    expect(rows).toEqual([{ conflict_id: 'c1' }]);
    expect(api.get).toHaveBeenCalledWith(
      '/api/v2/dispatch-orders/conflicts?window_start=2026-06-08T00%3A00%3A00.000Z&window_end=2026-06-08T06%3A00%3A00.000Z&severity=high',
    );
  });

  it('previews replan through v2 without applying changes', async () => {
    api.post.mockResolvedValueOnce({
      ok: true,
      status: 200,
      data: { suggestions: [{ id: 's1' }] },
      response: new Response(),
    });

    const response = await dispatchApi.previewReplan({
      strategy: 'balanced',
      max_suggestions: 20,
      scope: {
        window_start: '2026-06-08T01:00:00.000Z',
        window_end: '2026-06-08T05:00:00.000Z',
      },
    });

    expect(response).toEqual({ suggestions: [{ id: 's1' }] });
    expect(api.post).toHaveBeenCalledWith('/api/v2/dispatch-orders/replan', {
      window_start: '2026-06-08T01:00:00.000Z',
      window_end: '2026-06-08T05:00:00.000Z',
      strategy: 'balanced',
      max_suggestions: 20,
      apply_changes: false,
    });
  });

  it('applies replan through v2 using the same request shape with apply_changes', async () => {
    api.post.mockResolvedValueOnce({
      ok: true,
      status: 200,
      data: { applied: true },
      response: new Response(),
    });

    await dispatchApi.applyReplan({
      strategy: 'efficiency',
      max_suggestions: 3.8,
      window_start: '2026-06-08T02:00:00.000Z',
      window_end: '2026-06-08T04:00:00.000Z',
    });

    expect(api.post).toHaveBeenCalledWith('/api/v2/dispatch-orders/replan', {
      window_start: '2026-06-08T02:00:00.000Z',
      window_end: '2026-06-08T04:00:00.000Z',
      strategy: 'efficiency',
      max_suggestions: 3,
      apply_changes: true,
    });
  });

  it('uses the operational conflict window when no explicit replan window is provided', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-06-08T08:00:00.000Z'));
    api.post.mockResolvedValueOnce({
      ok: true,
      status: 200,
      data: { suggestions: [] },
      response: new Response(),
    });

    await dispatchApi.previewReplan({
      strategy: 'balanced',
      max_suggestions: 20,
    });

    expect(api.post).toHaveBeenCalledWith('/api/v2/dispatch-orders/replan', {
      window_start: '2026-06-08T06:00:00.000Z',
      window_end: '2026-06-08T12:00:00.000Z',
      strategy: 'balanced',
      max_suggestions: 20,
      apply_changes: false,
    });
  });
});
