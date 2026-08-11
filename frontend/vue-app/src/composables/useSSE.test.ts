// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useSSE } from './useSSE';

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
  readyState = MockEventSource.CONNECTING;
  onopen: ((event: Event) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;

  constructor(url: string | URL) {
    this.url = String(url);
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

describe('useSSE unauthenticated EventSource URLs', () => {
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
  });

  it('resolves relative unauthenticated EventSource URLs against the current origin', async () => {
    const sse = useSSE({
      authenticated: false,
      url: '/api/v2/public-stream?topic=health',
    });

    await sse.connect();

    expect(eventSourceUrls).toEqual([
      `${window.location.origin}/api/v2/public-stream?topic=health`,
    ]);
  });

  it('rejects cross-origin unauthenticated EventSource URLs', async () => {
    const sse = useSSE({
      authenticated: false,
      url: 'https://evil.example/api/v2/public-stream',
    });

    await expect(sse.connect()).rejects.toThrow(/same-origin/);
    expect(eventSourceUrls).toHaveLength(0);
  });

  it('rejects non-HTTP unauthenticated EventSource URLs', async () => {
    const sse = useSSE({
      authenticated: false,
      url: 'data:text/event-stream,data: ok',
    });

    await expect(sse.connect()).rejects.toThrow(/same-origin/);
    expect(eventSourceUrls).toHaveLength(0);
  });
});

describe('useSSE parse failure observability (Task 12d / F6)', () => {
  let mockSource: MockEventSource;

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
  });

  it('logs a debug message when SSE message is not valid JSON', async () => {
    const debugSpy = vi.spyOn(console, 'debug').mockImplementation(() => {});

    const sse = useSSE({
      authenticated: false,
      url: '/api/v2/public-stream',
      autoReconnect: false,
    });

    await sse.connect();

    // Get the created MockEventSource instance via the connect() return value
    const connected = await sse.connect();
    if (connected) {
      mockSource = connected as unknown as MockEventSource;
    }

    // Simulate a non-JSON message
    mockSource.onmessage?.({
      data: 'this is not json',
      type: 'message',
    } as MessageEvent);

    expect(debugSpy).toHaveBeenCalledWith(
      expect.stringContaining('sse_parse_failed'),
      expect.anything(),
    );

    debugSpy.mockRestore();
  });
});
