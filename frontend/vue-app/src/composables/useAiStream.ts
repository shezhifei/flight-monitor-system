import { ref } from 'vue';
import { useAuth } from './useAuth';
import { useToast } from './useToast';

export interface AiStreamMessage {
  type: 'ai_suggestion' | 'ai_tool_call' | 'ai_complete' | 'ai_error' | 'text' | 'done';
  content?: string;
  data?: Record<string, unknown>;
  timestamp: string;
}

export function useAiStream() {
  const auth = useAuth();
  const toast = useToast();
  const isConnected = ref(false);
  const messages = ref<AiStreamMessage[]>([]);
  const isStreaming = ref(false);
  const error = ref('');
  let eventSource: EventSource | null = null;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let reconnectAttempts = 0;
  let lastTopics: string[] = ['ai_execution', 'smart_monitor'];
  let manuallyStopped = false;

  function appendMessage(rawData: string, fallbackType = 'text') {
    try {
      const data = JSON.parse(rawData);
      messages.value.push({
        type: data.type || fallbackType,
        content: data.content || data.message || data.summary || '',
        data: data.data || data,
        timestamp: data.timestamp || new Date().toISOString(),
      });
    } catch {
      messages.value.push({
        type: fallbackType as AiStreamMessage['type'],
        content: rawData,
        timestamp: new Date().toISOString(),
      });
    }
  }

  function startStream(topics: string[] = ['ai_execution', 'smart_monitor']) {
    if (isStreaming.value) return;
    lastTopics = topics;
    manuallyStopped = false;
    error.value = '';
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }

    const topicParam = topics.join(',');
    const url = `/api/v2/sse/stream?topics=${encodeURIComponent(topicParam)}`;

    try {
      eventSource = auth.getEventSource(url, { clientScope: 'ai_assistant' });

      eventSource.onopen = () => {
        isConnected.value = true;
        isStreaming.value = true;
        reconnectAttempts = 0;
        error.value = '';
      };

      eventSource.onmessage = (event: MessageEvent) => {
        appendMessage(event.data, 'text');
      };

      topics.forEach((topic) => {
        eventSource?.addEventListener(topic, (event: MessageEvent) => {
          appendMessage(event.data, topic === 'ai_execution' ? 'ai_tool_call' : 'text');
        });
      });

      eventSource.onerror = () => {
        isConnected.value = false;
        isStreaming.value = false;
        error.value = 'AI 实时连接已断开，正在尝试重连。';
        toast.show('warning', error.value);
        eventSource?.close();
        eventSource = null;
        if (!manuallyStopped) {
          reconnectAttempts += 1;
          const delay = Math.min(3000 * 2 ** Math.max(0, reconnectAttempts - 1), 30000);
          reconnectTimer = setTimeout(() => {
            reconnectTimer = null;
            startStream(lastTopics);
          }, delay + Math.floor(Math.random() * 1000));
        }
      };
    } catch (err) {
      console.error('AI stream connection error:', err);
      isConnected.value = false;
      error.value = err instanceof Error ? err.message : 'AI 实时连接失败';
      toast.showToast('error', error.value);
    }
  }

  function stopStream() {
    manuallyStopped = true;
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    if (eventSource) {
      eventSource.close();
      eventSource = null;
    }
    isConnected.value = false;
    isStreaming.value = false;
  }

  function clearMessages() {
    messages.value = [];
  }

  return {
    isConnected,
    messages,
    isStreaming,
    error,
    startStream,
    stopStream,
    clearMessages,
  };
}
