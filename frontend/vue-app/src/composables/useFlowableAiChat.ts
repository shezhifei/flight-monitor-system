import { ref } from 'vue';
import { useApi } from './useApi';
import { consumeSSEBody } from '@/utils/aiEventParser';
import { usePendingActions } from './usePendingActions';

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: number;
  mode?: string;
  model?: string;
}

export interface ToolItem {
  id: string;
  toolName: string;
  status: string;
  message?: string;
}

export function useFlowableAiChat() {
  const api = useApi();
  const { actions: pendingActions, addAction, removeAction, approve, reject, fetchPending } = usePendingActions();

  const messages = ref<ChatMessage[]>([]);
  const sending = ref(false);
  const input = ref('');
  const mode = ref<'contextual' | 'general'>('contextual');
  const toolItems = ref<ToolItem[]>([]);
  const abortController = ref<AbortController | null>(null);

  function addMessage(msg: ChatMessage) {
    messages.value.push(msg);
  }

  function clearMessages() {
    messages.value = [];
    toolItems.value = [];
  }

  async function sendMessage(content: string, bpmnContext?: string) {
    if (!content.trim() || sending.value) return;

    const userMsg: ChatMessage = {
      id: `user_${Date.now()}`,
      role: 'user',
      content: content.trim(),
      timestamp: Date.now(),
      mode: mode.value,
    };
    addMessage(userMsg);
    input.value = '';
    sending.value = true;

    const requestId = `flowable_${Date.now()}`;
    const assistantMsg: ChatMessage = {
      id: `assistant_${requestId}`,
      role: 'assistant',
      content: '',
      timestamp: Date.now(),
      mode: mode.value,
    };
    addMessage(assistantMsg);

    try {
      abortController.value = new AbortController();
      const response = await api.raw('/api/v2/workflows/definitions/drafts/assistant-chat/stream', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          message: content.trim(),
          mode: mode.value,
          request_id: requestId,
          context: bpmnContext ? { bpmn_xml: bpmnContext } : undefined,
        }),
        signal: abortController.value.signal,
      });

      if (!response.ok) {
        assistantMsg.content = `请求失败 (${response.status})`;
        return;
      }

      const reader = response.body?.getReader();
      if (!reader) return;

      await consumeSSEBody(reader, (eventName, payload) => {
        const data = payload as Record<string, unknown>;
        switch (eventName) {
          case 'text_delta':
            assistantMsg.content += String(data.delta || '');
            break;
          case 'progress':
            break;
          case 'tool_executed':
          case 'tool_started':
            toolItems.value.push({
              id: String(data.tool_call_id || `tool_${Date.now()}`),
              toolName: String(data.tool_name || 'unknown'),
              status: String(data.status || 'running'),
              message: data.message as string | undefined,
            });
            break;
          case 'approval_required':
            addAction({
              actionId: String((data.pending_action as Record<string, unknown> | undefined)?.action_id || `pa_${Date.now()}`),
              toolName: String(data.tool_name || (data.pending_action as Record<string, unknown> | undefined)?.tool_name || 'unknown'),
              status: 'pending',
              message: data.message as string | undefined,
            });
            break;
          case 'approval_result':
            if ((data.pending_action as Record<string, unknown> | undefined)?.action_id) {
              removeAction(String((data.pending_action as Record<string, unknown>)?.action_id));
            }
            break;
          case 'done':
            assistantMsg.model = data.model as string | undefined;
            break;
          case 'error':
            assistantMsg.content += `\n\n**错误:** ${String(data.message || '未知错误')}`;
            break;
          case 'final_result':
            if (data.summary) {
              assistantMsg.content = String(data.summary);
            }
            break;
        }
      }, abortController.value.signal);
    } catch (err: unknown) {
      const error = err as { name?: string; message?: string };
      if (error.name !== 'AbortError') {
        assistantMsg.content = `连接错误: ${error.message}`;
      }
    } finally {
      sending.value = false;
      abortController.value = null;
    }
  }

  function cancelStream() {
    abortController.value?.abort();
    sending.value = false;
  }

  return {
    messages,
    sending,
    input,
    mode,
    toolItems,
    pendingActions,
    sendMessage,
    cancelStream,
    clearMessages,
    addMessage,
    approve,
    reject,
    fetchPending,
  };
}