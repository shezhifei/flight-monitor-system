<template>
  <div class="ai-assistant-panel" :class="{ 'is-open': isOpen }">
    <div v-if="!isOpen" class="panel-toggle" @click="togglePanel">
      <div v-if="unreadCount > 0" class="toggle-badge">
        {{ unreadCount }}
      </div>
      <span class="icon">💬</span>
      <span class="text">极智 AI 指挥官</span>
    </div>
    
    <div v-else class="panel-container">
      <div class="panel-header">
        <div class="header-info">
          <h3>Smart AI Assistant</h3>
          <span class="status-indicator" :class="{ 'online': isConnected }">
            {{ isConnected ? '实时桥接中' : '离线' }}
          </span>
        </div>
        <div class="header-actions">
          <div v-if="showMenu" class="dropdown-wrapper">
            <div class="dropdown-menu">
              <button class="menu-item" @click="clearHistory">
                <span class="menu-icon">🧹</span> 清空会话
              </button>
              <button class="menu-item" @click="endSession">
                <span class="menu-icon">🏁</span> 结束会话
              </button>
              <div class="menu-divider" />
              <button class="menu-item" @click="exportHistory('json')">
                <span class="menu-icon">📄</span> 导出 JSON
              </button>
              <button class="menu-item" @click="exportHistory('md')">
                <span class="menu-icon">📝</span> 导出 Markdown
              </button>
            </div>
          </div>
          <button class="action-icon-btn" title="更多操作" @click="showMenu = !showMenu">
            ⋮
          </button>
          <button class="close-btn" @click="togglePanel">
            ✕
          </button>
        </div>
      </div>
      
      <div ref="chatBody" class="panel-body">
        <div v-for="(msg, index) in chatHistory" :key="index" :class="['chat-message', msg.role]">
          <div class="message-content">
            <template v-if="msg.type === 'ai_suggestion' && msg.data">
              <div class="suggestion-card">
                <div class="suggestion-header">
                  💡 AI 诊断建议
                </div>
                <div class="suggestion-body" v-html="renderMarkdown(msg.content)" />
                <div v-if="msg.data.actions" class="suggestion-actions">
                  <button
                    v-for="(action, aIdx) in (msg.data.actions as Record<string, unknown>[])"
                    :key="aIdx"
                    class="action-btn-sm"
                    @click="handleAction(action)"
                  >
                    {{ action.label }}
                  </button>
                </div>
              </div>
            </template>
            <template v-else>
              <div v-html="renderMarkdown(msg.content)" />
              <AIVisualization v-if="msg.type || msg.data" :type="msg.type" :data="msg.data" />
            </template>
          </div>
          <div class="message-time">
            {{ msg.time }}
          </div>
        </div>
        <div v-if="isSending" class="chat-message assistant loading">
          <div class="typing-indicator">
            <span /><span /><span />
          </div>
        </div>
      </div>
      
      <div class="panel-footer">
        <div v-if="effectiveSelectedFlightNo" class="context-pill">
          Context: {{ effectiveSelectedFlightNo }}
          <button @click="clearContext">
            ✕
          </button>
        </div>
        <div class="input-wrapper">
          <input 
            v-model="inputText" 
            type="text" 
            :placeholder="effectiveSelectedFlightNo ? `针对 ${effectiveSelectedFlightNo} 提问...` : '咨询 AI 指挥官...'" 
            :disabled="isSending"
            @keyup.enter="sendMessage"
          >
          <button :disabled="!inputText.trim() || isSending" class="send-btn" @click="sendMessage">
            <svg
              width="16"
              height="16"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            ><line
              x1="22"
              y1="2"
              x2="11"
              y2="13"
            /><polygon points="22 2 15 22 11 13 2 9 22 2" /></svg>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, nextTick, watch, onMounted, onUnmounted } from 'vue';
import { useAuth } from '../../composables/useAuth';
import { useApi } from '../../composables/useApi';
import { useAiStream } from '../../composables/useAiStream';
import { renderMarkdown } from '../../lib/marked';
import { downloadTextFile } from '../../lib/download';
import { useToast } from '../../composables/useToast';
import AIVisualization from './AIVisualization.vue';

const props = defineProps<{
  selectedFlightId?: string | null;
  selectedFlightNo?: string | null;
}>();

const auth = useAuth();
const api = useApi();
const toast = useToast();
const { isConnected, messages, startStream, stopStream, clearMessages } = useAiStream();

interface ChatMessage {
  role: 'user' | 'assistant' | 'system';
  content: string;
  time: string;
  type?: string;
  data?: Record<string, unknown>;
}

const isOpen = ref(false);
const showMenu = ref(false);
const unreadCount = ref(0);
const inputText = ref('');
const isSending = ref(false);
const chatBody = ref<HTMLElement | null>(null);
const conversationId = ref<string | null>(null);
const contextDismissed = ref(false);

const effectiveSelectedFlightId = computed(() => contextDismissed.value ? null : props.selectedFlightId);
const effectiveSelectedFlightNo = computed(() => contextDismissed.value ? null : props.selectedFlightNo);

const DEFAULT_WELCOME = '你好！我是您的极智 AI 指挥官。针对当前航班编排或保障流程有任何需要推演的吗？';

const chatHistory = ref<ChatMessage[]>([
  {
    role: 'assistant',
    content: DEFAULT_WELCOME,
    time: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
  }
]);

const togglePanel = () => {
  isOpen.value = !isOpen.value;
  if (isOpen.value) {
    unreadCount.value = 0;
    scrollToBottom();
  } else {
    showMenu.value = false;
  }
};

const scrollToBottom = async () => {
  await nextTick();
  if (chatBody.value) {
    chatBody.value.scrollTop = chatBody.value.scrollHeight;
  }
};

const unwrapEnvelope = <T,>(payload: unknown): T | null => {
  if (!payload || typeof payload !== 'object') return null;
  const record = payload as Record<string, unknown>;
  return ('data' in record ? record.data ?? null : payload) as T | null;
};

const buildNlContext = () => ({
  source_page: 'flight_monitor',
  selected_flight_id: effectiveSelectedFlightId.value || undefined,
  selected_flight_no: effectiveSelectedFlightNo.value || undefined,
  scope_mode: 'selected_or_global',
});

let currentRequestId: string | null = null;

const sendMessage = async () => {
  const text = inputText.value.trim();
  if (!text || isSending.value) return;

  chatHistory.value.push({
    role: 'user',
    content: text,
    time: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
  });
  
  inputText.value = '';
  isSending.value = true;
  await scrollToBottom();

  try {
    const endpoint = `${auth.apiBase.value}/ai/nl-query`;
      
    currentRequestId = `req-${Date.now()}-${Math.floor(Math.random() * 10000)}`;
    const requestConversationId = conversationId.value || `conversation-${currentRequestId}`;

    const response = await api.raw(endpoint, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        question: text,
        conversation_id: requestConversationId,
        context: buildNlContext(),
        request_id: currentRequestId,
      })
    });
    
    if (!response.ok) {
      throw new Error(`AI Gateway Error: ${response.status}`);
    }
    
    const payload = unwrapEnvelope<{ answer?: string; summary?: string; conversation_id?: string; structured_data?: Record<string, unknown>; visualization_hint?: Record<string, unknown> }>(await response.json());
    conversationId.value = String(payload?.conversation_id || requestConversationId);
    
    chatHistory.value.push({
      role: 'assistant',
      content: payload?.answer || payload?.summary || '查询完成。',
      data: payload?.structured_data,
      type: (payload?.visualization_hint as { type?: string } | undefined)?.type,
      time: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
    });
  } catch (error: unknown) {
    chatHistory.value.push({
      role: 'system',
      content: 'AI 响应失败: ' + (error as { message?: string })?.message,
      time: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
    });
  } finally {
    isSending.value = false;
    currentRequestId = null;
    await scrollToBottom();
  }
};

const triggerDiagnosis = async (flightId: string, flightNo: string) => {
  if (!isOpen.value) isOpen.value = true;
  contextDismissed.value = false;
  
  chatHistory.value.push({
    role: 'system',
    content: `正在对航班 **${flightNo}** 进行深度诊断...`,
    time: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
  });
  
  await scrollToBottom();
  
  try {
    const response = await api.raw(`${auth.apiBase.value}/ai/tools/execute`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        tool_name: 'get_handling_recommendation',
        tool_args: { flight_id: flightId, context: 'flight_diagnosis' },
      })
    });
    
    if (!response.ok) throw new Error('Diagnosis Request Failed');
    const payload = unwrapEnvelope<Record<string, unknown>>(await response.json());
    const resultData = (payload?.result_data || {}) as Record<string, unknown>;
    
    chatHistory.value.push({
      role: 'assistant',
      content: String(resultData.summary || resultData.output || '诊断完成。'),
      time: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
    });
  } catch (err: unknown) {
    chatHistory.value.push({
      role: 'system',
      content: '诊断请求失败: ' + (err as { message?: string }).message,
      time: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
    });
  }
};

const clearHistory = () => {
  chatHistory.value = [
    {
      role: 'assistant',
      content: DEFAULT_WELCOME,
      time: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
    }
  ];
  conversationId.value = null;
  clearMessages();
  showMenu.value = false;
};

const endSession = async () => {
  const currentConversationId = conversationId.value;
  conversationId.value = null;
  if (currentConversationId) {
    try {
      await api.raw(`${auth.apiBase.value}/ai/nl-query/${encodeURIComponent(currentConversationId)}`, {
        method: 'DELETE',
      });
    } catch {
      // Ignore cleanup error
    }
  }
  chatHistory.value.push({
    role: 'system',
    content: '当前会话已结束。所有上下文已清空。',
    time: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
  });
  inputText.value = '';
  showMenu.value = false;
  await scrollToBottom();
};

const exportHistory = (format: 'json' | 'md') => {
  let content = '';
  let filename = `AI_Chat_Export_${new Date().getTime()}`;
  let mimeType = 'text/markdown;charset=utf-8';
  
  if (format === 'json') {
    content = JSON.stringify(chatHistory.value, null, 2);
    filename += '.json';
    mimeType = 'application/json;charset=utf-8';
  } else {
    content = chatHistory.value.map(m => {
      const role = m.role === 'user' ? '用户' : (m.role === 'assistant' ? 'AI 指挥官' : '系统');
      return `### [${m.time}] ${role}\n\n${m.content}\n\n---`;
    }).join('\n\n');
    filename += '.md';
  }
  
  try {
    downloadTextFile({ content, filename, mimeType });
    toast.showToast('success', 'AI 会话已导出', { duration: 3200 });
    showMenu.value = false;
  } catch (error) {
    toast.showToast('error', `导出失败: ${error instanceof Error ? error.message : String(error)}`, { duration: 5000 });
  }
};

const handleAction = (action: Record<string, unknown>) => {
  if (action.type === 'input') {
    inputText.value = String(action.value || action.label);
    sendMessage();
  }
};

const clearContext = () => {
  inputText.value = '';
  conversationId.value = null;
  currentRequestId = null;
  contextDismissed.value = true;
  chatHistory.value.push({
    role: 'system',
    content: '已清除当前航班上下文。后续提问将按全局航班监控视角处理。',
    time: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
  });
  scrollToBottom();
};

watch(
  () => [props.selectedFlightId, props.selectedFlightNo],
  () => {
    contextDismissed.value = false;
  },
);

defineExpose({
  triggerDiagnosis,
  open: () => { isOpen.value = true; }
});

// Watch for SSE messages
watch(() => messages.value.length, (newLen, oldLen) => {
  if (newLen > (oldLen || 0)) {
    const latest = messages.value[newLen - 1];
    
    // Process stream events to build legacy-style visual logs
    const eventName = String(latest.type || 'message').trim().toLowerCase();
    const payloadObj = latest.data || {};
    
    if (['connected', 'completed', 'heartbeat', 'final_result'].includes(eventName)) return;
    
    // Only accept events from the current request or flight_monitor scene
    if (payloadObj.request_id && payloadObj.request_id !== currentRequestId) {
      return;
    }
    if (payloadObj.scene && payloadObj.scene !== 'flight_monitor') {
      return;
    }
    
    chatHistory.value.push({
      role: 'system',
      content: `> [${eventName}] ${payloadObj.summary || payloadObj.message || payloadObj.tool_name || '处理中...'}`,
      time: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
    });
    
    if (!isOpen.value) {
      unreadCount.value++;
    }
    scrollToBottom();
  }
});

onMounted(() => {
  startStream(['ai_execution', 'smart_monitor']);
});

onUnmounted(() => {
  stopStream();
});
</script>

<style scoped>
.ai-assistant-panel {
  position: fixed;
  bottom: 24px;
  right: 170px;
  z-index: 9999;
  font-family: 'MiSans', -apple-system, BlinkMacSystemFont, sans-serif;
}

.panel-toggle {
  background-color: var(--bg-card, var(--admin-card-bg));
  border: 1px solid var(--system-blue, #007AFF);
  color: var(--text-primary, #1D1D1F);
  padding: 10px 20px;
  border-radius: 20px;
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 8px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  transition: transform 0.2s, background-color 0.2s;
  backdrop-filter: blur(8px);
  position: relative;
}

.toggle-badge {
  position: absolute;
  top: -8px;
  right: -8px;
  background: var(--system-red, #FF3B30);
  color: white;
  font-size: 10px;
  font-weight: bold;
  padding: 2px 6px;
  border-radius: 10px;
  border: 2px solid white;
}

.panel-toggle:hover {
  background-color: var(--bg-input, #F0F0F0);
  transform: translateY(-2px);
}

.panel-container {
  width: 400px;
  height: 600px;
  background: var(--bg-app, #F5F5F7);
  border-radius: 16px;
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.25);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
}

.panel-header {
  background-color: var(--bg-card, var(--admin-card-bg));
  padding: 14px 16px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  border-bottom: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
}

.header-info h3 {
  margin: 0;
  font-size: 15px;
  font-weight: 700;
  color: var(--system-blue, #007AFF);
}

.status-indicator {
  font-size: 10px;
  color: var(--text-tertiary, var(--admin-text-muted));
  display: flex;
  align-items: center;
  gap: 4px;
}

.status-indicator::before {
  content: '';
  display: inline-block;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background-color: var(--admin-text-muted);
}

.status-indicator.online::before {
  background-color: var(--system-green);
  box-shadow: 0 0 4px var(--system-green);
}

.header-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  position: relative;
}

.action-icon-btn {
  background: none;
  border: none;
  color: var(--text-secondary, #546E7A);
  font-size: 18px;
  cursor: pointer;
  padding: 0 4px;
}

.dropdown-wrapper {
  position: absolute;
  top: 30px;
  right: 0;
  z-index: 10;
}

.dropdown-menu {
  background: white;
  border: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
  border-radius: 8px;
  box-shadow: 0 4px 12px rgba(0,0,0,0.1);
  padding: 4px;
  min-width: 140px;
}

.menu-item {
  width: 100%;
  text-align: left;
  padding: 8px 12px;
  border: none;
  background: none;
  font-size: 13px;
  color: var(--text-primary, #1D1D1F);
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 8px;
  border-radius: 4px;
}

.menu-item:hover {
  background: var(--bg-app, #F5F5F7);
}

.menu-icon {
  font-size: 14px;
}

.menu-divider {
  height: 1px;
  background: var(--border-light, #E5E5EA);
  margin: 4px 0;
}

.close-btn {
  background: none;
  border: none;
  color: var(--text-secondary, #546E7A);
  font-size: 18px;
  cursor: pointer;
}

.panel-body {
  flex: 1;
  padding: 16px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 16px;
  background-color: var(--bg-app, #F5F5F7);
}

.chat-message {
  max-width: 90%;
  padding: 10px 14px;
  border-radius: 12px;
  font-size: 13px;
  line-height: 1.5;
  word-wrap: break-word;
  position: relative;
}

.chat-message.user {
  align-self: flex-end;
  background-color: var(--system-blue, #007AFF);
  color: var(--admin-card-bg);
  border-bottom-right-radius: 2px;
}

.chat-message.assistant {
  align-self: flex-start;
  background-color: var(--bg-card, var(--admin-card-bg));
  color: var(--text-primary, #1D1D1F);
  border: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
  border-bottom-left-radius: 2px;
}

.chat-message.system {
  align-self: center;
  background-color: rgba(0,0,0,0.05);
  color: var(--text-secondary, var(--admin-text-muted));
  font-size: 11px;
  max-width: 100%;
  text-align: center;
  padding: 4px 12px;
  border-radius: 999px;
}

.message-content :deep(p) {
  margin: 0 0 8px 0;
}
.message-content :deep(p:last-child) {
  margin-bottom: 0;
}
.message-content :deep(strong) {
  font-weight: 700;
}
.message-content :deep(code) {
  background: rgba(0,0,0,0.05);
  padding: 2px 4px;
  border-radius: 3px;
  font-family: monospace;
}

.typing-indicator {
  display: flex;
  gap: 4px;
  padding: 4px 0;
}

.typing-indicator span {
  width: 6px;
  height: 6px;
  background: var(--admin-text-muted);
  border-radius: 50%;
  animation: typing 1.4s infinite ease-in-out;
}

.typing-indicator span:nth-child(2) { animation-delay: 0.2s; }
.typing-indicator span:nth-child(3) { animation-delay: 0.4s; }

@keyframes typing {
  0%, 60%, 100% { transform: translateY(0); opacity: 0.4; }
  30% { transform: translateY(-4px); opacity: 1; }
}

.suggestion-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.suggestion-header {
  font-weight: 700;
  color: var(--system-orange, #FF9500);
  font-size: 12px;
  border-bottom: 1px solid rgba(255, 149, 0, 0.2);
  padding-bottom: 4px;
}

.suggestion-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 4px;
}

.action-btn-sm {
  background: var(--bg-app, #F5F5F7);
  border: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
  border-radius: 4px;
  padding: 4px 8px;
  font-size: 11px;
  cursor: pointer;
}

.action-btn-sm:hover {
  background: var(--system-blue-subtle);
  border-color: var(--system-blue);
  color: var(--system-blue);
}

.message-time {
  font-size: 9px;
  margin-top: 4px;
  opacity: 0.5;
  text-align: right;
}

.panel-footer {
  padding: 12px 16px 16px;
  background: var(--bg-card, var(--admin-card-bg));
  border-top: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.context-pill {
  font-size: 11px;
  background: var(--system-blue-subtle);
  color: var(--system-blue);
  padding: 2px 8px;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: fit-content;
}

.context-pill button {
  background: none;
  border: none;
  color: var(--system-blue);
  margin-left: 6px;
  cursor: pointer;
  font-weight: bold;
}

.input-wrapper {
  display: flex;
  gap: 8px;
}

.panel-footer input {
  flex: 1;
  padding: 10px 14px;
  border: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
  border-radius: 10px;
  outline: none;
  font-size: 13px;
  background-color: var(--bg-app, #F5F5F7);
}

.panel-footer input:focus {
  border-color: var(--system-blue, #007AFF);
  background-color: var(--admin-card-bg);
}

.send-btn {
  background-color: var(--system-blue, #007AFF);
  color: white;
  border: none;
  border-radius: 10px;
  width: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
}

.send-btn:disabled {
  opacity: 0.5;
  background-color: var(--admin-text-muted);
}
</style>
