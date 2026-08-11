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
  height: 36px;
  padding: 0 12px;
  border-radius: 999px;
  border: 1px solid var(--ws-border-strong, rgba(0, 0, 0, 0.08));
  background: var(--ws-surface, var(--glass-bg));
  color: var(--ws-text, #11233f);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  box-shadow: var(--ws-shadow-md, 0 6px 20px rgba(0,0,0,0.08));
  transition: transform 0.15s ease, box-shadow 0.15s ease, background 0.15s ease;
  font-family: inherit;
}

.theme-toggle--floating {
  position: fixed;
  right: 16px;
  bottom: 16px;
  z-index: 10020;
  height: 40px;
  padding: 0 14px;
}

.theme-toggle--inline {
  position: static;
  z-index: auto;
  height: 32px;
  padding: 0 10px;
  font-size: 12px;
  box-shadow: none;
  background: transparent;
  border-color: var(--border-light, rgba(100, 140, 190, 0.2));
  color: var(--text-secondary, var(--ws-text));
  flex-shrink: 0;
}

.theme-toggle--inline:hover {
  transform: none;
  background: rgba(61, 174, 255, 0.1);
  color: var(--text-primary);
  border-color: rgba(61, 174, 255, 0.35);
}

.theme-toggle--floating:hover {
  transform: translateY(-1px);
  box-shadow: 0 8px 24px rgba(0,0,0,0.12);
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
