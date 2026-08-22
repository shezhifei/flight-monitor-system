<script setup lang="ts">
withDefaults(defineProps<{
  open: boolean;
  title?: string;
  width?: number;
  /** 身内不铺内距，交给分区自己管（分区带线带内距的抽屉） */
  flush?: boolean;
}>(), {
  title: '',
  width: 420,
  flush: false,
});

const emit = defineEmits<{
  close: [];
}>();
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="ui-drawer-scrim" @click.self="emit('close')">
      <aside
        class="ui-drawer"
        role="dialog"
        :aria-label="title || '抽屉'"
        :style="{ width: `min(${width}px, calc(100vw - 32px))` }"
      >
        <header class="ui-drawer-header">
          <!-- 默认只有一行标题；眉标+标题等组合走 header 插槽 -->
          <slot name="header">
            <h3 class="ui-drawer-title">
              {{ title }}
            </h3>
          </slot>
          <button
            type="button"
            class="ui-drawer-close"
            aria-label="关闭"
            @click="emit('close')"
          >
            ×
          </button>
        </header>
        <div class="ui-drawer-body" :class="{ 'ui-drawer-body--flush': flush }">
          <slot />
        </div>
        <footer v-if="$slots.footer" class="ui-drawer-footer">
          <slot name="footer" />
        </footer>
      </aside>
    </div>
  </Teleport>
</template>

<style scoped>
.ui-drawer-scrim {
  position: fixed;
  inset: 0;
  background: var(--scrim);
  z-index: var(--z-modal);
  display: flex;
  justify-content: flex-end;
}

.ui-drawer {
  height: 100%;
  background: var(--face-raised);
  color: var(--ink);
  border-left: 1px solid var(--line);
  box-shadow: var(--shadow-md);
  display: flex;
  flex-direction: column;
}

.ui-drawer-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s2);
  padding: var(--s3) var(--s4);
  border-bottom: 1px solid var(--line);
  flex-shrink: 0;
}

.ui-drawer-title {
  margin: 0;
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
}

.ui-drawer-close {
  background: none;
  border: none;
  font-size: var(--fs-page);
  line-height: 1;
  cursor: pointer;
  color: var(--ink-subtle);
  padding: 0 var(--s1);
}

.ui-drawer-close:hover {
  color: var(--ink);
}

.ui-drawer-body {
  flex: 1;
  overflow-y: auto;
  padding: var(--s4);
  min-height: 0;
}

.ui-drawer-body--flush {
  padding: 0;
}

.ui-drawer-footer {
  padding: var(--s3) var(--s4);
  border-top: 1px solid var(--line);
  flex-shrink: 0;
}
</style>
