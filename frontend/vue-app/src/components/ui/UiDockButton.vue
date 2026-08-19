<script setup lang="ts">
defineProps<{
  label: string;
  count?: number | null;
  tone?: 'neutral' | 'act' | 'ok' | 'warn' | 'danger';
  pressed?: boolean;
}>();

const emit = defineEmits<{
  click: [];
}>();
</script>

<template>
  <button
    type="button"
    class="ui-dock-btn"
    :data-tone="tone ?? 'neutral'"
    :aria-pressed="pressed !== undefined ? (pressed ? 'true' : 'false') : undefined"
    :data-on="pressed === true ? 'true' : undefined"
    @click="emit('click')"
  >
    <span class="ui-dock-label">{{ label }}</span>
    <span v-if="count !== undefined && count !== null" class="ui-dock-count">{{ count }}</span>
    <slot />
  </button>
</template>

<style scoped>
/* 角浮抬起面板内的菜单条目：图标 + 名 + 右缘计数 */
.ui-dock-btn {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  height: 34px;
  padding: 0 10px;
  border-radius: var(--r-cell);
  border: none;
  background: transparent;
  color: var(--ink);
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  font-family: inherit;
  cursor: pointer;
  text-align: left;
  white-space: nowrap;
}

.ui-dock-btn:hover {
  background: color-mix(in srgb, var(--ink) 7%, transparent);
}

.ui-dock-btn:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}

.ui-dock-btn[data-on='true'] {
  background: var(--act-soft);
  color: var(--act);
}

/* 图标由父级经 slot 传入，比文字淡一档；持守时随四声 */
.ui-dock-btn :slotted(svg) {
  flex: none;
  color: var(--ink-subtle);
}

.ui-dock-btn[data-on='true'] :slotted(svg) {
  color: var(--act);
}

.ui-dock-count {
  margin-left: auto;
  font-family: var(--mono);
  font-size: 11px;
  color: var(--ink-subtle);
  font-variant-numeric: tabular-nums;
}

.ui-dock-btn[data-tone='warn'] .ui-dock-count { color: var(--warn); }
.ui-dock-btn[data-tone='danger'] .ui-dock-count { color: var(--danger); }
.ui-dock-btn[data-tone='ok'] .ui-dock-count { color: var(--ok); }
.ui-dock-btn[data-tone='act'] .ui-dock-count { color: var(--act); }
</style>
