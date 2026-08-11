import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  AuthBridgeUnavailableError,
  REQUIRED_AUTH_BRIDGE_VERSION,
  authFetch,
  createEventSource,
  getCurrentUser,
  getPermissions,
  hasPermission,
  hasRole,
  isAdmin,
  requireAuth,
  type AuthBridge,
} from './authBridge';

function createBridge(overrides: Partial<AuthBridge> = {}): AuthBridge {
  return {
    owner: 'vue-app',
    version: REQUIRED_AUTH_BRIDGE_VERSION,
    requireAuthAsync: vi.fn().mockResolvedValue(true),
    fetch: vi.fn().mockResolvedValue(new Response(null, { status: 204 })),
    getEventSource: vi.fn().mockReturnValue({ close: vi.fn() } as unknown as EventSource),
    getUser: vi.fn().mockReturnValue({
      sub: 'readonly-1',
      role: 'viewer',
      roles: ['operator'],
      permissions: ['ai:view'],
    }),
    getPermissions: vi.fn().mockReturnValue(['ai:view']),
    hasPermission: vi.fn((permission: string) => permission === 'ai:view'),
    logout: vi.fn(),
    isAdmin: vi.fn().mockReturnValue(false),
    ...overrides,
  };
}

beforeEach(() => {
  vi.stubGlobal('window', {
    Auth: undefined,
    location: { href: 'https://fms.example/frontend/nl_query.html' },
  } as unknown as Window);
  delete window.Auth;
});

afterEach(() => {
  delete window.Auth;
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('React auth bridge client', () => {
  it('fails closed when the Vue bridge is absent', async () => {
    const rawFetch = vi.spyOn(globalThis, 'fetch');

    await expect(requireAuth()).resolves.toBe(false);
    await expect(authFetch('/api/v2/ai/jobs')).rejects.toBeInstanceOf(AuthBridgeUnavailableError);
    expect(() => createEventSource('/api/v2/sse/stream')).toThrow(AuthBridgeUnavailableError);
    expect(getCurrentUser()).toBeNull();
    expect(getPermissions()).toEqual([]);
    expect(hasPermission('ai:view')).toBe(false);
    expect(isAdmin()).toBe(false);
    expect(rawFetch).not.toHaveBeenCalled();
  });

  it('rejects partial and non-Vue bridge objects', async () => {
    window.Auth = {
      ...createBridge(),
      owner: 'vue-app',
      getPermissions: undefined,
    } as unknown as AuthBridge;

    await expect(requireAuth()).resolves.toBe(false);
    await expect(authFetch('/api/v2/ai/jobs')).rejects.toBeInstanceOf(AuthBridgeUnavailableError);
  });

  it('delegates all transport and identity decisions to the complete bridge', async () => {
    const bridge = createBridge();
    window.Auth = bridge;

    await expect(requireAuth()).resolves.toBe(true);
    await authFetch('/api/v2/ai/jobs', { method: 'POST' });
    createEventSource('/api/v2/sse/stream', { clientScope: 'nl-query' });

    expect(bridge.fetch).toHaveBeenCalledWith('/api/v2/ai/jobs', { method: 'POST' });
    expect(bridge.getEventSource).toHaveBeenCalledWith(
      '/api/v2/sse/stream',
      { clientScope: 'nl-query' },
    );
    expect(getCurrentUser()).toMatchObject({ sub: 'readonly-1' });
    expect(getPermissions()).toEqual(['ai:view']);
    expect(hasPermission('ai:view')).toBe(true);
    expect(hasPermission('ai:execute')).toBe(false);
    expect(hasRole('viewer')).toBe(true);
    expect(hasRole('operator')).toBe(true);
    expect(isAdmin()).toBe(false);
  });

  it('treats bridge exceptions and non-boolean success as authentication failure', async () => {
    const logger = vi.spyOn(console, 'error').mockImplementation(() => {});
    window.Auth = createBridge({
      requireAuthAsync: vi.fn().mockRejectedValue(new Error('sensitive backend detail')),
    });

    await expect(requireAuth()).resolves.toBe(false);
    expect(logger).toHaveBeenCalledWith('[auth-bridge] authentication failed');

    window.Auth = createBridge({
      requireAuthAsync: vi.fn().mockResolvedValue('yes' as unknown as boolean),
    });
    await expect(requireAuth()).resolves.toBe(false);
  });

  it('fails identity and authorization reads closed when bridge methods throw', () => {
    window.Auth = createBridge({
      getUser: vi.fn(() => { throw new Error('identity unavailable'); }),
      getPermissions: vi.fn(() => { throw new Error('permissions unavailable'); }),
      hasPermission: vi.fn(() => { throw new Error('permission unavailable'); }),
      isAdmin: vi.fn(() => { throw new Error('admin unavailable'); }),
    });

    expect(getCurrentUser()).toBeNull();
    expect(getPermissions()).toEqual([]);
    expect(hasPermission('*')).toBe(false);
    expect(isAdmin()).toBe(false);
  });
});
