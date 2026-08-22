<script setup lang="ts">
withDefaults(defineProps<{
  tone?: 'act' | 'ok' | 'warn' | 'danger';
}>(), {
  tone: 'act',
});
</script>

<template>
  <div class="ui-banner" :data-tone="tone" role="status">
    <slot />
  </div>
</template>

<style scoped>
.ui-banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s3);
  padding: var(--s2) var(--s3);
  border-radius: var(--r-control);
  font-size: var(--fs-label);
  line-height: 1.45;
}

.ui-banner[data-tone='act'] {
  background: var(--act-soft);
  color: var(--act);
  border: 1px solid color-mix(in srgb, var(--act) 42%, transparent);
}

.ui-banner[data-tone='ok'] {
  background: var(--ok-soft);
  color: var(--ok);
  border: 1px solid color-mix(in srgb, var(--ok) 42%, transparent);
}

.ui-banner[data-tone='warn'] {
  background: var(--warn-soft);
  color: var(--warn);
  border: 1px solid color-mix(in srgb, var(--warn) 42%, transparent);
}

.ui-banner[data-tone='danger'] {
  background: var(--danger-soft);
  color: var(--danger);
  border: 1px solid color-mix(in srgb, var(--danger) 42%, transparent);
}
</style>
