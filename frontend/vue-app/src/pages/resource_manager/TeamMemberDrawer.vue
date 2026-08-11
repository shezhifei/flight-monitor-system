<script setup lang="ts">
import { computed } from 'vue';
import type { AssignableUser, Team, TeamMember } from '@/composables/useResourceManager';

const props = defineProps<{
  show: boolean;
  team: Team | null;
  members: TeamMember[];
  loading: boolean;
  add: { user_id: string; role: string; can_drive: boolean };
  addBusy: boolean;
  assignableUsers: AssignableUser[];
  search: string;
  canManage: boolean;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'add'): void;
  (e: 'remove', userId: string): void;
  (e: 'update:add', value: { user_id: string; role: string; can_drive: boolean }): void;
  (e: 'update:search', value: string): void;
}>();

const canAdd = computed(() => Boolean(props.add.user_id) && !props.addBusy);

function onAddInput(field: 'user_id' | 'role', value: string) {
  emit('update:add', { ...props.add, [field]: value });
}

function onCanDriveChange(checked: boolean) {
  emit('update:add', { ...props.add, can_drive: checked });
}
</script>

<template>
  <Teleport to="body">
    <div v-if="show" class="drawer-overlay" @click.self="emit('close')">
      <aside
        class="drawer"
        role="dialog"
        aria-modal="true"
        aria-labelledby="team-member-drawer-title"
      >
        <header class="drawer-header">
          <div>
            <div class="drawer-eyebrow">
              班组成员
            </div>
            <h2 id="team-member-drawer-title" class="drawer-title">
              {{ team?.name || '班组' }}
            </h2>
          </div>
          <button
            class="drawer-close"
            type="button"
            aria-label="关闭"
            @click="emit('close')"
          >
            ×
          </button>
        </header>

        <section v-if="canManage" class="drawer-section">
          <div class="add-row">
            <input
              class="search-input"
              type="text"
              :value="search"
              placeholder="搜索可分配用户..."
              @input="emit('update:search', ($event.target as HTMLInputElement).value)"
            >
            <select
              class="user-select"
              :value="add.user_id"
              @change="onAddInput('user_id', ($event.target as HTMLSelectElement).value)"
            >
              <option value="">
                选择用户...
              </option>
              <option v-for="u in assignableUsers" :key="u.id" :value="u.id">
                {{ u.display_name || u.username || u.id }}
              </option>
            </select>
            <select
              class="role-select"
              :value="add.role"
              aria-label="成员角色"
              @change="onAddInput('role', ($event.target as HTMLSelectElement).value)"
            >
              <option value="member">
                成员
              </option>
              <option value="leader">
                班组长
              </option>
            </select>
            <label class="drive-label">
              <input
                type="checkbox"
                :checked="add.can_drive"
                @change="onCanDriveChange(($event.target as HTMLInputElement).checked)"
              >
              可驾驶
            </label>
            <button
              type="button"
              class="btn btn-primary"
              :disabled="!canAdd"
              @click="emit('add')"
            >
              {{ addBusy ? '添加中...' : '添加' }}
            </button>
          </div>
        </section>

        <section class="drawer-section">
          <h3 class="section-heading">
            现有成员 ({{ members.length }})
          </h3>
          <div v-if="loading" class="loading-state">
            加载中...
          </div>
          <div v-else-if="members.length === 0" class="empty-state">
            暂无成员，添加后将参与派工资源分配
          </div>
          <ul v-else class="member-list">
            <li v-for="m in members" :key="m.user_id" class="member-row">
              <div class="member-main">
                <div class="member-name">
                  {{ m.user_display_name || m.username || m.user_id }}
                </div>
                <div class="member-meta">
                  {{ m.role === 'leader' ? '班组长' : '成员' }}
                  <span v-if="m.can_drive"> · 可驾驶</span>
                </div>
              </div>
              <button
                v-if="canManage"
                type="button"
                class="btn btn-secondary btn-sm danger"
                @click="emit('remove', m.user_id)"
              >
                移除
              </button>
            </li>
          </ul>
        </section>
      </aside>
    </div>
  </Teleport>
</template>

<style scoped>
.drawer-overlay {
  position: fixed;
  inset: 0;
  background: rgba(15, 23, 42, 0.35);
  backdrop-filter: blur(2px);
  display: flex;
  justify-content: flex-end;
  z-index: 2200;
}
.drawer {
  width: 480px;
  max-width: 100vw;
  background: var(--bg-card, #fff);
  display: flex;
  flex-direction: column;
  box-shadow: -14px 0 28px rgba(15, 23, 42, 0.16);
}
.drawer-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  padding: 20px 24px;
  border-bottom: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
}
.drawer-eyebrow {
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text-tertiary);
}
.drawer-title {
  margin: 4px 0 0;
  font-size: 18px;
  font-weight: 600;
}
.drawer-close {
  background: none;
  border: none;
  font-size: 24px;
  line-height: 1;
  cursor: pointer;
  color: var(--text-tertiary);
}
.drawer-section {
  padding: 16px 24px;
  border-bottom: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
}
.drawer-section:last-child {
  border-bottom: none;
  flex: 1;
  overflow-y: auto;
}
.add-row {
  display: grid;
  grid-template-columns: 1fr;
  gap: 8px;
}
.add-row .search-input,
.add-row .user-select,
.add-row .role-select {
  padding: 8px 10px;
  border: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
  border-radius: 6px;
  font-size: 13px;
}
.drive-label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: var(--text-primary);
}
.section-heading {
  margin: 0 0 12px;
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}
.loading-state,
.empty-state {
  padding: 24px 0;
  text-align: center;
  font-size: 13px;
  color: var(--text-tertiary);
}
.member-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.member-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  border: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
  border-radius: 8px;
  background: var(--bg-card, #fff);
}
.member-name {
  font-size: 14px;
  font-weight: 500;
  color: var(--text-primary);
}
.member-meta {
  font-size: 12px;
  color: var(--text-tertiary);
}
.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 8px 16px;
  font-size: 13px;
  font-weight: 500;
  border-radius: 8px;
  cursor: pointer;
  border: none;
  white-space: nowrap;
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.btn-primary {
  background: var(--system-blue);
  color: var(--text-inverse);
}
.btn-secondary {
  background: rgba(60, 60, 67, 0.08);
  color: var(--text-primary);
  border: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
}
.btn-secondary.danger {
  color: var(--system-red);
}
.btn-sm {
  padding: 6px 12px;
  font-size: 12px;
}
</style>
