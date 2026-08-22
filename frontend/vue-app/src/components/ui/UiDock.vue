<script setup lang="ts">
defineProps<{
  label?: string;
}>();
</script>

<template>
  <div class="ui-dock" :aria-label="label ?? '快捷入口'">
    <slot />
  </div>
</template>

<style scoped>
.ui-dock {
  position: fixed;
  right: var(--dock-right);
  bottom: var(--dock-bottom);
  display: flex;
  flex-direction: column;
  /* 角浮：悬钮锚右，浮舱（UiFloatPanel）在其上，落点共用同一组 token */
  align-items: flex-end;
  gap: var(--s2);
  z-index: var(--z-dock);
}
</style>
