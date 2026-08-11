import { describe, it, expect } from 'vitest';
import { normalizeDispatchChatEvent } from './sseEvents';

describe('normalizeDispatchChatEvent', () => {
  it('maps the SSE event name to a discriminated type', () => {
    expect(normalizeDispatchChatEvent({}, 'initial')?.type).toBe('dispatch_chat_initial');
    expect(normalizeDispatchChatEvent({ group_id: 'g1', message: { content: 'hi' } }, 'chat_message')?.type).toBe(
      'dispatch_chat_message',
    );
    expect(normalizeDispatchChatEvent({}, 'chat_group_upserted')?.type).toBe('dispatch_chat_group_upserted');
    expect(normalizeDispatchChatEvent({}, 'chat_group_archived')?.type).toBe('dispatch_chat_group_archived');
    expect(normalizeDispatchChatEvent({}, 'chat_read_synced')?.type).toBe('dispatch_chat_read_synced');
  });

  it('falls back to the payload type field when the event name is empty', () => {
    expect(normalizeDispatchChatEvent({ type: 'dispatch_chat_message', group_id: 'g', message: {} }, '')?.type).toBe(
      'dispatch_chat_message',
    );
    expect(normalizeDispatchChatEvent({ type: 'DISPATCH_CHAT_INITIAL' }, '')?.type).toBe('dispatch_chat_initial');
  });

  it('preserves the original payload fields on the normalized event', () => {
    const event = normalizeDispatchChatEvent({ group_id: 'g42', unread_total: 7 }, 'chat_read_synced');
    expect(event).not.toBeNull();
    if (event && event.type === 'dispatch_chat_read_synced') {
      expect(event.group_id).toBe('g42');
      expect(event.unread_total).toBe(7);
    }
  });

  it('returns null for unknown event names and non-object payloads', () => {
    expect(normalizeDispatchChatEvent({}, 'something_else')).toBeNull();
    expect(normalizeDispatchChatEvent(null, 'chat_message')).toBeNull();
    expect(normalizeDispatchChatEvent('not-an-object', 'chat_message')).toBeNull();
  });
});
