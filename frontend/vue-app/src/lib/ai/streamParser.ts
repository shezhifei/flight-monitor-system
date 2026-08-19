// 搬运自 frontend/ai-react/src/lib/sse/streamParser.ts（无改动，零 React 依赖）。
export interface ParsedSSEEvent {
  event: string;
  data: string;
}

export function parseEventBlock(block: string): ParsedSSEEvent | null {
  if (!block) {
    return null;
  }
  const lines = block.split(/\r?\n/);
  let event = 'message';
  const dataLines: string[] = [];

  lines.forEach((line) => {
    if (!line) {
      return;
    }
    if (line.startsWith('event:')) {
      const parsed = line.slice(6).trim();
      event = parsed || 'message';
      return;
    }
    if (line.startsWith('data:')) {
      dataLines.push(line.slice(5).trimStart());
    }
  });

  if (dataLines.length === 0) {
    return null;
  }
  return {
    event,
    data: dataLines.join('\n'),
  };
}

export async function consumeSSEBody(
  response: Response,
  onEvent: (event: ParsedSSEEvent) => void,
): Promise<void> {
  if (!response.body || typeof response.body.getReader !== 'function') {
    throw new Error('浏览器不支持流式读取');
  }

  const reader = response.body.getReader();
  const decoder = new TextDecoder('utf-8');
  let buffer = '';

  while (true) {
    const { value, done } = await reader.read();
    if (done) {
      break;
    }
    buffer += decoder.decode(value, { stream: true });
    buffer = buffer.replace(/\r\n/g, '\n');
    let boundary = buffer.indexOf('\n\n');
    while (boundary !== -1) {
      const block = buffer.slice(0, boundary);
      buffer = buffer.slice(boundary + 2);
      const parsed = parseEventBlock(block);
      if (parsed) {
        onEvent(parsed);
      }
      boundary = buffer.indexOf('\n\n');
    }
  }

  buffer += decoder.decode();
  const trailing = parseEventBlock(buffer.trim());
  if (trailing) {
    onEvent(trailing);
  }
}

export function safeJson<T>(raw: string): T | null {
  if (!raw) {
    return null;
  }
  try {
    return JSON.parse(raw) as T;
  } catch (_error) {
    return null;
  }
}
