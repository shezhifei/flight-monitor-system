<template>
  <div class="terminal-selector">
    <div class="selector-label">
      航站楼
    </div>
    <div class="terminal-list">
      <button
        v-for="terminal in terminals"
        :key="terminal.id"
        type="button"
        class="terminal-btn"
        :aria-pressed="currentTerminal === terminal.id ? 'true' : 'false'"
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
/* 航站楼过滤芯片：持守（当前楼）主声实底，不描渐变 */
.terminal-selector {
  display: flex;
  align-items: center;
  gap: var(--s3);
}

.selector-label {
  font-size: var(--fs-body);
  color: var(--ink-subtle);
  font-weight: var(--fw-medium);
}

.terminal-list {
  display: flex;
  gap: var(--s1);
}

.terminal-btn {
  display: inline-flex;
  align-items: center;
  gap: var(--s2);
  height: var(--h-sm);
  padding: 0 var(--s3);
  background: var(--face-raised);
  border: 1px solid var(--line);
  border-radius: var(--r-pill);
  font-family: inherit;
  font-size: var(--fs-body);
  color: var(--ink);
  cursor: pointer;
  transition: border-color var(--t-fast) var(--ease), color var(--t-fast) var(--ease);
}

.terminal-btn:hover {
  border-color: var(--act);
  color: var(--act);
}

.terminal-btn:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
}

.terminal-btn[aria-pressed='true'] {
  background: var(--act);
  border-color: transparent;
  color: var(--act-on);
}

.terminal-name {
  font-weight: var(--fw-medium);
}

/* 计数小片：常态淡墨面，持守时踩主声上的白 */
.terminal-count {
  padding: 0 var(--s2);
  border-radius: var(--r-pill);
  background: color-mix(in srgb, var(--ink) 8%, transparent);
  color: var(--ink-subtle);
  font-family: var(--mono);
  font-size: var(--fs-label);
  font-weight: var(--fw-semibold);
  font-variant-numeric: tabular-nums;
  line-height: 18px;
}

.terminal-btn[aria-pressed='true'] .terminal-count {
  background: color-mix(in srgb, var(--act-on) 22%, transparent);
  color: var(--act-on);
}
</style>
