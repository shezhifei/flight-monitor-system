<script setup lang="ts">
withDefaults(defineProps<{
  label?: string;
  inset?: 'page' | 'work';
}>(), {
  inset: 'page',
});
</script>

<template>
  <div class="ui-seg" role="radiogroup" :aria-label="label" :data-inset="inset">
    <slot />
  </div>
</template>

<style scoped>
.ui-seg {
  display: inline-flex;
  /* 在 column flex 容器（如模态体）中不被 stretch 拉成通栏 */
  align-self: flex-start;
  padding: 2px;
  border-radius: var(--r-control);
  background: var(--face-page);
  border: 1px solid var(--line);
}

.ui-seg[data-inset='work'] {
  background: var(--face-work);
}

.ui-seg :slotted(button) {
  height: 28px;
  padding: 0 10px;
  border: 0;
  border-radius: var(--r-cell);
  background: transparent;
  color: var(--ink-subtle);
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  font-family: inherit;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  cursor: pointer;
}

.ui-seg :slotted(button:hover) {
  color: var(--ink);
}

.ui-seg :slotted(button[aria-checked='true']),
.ui-seg :slotted(button[aria-pressed='true']) {
  background: var(--face-raised);
  color: var(--ink);
  /* 浅色主题 raised 与 work 同白，补一道内细线保证选中可辨 */
  box-shadow: var(--shadow-sm), inset 0 0 0 1px var(--line);
}

.ui-seg :slotted(button:focus-visible) {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}
</style>
