import { authFetch } from '@/lib/auth/authBridge';
import { requestEnvelope } from '@/lib/http/apiClient';
import { consumeSSEBody, safeJson } from '@/lib/sse/streamParser';
import type { NLQueryResult } from '@/lib/types/apiModels';

const BASE = '/api/v2/ai/nl-query';

export async function listSuggestions(): Promise<Array<{ label?: string; text?: string }>> {
  const payload = await requestEnvelope<{ suggestions: Array<{ label?: string; text?: string }> }>(`${BASE}/suggestions`);
  return payload.suggestions || [];
}

export async function listConversations(limit = 50, offset = 0): Promise<Array<Record<string, unknown>>> {
  const payload = await requestEnvelope<{ items: Array<Record<string, unknown>> }>(
    `${BASE}/conversations?limit=${encodeURIComponent(String(limit))}&offset=${encodeURIComponent(String(offset))}`,
  );
  return payload.items || [];
}

export async function listConversationMessages(
  conversationId: string,
  limit = 50,
  offset = 0,
  order: 'asc' | 'desc' = 'desc',
): Promise<Array<Record<string, unknown>>> {
  const payload = await requestEnvelope<{ items: Array<Record<string, unknown>> }>(
    `${BASE}/conversations/${encodeURIComponent(conversationId)}/messages?limit=${encodeURIComponent(String(limit))}&offset=${encodeURIComponent(String(offset))}&order=${encodeURIComponent(order)}`,
  );
  return payload.items || [];
}

export async function deleteConversation(conversationId: string): Promise<void> {
  await requestEnvelope<Record<string, unknown>>(`${BASE}/${encodeURIComponent(conversationId)}`, {
    method: 'DELETE',
  });
}

interface StreamRequestPayload {
  question: string;
  conversation_id?: string;
  request_id: string;
  context?: Record<string, unknown>;
}

export async function streamQuery(
  payload: StreamRequestPayload,
  onEvent: (eventName: string, data: Record<string, unknown>) => void,
): Promise<NLQueryResult> {
  const endpoint = payload.conversation_id ? `${BASE}/followup/stream` : `${BASE}/stream`;
  const response = await authFetch(endpoint, {
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

  let finalResult: NLQueryResult | null = null;
  let streamError = '';
  await consumeSSEBody(response, (event) => {
    const parsed = safeJson<Record<string, unknown>>(event.data) || {};
    if (event.event === 'final_result') {
      finalResult = parsed as unknown as NLQueryResult;
      return;
    }
    if (event.event === 'error') {
      streamError = String(parsed.message || parsed.detail || '流式查询失败');
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
  throw new Error('流式查询未返回最终结果');
}
