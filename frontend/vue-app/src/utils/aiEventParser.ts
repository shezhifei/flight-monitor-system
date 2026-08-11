export interface SSEEvent {
  event?: string;
  data?: string;
  id?: string;
  retry?: string;
}

export function parseEventBlock(block: string): SSEEvent {
  const result: SSEEvent = {};
  for (const line of block.split('\n')) {
    if (line.startsWith('event:')) {
      result.event = line.slice(6).trim();
    } else if (line.startsWith('data:')) {
      result.data = (result.data ? result.data + '\n' : '') + line.slice(5).trim();
    } else if (line.startsWith('id:')) {
      result.id = line.slice(3).trim();
    } else if (line.startsWith('retry:')) {
      result.retry = line.slice(6).trim();
    }
  }
  return result;
}

export function safeJson<T>(text: string | undefined): T | null {
  if (!text) return null;
  try {
    return JSON.parse(text) as T;
  } catch {
    return null;
  }
}

export async function consumeSSEBody(
  reader: ReadableStreamDefaultReader<Uint8Array>,
  onEvent: (name: string, data: unknown) => void,
  signal?: AbortSignal,
): Promise<void> {
  const decoder = new TextDecoder();
  let buffer = '';

  while (true) {
    if (signal?.aborted) break;
    const { done, value } = await reader.read();
    if (done) break;

    buffer += decoder.decode(value, { stream: true });
    const blocks = buffer.split('\n\n');
    buffer = blocks.pop() || '';

    for (const block of blocks) {
      if (!block.trim()) continue;
      const parsed = parseEventBlock(block);
      const eventName = parsed.event || 'message';
      const payload = safeJson(parsed.data) ?? parsed.data;
      onEvent(eventName, payload);
    }
  }
}