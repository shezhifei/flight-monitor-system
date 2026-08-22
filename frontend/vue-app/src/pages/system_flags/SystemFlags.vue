<script setup lang="ts">
import { pageUrl } from '@/shared/page-routes';
import { computed, ref, watch } from 'vue';
import { useSystemFlags } from '@/composables/useSystemFlags';
import type { SystemFlag } from '@/composables/useSystemFlags';
import { useApi } from '@/composables/useApi';
import { hasUserPermission, useAuth } from '@/composables/useAuth';
import { useToast } from '@/composables/useToast';
import { downloadTextFile } from '@/lib/download';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import UiSkeleton from '@/components/ui/UiSkeleton.vue';
import SvgIcon from '../../components/ui/SvgIcon.vue';
import ConfigFieldList from '@/components/config/ConfigFieldList.vue';
import {
  cleanConfigDescription,
  humanizeConfigPath,
  isSensitiveConfigPath,
  type ConfigFieldItem,
  type ConfigValueType,
} from '@/components/config/configFieldTypes';
import '@/components/config/config-fields.css';

const api = useApi();
const toast = useToast();
const {
  loading,
  error,
  flags,
  categories,
  categoryCounts,
  activeCategory,
  searchQuery,
  fetchFlags,
  updateFlag,
} = useSystemFlags();

const auth = useAuth();
const canConfigure = computed(() => hasUserPermission(auth.getUser(), 'system:config'));

const CATEGORY_META: Record<string, { icon: string; name: string }> = {
  all: { icon: '/frontend/icons/search.svg', name: '全部配置' },
  app: { icon: '/frontend/icons/plane.svg', name: '应用设置' },
  api: { icon: '/frontend/icons/connection.svg', name: 'API 接口' },
  database: { icon: '/frontend/icons/storage.svg', name: '数据库' },
  cache: { icon: '/frontend/icons/fast.svg', name: '缓存策略' },
  monitoring: { icon: '/frontend/icons/bar_chart.svg', name: '监控告警' },
  scheduler: { icon: '/frontend/icons/clock.svg', name: '任务调度' },
  todo: { icon: '/frontend/icons/ok.svg', name: '待办系统' },
  ai: { icon: '/frontend/icons/ai.svg', name: 'AI 模型' },
  general: { icon: '/frontend/icons/settings.svg', name: '通用设置' },
};

const pageTitle = ref('配置中心');

const sidebarUser = computed(() => {
  const user = auth.getUser();
  const name = user?.username || 'Admin';
  const role = user?.is_admin ? '系统管理员' : '普通用户';
  const avatar = name.trim().charAt(0).toUpperCase() || 'A';
  return { name, role, avatar };
});

const navItems = computed(() => categories.value.map((cat) => {
  const meta = CATEGORY_META[cat] || { icon: '/frontend/icons/folder.svg', name: cat };
  return {
    id: cat,
    name: meta.name,
    icon: meta.icon,
    count: categoryCounts.value[cat] || 0,
    active: activeCategory.value === cat,
  };
}));

function flagToField(flag: SystemFlag): ConfigFieldItem {
  const path = flag.path;
  const sensitive = !!flag.masked || isSensitiveConfigPath(path);
  let type: ConfigValueType = 'string';
  if (sensitive) type = 'password';
  else if (flag.type === 'boolean') type = 'boolean';
  else if (flag.type === 'integer' || flag.type === 'float') type = flag.type;
  else if (flag.type === 'list' || Array.isArray(flag.value)) type = 'list';

  const label = flag.label?.trim();
  const title =
    label && label !== path && !/^configuration for/i.test(label)
      ? label
      : humanizeConfigPath(path);

  return {
    id: path,
    title,
    path,
    description: cleanConfigDescription(flag.description, path),
    type,
    value: flag.value,
    masked: flag.masked,
    disabled: !canConfigure.value || !!flag.masked,
  };
}

const fieldItems = computed(() => flags.value.map(flagToField));
const hasSearchQuery = computed(() => searchQuery.value.trim().length > 0);
const resultLabel = computed(() => {
  if (loading.value) return '加载中…';
  if (error.value) return '加载失败';
  const n = fieldItems.value.length;
  if (hasSearchQuery.value) return `匹配 ${n} 项`;
  return `共 ${n} 项`;
});

function handleLogout() {
  auth.logout();
}

function selectCategory(cat: string) {
  activeCategory.value = cat;
  searchQuery.value = '';
  const meta = CATEGORY_META[cat] || { name: '配置列表' };
  pageTitle.value = meta.name;
}

function clearSearch() {
  searchQuery.value = '';
}

watch(searchQuery, (query) => {
  if (query && activeCategory.value !== 'all') {
    activeCategory.value = 'all';
    pageTitle.value = '搜索结果';
  } else if (!query && activeCategory.value === 'all') {
    pageTitle.value = '全部配置';
  }
});

function onFieldChange(id: string, value: unknown) {
  void updateFlag(id, value);
}

async function exportConfig() {
  if (!canConfigure.value) {
    toast.showToast('error', '缺少权限: system:config', { duration: 5000 });
    return;
  }
  try {
    const res = await api.get('/api/v2/system/flags/export');
    if (!res.ok || !res.data) {
      toast.showToast('error', `配置导出失败 (${res.status})`, { duration: 5000 });
      return;
    }
    downloadTextFile({
      content: JSON.stringify(res.data, null, 2),
      filename: 'system-flags-export.json',
      mimeType: 'application/json;charset=utf-8',
    });
    toast.showToast('success', '系统标志配置已导出', { duration: 3200 });
  } catch (err) {
    toast.showToast('error', `配置导出失败: ${err instanceof Error ? err.message : String(err)}`, {
      duration: 5000,
    });
  }
}
</script>

<template>
  <div class="admin-container">
    <aside class="admin-sidebar">
      <div class="sidebar-header">
        <div class="sidebar-logo">
          <SvgIcon src="/frontend/icons/settings.svg" :size="20" />
          <span>系统开关</span>
        </div>
      </div>

      <div class="sidebar-nav">
        <div class="nav-section">
          <div class="nav-section-title">
            配置分类
          </div>
          <div id="nav-list" class="sidebar-section-stack">
            <div
              v-for="item in navItems"
              :key="item.id"
              class="nav-item"
              :aria-current="item.active ? 'page' : undefined"
              role="button"
              tabindex="0"
              @click="selectCategory(item.id)"
              @keydown.enter.prevent="selectCategory(item.id)"
              @keydown.space.prevent="selectCategory(item.id)"
            >
              <span class="nav-item-icon">
                <SvgIcon :src="item.icon" />
              </span>
              <span>{{ item.name }}</span>
              <span class="nav-count">{{ item.count }}</span>
            </div>
            <div v-if="!navItems.length && !loading" class="nav-item nav-item-empty">
              暂无分类
            </div>
          </div>
        </div>
      </div>

      <div class="sidebar-footer">
        <div class="user-info">
          <div id="userAvatar" class="user-avatar">
            {{ sidebarUser.avatar }}
          </div>
          <div class="user-details">
            <div id="userName" class="user-name">
              {{ sidebarUser.name }}
            </div>
            <div id="userRole" class="user-role">
              {{ sidebarUser.role }}
            </div>
          </div>
        </div>
        <div class="sidebar-footer-actions">
          <ThemeToggle />
          <button class="logout-btn" type="button" title="退出登录" @click="handleLogout">
            <SvgIcon src="/frontend/icons/logout.svg" :size="14" />
          </button>
          <a :href="pageUrl('dashboard')" class="nav-item sidebar-home-link">
            <span class="nav-item-icon"><SvgIcon src="/frontend/icons/home.svg" /></span>
            <span>返回工作台</span>
          </a>
        </div>
      </div>
    </aside>

    <main class="main-content">
      <header class="content-header">
        <div class="content-heading">
          <div id="pageTitle" class="content-title">
            {{ pageTitle }}
          </div>
          <div class="content-subtitle" aria-live="polite">
            {{ resultLabel }}
          </div>
        </div>
        <div class="header-actions">
          <button
            v-if="canConfigure"
            type="button"
            class="btn btn-secondary btn-sm"
            @click="exportConfig"
          >
            导出
          </button>
        </div>
      </header>

      <div class="admin-chrome-extra system-flags-chrome">
        <div class="admin-chrome-warn" role="note">
          <SvgIcon src="/frontend/icons/forbidden.svg" :size="14" />
          <span>修改配置可能影响稳定性，请谨慎操作。</span>
        </div>
        <div class="section-toolbar">
          <div class="filter-group">
            <div class="search-group">
              <span class="search-icon" aria-hidden="true">
                <SvgIcon src="/frontend/icons/search.svg" :size="16" />
              </span>
              <input
                id="search-box"
                v-model="searchQuery"
                type="search"
                class="search-input"
                placeholder="搜索 key、名称…"
                aria-label="搜索配置项"
                autocomplete="off"
              >
              <button
                v-if="hasSearchQuery"
                type="button"
                class="search-clear"
                aria-label="清除搜索"
                @click="clearSearch"
              >
                ×
              </button>
            </div>
          </div>
        </div>
      </div>

      <div class="content-body">
        <div id="content-area" class="flags-container">
          <div id="flags-list">
            <template v-if="loading">
              <div class="flags-skeleton" aria-busy="true" aria-label="正在加载系统配置">
                <UiSkeleton v-for="i in 6" :key="i" height="20px" />
              </div>
            </template>
            <template v-else-if="error">
              <div class="empty-state" role="alert">
                <div class="empty-state-title">
                  无法连接服务器
                </div>
                <p>{{ error }}</p>
                <button class="btn btn-primary" type="button" @click="fetchFlags">
                  重试
                </button>
              </div>
            </template>
            <template v-else-if="fieldItems.length">
              <div class="sf-list-head" aria-hidden="true">
                <span>配置项</span>
                <span>值</span>
              </div>
              <ConfigFieldList :items="fieldItems" @change="onFieldChange" />
            </template>
            <template v-else>
              <div class="empty-state">
                <div class="empty-state-title">
                  没有找到相关配置
                </div>
                <p>{{ searchQuery ? '尝试更换关键词或切换分类' : '暂无系统配置' }}</p>
              </div>
            </template>
          </div>
        </div>
      </div>
    </main>
  </div>
</template>

<style scoped>
/* 首轮等待的骨架群：与配置行同构（§3.9），洗光配方只在 UiSkeleton */
.flags-skeleton {
  display: flex;
  flex-direction: column;
  gap: var(--s4);
  padding: var(--s3) var(--s1);
}
</style>
