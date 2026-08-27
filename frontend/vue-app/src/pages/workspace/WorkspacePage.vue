<script setup lang="ts">
import { watch } from 'vue';
import { pageUrl } from '@/shared/page-routes';
import SvgIcon from '@/components/ui/SvgIcon.vue';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import OccupySeatButton from '@/components/ops/OccupySeatButton.vue';
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
          <svg
            width="16"
            height="16"
            viewBox="0 0 16 16"
            fill="none"
            aria-hidden="true"
          >
            <path
              d="M9.5 3.5L5 8L9.5 12.5"
              stroke="currentColor"
              stroke-width="1.5"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        </a>
        <a class="ws-topbar__logo" :href="pageUrl('dashboard')" title="返回工作台">
          <span class="ws-topbar__logo-icon" aria-hidden="true">
            <svg
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="currentColor"
            >
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
        <OccupySeatButton />
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
/* 信号面：工作区壳是 iframe 宿主，顶栏与标签栏用工作面的洗光，
   内容区让给被嵌入的页面。两面 token 自动变位，不写死夜色。 */
.ws-app {
  display: flex;
  flex-direction: column;
  height: 100vh;
  width: 100%;
  overflow: hidden;
  background: var(--face-page);
  color: var(--ink);
  font-family: var(--sans);
}

/* —— Top bar —— */
.ws-topbar {
  display: flex;
  align-items: center;
  gap: var(--s3);
  min-height: 44px;
  height: 44px;
  padding: 0 var(--s3) 0 var(--s2);
  border-bottom: 1px solid var(--line);
  background: var(--face-work);
  flex-shrink: 0;
  z-index: 20;
}

.ws-topbar__brand {
  display: flex;
  align-items: center;
  gap: var(--s2);
  flex-shrink: 0;
}

.ws-topbar__back {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: var(--h-sm);
  height: var(--h-sm);
  border-radius: var(--r-control);
  color: var(--ink-subtle);
  text-decoration: none;
  transition: background var(--t-fast) var(--ease), color var(--t-fast) var(--ease);
}

.ws-topbar__back:hover {
  background: color-mix(in srgb, var(--ink) 8%, transparent);
  color: var(--ink);
}

.ws-topbar__logo {
  display: inline-flex;
  align-items: center;
  gap: var(--s2);
  text-decoration: none;
  color: inherit;
  padding-right: var(--s1);
}

.ws-topbar__logo-icon {
  display: inline-flex;
  color: var(--act);
}

.ws-topbar__logo-text {
  display: flex;
  flex-direction: column;
  line-height: 1.15;
}

.ws-topbar__logo-text strong {
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
  letter-spacing: -0.01em;
}

.ws-topbar__logo-text small {
  font-size: var(--fs-label);
  color: var(--ink-muted);
  font-weight: var(--fw-medium);
}

.ws-func-rail {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: var(--s2);
  overflow-x: auto;
  overflow-y: hidden;
  padding: 2px var(--s1);
  scrollbar-width: thin;
}

.ws-func-rail::-webkit-scrollbar {
  height: 4px;
}

.ws-func-rail::-webkit-scrollbar-thumb {
  background: color-mix(in srgb, var(--ink) 22%, transparent);
  border-radius: var(--r-pill);
}

/* 功能签：三态走声调 —— 未开是静音，已开是轻蓝染，当前是行动色实底 */
.ws-func-chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  flex-shrink: 0;
  height: 28px;
  padding: 0 9px;
  border-radius: var(--r-pill);
  border: 1px solid var(--line);
  background: transparent;
  color: var(--ink-subtle);
  font-size: var(--fs-label);
  font-weight: var(--fw-semibold);
  cursor: pointer;
  transition:
    background var(--t-fast) var(--ease),
    color var(--t-fast) var(--ease),
    border-color var(--t-fast) var(--ease),
    box-shadow var(--t-fast) var(--ease);
}

.ws-func-chip:hover {
  color: var(--ink);
  border-color: color-mix(in srgb, var(--act) 38%, transparent);
  background: var(--act-soft);
}

.ws-func-chip.is-open {
  border-color: color-mix(in srgb, var(--act) 30%, transparent);
  color: var(--ink);
  background: color-mix(in srgb, var(--act) 7%, transparent);
}

.ws-func-chip.is-active {
  color: var(--act-on);
  background: var(--act);
  border-color: transparent;
  box-shadow: var(--shadow-sm);
}

.ws-func-chip__icon {
  opacity: 0.9;
}

/* 行动色实底上，图标洗成 --act-on 同色：夜色底黑、白天底白。
   内联 SVG 自己就吃 currentColor（见 SvgIcon），随 .is-active 的 --act-on 变位，
   不需要再洗一遍。只有取图失败退回 <img> 那一路无法重着色，按面各洗一次。

   注意：scoped 块里的选择器不能以 :global(...) 开头 —— 编译器会把 :global()
   之后的部分整段丢掉，规则落到 <html> 上（曾把浅色整页 filter 成纯白）。
   要带主题前缀就把整条选择器包进 :global()。两个主题互斥，不存在覆盖次序问题。 */
:global([data-theme='dark'] .ws-func-chip.is-active img.svg-icon--fallback) {
  filter: brightness(0) saturate(100%);
}

:global([data-theme='light'] .ws-func-chip.is-active img.svg-icon--fallback) {
  filter: brightness(0) invert(1);
}

.ws-topbar__actions {
  display: flex;
  align-items: center;
  gap: var(--s3);
  flex-shrink: 0;
}

.ws-topbar__hint {
  font-size: var(--fs-label);
  font-weight: var(--fw-semibold);
  color: var(--ink-muted);
  font-variant-numeric: tabular-nums;
}

/* —— Tab bar —— */
.ws-tabbar {
  display: flex;
  align-items: flex-end;
  gap: 2px;
  min-height: 34px;
  height: 34px;
  padding: 0 var(--s2);
  background: var(--face-page);
  border-bottom: 1px solid var(--line);
  overflow-x: auto;
  flex-shrink: 0;
  scrollbar-width: thin;
}

.ws-tab {
  display: inline-flex;
  align-items: center;
  gap: var(--s2);
  flex-shrink: 0;
  max-width: 180px;
  height: 28px;
  margin: 0;
  padding: 0 10px 0 var(--s3);
  border: 1px solid transparent;
  border-radius: var(--r-control) var(--r-control) 0 0;
  background: transparent;
  color: var(--ink-muted);
  font-size: var(--fs-label);
  font-weight: var(--fw-semibold);
  cursor: pointer;
  transition: background var(--t-fast) var(--ease), color var(--t-fast) var(--ease);
}

.ws-tab:hover {
  color: var(--ink);
  background: color-mix(in srgb, var(--ink) 6%, transparent);
}

/* 激活标签：浮到工作面上，底线由行动色点亮，与下方 iframe 无缝相接 */
.ws-tab.is-active {
  color: var(--ink);
  background: var(--face-work);
  border-color: var(--line);
  border-bottom-color: transparent;
  box-shadow: 0 -2px 0 var(--act) inset;
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
  border-radius: var(--r-cell);
  font-size: var(--fs-section);
  line-height: 1;
  color: var(--ink-muted);
  opacity: 0.75;
}

.ws-tab__close:hover {
  background: var(--danger-soft);
  color: var(--danger);
  opacity: 1;
}

/* —— Content —— */
.ws-content {
  position: relative;
  flex: 1;
  min-height: 0;
  background: var(--face-page);
}

.ws-frame {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  border: 0;
  background: var(--face-page);
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
}
</style>
