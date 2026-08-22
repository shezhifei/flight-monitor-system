<script setup lang="ts">
import { computed } from 'vue';
import type { CaseTypeItem } from '../types';
import SvgIcon from '@/components/ui/SvgIcon.vue';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import AdminOverviewList from '@/components/admin/AdminOverviewList.vue';
import AdminOverviewTools from '@/components/admin/AdminOverviewTools.vue';
import type { AdminOverviewItem } from '@/components/admin/adminOverviewTypes';
import { pageUrl } from '@/shared/page-routes';
import { useAuth } from '@/composables/useAuth';

const props = defineProps<{
  connectionStatus: string;
  activeScopeLabel: string;
  activeTenantLabel: string;
  currentScope: string;
  hasDepartmentScope: boolean;
  searchQuery: string;
  caseTypeLoadError: string;
  filteredEventList: CaseTypeItem[];
  selectedCaseId: string | null;
  userName: string;
  userRole: string;
  userAvatar: string;
}>();

const emit = defineEmits<{
  (e: 'switch-scope', scope: 'department' | 'common'): void;
  (e: 'update:search-query', value: string): void;
  (e: 'search'): void;
  (e: 'select-case-type', id: string): void;
  (e: 'create-case'): void;
  (e: 'deprecate-case', id: string): void;
  (e: 'restore-case', id: string): void;
}>();

const auth = useAuth();

const overviewItems = computed<AdminOverviewItem[]>(() =>
  props.filteredEventList.map((item) => ({
    id: item.id,
    title: item.name,
    meta: item.code,
    description: item.description || undefined,
    deprecated: item.is_active === false,
    deletable: true,
  })),
);

function onSearchUpdate(value: string): void {
  emit('update:search-query', value);
  emit('search');
}

function handleLogout(): void {
  auth.logout();
}
</script>

<template>
  <aside class="admin-sidebar">
    <div class="sidebar-header">
      <div class="sidebar-logo">
        <SvgIcon src="/frontend/icons/refresh.svg" :size="20" />
        <span>流程设计</span>
        <div class="connection-status" :title="connectionStatus">
          <span class="dot" />
          <span>{{ connectionStatus }}</span>
        </div>
      </div>
    </div>

    <nav class="sidebar-nav flowable-sidebar-nav">
      <!-- 对齐航班监控 view-switcher：仅互斥两项，无多余标题/摘要 -->
      <div
        class="view-switcher scope-switcher"
        :data-active="currentScope"
        role="group"
        aria-label="部署作用域"
      >
        <div class="view-glider" aria-hidden="true" />
        <button
          class="view-btn scope-switcher-btn"
          type="button"
          :class="{ active: currentScope === 'department' }"
          :disabled="!hasDepartmentScope"
          :title="hasDepartmentScope ? activeScopeLabel : '未配置部门'"
          @click="emit('switch-scope', 'department')"
        >
          当前部门
        </button>
        <button
          class="view-btn scope-switcher-btn"
          type="button"
          :class="{ active: currentScope === 'common' }"
          :title="activeTenantLabel === 'COMMON' ? '通用作用域' : activeTenantLabel"
          @click="emit('switch-scope', 'common')"
        >
          通用
        </button>
      </div>

      <div class="nav-section">
        <div class="nav-section-title">
          事项类型
        </div>
        <AdminOverviewTools
          :model-value="searchQuery"
          placeholder="搜索业务事项..."
          search-aria-label="搜索业务事项"
          create-title="新建业务事项流程"
          compact-create
          @update:model-value="onSearchUpdate"
          @create="emit('create-case')"
        />
      </div>

      <AdminOverviewList
        :items="overviewItems"
        :selected-id="selectedCaseId"
        :error-text="caseTypeLoadError"
        empty-text="当前作用域暂无事项类型"
        aria-label="事项类型列表"
        density="compact"
        show-delete
        action-mode="deprecate"
        delete-title="弃用该类型"
        restore-title="恢复使用"
        @select="emit('select-case-type', $event)"
        @delete="emit('deprecate-case', $event)"
        @restore="emit('restore-case', $event)"
      />
    </nav>

    <div class="sidebar-footer">
      <div class="user-info">
        <div class="user-avatar">
          {{ userAvatar }}
        </div>
        <div class="user-details">
          <div class="user-name">
            {{ userName }}
          </div>
          <div class="user-role">
            {{ userRole }}
          </div>
        </div>
      </div>
      <div class="sidebar-footer-actions">
        <ThemeToggle />
        <button
          type="button"
          class="logout-btn"
          title="退出登录"
          @click="handleLogout"
        >
          <SvgIcon src="/frontend/icons/logout.svg" :size="14" />
        </button>
        <a :href="pageUrl('dashboard')" class="nav-item sidebar-home-link">
          <span class="nav-item-icon"><SvgIcon src="/frontend/icons/home.svg" /></span>
          <span>返回工作台</span>
        </a>
      </div>
    </div>
  </aside>
</template>

<style scoped>
/*
 * 壳层 class（admin-sidebar / sidebar-header / sidebar-nav / sidebar-footer / user-*）
 * 一律走 admin-layout.css 的 --admin-* token，这里只保留流程页特有块。
 */

.sidebar-logo .connection-status {
  margin-left: auto;
}

.connection-status {
  display: inline-flex;
  align-items: center;
  gap: var(--s1);
  padding: var(--s1) var(--s2);
  border-radius: var(--r-pill);
  font-size: var(--fs-label);
  font-weight: var(--fw-semibold);
  line-height: 1.3;
  /* 配置模式是持守身份（动蓝其衬），不是警告 */
  background: var(--act-soft);
  color: var(--act);
  flex-shrink: 0;
}

.connection-status .dot {
  width: 4px;
  height: 4px;
  border-radius: 50%;
  background: currentColor;
}

/* 列表 + 作用域工具需要吃掉侧栏剩余高度 */
.flowable-sidebar-nav {
  overflow: hidden;
  min-height: 0;
  gap: var(--s3);
  padding-top: var(--s3);
}

.flowable-sidebar-nav :deep(.admin-overview-list) {
  flex: 1;
  min-height: 0;
}

.nav-section {
  flex-shrink: 0;
}

.nav-section > .nav-section-title {
  padding-left: 2px;
}

/*
 * 复用航班监控 .view-switcher / .view-glider 骨架；
 * 文字双项等宽 flex，滑块按半宽平移。
 */
.scope-switcher {
  width: 100%;
  flex-shrink: 0;
  box-sizing: border-box;
}

.scope-switcher .view-glider {
  top: 2px;
  left: 2px;
  width: calc((100% - 4px) / 2);
  height: calc(100% - 4px);
  transform: translateX(0);
  background: var(--face-raised);
}

.scope-switcher[data-active='department'] .view-glider {
  transform: translateX(0);
}

.scope-switcher[data-active='common'] .view-glider {
  /* 100% = 滑块自身宽度 = 半轨 */
  transform: translateX(100%);
}

.scope-switcher-btn.view-btn {
  flex: 1 1 50%;
  width: auto;
  height: var(--h-sm);
  min-height: var(--h-sm);
  padding: 0 var(--s2);
  line-height: 1.2;
  font-size: var(--fs-label);
  font-weight: var(--fw-semibold);
  font-family: inherit;
  color: var(--ink-subtle);
}

.scope-switcher-btn.view-btn:hover:not(:disabled) {
  color: var(--ink);
}

.scope-switcher-btn.view-btn.active {
  color: var(--act);
}

.scope-switcher-btn.view-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}
</style>
