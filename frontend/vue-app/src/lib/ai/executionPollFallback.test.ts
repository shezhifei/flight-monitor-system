// 搬运自 frontend/ai-react/src/lib/sse/executionPollFallback.test.ts。
// 适配点：源测试用 vi.mock('@/lib/api/aiApi') mock getExecutionDetail；
// 这里改为直接 mock 注入构造函数的 fetchDetail。断言与计时器推进不变。
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ExecutionPollFallback, type ExecutionDetailFetcher } from './executionPollFallback';

describe('ExecutionPollFallback', () => {
  let fetchDetail: ReturnType<typeof vi.fn<ExecutionDetailFetcher>>;

  beforeEach(() => {
    vi.useFakeTimers();
    fetchDetail = vi.fn<ExecutionDetailFetcher>();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('emits only when execution detail materially changes', async () => {
    const seen: Array<Record<string, unknown>> = [];
    fetchDetail
      .mockResolvedValueOnce({ status: 'in_progress', tool_name: 'planner', message: 'running' })
      .mockResolvedValueOnce({ status: 'in_progress', tool_name: 'planner', message: 'running' })
      .mockResolvedValueOnce({ status: 'success', tool_name: 'planner', message: 'done' });

    const fallback = new ExecutionPollFallback('exec-1', (payload) => {
      seen.push(payload);
    }, fetchDetail, 10);

    fallback.start();

    await vi.advanceTimersByTimeAsync(10);
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(10);
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(20);
    await Promise.resolve();

    expect(seen).toEqual([
      { status: 'in_progress', tool_name: 'planner', message: 'running' },
      { status: 'success', tool_name: 'planner', message: 'done' },
    ]);
  });

  it('backs off after unchanged responses', async () => {
    fetchDetail
      .mockResolvedValueOnce({ status: 'in_progress', tool_name: 'planner', message: 'running' })
      .mockResolvedValueOnce({ status: 'in_progress', tool_name: 'planner', message: 'running' });

    const fallback = new ExecutionPollFallback('exec-1', () => undefined, fetchDetail, 10);

    fallback.start();

    await vi.advanceTimersByTimeAsync(10);
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(10);
    await Promise.resolve();

    expect(fetchDetail).toHaveBeenCalledTimes(2);

    await vi.advanceTimersByTimeAsync(19);
    await Promise.resolve();

    expect(fetchDetail).toHaveBeenCalledTimes(2);
  });
});
