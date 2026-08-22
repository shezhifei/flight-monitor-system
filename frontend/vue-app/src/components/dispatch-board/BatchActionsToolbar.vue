<template>
  <div class="batch-actions-toolbar">
    <div class="toolbar-left">
      <span v-if="selectedCount > 0" class="selected-count">
        已选择 <strong>{{ selectedCount }}</strong> 个任务
      </span>
      <span v-else class="selected-count none">
        未选择任务
      </span>
    </div>
    <div class="toolbar-right">
      <UiButton variant="primary" :disabled="selectedCount === 0" @click="$emit('complete')">
        批量完成
      </UiButton>
      <UiButton variant="tonal" :disabled="selectedCount === 0" @click="$emit('publish')">
        批量发布
      </UiButton>
      <UiButton @click="$emit('clear')">
        清除选择
      </UiButton>
    </div>
  </div>
</template>

<script setup lang="ts">
import UiButton from '../ui/UiButton.vue';

defineProps<{
  selectedCount: number;
}>();

defineEmits<{
  (e: 'complete'): void;
  (e: 'publish'): void;
  (e: 'clear'): void;
}>();
</script>

<style scoped>
/* 批量动作条：贴在工作面上沿，一根线收底；钮的形归 UiButton */
.batch-actions-toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--s4);
  padding: var(--s3) var(--s5);
  background: var(--face-work);
  border-bottom: 1px solid var(--line);
}

.toolbar-left {
  display: flex;
  align-items: center;
}

.selected-count {
  font-size: var(--fs-section);
  color: var(--ink);
}

.selected-count strong {
  color: var(--act);
  font-weight: var(--fw-semibold);
  font-variant-numeric: tabular-nums;
}

.selected-count.none {
  color: var(--ink-muted);
}

.toolbar-right {
  display: flex;
  gap: var(--s2);
}
</style>
