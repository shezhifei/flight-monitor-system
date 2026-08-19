// 搬运自 frontend/ai-react/src/features/nl-query/toolTimeline.ts（无逻辑改动）。
export interface ToolTimelineItem {
  id: string;
  toolName: string;
  status: string;
  message?: string;
  time?: string;
  blockedBy?: string;
  rule?: string;
  detail?: string;
}

export function upsertToolTimelineEntry(
  items: ToolTimelineItem[],
  nextItem: ToolTimelineItem,
): ToolTimelineItem[] {
  const lastIndex = items.length - 1;
  if (lastIndex < 0) {
    return [nextItem];
  }

  const lastItem = items[lastIndex];
  if (
    lastItem.toolName === nextItem.toolName &&
    lastItem.status === nextItem.status &&
    String(lastItem.message || '') === String(nextItem.message || '')
  ) {
    const updated = [...items];
    updated[lastIndex] = {
      ...lastItem,
      time: nextItem.time || lastItem.time,
    };
    return updated;
  }

  return [...items.slice(-79), nextItem];
}
