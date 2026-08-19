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
  right: 20px;
  bottom: 64px;
  display: flex;
  flex-direction: column;
  /* 角浮：悬钮锚右，抬起面板在其上 */
  align-items: flex-end;
  gap: 8px;
  z-index: 9000;
}
</style>
