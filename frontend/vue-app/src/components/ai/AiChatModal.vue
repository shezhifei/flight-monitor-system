<script setup lang="ts">
import { ref, watch, nextTick, computed } from 'vue';
import PendingActionCard from './PendingActionCard.vue';
import type { ChatMessage, ToolItem } from '@/composables/useFlowableAiChat';
import type { PendingAction } from '@/composables/usePendingActions';
import { deriveInputAccept, type InputModality } from '@/utils/aiInputAccept';

const props = withDefaults(defineProps<{
  show: boolean;
  messages: ChatMessage[];
  sending: boolean;
  mode: 'contextual' | 'general';
  toolItems: ToolItem[];
  pendingActions: PendingAction[];
  canUseChat: boolean;
  missingLabel: string;
  // 能力快照派生的输入约束：缺省 text-only（不显示上传入口）。
  inputModalities?: InputModality[];
  allowedInputMimeTypes?: string[];
}>(), {
  inputModalities: () => ['text'],
  allowedInputMimeTypes: () => [],
});

const emit = defineEmits<{
  close: [];
  send: [content: string, files: File[]];
  cancel: [];
  'update:mode': [mode: 'contextual' | 'general'];
  approve: [actionId: string];
  reject: [actionId: string];
}>();

const input = ref('');
const messagesContainer = ref<HTMLElement | null>(null);
const fileInput = ref<HTMLInputElement | null>(null);
const selectedFiles = ref<File[]>([]);

// accept 白名单完全由能力快照动态派生；text-only 时 uploadEnabled=false。
const acceptDerivation = computed(() =>
  deriveInputAccept(props.inputModalities, props.allowedInputMimeTypes),
);

watch(() => props.messages.length, async () => {
  await nextTick();
  if (messagesContainer.value) {
    messagesContainer.value.scrollTop = messagesContainer.value.scrollHeight;
  }
});

// 关闭弹窗或上传被禁用时清空已选文件，避免陈留。
watch(() => [props.show, acceptDerivation.value.uploadEnabled], () => {
  if (!props.show || !acceptDerivation.value.uploadEnabled) {
    selectedFiles.value = [];
    if (fileInput.value) fileInput.value.value = '';
  }
});

function openFilePicker() {
  fileInput.value?.click();
}

function handleFileChange(e: Event) {
  const target = e.target as HTMLInputElement;
  selectedFiles.value = target.files ? Array.from(target.files) : [];
}

function removeSelectedFile(index: number) {
  selectedFiles.value.splice(index, 1);
  if (fileInput.value) fileInput.value.value = '';
}

function handleSend() {
  if (!input.value.trim() || props.sending) return;
  emit('send', input.value, selectedFiles.value.slice());
  input.value = '';
  selectedFiles.value = [];
  if (fileInput.value) fileInput.value.value = '';
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault();
    handleSend();
  }
}
</script>

<template>
  <Teleport to="body">
    <div v-if="show" class="ai-chat-overlay" @click.self="emit('close')">
      <div class="ai-chat-dialog">
        <header class="ai-chat-header">
          <h3>流程对话助手</h3>
          <div class="ai-chat-mode">
            <button :class="{ active: mode === 'contextual' }" @click="emit('update:mode', 'contextual')">
              上下文
            </button>
            <button :class="{ active: mode === 'general' }" @click="emit('update:mode', 'general')">
              通用
            </button>
          </div>
          <button class="ai-chat-close" @click="emit('close')">
            ×
          </button>
        </header>

        <div v-if="!canUseChat" class="ai-chat-disabled">
          <p>AI 助手不可用</p>
          <p class="ai-chat-disabled-reason">
            {{ missingLabel }}
          </p>
        </div>

        <template v-else>
          <div ref="messagesContainer" class="ai-chat-messages">
            <div v-if="!messages.length" class="ai-chat-empty">
              <p>向 AI 助手提问关于当前流程设计的问题</p>
            </div>
            <div
              v-for="msg in messages"
              :key="msg.id"
              class="ai-chat-bubble"
              :class="msg.role"
            >
              <div class="bubble-content">
                {{ msg.content }}
              </div>
            </div>
            <div v-if="sending" class="ai-chat-typing">
              <span /><span /><span />
            </div>
          </div>

          <div v-if="pendingActions.length" class="ai-chat-pending">
            <PendingActionCard
              v-for="action in pendingActions"
              :key="action.actionId"
              v-bind="action"
              @approve="emit('approve', $event)"
              @reject="emit('reject', $event)"
            />
          </div>

          <div class="ai-chat-input-area">
            <div v-if="selectedFiles.length" class="ai-chat-files">
              <span
                v-for="(file, idx) in selectedFiles"
                :key="`${file.name}-${idx}`"
                class="ai-chat-file-chip"
              >
                {{ file.name }}
                <button
                  type="button"
                  class="ai-chat-file-remove"
                  aria-label="移除文件"
                  @click="removeSelectedFile(idx)"
                >×</button>
              </span>
            </div>
            <textarea
              v-model="input"
              placeholder="输入问题... (Enter 发送, Shift+Enter 换行)"
              :disabled="sending"
              @keydown="handleKeydown"
            />
            <div class="ai-chat-input-actions">
              <!-- 上传入口仅在能力快照允许图片/音频/文件输入时出现，accept 由快照动态派生 -->
              <template v-if="acceptDerivation.uploadEnabled">
                <input
                  ref="fileInput"
                  type="file"
                  multiple
                  class="ai-chat-file-input"
                  :accept="acceptDerivation.accept"
                  @change="handleFileChange"
                >
                <button
                  type="button"
                  class="ai-btn-attach"
                  :disabled="sending"
                  :title="`允许类型: ${acceptDerivation.accept}`"
                  @click="openFilePicker"
                >
                  附件
                </button>
              </template>
              <span class="ai-chat-input-spacer" />
              <button v-if="sending" class="ai-btn-cancel" @click="emit('cancel')">
                停止
              </button>
              <button
                v-else
                class="ai-btn-send"
                :disabled="!input.trim()"
                @click="handleSend"
              >
                发送
              </button>
            </div>
          </div>
        </template>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.ai-chat-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0,0,0,0.4);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 10000;
}
.ai-chat-dialog {
  width: 560px;
  max-height: 80vh;
  background: var(--bg-card, #fff);
  border-radius: 12px;
  display: flex;
  flex-direction: column;
  box-shadow: 0 20px 60px rgba(0,0,0,0.2);
}
.ai-chat-header {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 14px 16px;
  border-bottom: 1px solid var(--border-light);
}
.ai-chat-header h3 { margin: 0; font-size: 15px; font-weight: 600; flex: 1; }
.ai-chat-mode { display: flex; gap: 4px; }
.ai-chat-mode button {
  padding: 4px 10px;
  border-radius: 6px;
  border: 1px solid var(--border-light);
  background: transparent;
  font-size: 12px;
  cursor: pointer;
}
.ai-chat-mode button.active {
  background: var(--system-blue, #007AFF);
  color: var(--text-inverse);
  border-color: transparent;
}
.ai-chat-close {
  background: none;
  border: none;
  font-size: 20px;
  cursor: pointer;
  color: var(--text-secondary);
  padding: 0 4px;
}
.ai-chat-disabled {
  padding: 40px 20px;
  text-align: center;
  color: var(--text-tertiary);
}
.ai-chat-disabled-reason { font-size: 12px; color: var(--system-orange, #FF9500); }
.ai-chat-messages {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
  min-height: 200px;
  max-height: 400px;
}
.ai-chat-empty {
  text-align: center;
  color: var(--text-tertiary);
  padding: 40px 0;
  font-size: 13px;
}
.ai-chat-bubble { margin-bottom: 12px; }
.ai-chat-bubble.user { text-align: right; }
.ai-chat-bubble.user .bubble-content {
  display: inline-block;
  background: var(--system-blue, #007AFF);
  color: var(--text-inverse);
  padding: 8px 12px;
  border-radius: 12px 12px 4px 12px;
  font-size: 13px;
  max-width: 80%;
  text-align: left;
  white-space: pre-wrap;
}
.ai-chat-bubble.assistant .bubble-content {
  display: inline-block;
  background: var(--bg-sidebar, #F5F5F7);
  color: var(--text-primary, #1D1D1F);
  padding: 8px 12px;
  border-radius: 12px 12px 12px 4px;
  font-size: 13px;
  max-width: 80%;
  text-align: left;
  white-space: pre-wrap;
}
.ai-chat-bubble.system .bubble-content {
  font-size: 12px;
  color: var(--text-tertiary);
  text-align: center;
}
.ai-chat-typing {
  display: flex;
  gap: 4px;
  padding: 8px 12px;
}
.ai-chat-typing span {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--text-tertiary);
  animation: blink 1.4s infinite both;
}
.ai-chat-typing span:nth-child(2) { animation-delay: 0.2s; }
.ai-chat-typing span:nth-child(3) { animation-delay: 0.4s; }
@keyframes blink { 0%,80%,100%{opacity:0.3} 40%{opacity:1} }
.ai-chat-pending { padding: 0 16px; }
.ai-chat-input-area {
  padding: 12px 16px;
  border-top: 1px solid var(--border-light);
}
.ai-chat-input-area textarea {
  width: 100%;
  min-height: 60px;
  max-height: 120px;
  border: 1px solid var(--border-light);
  border-radius: 8px;
  padding: 8px 10px;
  font-size: 13px;
  resize: vertical;
  font-family: inherit;
}
.ai-chat-input-area textarea:focus { outline: 2px solid var(--system-blue); outline-offset: -1px; }
.ai-chat-input-actions { display: flex; align-items: center; margin-top: 8px; gap: 8px; }
.ai-chat-input-spacer { flex: 1; }
.ai-btn-send, .ai-btn-cancel, .ai-btn-attach {
  padding: 6px 16px;
  border-radius: 6px;
  border: none;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
}
.ai-btn-send { background: var(--system-blue); color: var(--text-inverse); }
.ai-btn-send:disabled { opacity: 0.5; cursor: not-allowed; }
.ai-btn-cancel { background: var(--system-red); color: var(--text-inverse); }
.ai-btn-attach { background: transparent; border: 1px solid var(--border-light); color: var(--text-secondary, #5f6368); }
.ai-btn-attach:disabled { opacity: 0.5; cursor: not-allowed; }
.ai-chat-file-input { display: none; }
.ai-chat-files { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 8px; }
.ai-chat-file-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 3px 8px;
  border-radius: 6px;
  background: var(--bg-sidebar, #F5F5F7);
  font-size: 12px;
  color: var(--text-primary, #1D1D1F);
}
.ai-chat-file-remove {
  background: none;
  border: none;
  cursor: pointer;
  font-size: 14px;
  line-height: 1;
  color: var(--text-tertiary);
  padding: 0;
}
</style>