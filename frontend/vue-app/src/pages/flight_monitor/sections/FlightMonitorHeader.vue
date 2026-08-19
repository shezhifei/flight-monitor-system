<script setup lang="ts">
import { pageUrl } from '../../../shared/page-routes';

defineProps<{
  ariaAnnouncement: string;
}>();
</script>

<template>
  <div id="header-host">
    <header class="unified-header" role="banner">
      <div class="unified-header__left">
        <a class="unified-header__back" :href="pageUrl('dashboard')" aria-label="返回工作台">
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
            <path d="M9.5 3.5L5 8L9.5 12.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </a>
        <a class="unified-header__brand" :href="pageUrl('dashboard')" aria-label="返回工作台主页">
          <span class="unified-header__brand-icon" aria-hidden="true">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
              <path d="M2 14.5 22 4l-6.8 16-4.1-5.1L7 18l1.1-4.1L2 14.5Z" />
            </svg>
          </span>
          <span class="unified-header__brand-text">Flight Monitor</span>
        </a>
        <div class="unified-header__title-group">
          <h1 class="unified-header__title">航班监控</h1>
          <span class="unified-header__subtitle">实时状态</span>
        </div>
      </div>
      <div class="unified-header__right" />
    </header>
  </div>

  <div id="breadcrumb-host">
    <nav class="unified-breadcrumb" aria-label="面包屑导航">
      <ol class="unified-breadcrumb__list">
        <li class="unified-breadcrumb__item">
          <a class="unified-breadcrumb__link" :href="pageUrl('dashboard')">工作台</a>
        </li>
        <li class="unified-breadcrumb__item unified-breadcrumb__separator" aria-hidden="true">/</li>
        <li class="unified-breadcrumb__item">
          <span class="unified-breadcrumb__current" aria-current="page">航班监控</span>
        </li>
      </ol>
    </nav>
  </div>

  <div id="ariaAnnouncer" class="sr-only" aria-live="polite" aria-atomic="true">
    {{ ariaAnnouncement }}
  </div>

  <a href="#flight-list-main" class="skip-link">跳转到航班列表</a>
</template>

<style scoped>
.unified-header {
  background: var(--face-work);
  border-bottom: 1px solid var(--line);
  color: var(--ink);
}
.unified-header__title {
  color: var(--ink);
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
}
.unified-header__subtitle,
.unified-header__brand-text {
  color: var(--ink-subtle);
}
.unified-breadcrumb {
  background: var(--face-page);
  color: var(--ink-muted);
}
.unified-breadcrumb__current {
  color: var(--ink);
}
.skip-link:focus {
  background: var(--act);
  color: var(--act-on);
}
</style>
