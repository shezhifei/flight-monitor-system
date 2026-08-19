export interface AiCapabilities {
  ai_ready: boolean;
  ai_execute_permission: boolean;
  ai_chat_permission: boolean;
  missing_reasons: string[];
}

export interface PendingActionsResponse {
  items: Array<Record<string, unknown>>;
  total: number;
  total_count: number;
  pagination?: {
    limit: number;
    offset: number;
    next_offset?: number;
    has_more: boolean;
  };
}

export interface NLQueryResult {
  conversation_id?: string;
  summary?: string;
  structured_data?: unknown;
  visualization_hint?: string;
}

export interface AiEntityConfig {
  id: string;
  base_url?: string;
  api_key?: string;
  default_model?: string;
  asr_model?: string;
  tts_model?: string;
  tts_voice?: string;
  api_format?: string;
  timeout?: number;
  max_retries?: number;
  retry_delay?: number;
  system_prompt?: string;
  task_template?: string;
  denied_tools?: string[];
}

// ---------------------------------------------------------------------------
// Realtime Audio Types
// ---------------------------------------------------------------------------

export interface RealtimeAudioFormat {
  format: string;
  sample_rate_hz: number;
  channels: number;
  voice?: string;
}

export interface RealtimeSessionStartInput {
  session_id?: string;
  entity_id: string;
  input_audio?: RealtimeAudioFormat;
  output_audio?: RealtimeAudioFormat;
}

export interface RealtimeResolvedConfig {
  entity_id: string;
  asr_model: string;
  tts_model: string;
  sample_rate_hz: number;
  chunk_ms: number;
}

export interface RealtimeSessionReady {
  type: 'session.ready';
  session_id: string;
  protocol_version: number;
  resolved_config: RealtimeResolvedConfig;
}

export interface RealtimeAsrPartial {
  type: 'asr.partial';
  sequence: number;
  text: string;
  confidence: number;
}

export interface RealtimeAsrFinal {
  type: 'asr.final';
  sequence: number;
  text: string;
  confidence: number;
}

export interface RealtimeIntentPartial {
  type: 'intent.partial';
  intent: string;
  slots: Record<string, string>;
}

export interface RealtimeAgentDelta {
  type: 'agent.delta';
  sequence: number;
  text: string;
}

export interface RealtimeTtsChunk {
  type: 'tts.chunk';
  sequence: number;
  audio_base64: string;
  format: string;
  sample_rate_hz: number;
}

export interface RealtimeSessionInterrupted {
  type: 'session.interrupted';
  reason: string;
  cancelled_output_sequence: number;
}

export interface RealtimeErrorEvent {
  type: 'error';
  code: string;
  message: string;
  retryable: boolean;
}

export interface RealtimeSessionClosed {
  type: 'session.closed';
  reason: string;
}

export type RealtimeAudioServerEvent =
  | RealtimeSessionReady
  | RealtimeAsrPartial
  | RealtimeAsrFinal
  | RealtimeIntentPartial
  | RealtimeAgentDelta
  | RealtimeTtsChunk
  | RealtimeSessionInterrupted
  | RealtimeErrorEvent
  | RealtimeSessionClosed;
