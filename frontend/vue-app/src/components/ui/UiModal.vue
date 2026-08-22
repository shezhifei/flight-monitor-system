<script setup lang="ts">
import { onBeforeUnmount, watch } from 'vue';

/**
 * 弹窗（信号面 §3.8）：幕之上的一件事。帽 / 身 / 脚三段，身是唯一的滚动口。
 *
 * bleed 给「身里自己是一整套布局」的那一类（两栏群聊、全宽表）：
 * 身让出内衬与滚动权，由内容自己撑高、自己滚。
 * 各页不要再用负 margin 去顶掉身的内衬 —— 那是把弹窗的内衬数字抄进了页里。
 */
const props = withDefaults(defineProps<{
  open: boolean;
  title: string;
  width?: number;
  closable?: boolean;
  id?: string;
  /** 身让出内衬与滚动权，内容自己管高度与滚动 */
  bleed?: boolean;
}>(), {
  width: 560,
  closable: true,
  bleed: false,
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
        :id="id"
        class="ui-modal"
        role="dialog"
        aria-modal="true"
        :aria-label="title"
        :style="{ width: `min(${width}px, calc(100vw - 32px))` }"
      >
        <header class="ui-modal-header">
          <!-- 默认只有一行标题；眉标+标题等组合走 header 插槽 -->
          <slot name="header">
            <h3 class="ui-modal-title">
              {{ title }}
            </h3>
          </slot>
          <button
            v-if="closable"
            type="button"
            class="ui-modal-close"
            aria-label="关闭"
            @click="requestClose"
          >
            ×
          </button>
        </header>
        <div class="ui-modal-body" :data-bleed="bleed ? 'true' : undefined">
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
  z-index: var(--z-modal);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: var(--s4);
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
  gap: var(--s2);
  padding: var(--s3) var(--s4);
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
  font-size: var(--fs-page);
  line-height: 1;
  cursor: pointer;
  color: var(--ink-subtle);
  padding: 0 var(--s1);
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
  padding: var(--s4);
  min-height: 0;
}

/* 满幅：内容自带布局与滚动口，身只当容器 */
.ui-modal-body[data-bleed='true'] {
  padding: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.ui-modal-footer {
  padding: var(--s3) var(--s4);
  border-top: 1px solid var(--line);
  flex-shrink: 0;
  display: flex;
  justify-content: flex-end;
  gap: var(--s2);
}
</style>
