<script setup lang="ts">
import { watch } from 'vue';
import { pageUrl } from '@/shared/page-routes';
import SvgIcon from '@/components/ui/SvgIcon.vue';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import { useTheme } from '@/composables/useTheme';
import { useWorkspaceTabs } from './useWorkspaceTabs';

const {
  modules,
  openTabs,
  activeId,
  openTab,
  activateTab,
  closeTab,
  maxTabs,
} = useWorkspaceTabs();

// 确保壳层加载 useTheme（含 postMessage 广播）；主题变化时再推一次给已挂载 iframe
const { theme } = useTheme();

function pushThemeToFrames(): void {
  const payload = { type: 'fms-theme-change', theme: theme.value };
  const origin = window.location.origin;
  document.querySelectorAll<HTMLIFrameElement>('.ws-frame').forEach((frame) => {
    try {
      frame.contentWindow?.postMessage(payload, origin);
    } catch {
      // ignore
    }
  });
}

watch(theme, () => {
  pushThemeToFrames();
});

function onFrameLoad(event: Event): void {
  // 新打开 / 刷新的标签加载完成后立即对齐当前主题
  const frame = event.target as HTMLIFrameElement | null;
  if (!frame?.contentWindow) return;
  try {
    frame.contentWindow.postMessage(
      { type: 'fms-theme-change', theme: theme.value },
      window.location.origin,
    );
  } catch {
    // ignore
  }
}

function onModuleClick(moduleId: string): void {
  openTab(moduleId);
}

function onTabClick(moduleId: string): void {
  activateTab(moduleId);
}

function onTabClose(event: Event, moduleId: string): void {
  event.stopPropagation();
  closeTab(moduleId);
}
</script>

<template>
  <div class="ws-app">
    <!-- Row 1: brand + all functions -->
    <header class="ws-topbar" role="banner">
      <div class="ws-topbar__brand">
        <a class="ws-topbar__back" :href="pageUrl('dashboard')" aria-label="返回工作台">
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
            <path d="M9.5 3.5L5 8L9.5 12.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </a>
        <a class="ws-topbar__logo" :href="pageUrl('dashboard')" title="返回工作台">
          <span class="ws-topbar__logo-icon" aria-hidden="true">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
              <path d="M2 14.5 22 4l-6.8 16-4.1-5.1L7 18l1.1-4.1L2 14.5Z" />
            </svg>
          </span>
          <span class="ws-topbar__logo-text">
            <strong>Ops Workspace</strong>
            <small>运行工作区</small>
          </span>
        </a>
      </div>

      <nav class="ws-func-rail" aria-label="全部功能">
        <button
          v-for="mod in modules"
          :key="mod.id"
          type="button"
          class="ws-func-chip"
          :class="{
            'is-open': openTabs.some((t) => t.id === mod.id),
            'is-active': activeId === mod.id,
          }"
          :title="mod.description"
          @click="onModuleClick(mod.id)"
        >
          <SvgIcon :src="`/frontend/icons/${mod.icon}.svg`" :size="14" class="ws-func-chip__icon" />
          <span class="ws-func-chip__label">{{ mod.shortTitle }}</span>
        </button>
      </nav>

      <div class="ws-topbar__actions">
        <span class="ws-topbar__hint" :title="`最多 ${maxTabs} 个标签`">
          {{ openTabs.length }}/{{ maxTabs }}
        </span>
        <ThemeToggle variant="inline" />
      </div>
    </header>

    <!-- Row 2: open tabs -->
    <div class="ws-tabbar" role="tablist" aria-label="已打开标签">
      <button
        v-for="tab in openTabs"
        :key="tab.id"
        type="button"
        role="tab"
        class="ws-tab"
        :class="{ 'is-active': activeId === tab.id }"
        :aria-selected="activeId === tab.id"
        :title="tab.title"
        @click="onTabClick(tab.id)"
      >
        <span class="ws-tab__title">{{ tab.title }}</span>
        <span
          v-if="!tab.pinned"
          class="ws-tab__close"
          role="button"
          tabindex="0"
          aria-label="关闭标签"
          @click="onTabClose($event, tab.id)"
          @keydown.enter.prevent="onTabClose($event, tab.id)"
        >
          ×
        </span>
      </button>
    </div>

    <!-- Content: keep-alive iframes -->
    <main class="ws-content" role="main">
      <iframe
        v-for="tab in openTabs"
        :key="tab.id"
        class="ws-frame"
        :class="{ 'is-active': activeId === tab.id }"
        :src="tab.src"
        :title="tab.title"
        :aria-hidden="activeId !== tab.id"
        loading="lazy"
        referrerpolicy="same-origin"
        @load="onFrameLoad"
      />
    </main>
  </div>
</template>

<style scoped>
.ws-app {
  display: flex;
  flex-direction: column;
  height: 100vh;
  width: 100%;
  overflow: hidden;
  background: var(--bg-app, #0a1220);
  color: var(--text-primary);
}

/* —— Top bar —— */
.ws-topbar {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 44px;
  height: 44px;
  padding: 0 10px 0 8px;
  border-bottom: 1px solid var(--border-light, rgba(100, 140, 190, 0.14));
  background: var(--glass-bg, rgba(17, 25, 39, 0.94));
  backdrop-filter: blur(16px);
  flex-shrink: 0;
  z-index: 20;
}

.ws-topbar__brand {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.ws-topbar__back {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border-radius: 8px;
  color: var(--text-secondary);
  text-decoration: none;
  transition: background 0.15s, color 0.15s;
}

.ws-topbar__back:hover {
  background: var(--bg-hover, rgba(100, 140, 190, 0.12));
  color: var(--text-primary);
}

.ws-topbar__logo {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  text-decoration: none;
  color: inherit;
  padding-right: 4px;
}

.ws-topbar__logo-icon {
  display: inline-flex;
  color: var(--system-blue, #3daeff);
}

.ws-topbar__logo-text {
  display: flex;
  flex-direction: column;
  line-height: 1.15;
}

.ws-topbar__logo-text strong {
  font-size: 13px;
  font-weight: 700;
  letter-spacing: -0.01em;
}

.ws-topbar__logo-text small {
  font-size: 11px;
  color: var(--text-tertiary);
  font-weight: 500;
}

.ws-func-rail {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 6px;
  overflow-x: auto;
  overflow-y: hidden;
  padding: 2px 4px;
  scrollbar-width: thin;
}

.ws-func-rail::-webkit-scrollbar {
  height: 4px;
}

.ws-func-rail::-webkit-scrollbar-thumb {
  background: rgba(100, 140, 190, 0.25);
  border-radius: 4px;
}

.ws-func-chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  flex-shrink: 0;
  height: 28px;
  padding: 0 9px;
  border-radius: 999px;
  border: 1px solid var(--border-light, rgba(100, 140, 190, 0.16));
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.15s, color 0.15s, border-color 0.15s, box-shadow 0.15s;
}

.ws-func-chip:hover {
  color: var(--text-primary);
  border-color: rgba(61, 174, 255, 0.35);
  background: rgba(61, 174, 255, 0.08);
}

.ws-func-chip.is-open {
  border-color: rgba(61, 174, 255, 0.28);
  color: var(--text-primary);
  background: rgba(61, 174, 255, 0.06);
}

.ws-func-chip.is-active {
  color: #fff;
  background: var(--system-blue, #0a7cff);
  border-color: transparent;
  box-shadow: 0 2px 8px rgba(10, 124, 255, 0.28);
}

.ws-func-chip__icon {
  opacity: 0.9;
}

.ws-func-chip.is-active :deep(img),
.ws-func-chip.is-active :deep(svg) {
  filter: brightness(0) invert(1);
}

.ws-topbar__actions {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-shrink: 0;
}

.ws-topbar__hint {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-tertiary);
  font-variant-numeric: tabular-nums;
}

/* —— Tab bar —— */
.ws-tabbar {
  display: flex;
  align-items: flex-end;
  gap: 2px;
  min-height: 34px;
  height: 34px;
  padding: 0 8px;
  background: var(--bg-sidebar, #0a1220);
  border-bottom: 1px solid var(--border-light, rgba(100, 140, 190, 0.12));
  overflow-x: auto;
  flex-shrink: 0;
  scrollbar-width: thin;
}

.ws-tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
  max-width: 180px;
  height: 28px;
  margin: 0;
  padding: 0 10px 0 12px;
  border: 1px solid transparent;
  border-radius: 8px 8px 0 0;
  background: transparent;
  color: var(--text-tertiary);
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
}

.ws-tab:hover {
  color: var(--text-primary);
  background: rgba(100, 140, 190, 0.08);
}

.ws-tab.is-active {
  color: var(--text-primary);
  background: var(--bg-card, rgba(17, 24, 31, 0.96));
  border-color: var(--border-light, rgba(100, 140, 190, 0.14));
  border-bottom-color: transparent;
  box-shadow: 0 -1px 0 var(--system-blue) inset;
}

.ws-tab__title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ws-tab__close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  border-radius: 4px;
  font-size: 14px;
  line-height: 1;
  color: var(--text-tertiary);
  opacity: 0.75;
}

.ws-tab__close:hover {
  background: rgba(239, 68, 68, 0.16);
  color: var(--system-red, #ef5350);
  opacity: 1;
}

/* —— Content —— */
.ws-content {
  position: relative;
  flex: 1;
  min-height: 0;
  background: var(--bg-app);
}

.ws-frame {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  border: 0;
  background: var(--bg-app);
  opacity: 0;
  pointer-events: none;
  z-index: 0;
}

.ws-frame.is-active {
  opacity: 1;
  pointer-events: auto;
  z-index: 1;
}

@media (max-width: 900px) {
  .ws-topbar__logo-text small {
    display: none;
  }

  .ws-func-chip__label {
    /* keep labels; rail scrolls */
  }
}
</style>
