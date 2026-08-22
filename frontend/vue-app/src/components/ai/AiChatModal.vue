<script setup lang="ts">
import { ref, watch, nextTick, computed } from 'vue';
import PendingActionCard from './PendingActionCard.vue';
import UiButton from '@/components/ui/UiButton.vue';
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
            <UiButton :pressed="mode === 'contextual'" @click="emit('update:mode', 'contextual')">
              上下文
            </UiButton>
            <UiButton :pressed="mode === 'general'" @click="emit('update:mode', 'general')">
              通用
            </UiButton>
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
              :action="action"
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
                <UiButton
                  variant="ghost"
                  :disabled="sending"
                  :title="`允许类型: ${acceptDerivation.accept}`"
                  @click="openFilePicker"
                >
                  附件
                </UiButton>
              </template>
              <span class="ai-chat-input-spacer" />
              <UiButton v-if="sending" variant="danger" @click="emit('cancel')">
                停止
              </UiButton>
              <UiButton
                v-else
                variant="primary"
                :disabled="!input.trim()"
                @click="handleSend"
              >
                发送
              </UiButton>
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
  background: rgba(0, 0, 0, 0.4);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: var(--z-modal);
}
.ai-chat-dialog {
  width: 560px;
  max-height: 80vh;
  background: var(--face-raised);
  border-radius: var(--r-panel);
  display: flex;
  flex-direction: column;
  box-shadow: var(--shadow-md);
}
.ai-chat-header {
  display: flex;
  align-items: center;
  gap: var(--s3);
  padding: var(--s3) var(--s4);
  border-bottom: 1px solid var(--line);
}
.ai-chat-header h3 { margin: 0; font-size: var(--fs-title); font-weight: var(--fw-semibold); flex: 1; }
.ai-chat-mode { display: flex; gap: var(--s1); }
.ai-chat-close {
  background: none;
  border: none;
  font-size: 20px;
  cursor: pointer;
  color: var(--ink-subtle);
  padding: 0 4px;
}
.ai-chat-close:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
}
.ai-chat-disabled {
  padding: 40px var(--s4);
  text-align: center;
  color: var(--ink-muted);
}
.ai-chat-disabled-reason { font-size: var(--fs-label); color: var(--warn); }
.ai-chat-messages {
  flex: 1;
  overflow-y: auto;
  padding: var(--s4);
  min-height: 200px;
  max-height: 400px;
}
.ai-chat-empty {
  text-align: center;
  color: var(--ink-muted);
  padding: 40px 0;
  font-size: var(--fs-body);
}
.ai-chat-bubble { margin-bottom: var(--s3); }
.ai-chat-bubble.user { text-align: right; }
.ai-chat-bubble.user .bubble-content {
  display: inline-block;
  background: var(--act);
  color: var(--act-on);
  padding: var(--s2) var(--s3);
  border-radius: var(--r-panel) var(--r-panel) var(--r-cell) var(--r-panel);
  font-size: var(--fs-body);
  max-width: 80%;
  text-align: left;
  white-space: pre-wrap;
}
.ai-chat-bubble.assistant .bubble-content {
  display: inline-block;
  background: color-mix(in srgb, var(--ink) 6%, transparent);
  color: var(--ink);
  padding: var(--s2) var(--s3);
  border-radius: var(--r-panel) var(--r-panel) var(--r-panel) var(--r-cell);
  font-size: var(--fs-body);
  max-width: 80%;
  text-align: left;
  white-space: pre-wrap;
}
.ai-chat-bubble.system .bubble-content {
  font-size: var(--fs-label);
  color: var(--ink-muted);
  text-align: center;
}
.ai-chat-typing {
  display: flex;
  gap: var(--s1);
  padding: var(--s2) var(--s3);
}
.ai-chat-typing span {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--ink-muted);
  animation: blink 1.4s infinite both;
}
.ai-chat-typing span:nth-child(2) { animation-delay: 0.2s; }
.ai-chat-typing span:nth-child(3) { animation-delay: 0.4s; }
@keyframes blink { 0%, 80%, 100% { opacity: 0.3; } 40% { opacity: 1; } }
.ai-chat-pending { padding: 0 var(--s4); }
.ai-chat-input-area {
  padding: var(--s3) var(--s4);
  border-top: 1px solid var(--line);
}
.ai-chat-input-area textarea {
  width: 100%;
  min-height: 60px;
  max-height: 120px;
  border: 1px solid var(--line);
  border-radius: var(--r-control);
  padding: var(--s2) var(--s3);
  font-size: var(--fs-body);
  resize: vertical;
  font-family: inherit;
}
.ai-chat-input-area textarea:focus { outline: 2px solid var(--act); outline-offset: 2px; }
.ai-chat-input-actions { display: flex; align-items: center; margin-top: var(--s2); gap: var(--s2); }
.ai-chat-input-spacer { flex: 1; }
.ai-chat-file-input { display: none; }
.ai-chat-files { display: flex; flex-wrap: wrap; gap: var(--s2); margin-bottom: var(--s2); }
.ai-chat-file-chip {
  display: inline-flex;
  align-items: center;
  gap: var(--s2);
  padding: var(--s1) var(--s2);
  border-radius: var(--r-cell);
  background: color-mix(in srgb, var(--ink) 6%, transparent);
  font-size: var(--fs-label);
  color: var(--ink);
}
.ai-chat-file-remove {
  background: none;
  border: none;
  cursor: pointer;
  font-size: 14px;
  line-height: 1;
  color: var(--ink-muted);
  padding: 0;
}
</style>