<script setup lang="ts">
import { nextTick, ref, watch } from 'vue';

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  time?: string;
}

const props = defineProps<{
  messages: ChatMessage[];
  streaming?: boolean;
  emptyText?: string;
}>();

const container = ref<HTMLElement | null>(null);

watch(
  () => [props.messages.length, props.messages[props.messages.length - 1]?.content],
  async () => {
    await nextTick();
    if (container.value) {
      container.value.scrollTop = container.value.scrollHeight;
    }
  },
);
</script>

<template>
  <div ref="container" class="chat-list">
    <div v-if="!messages.length" class="chat-empty">
      {{ emptyText ?? '暂无消息' }}
    </div>
    <div
      v-for="msg in messages"
      :key="msg.id"
      class="chat-msg"
      :class="`is-${msg.role}`"
    >
      <div class="chat-bubble">
        <span class="chat-text">{{ msg.content }}</span>
        <span v-if="msg.time" class="chat-time">{{ msg.time }}</span>
      </div>
    </div>
    <div v-if="streaming" class="chat-msg is-assistant">
      <div class="chat-bubble chat-streaming">
        <span class="chat-cursor" />
      </div>
    </div>
  </div>
</template>

<style scoped>
.chat-list {
  flex: 1;
  overflow-y: auto;
  padding: 12px 4px;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.chat-empty {
  margin: auto;
  color: var(--ink-muted);
  font-size: var(--fs-body);
}

.chat-msg {
  display: flex;
}

.chat-msg.is-user {
  justify-content: flex-end;
}

.chat-bubble {
  max-width: 82%;
  padding: 8px 12px;
  border-radius: var(--r-panel);
  font-size: var(--fs-body);
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-word;
}

.is-user .chat-bubble {
  background: var(--act-soft);
  color: var(--ink);
}

.is-assistant .chat-bubble {
  background: var(--face-work);
  border: 1px solid var(--line);
  color: var(--ink);
}

.is-system .chat-bubble {
  background: transparent;
  color: var(--ink-muted);
  font-size: var(--fs-label);
  padding: 2px 0;
}

.chat-time {
  display: block;
  margin-top: 4px;
  font-size: 10px;
  font-family: var(--mono);
  color: var(--ink-muted);
}

.chat-streaming {
  display: inline-flex;
}

.chat-cursor {
  width: 7px;
  height: 14px;
  background: var(--act);
}
</style>
