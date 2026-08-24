export type ChatNotificationTarget = {
  flightId: string | null;
  groupId: string | null;
};

function trimToNull(value?: string | null): string | null {
  const trimmed = String(value ?? '').trim();
  return trimmed ? trimmed : null;
}

export function chatTargetFromNotification(n: {
  category?: string | null;
  flight_id?: string | null;
  group_id?: string | null;
  related_entity_type?: string | null;
  related_entity_id?: string | null;
}): ChatNotificationTarget | null {
  const category = String(n.category ?? '').trim();
  if (category !== 'dispatch_chat_mention') {
    return null;
  }

  const relatedType = String(n.related_entity_type ?? '').trim();
  const relatedId = trimToNull(n.related_entity_id);
  const fallbackGroupId = trimToNull(n.group_id);
  const groupId = relatedType === 'dispatch_chat_group'
    ? (relatedId ?? fallbackGroupId)
    : fallbackGroupId;
  const flightId = trimToNull(n.flight_id);

  if (!flightId && !groupId) {
    return null;
  }
  return { flightId, groupId };
}
