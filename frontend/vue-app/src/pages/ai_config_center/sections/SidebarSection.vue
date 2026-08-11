<script setup lang="ts">
import { pageUrl } from '@/shared/page-routes';
import SvgIcon from '../../../components/ui/SvgIcon.vue';

defineProps<{
  activeTab: 'objects' | 'actions' | 'models';
  objectsCount: number;
  actionsCount: number;
  entitiesCount: number;
  sidebarUser: { name: string; role: string; avatar: string };
}>();
const emit = defineEmits<{
  setTab: [tab: 'objects' | 'actions' | 'models'];
  logout: [];
}>();
</script>

<template>
  <aside class="admin-sidebar">
    <div class="sidebar-header">
      <div class="sidebar-logo">
        <a :href="pageUrl('dashboard')" title="返回工作台">
          <SvgIcon src="/frontend/icons/fast.svg" />
          <span>AI 配置</span>
        </a>
      </div>
    </div>

    <div class="sidebar-nav">
      <div class="nav-section">
        <div class="nav-section-title">
          配置分类
        </div>
        <div class="sidebar-section-stack">
          <a
            class="nav-item"
            :class="{ active: activeTab === 'objects' }"
            @click="emit('setTab', 'objects')"
          >
            <SvgIcon src="/frontend/icons/folder.svg" />
            <span>对象定义</span>
            <span class="nav-badge">{{ objectsCount }}</span>
          </a>
          <a
            class="nav-item"
            :class="{ active: activeTab === 'actions' }"
            @click="emit('setTab', 'actions')"
          >
            <SvgIcon src="/frontend/icons/activity.svg" />
            <span>动作定义</span>
            <span class="nav-badge">{{ actionsCount }}</span>
          </a>
          <a
            class="nav-item"
            :class="{ active: activeTab === 'models' }"
            @click="emit('setTab', 'models')"
          >
            <SvgIcon src="/frontend/icons/activity.svg" />
            <span>模型与工具</span>
            <span class="nav-badge">{{ entitiesCount }}</span>
          </a>
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
        <button class="logout-btn" title="退出登录" @click="emit('logout')">
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
