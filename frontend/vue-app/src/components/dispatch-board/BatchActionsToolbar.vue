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
      <button
        class="action-btn"
        :disabled="selectedCount === 0"
        @click="$emit('complete')"
      >
        <span class="icon">✅</span>
        批量完成
      </button>
      <button
        class="action-btn"
        :disabled="selectedCount === 0"
        @click="$emit('publish')"
      >
        <span class="icon">📤</span>
        批量发布
      </button>
      <button
        class="action-btn secondary"
        @click="$emit('clear')"
      >
        清除选择
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
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
.batch-actions-toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 20px;
  background: var(--bg-sidebar);
  border-bottom: 1px solid var(--border-light);
  gap: 16px;
}

.toolbar-left {
  display: flex;
  align-items: center;
  gap: 12px;
}

.selected-count {
  font-size: 14px;
  color: var(--admin-text);
}

.selected-count strong {
  color: var(--ws-primary);
  font-weight: 600;
}

.selected-count.none {
  color: var(--admin-text-muted);
}

.toolbar-right {
  display: flex;
  gap: 8px;
}

.action-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 16px;
  background: var(--bg-card);
  border: 1px solid var(--border-light);
  border-radius: 6px;
  font-size: 13px;
  color: var(--admin-text);
  cursor: pointer;
  transition: all 0.2s;
}

.action-btn:hover:not(:disabled) {
  border-color: var(--ws-primary);
  color: var(--ws-primary);
  background: var(--dh-signal-accent-soft);
}

.action-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.action-btn.secondary {
  color: var(--admin-text-subtle);
}

.action-btn.secondary:hover:not(:disabled) {
  border-color: var(--admin-text-muted);
  background: var(--border-light);
  color: var(--admin-text);
}

.icon {
  font-size: 14px;
}
</style>
