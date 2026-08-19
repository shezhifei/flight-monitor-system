<script setup lang="ts">
withDefaults(defineProps<{
  open: boolean;
  title?: string;
  width?: number;
}>(), {
  title: '',
  width: 420,
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
          <h3 class="ui-drawer-title">{{ title }}</h3>
          <button type="button" class="ui-drawer-close" aria-label="关闭" @click="emit('close')">×</button>
        </header>
        <div class="ui-drawer-body">
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
  z-index: 10000;
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
  padding: 14px 18px;
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
  font-size: 22px;
  line-height: 1;
  cursor: pointer;
  color: var(--ink-subtle);
  padding: 0 4px;
}

.ui-drawer-close:hover {
  color: var(--ink);
}

.ui-drawer-body {
  flex: 1;
  overflow-y: auto;
  padding: 16px 18px;
  min-height: 0;
}

.ui-drawer-footer {
  padding: 12px 18px;
  border-top: 1px solid var(--line);
  flex-shrink: 0;
}
</style>
