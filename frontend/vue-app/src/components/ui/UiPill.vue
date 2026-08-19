<script setup lang="ts">
withDefaults(defineProps<{
  tone?: 'act' | 'ok' | 'warn' | 'danger' | 'mute';
}>(), {
  tone: 'mute',
});
</script>

<template>
  <span class="ui-pill" :data-tone="tone">
    <slot />
  </span>
</template>

<style scoped>
.ui-pill {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 22px;
  padding: 0 8px;
  border-radius: var(--r-pill);
  font-size: 11px;
  font-weight: var(--fw-medium);
  font-variant-numeric: tabular-nums;
}

.ui-pill[data-tone='act'] { background: var(--act-soft); color: var(--act); }
.ui-pill[data-tone='ok'] { background: var(--ok-soft); color: var(--ok); }
.ui-pill[data-tone='warn'] { background: var(--warn-soft); color: var(--warn); }
.ui-pill[data-tone='danger'] { background: var(--danger-soft); color: var(--danger); }
.ui-pill[data-tone='mute'] {
  background: color-mix(in srgb, var(--ink) 8%, transparent);
  color: var(--ink-subtle);
}
</style>
