import { createEventSource } from '@/lib/auth/authBridge';
import { safeJson } from '@/lib/sse/streamParser';
import type { AiRuntimeEvent } from '@/lib/types/aiEvents';

interface EventSourceClientOptions {
  url: string;
  eventNames?: string[];
  clientScope?: string;
  reconnectMs?: number;
  onOpen?: () => void;
  onEvent?: (eventName: string, payload: AiRuntimeEvent) => void;
  onError?: (error: Event) => void;
}

export class EventSourceClient {
  private readonly options: EventSourceClientOptions;
  private source: EventSource | null = null;
  private reconnectTimer: number | null = null;
  private reconnectAttempt = 0;
  private closed = true;

  constructor(options: EventSourceClientOptions) {
    this.options = options;
  }

  connect(): void {
    this.closeSource();
    this.closed = false;
    const source = createEventSource(this.options.url, {
      clientScope: this.options.clientScope,
    });
    this.source = source;

    const eventNames = this.options.eventNames && this.options.eventNames.length > 0
      ? this.options.eventNames
      : ['ai_execution'];

    const onPayload = (eventName: string, raw: MessageEvent): void => {
      const parsed = safeJson<AiRuntimeEvent>(String(raw.data || ''));
      if (!parsed) {
        return;
      }
      this.options.onEvent?.(eventName, parsed);
    };

    source.onopen = () => {
      this.reconnectAttempt = 0;
      this.options.onOpen?.();
    };
    source.onmessage = (event) => onPayload('message', event);
    source.onerror = (event) => {
      this.options.onError?.(event);
      if (!this.closed) {
        this.scheduleReconnect();
      }
    };
    eventNames.forEach((eventName) => {
      source.addEventListener(eventName, (event) => onPayload(eventName, event as MessageEvent));
    });
  }

  disconnect(clearTimer = true): void {
    this.closed = true;
    this.closeSource();
    if (clearTimer && this.reconnectTimer !== null) {
      window.clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    if (clearTimer) {
      this.reconnectAttempt = 0;
    }
  }

  private closeSource(): void {
    if (!this.source) {
      return;
    }
    try {
      this.source.close();
    } catch (_error) {
      // ignore
    }
    this.source = null;
  }

  private scheduleReconnect(): void {
    if (this.reconnectTimer !== null) {
      return;
    }
    this.closeSource();
    this.reconnectAttempt += 1;
    const base = Number.isFinite(this.options.reconnectMs) ? Number(this.options.reconnectMs) : 3000;
    const exponential = Math.min(base * (2 ** Math.max(0, this.reconnectAttempt - 1)), 30000);
    const wait = exponential + Math.floor(Math.random() * 1000);
    this.reconnectTimer = window.setTimeout(() => {
      this.reconnectTimer = null;
      if (!this.closed) {
        this.connect();
      }
    }, wait);
  }
}
