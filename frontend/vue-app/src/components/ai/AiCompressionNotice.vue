<script setup lang="ts">
import type { CompressionNoticeModel } from '@/lib/ai/runResume';

defineProps<{
  notice: CompressionNoticeModel | null;
}>();
</script>

<template>
  <div v-if="notice" class="ai-compression" data-testid="compression-notice">
    <span class="ai-compression-text">
      上下文已压缩<template v-if="notice.strategy">（{{ notice.strategy }}）</template>
      <template v-if="notice.beforeTokens && notice.afterTokens">
        ：{{ notice.beforeTokens }} → {{ notice.afterTokens }} tokens
      </template>
      <template v-if="notice.latencyMs">，耗时 {{ notice.latencyMs }}ms</template>
    </span>
    <span class="ai-compression-time">{{ notice.at }}</span>
  </div>
</template>

<style scoped>
.ai-compression {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 6px 10px;
  border-radius: var(--r-control);
  background: var(--warn-soft);
  font-size: var(--fs-label);
}

.ai-compression-text {
  color: var(--warn);
}

.ai-compression-time {
  font-family: var(--mono);
  font-size: 11px;
  color: var(--ink-muted);
  flex-shrink: 0;
}
</style>
