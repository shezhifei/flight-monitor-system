import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  REQUIRED_AUTH_BRIDGE_VERSION,
  type AuthBridge,
} from '@/lib/auth/authBridge';
import { EventSourceClient } from './eventSourceClient';

class FakeEventSource extends EventTarget {
  onopen: ((event: Event) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;
  close = vi.fn();
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(Math, 'random').mockReturnValue(0);
  vi.stubGlobal('window', {
    Auth: undefined,
    setTimeout,
    clearTimeout,
  } as unknown as Window);
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('EventSourceClient authentication reconnect', () => {
  it('reconnects through the Vue-owned EventSource transport after a stream error', async () => {
    const sources: FakeEventSource[] = [];
    const getEventSource = vi.fn(() => {
      const source = new FakeEventSource();
      sources.push(source);
      return source as unknown as EventSource;
    });
    const bridge: AuthBridge = {
      owner: 'vue-app',
      version: REQUIRED_AUTH_BRIDGE_VERSION,
      requireAuthAsync: vi.fn().mockResolvedValue(true),
      fetch: vi.fn(),
      getEventSource,
      getUser: vi.fn().mockReturnValue({ sub: 'operator-1' }),
      getPermissions: vi.fn().mockReturnValue(['ai:view']),
      hasPermission: vi.fn().mockReturnValue(true),
      logout: vi.fn(),
      isAdmin: vi.fn().mockReturnValue(false),
    };
    window.Auth = bridge;
    const client = new EventSourceClient({
      url: '/api/v2/sse/stream',
      clientScope: 'ai-reconnect-test',
      reconnectMs: 10,
    });

    client.connect();
    expect(getEventSource).toHaveBeenCalledTimes(1);

    sources[0]?.onerror?.(new Event('error'));
    await vi.advanceTimersByTimeAsync(10);

    expect(sources[0]?.close).toHaveBeenCalledTimes(1);
    expect(getEventSource).toHaveBeenCalledTimes(2);
    expect(getEventSource).toHaveBeenLastCalledWith('/api/v2/sse/stream', {
      clientScope: 'ai-reconnect-test',
    });

    client.disconnect();
  });
});
