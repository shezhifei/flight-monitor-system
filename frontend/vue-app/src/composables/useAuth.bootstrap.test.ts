// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useAuth } from './useAuth';

function createMemoryStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() { return values.size; },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => Array.from(values.keys())[index] ?? null,
    removeItem: (key) => { values.delete(key); },
    setItem: (key, value) => { values.set(key, String(value)); },
  };
}

function requestUrl(input: RequestInfo | URL): URL {
  if (input instanceof Request) {
    return new URL(input.url);
  }
  return new URL(String(input), window.location.origin);
}

describe('useAuth cookie-backed session restore', () => {
  const auth = useAuth();

  beforeEach(() => {
    const localStorage = createMemoryStorage();
    const sessionStorage = createMemoryStorage();
    vi.stubGlobal('localStorage', localStorage);
    vi.stubGlobal('sessionStorage', sessionStorage);
    Object.defineProperty(window, 'localStorage', { configurable: true, value: localStorage });
    Object.defineProperty(window, 'sessionStorage', { configurable: true, value: sessionStorage });
  });

  afterEach(() => {
    auth.stopAutoRenewal();
    vi.unstubAllGlobals();
  });

  it('does not trust a client-controlled auth_session storage flag', () => {
    sessionStorage.setItem('auth_session', '1');

    expect(auth.isAuthenticated()).toBe(false);
    expect(auth.getUser()).toBeNull();
  });

  it('fails closed when the HttpOnly refresh cookie cannot restore a session', async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response(null, { status: 401 }));
    vi.stubGlobal('fetch', fetchMock);

    await expect(auth.restoreSession()).resolves.toBe(false);

    expect(fetchMock).toHaveBeenCalledOnce();
    expect(requestUrl(fetchMock.mock.calls[0][0]).pathname).toBe('/api/v2/auth/refresh');
    expect(auth.isAuthenticated()).toBe(false);
    expect(auth.getUser()).toBeNull();
  });

  it('refreshes the cookie session and restores authoritative identity and permissions from auth/me', async () => {
    const requestedPaths: string[] = [];
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = requestUrl(input);
      requestedPaths.push(url.pathname);

      if (url.pathname === '/api/v2/auth/refresh') {
        return new Response(JSON.stringify({
          access_token: 'opaque-access-token',
          expires_in: 3600,
        }), { status: 200, headers: { 'Content-Type': 'application/json' } });
      }
      if (url.pathname === '/api/v2/auth/me') {
        return new Response(JSON.stringify({
          id: 'user-readonly',
          username: 'readonly',
          is_admin: false,
          roles: ['operations_viewer'],
          permissions: ['flight:read', 'system.config_read'],
          department: '运行质量部',
        }), { status: 200, headers: { 'Content-Type': 'application/json' } });
      }
      if (url.pathname === '/api/v2/auth/heartbeat') {
        return new Response(JSON.stringify({ success: true }), { status: 200 });
      }
      if (url.pathname === '/api/v2/auth/sse-token') {
        return new Response(JSON.stringify({
          sse_token: 'fixture-sse-token',
          sse_expires_in: 3600,
        }), { status: 200, headers: { 'Content-Type': 'application/json' } });
      }
      return new Response(null, { status: 599 });
    });
    vi.stubGlobal('fetch', fetchMock);

    await expect(auth.restoreSession()).resolves.toBe(true);

    expect(requestedPaths.slice(0, 2)).toEqual([
      '/api/v2/auth/refresh',
      '/api/v2/auth/me',
    ]);
    expect(auth.getUser()).toMatchObject({
      id: 'user-readonly',
      sub: 'user-readonly',
      username: 'readonly',
      roles: ['operations_viewer'],
      permissions: ['flight:read', 'system.config_read'],
      department: '运行质量部',
    });
    expect(auth.isAdmin()).toBe(false);
    expect(auth.isAuthenticated()).toBe(true);
    await vi.waitFor(() => {
      expect(requestedPaths).toContain('/api/v2/auth/heartbeat');
      expect(requestedPaths).toContain('/api/v2/auth/sse-token');
    });
  });
});
