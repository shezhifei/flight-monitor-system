import { computed, readonly, ref } from 'vue';
import { useAuth } from './useAuth';

export interface RealtimeResolvedConfig {
  entity_id: string;
  asr_model: string;
  tts_model: string;
  sample_rate_hz: number;
  chunk_ms: number;
}

export type RealtimeAudioStatus =
  | 'idle'
  | 'connecting'
  | 'connected'
  | 'closed'
  | 'error';

export type RealtimeServerEvent =
  | {
      type: 'session.ready';
      session_id: string;
      protocol_version: number;
      resolved_config: RealtimeResolvedConfig;
    }
  | {
      type: 'asr.partial';
      sequence: number;
      text: string;
      confidence: number;
    }
  | {
      type: 'asr.final';
      sequence: number;
      text: string;
      confidence: number;
    }
  | {
      type: 'error';
      code?: string;
      message: string;
      retryable?: boolean;
    }
  | {
      type: 'session.closed';
      reason?: string;
    }
  | Record<string, unknown>;

export interface RealtimeAudioSessionOptions {
  entityId: string;
  protocolVersion?: number;
  onEvent?: (event: RealtimeServerEvent) => void;
  onAsrFinal?: (text: string, event: RealtimeServerEvent) => void;
  onAsrPartial?: (text: string, event: RealtimeServerEvent) => void;
}

function websocketUrl(apiBase: string, entityId: string, protocolVersion: number, token: string): string {
  const base = new URL(apiBase, window.location.origin);
  base.protocol = base.protocol === 'https:' ? 'wss:' : 'ws:';
  base.pathname = `${base.pathname.replace(/\/$/, '')}/ai/realtime/audio`;
  base.search = '';
  base.searchParams.set('entity_id', entityId);
  base.searchParams.set('protocol_version', String(protocolVersion));
  base.searchParams.set('token', token);
  return base.toString();
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value && typeof value === 'object');
}

function readText(event: RealtimeServerEvent): string {
  if (!isRecord(event)) {
    return '';
  }
  const record = event as Record<string, unknown>;
  const text = record.text;
  return typeof text === 'string' ? text : '';
}

export function useRealtimeAudioSession(options: RealtimeAudioSessionOptions) {
  const auth = useAuth();
  const status = ref<RealtimeAudioStatus>('idle');
  const errorMessage = ref('');
  const resolvedConfig = ref<RealtimeResolvedConfig | null>(null);
  const sessionId = ref<string | null>(null);
  const sequence = ref(0);
  let socket: WebSocket | null = null;

  const isConnected = computed(() => status.value === 'connected' && socket?.readyState === WebSocket.OPEN);

  function sendJson(payload: Record<string, unknown>): void {
    if (!socket || socket.readyState !== WebSocket.OPEN) {
      throw new Error('实时音频连接未就绪');
    }
    socket.send(JSON.stringify(payload));
  }

  async function connect(): Promise<void> {
    if (isConnected.value || status.value === 'connecting') {
      return;
    }
    if (typeof window === 'undefined') {
      throw new Error('实时音频仅支持浏览器环境');
    }

    const token = auth.getSSEToken() || auth.getToken();
    if (!token) {
      throw new Error('请先登录后再启用 Auto Copilot');
    }

    status.value = 'connecting';
    errorMessage.value = '';
    const url = websocketUrl(auth.apiBase.value, options.entityId, options.protocolVersion ?? 1, token);

    await new Promise<void>((resolve, reject) => {
      const ws = new WebSocket(url);
      socket = ws;

      const fail = (message: string) => {
        status.value = 'error';
        errorMessage.value = message;
        reject(new Error(message));
      };

      ws.onopen = () => {
        status.value = 'connected';
        sendJson({
          type: 'session.start',
          entity_id: options.entityId,
          input_audio: {
            format: 'pcm_s16le',
            sample_rate_hz: 16000,
            channels: 1,
          },
        });
        resolve();
      };

      ws.onmessage = (message) => {
        try {
          const event = JSON.parse(String(message.data)) as RealtimeServerEvent;
          options.onEvent?.(event);

          if (event.type === 'session.ready' && isRecord(event.resolved_config)) {
            sessionId.value = typeof event.session_id === 'string' ? event.session_id : null;
            resolvedConfig.value = event.resolved_config as unknown as RealtimeResolvedConfig;
          } else if (event.type === 'asr.partial') {
            options.onAsrPartial?.(readText(event), event);
          } else if (event.type === 'asr.final') {
            options.onAsrFinal?.(readText(event), event);
          } else if (event.type === 'error') {
            errorMessage.value = typeof event.message === 'string' ? event.message : '实时音频服务异常';
          }
        } catch {
          errorMessage.value = '实时音频消息解析失败';
        }
      };

      ws.onerror = () => {
        fail('实时音频连接失败');
      };

      ws.onclose = () => {
        if (status.value !== 'error') {
          status.value = 'closed';
        }
        socket = null;
      };
    });
  }

  function sendAudioChunk(audioBase64: string, timestampMs = Date.now()): void {
    const normalized = String(audioBase64 || '').trim();
    if (!normalized) {
      return;
    }
    sequence.value += 1;
    sendJson({
      type: 'audio.chunk',
      sequence: sequence.value,
      timestamp_ms: timestampMs,
      audio_base64: normalized,
    });
  }

  function endAudio(): void {
    if (isConnected.value) {
      sendJson({ type: 'audio.end' });
    }
  }

  function cancel(reason = 'user_cancelled'): void {
    if (isConnected.value) {
      sendJson({ type: 'session.cancel', reason });
    }
    socket?.close();
    socket = null;
    status.value = 'closed';
  }

  return {
    status: readonly(status),
    errorMessage: readonly(errorMessage),
    resolvedConfig: readonly(resolvedConfig),
    sessionId: readonly(sessionId),
    isConnected,
    connect,
    sendAudioChunk,
    endAudio,
    cancel,
  };
}
