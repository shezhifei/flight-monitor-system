<script setup lang="ts">
import { onBeforeUnmount, watch } from 'vue';

const props = withDefaults(defineProps<{
  open: boolean;
  title: string;
  width?: number;
  closable?: boolean;
  id?: string;
}>(), {
  width: 560,
  closable: true,
});

const emit = defineEmits<{
  close: [];
}>();

function requestClose() {
  if (props.closable) emit('close');
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && props.open) requestClose();
}

watch(
  () => props.open,
  (isOpen) => {
    if (isOpen) window.addEventListener('keydown', onKeydown);
    else window.removeEventListener('keydown', onKeydown);
  },
);

onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown));
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="ui-modal-scrim" @click.self="requestClose">
      <div
        class="ui-modal"
        role="dialog"
        aria-modal="true"
        :id="id"
        :aria-label="title"
        :style="{ width: `min(${width}px, calc(100vw - 32px))` }"
      >
        <header class="ui-modal-header">
          <h3 class="ui-modal-title">{{ title }}</h3>
          <button
            v-if="closable"
            type="button"
            class="ui-modal-close"
            aria-label="关闭"
            @click="requestClose"
          >×</button>
        </header>
        <div class="ui-modal-body">
          <slot />
        </div>
        <footer v-if="$slots.footer" class="ui-modal-footer">
          <slot name="footer" />
        </footer>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.ui-modal-scrim {
  position: fixed;
  inset: 0;
  background: var(--scrim);
  z-index: 11000;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 20px;
  box-sizing: border-box;
}

.ui-modal {
  max-height: min(86vh, 760px);
  background: var(--face-raised);
  color: var(--ink);
  border: 1px solid var(--line);
  border-radius: var(--r-panel);
  box-shadow: var(--shadow-md);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.ui-modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 18px;
  border-bottom: 1px solid var(--line);
  flex-shrink: 0;
}

.ui-modal-title {
  margin: 0;
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
}

.ui-modal-close {
  background: none;
  border: none;
  font-size: 22px;
  line-height: 1;
  cursor: pointer;
  color: var(--ink-subtle);
  padding: 0 4px;
}

.ui-modal-close:hover {
  color: var(--ink);
}

.ui-modal-close:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}

.ui-modal-body {
  flex: 1;
  overflow-y: auto;
  padding: 16px 18px;
  min-height: 0;
}

.ui-modal-footer {
  padding: 12px 18px;
  border-top: 1px solid var(--line);
  flex-shrink: 0;
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
