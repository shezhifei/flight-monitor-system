export type ChatMentionSegment = {
  type: 'text' | 'mention';
  value: string;
};

/** `@全体`, `@all` (any case), then `@` + non-whitespace / non-`@` usernames. */
const MENTION_TOKEN = /@全体|@all(?=$|[\s@])|@[^\s@]+/gi;

export function splitChatMentionSegments(content: string): ChatMentionSegment[] {
  const text = String(content ?? '');
  if (!text) return [];

  const segments: ChatMentionSegment[] = [];
  const re = new RegExp(MENTION_TOKEN.source, MENTION_TOKEN.flags);
  let lastIndex = 0;
  let match = re.exec(text);
  while (match) {
    if (match.index > lastIndex) {
      segments.push({ type: 'text', value: text.slice(lastIndex, match.index) });
    }
    segments.push({ type: 'mention', value: match[0] });
    lastIndex = match.index + match[0].length;
    if (match[0].length === 0) {
      re.lastIndex += 1;
    }
    match = re.exec(text);
  }
  if (lastIndex < text.length) {
    segments.push({ type: 'text', value: text.slice(lastIndex) });
  }
  return segments;
}
