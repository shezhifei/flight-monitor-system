import { describe, it, expect } from 'vitest';
import { describeApiError } from './useApiError';

describe('describeApiError', () => {
  it('extracts detail/message/error from a flat error body', () => {
    expect(describeApiError({ detail: ' boom ' }, 'fallback')).toBe('boom');
    expect(describeApiError({ message: 'nope' }, 'fallback')).toBe('nope');
    expect(describeApiError({ error: 'bad' }, 'fallback')).toBe('bad');
  });

  it('reads the nested error envelope', () => {
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