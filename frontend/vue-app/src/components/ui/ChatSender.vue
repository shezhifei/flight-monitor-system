<script setup lang="ts">
const props = withDefaults(defineProps<{
  modelValue: string;
  loading?: boolean;
  disabled?: boolean;
  placeholder?: string;
}>(), {
  loading: false,
  disabled: false,
  placeholder: '输入问题，Enter 发送，Shift+Enter 换行',
});

const emit = defineEmits<{
  'update:modelValue': [value: string];
  send: [];
  cancel: [];
}>();

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault();
    if (props.modelValue.trim() && !props.loading && !props.disabled) {
      emit('send');
    }
  }
}
</script>

<template>
  <div class="chat-sender">
    <textarea
      class="chat-sender-input"
      :value="modelValue"
      :placeholder="placeholder"
      :disabled="disabled"
      rows="2"
      @input="emit('update:modelValue', ($event.target as HTMLTextAreaElement).value)"
      @keydown="onKeydown"
    />
    <div class="chat-sender-actions">
      <button
        v-if="loading"
        type="button"
        class="chat-sender-btn is-stop"
        @click="emit('cancel')"
      >
        停止
      </button>
      <button
        v-else
        type="button"
        class="chat-sender-btn is-send"
        :disabled="disabled || !modelValue.trim()"
        @click="emit('send')"
      >
        发送
      </button>
    </div>
  </div>
</template>

<style scoped>
.chat-sender {
  border: 1px solid var(--line-strong);
  border-radius: var(--r-panel);
  background: var(--face-page);
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.chat-sender:focus-within {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}

.chat-sender-input {
  width: 100%;
  border: none;
  background: transparent;
  color: var(--ink);
  font-size: var(--fs-body);
  font-family: inherit;
  resize: none;
  box-sizing: border-box;
}

.chat-sender-input:focus {
  outline: none;
}

.chat-sender-input::placeholder {
  color: var(--ink-muted);
}

.chat-sender-actions {
  display: flex;
  justify-content: flex-end;
}

.chat-sender-btn {
  min-height: var(--h-sm);
  padding: 0 16px;
  border-radius: var(--r-control);
  border: none;
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  cursor: pointer;
}

.is-send {
  background: var(--act);
  color: var(--act-on);
}

.is-send:disabled {
  background: color-mix(in srgb, var(--ink) 8%, transparent);
  color: var(--ink-muted);
  cursor: not-allowed;
}

.is-stop {
  background: var(--danger-soft);
  color: var(--danger);
}
</style>
