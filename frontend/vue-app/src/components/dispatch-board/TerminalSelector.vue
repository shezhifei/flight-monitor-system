<template>
  <div class="terminal-selector">
    <div class="selector-label">
      航站楼
    </div>
    <div class="terminal-list">
      <button
        v-for="terminal in terminals"
        :key="terminal.id"
        class="terminal-btn"
        :class="{ active: currentTerminal === terminal.id }"
        @click="$emit('change', terminal.id)"
      >
        <span class="terminal-name">{{ terminal.name }}</span>
        <span v-if="terminal.count > 0" class="terminal-count">{{ terminal.count }}</span>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
interface Terminal {
  id: string;
  name: string;
  count: number;
}

defineProps<{
  terminals: Terminal[];
  currentTerminal: string;
}>();

defineEmits<{
  (e: 'change', terminalId: string): void;
}>();
</script>

<style scoped>
.terminal-selector {
  display: flex;
  align-items: center;
  gap: 12px;
}

.selector-label {
  font-size: 13px;
  color: var(--admin-text-subtle);
  font-weight: 500;
}

.terminal-list {
  display: flex;
  gap: 4px;
}

.terminal-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 14px;
  background: var(--bg-card);
  border: 1px solid var(--border-light);
  border-radius: 6px;
  font-size: 13px;
  color: var(--admin-text);
  cursor: pointer;
  transition: all 0.2s;
}

.terminal-btn:hover {
  border-color: var(--ws-primary);
  color: var(--ws-primary);
}

.terminal-btn.active {
  background: linear-gradient(135deg, var(--ws-primary) 0%, var(--system-blue) 100%);
  color: white;
  border-color: transparent;
}

.terminal-name {
  font-weight: 500;
}

.terminal-count {
  background: rgba(0, 0, 0, 0.15);
  color: white;
  font-size: 11px;
  font-weight: 600;
  padding: 2px 8px;
  border-radius: 10px;
}
</style>
