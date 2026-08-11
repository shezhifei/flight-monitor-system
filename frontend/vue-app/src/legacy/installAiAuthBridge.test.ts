/**
 * @vitest-environment jsdom
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { JwtUser } from '@/composables/useAuth';
import {
  AI_AUTH_BRIDGE_VERSION,
  __resetAiAuthBridgeForTests,
  installAiAuthBridge,
  type AiAuthProvider,
} from './installAiAuthBridge';

function createProvider(user: JwtUser | null = null): AiAuthProvider {
  return {
    requireAuthAsync: vi.fn().mockResolvedValue(true),
    fetch: vi.fn().mockResolvedValue(new Response(null, { status: 204 })),
    getEventSource: vi.fn().mockReturnValue({ close: vi.fn() } as unknown as EventSource),
    getUser: vi.fn().mockReturnValue(user),
    logout: vi.fn().mockResolvedValue(undefined),
    isAdmin: vi.fn().mockReturnValue(user?.is_admin === true || user?.role === 'admin'),
  };
}

beforeEach(() => {
  __resetAiAuthBridgeForTests();
});

afterEach(() => {
  __resetAiAuthBridgeForTests();
  vi.restoreAllMocks();
});

describe('installAiAuthBridge', () => {
  it('installs an immutable Vue-owned bridge with the complete contract', () => {
    const provider = createProvider({ sub: 'admin-1', role: 'admin' });

    const bridge = installAiAuthBridge(provider);

    expect(window.Auth).toBe(bridge);
    expect(bridge.owner).toBe('vue-app');
    expect(bridge.version).toBe(AI_AUTH_BRIDGE_VERSION);
    expect(Object.isFrozen(bridge)).toBe(true);
    expect(Object.getOwnPropertyDescriptor(window, 'Auth')?.writable).toBe(false);
    expect(installAiAuthBridge(provider)).toBe(bridge);
  });

  it('fails closed and does not call the transport when authentication is rejected', async () => {
    const provider = createProvider();
    vi.mocked(provider.requireAuthAsync).mockResolvedValue(false);
    const bridge = installAiAuthBridge(provider);

    await expect(bridge.fetch('/api/v2/ai/jobs')).rejects.toThrow(/session is unavailable/);
    expect(provider.fetch).not.toHaveBeenCalled();
    expect(() => bridge.getEventSource('/api/v2/sse/stream')).toThrow(/until authentication succeeds/);
    expect(provider.getEventSource).not.toHaveBeenCalled();
  });

  it('awaits authentication before delegating fetch and then permits authenticated SSE', async () => {
    const provider = createProvider({ sub: 'operator-1', permissions: ['ai:chat'] });
    const bridge = installAiAuthBridge(provider);

    await bridge.fetch('/api/v2/ai/jobs', { method: 'POST' });
    bridge.getEventSource('/api/v2/sse/stream', { clientScope: 'ai' });

    expect(provider.requireAuthAsync).toHaveBeenCalledTimes(1);
    expect(provider.fetch).toHaveBeenCalledWith('/api/v2/ai/jobs', { method: 'POST' });
    expect(provider.getEventSource).toHaveBeenCalledWith(
      '/api/v2/sse/stream',
      { clientScope: 'ai' },
    );
  });

  it('exposes normalized permissions and preserves read-only authorization', () => {
    const provider = createProvider({
      sub: 'readonly-1',
      role: 'viewer',
      permissions: ['dispatch:view'],
    });
    const bridge = installAiAuthBridge(provider);

    expect(bridge.getPermissions()).toEqual(expect.arrayContaining([
      'dispatch:view',
      'dispatch_order.read',
    ]));
    expect(bridge.hasPermission('dispatch_order.read')).toBe(true);
    expect(bridge.hasPermission('dispatch_order.update')).toBe(false);
    expect(bridge.isAdmin()).toBe(false);
  });

  it('closes the bridge before awaiting logout', async () => {
    const provider = createProvider({ sub: 'admin-1', is_admin: true });
    const bridge = installAiAuthBridge(provider);
    await expect(bridge.requireAuthAsync()).resolves.toBe(true);

    await bridge.logout();

    expect(provider.logout).toHaveBeenCalledTimes(1);
    expect(() => bridge.getEventSource('/api/v2/sse/stream')).toThrow(/until authentication succeeds/);
  });

  it('fails identity and authorization checks closed when the provider throws', () => {
    const provider = createProvider();
    vi.mocked(provider.getUser).mockImplementation(() => {
      throw new Error('identity unavailable');
    });
    vi.mocked(provider.isAdmin).mockImplementation(() => {
      throw new Error('authorization unavailable');
    });
    const bridge = installAiAuthBridge(provider);

    expect(bridge.getUser()).toBeNull();
    expect(bridge.getPermissions()).toEqual([]);
    expect(bridge.hasPermission('*')).toBe(false);
    expect(bridge.isAdmin()).toBe(false);
  });
});
