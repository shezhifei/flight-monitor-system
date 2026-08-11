import { describe, it, expect, vi } from 'vitest';
import { describeApiError, retryOn5xx } from './useApiError';
import type { ApiResult } from './useApi';

function result<T>(status: number, data: T | null): ApiResult<T> {
  return { ok: status >= 200 && status < 300, status, data, response: new Response() };
}

describe('describeApiError', () => {
  it('extracts detail/message/error from a flat error body', () => {
    expect(describeApiError({ detail: ' boom ' }, 'fallback')).toBe('boom');
    expect(describeApiError({ message: 'nope' }, 'fallback')).toBe('nope');
    expect(describeApiError({ error: 'bad' }, 'fallback')).toBe('bad');
  });

  it('reads the nested unified error envelope (message + kind)', () => {
    expect(describeApiError({ success: false, error: { message: '数据存储错误', kind: 'database' } }, 'fb')).toBe(
      '数据存储错误',
    );
  });

  it('falls back when the payload has no usable text', () => {
    expect(describeApiError(null, 'fb')).toBe('fb');
    expect(describeApiError('not-an-object', 'fb')).toBe('fb');
    expect(describeApiError({}, 'fb')).toBe('fb');
    expect(describeApiError({ detail: '   ' }, 'fb')).toBe('fb');
  });
});

describe('retryOn5xx', () => {
  it('retries only on 5xx and eventually returns the success result', async () => {
    const calls: number[] = [];
    let n = 0;
    const fn = vi.fn(async () => {
      n += 1;
      calls.push(n);
      return n < 3 ? result(503, null) : result(200, { ok: true });
    });
    const res = await retryOn5xx(fn, { retries: 3, baseDelayMs: 0 });
    expect(res.status).toBe(200);
    expect(fn).toHaveBeenCalledTimes(3);
  });

  it('does NOT retry on 4xx (e.g. 401 owned by useAuth)', async () => {
    const fn = vi.fn(async () => result(401, null));
    const res = await retryOn5xx(fn, { retries: 3, baseDelayMs: 0 });
    expect(res.status).toBe(401);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('stops after exhausting retries and returns the last 5xx result', async () => {
    const fn = vi.fn(async () => result(500, null));
    const res = await retryOn5xx(fn, { retries: 2, baseDelayMs: 0 });
    expect(res.status).toBe(500);
    expect(fn).toHaveBeenCalledTimes(3); // initial + 2 retries
  });
});
