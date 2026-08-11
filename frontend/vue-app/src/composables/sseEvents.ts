import type { ChatGroup, ChatMessage } from './useDispatchChat';

// ─── Dispatch-collaboration chat SSE events ──────────────────────────────────
// Discriminated union keyed on `type`. The wire format is loosely structured
// (server may signal the kind via the SSE event name OR an inline `type`
// field), so `normalizeDispatchChatEvent` is the single trust boundary that
// turns the untyped frame into one of these typed variants.

export interface DispatchChatInitialEvent {
  type: 'dispatch_chat_initial';
  items?: ChatGroup[];
  unread_total?: number;
}

export interface DispatchChatMessageEvent {
  type: 'dispatch_chat_message';
  group_id: string;
  flight_id?: string;
  message: ChatMessage;
  unread_count?: number;
  unread_total?: number;
}

export interface DispatchChatGroupUpsertedEvent {
  type: 'dispatch_chat_group_upserted';
  group: ChatGroup;
}

export interface DispatchChatGroupArchivedEvent {
  type: 'dispatch_chat_group_archived';
  group_id: string;
}

export interface DispatchChatReadSyncedEvent {
  type: 'dispatch_chat_read_synced';
  group_id: string;
  unread_count?: number;
  unread_total?: number;
}

export type DispatchChatSSEEvent =
  | DispatchChatInitialEvent
  | DispatchChatMessageEvent
  | DispatchChatGroupUpsertedEvent
  | DispatchChatGroupArchivedEvent
  | DispatchChatReadSyncedEvent;

// ─── Command-center stream events ────────────────────────────────────────────
// The command center merely buffers raw broadcast frames for display, so the
// shape is intentionally open but no longer `any`.
export interface CommandEvent {
  type?: string;
  topic?: string;
  payload?: unknown;
  ts?: number;
  timestamp?: string | number;
  message?: string;
  description?: string;
  [key: string]: unknown;
}

export type SSEEvent = DispatchChatSSEEvent | CommandEvent;

const CHAT_EVENT_NAME_TO_TYPE: Record<string, DispatchChatSSEEvent['type']> = {
  initial: 'dispatch_chat_initial',
  chat_message: 'dispatch_chat_message',
  chat_group_upserted: 'dispatch_chat_group_upserted',
  chat_group_archived: 'dispatch_chat_group_archived',
  chat_read_synced: 'dispatch_chat_read_synced',
};

const CHAT_TYPE_VALUES: ReadonlySet<DispatchChatSSEEvent['type']> = new Set([
  'dispatch_chat_initial',
  'dispatch_chat_message',
  'dispatch_chat_group_upserted',
  'dispatch_chat_group_archived',
  'dispatch_chat_read_synced',
]);

function resolveChatEventType(
  record: Record<string, unknown>,
  eventName: string,
): DispatchChatSSEEvent['type'] | null {
  const byName = CHAT_EVENT_NAME_TO_TYPE[eventName];
  if (byName) return byName;

  const explicit = String(record.type ?? '').trim().toLowerCase() as DispatchChatSSEEvent['type'];
  return CHAT_TYPE_VALUES.has(explicit) ? explicit : null;
}

/**
 * Normalize a raw SSE frame (parsed JSON object + SSE event name) into a typed,
 * discriminated dispatch-chat event. Returns `null` for non-object payloads or
 * frames that do not map to a known chat event — the single place where the
 * untyped network boundary is crossed.
 */
export function normalizeDispatchChatEvent(raw: unknown, eventName: string): DispatchChatSSEEvent | null {
  if (!raw || typeof raw !== 'object') return null;
  const record = raw as Record<string, unknown>;
  const type = resolveChatEventType(record, eventName);
  if (!type) return null;
  return { ...record, type } as unknown as DispatchChatSSEEvent;
}
