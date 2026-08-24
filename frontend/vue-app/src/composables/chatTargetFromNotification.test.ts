// @vitest-environment node
import { describe, expect, it } from 'vitest';
import { chatTargetFromNotification } from './chatTargetFromNotification';

describe('chatTargetFromNotification', () => {
  it('returns flight and group for a mention with related dispatch chat group', () => {
    expect(chatTargetFromNotification({
      category: 'dispatch_chat_mention',
      flight_id: 'flight-1',
      related_entity_type: 'dispatch_chat_group',
      related_entity_id: 'group-1',
    })).toEqual({ flightId: 'flight-1', groupId: 'group-1' });
  });

  it('returns only groupId when the mention has a related group and no flight', () => {
    expect(chatTargetFromNotification({
      category: 'dispatch_chat_mention',
      related_entity_type: 'dispatch_chat_group',
      related_entity_id: 'group-only',
    })).toEqual({ flightId: null, groupId: 'group-only' });
  });

  it('returns null for other categories', () => {
    expect(chatTargetFromNotification({
      category: 'business_case_mention',
      flight_id: 'flight-1',
      related_entity_type: 'dispatch_chat_group',
      related_entity_id: 'group-1',
    })).toBeNull();
  });

  it('returns null when a mention has no ids', () => {
    expect(chatTargetFromNotification({
      category: 'dispatch_chat_mention',
    })).toBeNull();
  });

  it('treats blank strings as empty', () => {
    expect(chatTargetFromNotification({
      category: '  dispatch_chat_mention  ',
      flight_id: '   ',
      group_id: '',
      related_entity_type: 'dispatch_chat_group',
      related_entity_id: '  ',
    })).toBeNull();
  });

  it('falls back to group_id when related type matches but related id is blank', () => {
    expect(chatTargetFromNotification({
      category: 'dispatch_chat_mention',
      related_entity_type: 'dispatch_chat_group',
      related_entity_id: '  ',
      group_id: 'group-fallback',
    })).toEqual({ flightId: null, groupId: 'group-fallback' });
  });

  it('uses group_id when related_entity_type is not a dispatch chat group', () => {
    expect(chatTargetFromNotification({
      category: 'dispatch_chat_mention',
      flight_id: 'flight-2',
      group_id: 'group-2',
      related_entity_type: 'flight',
      related_entity_id: 'not-a-group',
    })).toEqual({ flightId: 'flight-2', groupId: 'group-2' });
  });
});
