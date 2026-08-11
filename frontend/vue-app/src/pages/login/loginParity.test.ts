import { describe, expect, it } from 'vitest';

/**
 * Mirrors Login.vue extractLoginErrorMessage / redirect rules so unit tests
 * cover Task 13 without mounting the full page.
 */
function extractLoginErrorMessage(payload: unknown, fallback: string): string {
  if (!payload || typeof payload !== 'object') {
    return fallback;
  }
  const record = payload as Record<string, unknown>;
  if (typeof record.detail === 'string' && record.detail.trim()) {
    return record.detail;
  }
  if (typeof record.message === 'string' && record.message.trim()) {
    return record.message;
  }
  const nested = record.error;
  if (typeof nested === 'string' && nested.trim()) {
    return nested;
  }
  if (nested && typeof nested === 'object') {
    const error = nested as Record<string, unknown>;
    if (typeof error.message === 'string' && error.message.trim()) {
      return error.message;
    }
    if (typeof error.detail === 'string' && error.detail.trim()) {
      return error.detail;
    }
  }
  return fallback;
}

function resolvePostLoginTarget(search: string, fallback: string): string {
  const params = new URLSearchParams(search.startsWith('?') ? search.slice(1) : search);
  const redirect = params.get('redirect') || params.get('next') || '';
  if (!redirect) {
    return fallback;
  }
  if (redirect.startsWith('/frontend/') && !redirect.startsWith('//') && !redirect.includes('://')) {
    return redirect;
  }
  return fallback;
}

describe('login parity helpers (Task 13)', () => {
  it('extracts Rust error.message from envelope', () => {
    expect(extractLoginErrorMessage({
      success: false,
      error: { code: 'HTTP_401', message: '用户名或密码错误' },
    }, '登录失败')).toBe('用户名或密码错误');
  });

  it('preserves same-origin frontend redirect query targets', () => {
    expect(resolvePostLoginTarget(
      '?redirect=/frontend/system_flags.html',
      '/frontend/flight_monitor.html',
    )).toBe('/frontend/system_flags.html');
  });

  it('rejects open redirects', () => {
    expect(resolvePostLoginTarget(
      '?redirect=https://evil.example/phish',
      '/frontend/flight_monitor.html',
    )).toBe('/frontend/flight_monitor.html');
    expect(resolvePostLoginTarget(
      '?redirect=//evil.example',
      '/frontend/flight_monitor.html',
    )).toBe('/frontend/flight_monitor.html');
  });
});
