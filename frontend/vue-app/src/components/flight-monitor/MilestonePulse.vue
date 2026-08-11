<script setup lang="ts">
/**
 * 里程碑强提醒（EP-04 绿屏闪烁）：
 * 全屏绿色径向脉冲 2 次 + 中央黑色毛玻璃大卡（大号绿色航班号 + 「XX · 已就绪」）。
 * 样式定义在 src/styles/components.css 尾部（.milestone-pulse）。
 */
defineProps<{
  flightNo: string;
  label: string;
  visible: boolean;
}>();
</script>

<template>
  <teleport to="body">
    <div
      v-if="visible"
      class="milestone-pulse active"
      role="status"
      aria-live="assertive"
    >
      <div class="pulse-content">
        <h1 class="pulse-flight-no">{{ flightNo }}</h1>
        <p class="pulse-label">{{ label }} · 已就绪</p>
      </div>
    </div>
  </teleport>
</template>
