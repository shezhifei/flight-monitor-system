import { authFetch } from '@/lib/auth/authBridge';
import { requestEnvelope } from '@/lib/http/apiClient';
import { consumeSSEBody, safeJson } from '@/lib/sse/streamParser';

const BASE = '/api/v2/workflows/definitions';

export async function streamFlowableAssistant(
  payload: {
    message: string;
    mode: 'contextual' | 'general';
    request_id: string;
    context?: Record<string, unknown>;
  },
  onEvent: (eventName: string, data: Record<string, unknown>) => void,
): Promise<Record<string, unknown>> {
  const response = await authFetch(`${BASE}/drafts/assistant-chat/stream`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Accept: 'text/event-stream',
    },
    body: JSON.stringify(payload),
  });

  if (!response.ok) {
    const text = await response.text().catch(() => '');
    throw new Error(text || `HTTP ${response.status}`);
  }

  let finalResult: Record<string, unknown> | null = null;
  let streamError = '';
  await consumeSSEBody(response, (event) => {
    const parsed = safeJson<Record<string, unknown>>(event.data) || {};
    if (event.event === 'final_result') {
      finalResult = parsed;
      return;
    }
    if (event.event === 'error') {
      streamError = String(parsed.message || parsed.detail || '流程助手流式请求失败');
      return;
    }
    onEvent(event.event, parsed);
  });

  if (finalResult) {
    return finalResult;
  }
  if (streamError) {
    throw new Error(streamError);
  }
  throw new Error('流程助手未返回最终结果');
}

export async function createDraftFromFile(form: FormData): Promise<Record<string, unknown>> {
  return requestEnvelope<Record<string, unknown>>(`${BASE}/drafts/from-file`, {
    method: 'POST',
    body: form,
  });
}
