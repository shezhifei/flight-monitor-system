import { computed, getCurrentInstance, onBeforeUnmount, readonly, ref } from 'vue';
import { useAuth } from './useAuth';
import { assertSameOriginHttpUrl } from '@/lib/url-guard';

export type SSEStatus = 'idle' | 'connecting' | 'online' | 'reconnecting' | 'offline';
export type SSEListener = (event: Event | MessageEvent<string>) => void;

export interface UseSSEOptions {
  url?: string;
  authenticated?: boolean;
  autoConnect?: boolean;
  autoReconnect?: boolean;
  reconnectMs?: number;
  clientScope?: string;
  clientInstanceId?: string;
}

const DEFAULT_RECONNECT_MS = 5000;

export function useSSE(initialOptions: UseSSEOptions = {}) {
  const auth = useAuth();
  const source = ref<EventSource | null>(null);
  const status = ref<SSEStatus>('idle');
  const error = ref<Event | null>(null);
  const lastMessage = ref<MessageEvent<string> | null>(null);
  const reconnectTimer = ref<number | null>(null);
  const listeners = new Map<string, Set<SSEListener>>();
  let options: UseSSEOptions = {
    authenticated: true,
    autoConnect: false,
    autoReconnect: true,
    reconnectMs: DEFAULT_RECONNECT_MS,
    ...initialOptions,
  };
  let manuallyDisconnected = false;

  function dispatch(eventName: string, event: Event | MessageEvent<string>): void {
    const handlers = listeners.get(eventName);
    if (!handlers) {
      return;
    }

    handlers.forEach((handler) => {
      handler(event);
    });
  }

  function clearReconnectTimer(): void {
    if (reconnectTimer.value !== null) {
      window.clearTimeout(reconnectTimer.value);
      reconnectTimer.value = null;
    }
  }

  function closeSource(): void {
    if (source.value) {
      source.value.close();
      source.value = null;
    }
  }

  function scheduleReconnect(): void {
    if (manuallyDisconnected || options.autoReconnect === false || reconnectTimer.value !== null) {
      return;
    }

    status.value = 'reconnecting';
    reconnectTimer.value = window.setTimeout(() => {
      reconnectTimer.value = null;
      void connect();
    }, options.reconnectMs ?? DEFAULT_RECONNECT_MS);
  }

  function addCustomListeners(activeSource: EventSource): void {
    listeners.forEach((handlers, eventName) => {
      if (eventName === 'open' || eventName === 'message' || eventName === 'error') {
        return;
      }

      handlers.forEach((handler) => {
        activeSource.addEventListener(eventName, handler as EventListener);
      });
    });
  }

  async function connect(overrideOptions: Partial<UseSSEOptions> = {}): Promise<EventSource | null> {
    options = {
      ...options,
      ...overrideOptions,
    };

    const url = String(options.url ?? '').trim();
    if (!url || typeof window === 'undefined') {
      return null;
    }

    manuallyDisconnected = false;
    clearReconnectTimer();
    closeSource();
    status.value = status.value === 'reconnecting' ? 'reconnecting' : 'connecting';
    error.value = null;

    const nextSource = options.authenticated === false
      ? new EventSource(assertSameOriginHttpUrl(url, 'EventSource').toString())
      : auth.getEventSource(url, {
          clientScope: options.clientScope,
          clientInstanceId: options.clientInstanceId,
        });

    nextSource.onopen = (event) => {
      status.value = 'online';
      error.value = null;
      dispatch('open', event);
    };

    nextSource.onmessage = (event) => {
      status.value = 'online';
      lastMessage.value = event;
      const data = event.data;
      let eventType = 'message';
      try {
        const parsed = JSON.parse(data);
        (event as MessageEvent & { __parsedJson?: unknown }).__parsedJson = parsed;
        if (parsed && typeof parsed === 'object' && parsed.topic) {
          eventType = String(parsed.topic);
        } else if (parsed && typeof parsed === 'object' && parsed.type) {
          eventType = String(parsed.type);
        }
      } catch (parseError) {
        // Not JSON — log for observability so parse failures are diagnosable
        console.debug('sse_parse_failed', { url, data, error: parseError });
      }
      dispatch(eventType, event);
      dispatch('message', event);
    };

    nextSource.onerror = (event) => {
      error.value = event;
      dispatch('error', event);
      closeSource();
      status.value = 'offline';
      auth.invalidateSSEToken?.();
      scheduleReconnect();
    };

    addCustomListeners(nextSource);
    source.value = nextSource;
    return nextSource;
  }

  function disconnect(): void {
    manuallyDisconnected = true;
    clearReconnectTimer();
    closeSource();
    status.value = 'offline';
  }

  async function reconnect(): Promise<EventSource | null> {
    status.value = 'reconnecting';
    return connect();
  }

  function on(eventName: string, handler: SSEListener): () => void {
    const normalizedName = String(eventName || '').trim() || 'message';
    const existing = listeners.get(normalizedName) ?? new Set<SSEListener>();
    existing.add(handler);
    listeners.set(normalizedName, existing);

    if (source.value && normalizedName !== 'open' && normalizedName !== 'message' && normalizedName !== 'error') {
      source.value.addEventListener(normalizedName, handler as EventListener);
    }

    return () => {
      off(normalizedName, handler);
    };
  }

  function off(eventName: string, handler: SSEListener): void {
    const normalizedName = String(eventName || '').trim() || 'message';
    const existing = listeners.get(normalizedName);
    if (!existing) {
      return;
    }

    existing.delete(handler);
    if (existing.size === 0) {
      listeners.delete(normalizedName);
    }

    if (source.value && normalizedName !== 'open' && normalizedName !== 'message' && normalizedName !== 'error') {
      source.value.removeEventListener(normalizedName, handler as EventListener);
    }
  }

  if (getCurrentInstance()) {
    onBeforeUnmount(() => {
      disconnect();
    });
  }

  if (options.autoConnect) {
    void connect();
  }

  return {
    source: readonly(source),
    status: readonly(status),
    error: readonly(error),
    lastMessage: readonly(lastMessage),
    isConnected: computed(() => status.value === 'online'),
    connect,
    disconnect,
    reconnect,
    on,
    off,
  };
}
