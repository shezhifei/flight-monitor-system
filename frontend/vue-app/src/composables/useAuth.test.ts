// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { hasUserPermission, useAuth } from './useAuth';

const eventSourceUrls: string[] = [];

function createMemoryStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() {
      return values.size;
    },
    clear: () => values.clear(),
    getItem: (key: string) => values.get(key) ?? null,
    key: (index: number) => Array.from(values.keys())[index] ?? null,
    removeItem: (key: string) => {
      values.delete(key);
    },
    setItem: (key: string, value: string) => {
      values.set(key, String(value));
    },
  };
}

class MockEventSource {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSED = 2;

  readonly url: string;
  readonly withCredentials: boolean;
  readyState = MockEventSource.CONNECTING;
  onopen: ((event: Event) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;

  constructor(url: string | URL, options?: { withCredentials?: boolean }) {
    this.url = String(url);
    this.withCredentials = options?.withCredentials ?? false;
    eventSourceUrls.push(this.url);
  }

  close(): void {
    this.readyState = MockEventSource.CLOSED;
  }

  addEventListener(): void {}
  removeEventListener(): void {}
  dispatchEvent(): boolean {
    return true;
  }
}

describe('useAuth getEventSource', () => {
  beforeEach(() => {
    eventSourceUrls.length = 0;
    vi.stubGlobal('localStorage', createMemoryStorage());
    vi.stubGlobal('sessionStorage', createMemoryStorage());
    Object.defineProperty(window, 'localStorage', {
      configurable: true,
      value: globalThis.localStorage,
    });
    Object.defineProperty(window, 'sessionStorage', {
      configurable: true,
      value: globalThis.sessionStorage,
    });
    vi.stubGlobal('EventSource', MockEventSource);
    vi.stubGlobal('fetch', vi.fn());
  });

  it('does not append bearer tokens to the SSE URL by default', () => {
    sessionStorage.setItem('sse_token', 'sse-token-value');
    sessionStorage.setItem('sse_token_expires_at', String(Date.now() + 60_000));

    const auth = useAuth();
    const source = auth.getEventSource('/api/v2/sse/stream?topics=flights', {
      clientInstanceId: 'client-1',
    });

    expect(source.url).toContain('/api/v2/sse/stream?topics=flights');
    expect(source.url).toContain('client_instance_id=client-1');
    expect(source.url).not.toContain('sse_token=');
    expect(source.url).not.toContain('token=');
    expect(source.url).not.toContain('sse-token-value');
    source.close();
  });

  it('connects authenticated SSE streams with credentials included', async () => {
    const capturedRequests: Request[] = [];
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      capturedRequests.push(input as Request);
      const stream = new ReadableStream<Uint8Array>({
        start(controller) {
          controller.enqueue(new TextEncoder().encode('event: flights\ndata: {"ok":true}\n\n'));
          controller.close();
        },
      });
      return new Response(stream, {
        status: 200,
        headers: { 'content-type': 'text/event-stream' },
      });
    }));

    const auth = useAuth();
    auth.saveToken({
      access_token: 'access-token-value',
      token_type: 'bearer',
      expires_in: 3600,
    });

    const message = new Promise<string>((resolve) => {
      const source = auth.getEventSource(`${window.location.origin}/api/v2/sse/stream?topics=flights`, {
        clientInstanceId: 'client-1',
      });
      source.addEventListener('flights', (event) => {
        resolve((event as MessageEvent<string>).data);
        source.close();
      });
    });

    await expect(message).resolves.toBe('{"ok":true}');
    const sseRequests = capturedRequests.filter((request) => request.url.includes('/api/v2/sse/stream'));
    const authenticatedRequest = sseRequests.find((request) => request.credentials === 'include');
    expect(authenticatedRequest).toBeDefined();
    expect(authenticatedRequest?.url).toBe(`${window.location.origin}/api/v2/sse/stream?topics=flights&client_instance_id=client-1`);
    expect(authenticatedRequest?.headers.get('Accept')).toBe('text/event-stream');
  });

  it('rejects authenticated fetches to cross-origin URLs before adding auth headers', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);

    const auth = useAuth();

    await expect(auth.fetch('https://evil.example/api/collect')).rejects.toThrow(/same-origin/);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('rejects EventSource URLs outside the current origin', () => {
    const auth = useAuth();

    expect(() => auth.getEventSource('https://evil.example/api/v2/sse/stream', {
      clientInstanceId: 'client-1',
    })).toThrow(/same-origin/);
    expect(eventSourceUrls).toHaveLength(0);
  });
});

describe('hasUserPermission', () => {
  it('supports backend colon-delimited wildcard grants', () => {
    expect(hasUserPermission({ permissions: ['team:*'] }, 'team:manage')).toBe(true);
    expect(hasUserPermission({ permissions: ['team:*'] }, 'equipment:manage')).toBe(false);
  });

  it('keeps dot-delimited compatibility grants working', () => {
    expect(hasUserPermission({ permissions: ['system.*'] }, 'system.config_write')).toBe(true);
  });

  it('does not expand flight:manage into granular permissions', () => {
    const user = { permissions: ['flight:manage'] };
    expect(hasUserPermission(user, 'flight.update')).toBe(false);
    expect(hasUserPermission(user, 'business_case.update')).toBe(false);
  });
});
