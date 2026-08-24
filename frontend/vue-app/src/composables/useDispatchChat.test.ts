// @vitest-environment node
import { describe, expect, it } from 'vitest';
import {
  appendUniqueChatMessage,
  chatMessageIdentity,
  mentionUserIdsFromRaw,
  type ChatMessage,
} from './useDispatchChat';

function msg(overrides: Partial<ChatMessage> = {}): ChatMessage {
  return {
    message_id: 'm1',
    id: 'm1',
    group_id: 'g1',
    seq_no: 3,
    content: 'hello',
    sender_user_id: 'u1',
    ...overrides,
  };
}

describe('chat message identity', () => {
  it('prefers message_id over seq fallback', () => {
    expect(chatMessageIdentity(msg())).toBe('id:m1');
    expect(chatMessageIdentity(msg({ message_id: undefined, id: undefined }))).toBe('seq:g1:3');
  });

  it('does not append a POST echo after the SSE copy', () => {
    const fromSse = msg({ message_id: 'm9', id: 'm9', seq_no: 9 });
    const fromPost = msg({ message_id: 'm9', id: 'm9', seq_no: 9, content: 'hello' });
    const merged = appendUniqueChatMessage([fromSse], fromPost);
    expect(merged).toHaveLength(1);
  });

  it('keeps a genuinely new message', () => {
    const first = msg({ message_id: 'm1', id: 'm1', seq_no: 1 });
    const second = msg({ message_id: 'm2', id: 'm2', seq_no: 2, content: 'later' });
    const merged = appendUniqueChatMessage([first], second);
    expect(merged.map((item) => item.message_id)).toEqual(['m1', 'm2']);
  });
});

describe('mentionUserIdsFromRaw', () => {
  it('reads top-level mention_user_ids', () => {
    expect(mentionUserIdsFromRaw({ mention_user_ids: ['u1', 'u2'] })).toEqual(['u1', 'u2']);
  });

  it('falls back to metadata.mention_user_ids', () => {
    expect(mentionUserIdsFromRaw({
      metadata: { mention_user_ids: ['u9'] },
    })).toEqual(['u9']);
  });

  it('prefers top-level over metadata', () => {
    expect(mentionUserIdsFromRaw({
      mention_user_ids: ['u1'],
      metadata: { mention_user_ids: ['u9'] },
    })).toEqual(['u1']);
  });

  it('strips blanks and non-strings', () => {
    expect(mentionUserIdsFromRaw({
      mention_user_ids: ['u1', '', '  ', 'u2', 3, null],
    })).toEqual(['u1', 'u2']);
  });

  it('returns [] when neither source is an array', () => {
    expect(mentionUserIdsFromRaw({ content: 'hi' })).toEqual([]);
    expect(mentionUserIdsFromRaw({ mention_user_ids: 'u1', metadata: {} })).toEqual([]);
  });
});
