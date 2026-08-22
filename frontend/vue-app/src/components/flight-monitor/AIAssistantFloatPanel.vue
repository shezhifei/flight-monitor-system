<template>
  <UiFloatPanel
    :open="isOpen"
    title="极智 AI 指挥官"
    :subtitle="isConnected ? '实时桥接中' : '桥接离线'"
    :scroll="false"
    @close="togglePanel"
  >
    <template #meta>
      <UiPill :tone="isConnected ? 'ok' : 'mute'">
        {{ isConnected ? '在线' : '离线' }}
      </UiPill>
      <div class="assistant__overflow">
        <UiButton
          variant="quiet"
          title="更多操作"
          aria-label="更多操作"
          :pressed="showMenu"
          @click="showMenu = !showMenu"
        >
          ⋮
        </UiButton>
        <UiMenu v-if="showMenu" class="assistant__menu" label="会话操作">
          <UiMenuItem @click="clearHistory">
            <template #icon>
              🧹
            </template>
            清空会话
          </UiMenuItem>
          <UiMenuItem @click="endSession">
            <template #icon>
              🏁
            </template>
            结束会话
          </UiMenuItem>
          <hr>
          <UiMenuItem @click="exportHistory('json')">
            <template #icon>
              📄
            </template>
            导出 JSON
          </UiMenuItem>
          <UiMenuItem @click="exportHistory('md')">
            <template #icon>
              📝
            </template>
            导出 Markdown
          </UiMenuItem>
        </UiMenu>
      </div>
    </template>

    <ChatMessageList
      :messages="chatHistory"
      :streaming="isSending"
      class="assistant__stream"
    >
      <template #body="{ msg }">
        <div v-if="isSuggestion(msg)" class="assistant__suggest-title">
          AI 诊断建议
        </div>
        <!-- eslint-disable-next-line vue/no-v-html -->
        <div v-html="renderMarkdown(msg.content)" />
        <div v-if="suggestionActions(msg).length" class="assistant__suggest-verbs">
          <UiButton
            v-for="(action, aIdx) in suggestionActions(msg)"
            :key="aIdx"
            size="sm"
            @click="handleAction(action)"
          >
            {{ action.label }}
          </UiButton>
        </div>
        <AIVisualization
          v-if="!isSuggestion(msg) && (msg.type || msg.data)"
          :type="msg.type"
          :data="msg.data"
        />
      </template>
    </ChatMessageList>

    <template #footer>
      <div v-if="effectiveSelectedFlightNo" class="assistant__context">
        <UiPill tone="act">
          追问 {{ effectiveSelectedFlightNo }}
        </UiPill>
        <UiButton variant="quiet" size="sm" aria-label="清除航班上下文" @click="clearContext">
          ✕
        </UiButton>
      </div>
      <ChatSender
        v-model="inputText"
        :disabled="isSending"
        :placeholder="senderPlaceholder"
        @send="sendMessage"
      />
    </template>
  </UiFloatPanel>
</template>

<script setup lang="ts">
import { computed, ref, watch, onMounted, onUnmounted } from 'vue';
import { useAuth } from '../../composables/useAuth';
import { useApi } from '../../composables/useApi';
import { useAiStream } from '../../composables/useAiStream';
import { renderMarkdown } from '../../lib/marked';
import { downloadTextFile } from '../../lib/download';
import { useToast } from '../../composables/useToast';
import { unwrapApiData } from '../../shared/apiEnvelope';
import AIVisualization from './AIVisualization.vue';
import ChatMessageList from '../ui/ChatMessageList.vue';
import ChatSender from '../ui/ChatSender.vue';
import UiButton from '../ui/UiButton.vue';
import UiFloatPanel from '../ui/UiFloatPanel.vue';
import UiMenu from '../ui/UiMenu.vue';
import UiMenuItem from '../ui/UiMenuItem.vue';
import UiPill from '../ui/UiPill.vue';

const props = defineProps<{
  open: boolean;
  selectedFlightId?: string | null;
  selectedFlightNo?: string | null;
}>();

const emit = defineEmits<{
  (e: 'update:open', value: boolean): void;
  (e: 'update:unread', value: number): void;
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

const isOpen = computed({
  get: () => props.open,
  set: (value: boolean) => emit('update:open', value),
});
const showMenu = ref(false);
const unreadCount = ref(0);

watch(unreadCount, (value) => emit('update:unread', value));
const inputText = ref('');
const isSending = ref(false);
const conversationId = ref<string | null>(null);
const contextDismissed = ref(false);

/** 建议卡 = 带 ai_suggestion 标与载荷的那一条；判定只在这里写一次。 */
function isSuggestion(msg: ChatMessage): boolean {
  return msg.type === 'ai_suggestion' && !!msg.data;
}

/** 建议卡的动作条；没有就是空数组，模板不必判空。 */
function suggestionActions(msg: ChatMessage): Record<string, unknown>[] {
  const actions = msg.data?.actions;
  return Array.isArray(actions) ? actions as Record<string, unknown>[] : [];
}

const effectiveSelectedFlightId = computed(() => contextDismissed.value ? null : props.selectedFlightId);
const effectiveSelectedFlightNo = computed(() => contextDismissed.value ? null : props.selectedFlightNo);

const senderPlaceholder = computed(() => (
  effectiveSelectedFlightNo.value
    ? '针对 ' + effectiveSelectedFlightNo.value + ' 提问，Enter 发送'
    : '咨询 AI 指挥官，Enter 发送'
));

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
  } else {
    showMenu.value = false;
  }
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
    
    const payload = unwrapApiData<{ answer?: string; summary?: string; conversation_id?: string; structured_data?: Record<string, unknown>; visualization_hint?: Record<string, unknown> }>(await response.json());
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
    const payload = unwrapApiData<Record<string, unknown>>(await response.json());
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
/* 浮舱的形与帽在 UiFloatPanel；气泡与 Markdown 排版在 ChatMessageList；
   发送器在 ChatSender；溢出菜单在 UiMenu。这一页只剩落点。 */

.assistant__overflow {
  position: relative;
  display: inline-flex;
}

/* 溢出菜单贴扳机右对齐落下 */
.assistant__menu {
  position: absolute;
  top: calc(100% + 4px);
  right: 0;
  z-index: 10;
}

.assistant__stream {
  padding: 12px 14px;
}

/* 建议卡不是第二张卡：气泡里一根警声线起头，就够把它认出来 */
.assistant__suggest-title {
  margin-bottom: 6px;
  padding-bottom: 4px;
  border-bottom: 1px solid color-mix(in srgb, var(--warn) 30%, transparent);
  color: var(--warn);
  font-size: var(--fs-label);
  font-weight: var(--fw-semibold);
}

.assistant__suggest-verbs {
  display: flex;
  flex-wrap: wrap;
  gap: var(--s2);
  margin-top: 8px;
}

.assistant__context {
  display: flex;
  align-items: center;
  gap: var(--s1);
  margin-bottom: 8px;
}
</style>
