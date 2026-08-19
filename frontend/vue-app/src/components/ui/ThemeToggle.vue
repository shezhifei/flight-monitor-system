<script setup lang="ts">
import { useTheme } from '@/composables/useTheme';

withDefaults(defineProps<{
  /** floating: 右下角悬浮（独立页面）；inline: 顶栏内联（工作区壳） */
  variant?: 'floating' | 'inline';
}>(), {
  variant: 'floating',
});

const { theme, cycleTheme } = useTheme();

const icons: Record<string, string> = {
  light: '☀️',
  dark: '🌙',
};

const labels: Record<string, string> = {
  light: '浅色',
  dark: '深色',
};
</script>

<template>
  <button
    class="theme-toggle"
    :class="variant === 'inline' ? 'theme-toggle--inline' : 'theme-toggle--floating'"
    type="button"
    :title="`当前: ${labels[theme]} (点击切换)`"
    @click="cycleTheme"
  >
    <span class="theme-toggle-icon">{{ icons[theme] }}</span>
    <span class="theme-toggle-label">{{ labels[theme] }}</span>
  </button>
</template>

<style scoped>
.theme-toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: var(--h-md);
  padding: 0 12px;
  border-radius: var(--r-control);
  border: 1px solid var(--line-strong);
  background: var(--face-raised);
  color: var(--ink);
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  cursor: pointer;
  box-shadow: var(--shadow-sm);
  font-family: inherit;
}

.theme-toggle--floating {
  position: fixed;
  right: 16px;
  bottom: 16px;
  z-index: 10020;
  height: var(--h-lg);
  padding: 0 14px;
}

.theme-toggle--inline {
  position: static;
  z-index: auto;
  height: var(--h-sm);
  padding: 0 10px;
  font-size: var(--fs-label);
  box-shadow: none;
  background: transparent;
  border-color: var(--line-strong);
  color: var(--ink-subtle);
  flex-shrink: 0;
}

.theme-toggle--inline:hover {
  color: var(--ink);
  border-color: var(--ink-muted);
  background: transparent;
}

.theme-toggle--floating:hover {
  border-color: var(--ink-muted);
}

.theme-toggle:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
}

.theme-toggle-icon {
  font-size: 15px;
  line-height: 1;
}

.theme-toggle--inline .theme-toggle-icon {
  font-size: 14px;
}

.theme-toggle-label {
  white-space: nowrap;
}
</style>
