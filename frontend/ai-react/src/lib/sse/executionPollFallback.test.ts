import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ExecutionPollFallback } from '@/lib/sse/executionPollFallback';
import { getExecutionDetail } from '@/lib/api/aiApi';

vi.mock('@/lib/api/aiApi', () => ({
  getExecutionDetail: vi.fn(),
}));

const mockedGetExecutionDetail = vi.mocked(getExecutionDetail);

describe('ExecutionPollFallback', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mockedGetExecutionDetail.mockReset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('emits only when execution detail materially changes', async () => {
    const seen: Array<Record<string, unknown>> = [];
    mockedGetExecutionDetail
      .mockResolvedValueOnce({ status: 'in_progress', tool_name: 'planner', message: 'running' })
      .mockResolvedValueOnce({ status: 'in_progress', tool_name: 'planner', message: 'running' })
      .mockResolvedValueOnce({ status: 'success', tool_name: 'planner', message: 'done' });

    const fallback = new ExecutionPollFallback('exec-1', (payload) => {
      seen.push(payload);
    }, 10);

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
    mockedGetExecutionDetail
      .mockResolvedValueOnce({ status: 'in_progress', tool_name: 'planner', message: 'running' })
      .mockResolvedValueOnce({ status: 'in_progress', tool_name: 'planner', message: 'running' });

    const fallback = new ExecutionPollFallback('exec-1', () => undefined, 10);

    fallback.start();

    await vi.advanceTimersByTimeAsync(10);
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(10);
    await Promise.resolve();

    expect(mockedGetExecutionDetail).toHaveBeenCalledTimes(2);

    await vi.advanceTimersByTimeAsync(19);
    await Promise.resolve();

    expect(mockedGetExecutionDetail).toHaveBeenCalledTimes(2);
  });
});
