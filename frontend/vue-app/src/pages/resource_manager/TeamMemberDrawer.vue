<script setup lang="ts">
import { computed } from 'vue';
import type { AssignableUser, Team, TeamMember } from '@/composables/useResourceManager';
import UiButton from '@/components/ui/UiButton.vue';
import UiDrawer from '@/components/ui/UiDrawer.vue';
import UiSearch from '@/components/ui/UiSearch.vue';
import UiSelect from '@/components/ui/UiSelect.vue';

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

/* 库件收 string，桥回父级的受控 props */
const searchModel = computed<string>({
  get: () => props.search,
  set: (value) => emit('update:search', value),
});

const userIdModel = computed<string>({
  get: () => props.add.user_id,
  set: (value) => onAddInput('user_id', value),
});

const roleModel = computed<string>({
  get: () => props.add.role,
  set: (value) => onAddInput('role', value),
});

const userOptions = computed(() => [
  { value: '', label: '选择用户...' },
  ...props.assignableUsers.map((u) => ({
    value: u.id,
    label: u.display_name || u.username || u.id,
  })),
]);

const roleOptions = [
  { value: 'member', label: '成员' },
  { value: 'leader', label: '班组长' },
];
</script>

<template>
  <UiDrawer
    :open="show"
    :title="team?.name || '班组'"
    :width="480"
    flush
    @close="emit('close')"
  >
    <template #header>
      <div class="drawer-heading">
        <div class="drawer-eyebrow">
          班组成员
        </div>
        <h2 id="team-member-drawer-title" class="drawer-title">
          {{ team?.name || '班组' }}
        </h2>
      </div>
    </template>

    <section v-if="canManage" class="drawer-section">
      <div class="add-row">
        <UiSearch
          v-model="searchModel"
          label="搜索可分配用户"
          placeholder="搜索可分配用户..."
        />
        <UiSelect
          v-model="userIdModel"
          :options="userOptions"
          label="选择要添加的用户"
          min-width="100%"
        />
        <UiSelect
          v-model="roleModel"
          :options="roleOptions"
          label="成员角色"
          min-width="100%"
        />
        <label class="drive-label">
          <input
            type="checkbox"
            :checked="add.can_drive"
            @change="onCanDriveChange(($event.target as HTMLInputElement).checked)"
          >
          可驾驶
        </label>
        <UiButton
          variant="primary"
          :disabled="!canAdd"
          @click="emit('add')"
        >
          {{ addBusy ? '添加中...' : '添加' }}
        </UiButton>
      </div>
    </section>

    <section class="drawer-section drawer-section--last">
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
          <UiButton
            v-if="canManage"
            variant="danger"
            size="sm"
            @click="emit('remove', m.user_id)"
          >
            移除
          </UiButton>
        </li>
      </ul>
    </section>
  </UiDrawer>
</template>

<style scoped>
/* 信号面：帽、幕、Esc、关都归 UiDrawer；这里只有分区、线与列表。 */
.drawer-heading {
  min-width: 0;
}

.drawer-eyebrow {
  font-size: var(--fs-label);
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--ink-muted);
}

.drawer-title {
  margin: var(--s1) 0 0;
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.drawer-section {
  padding: var(--s4);
  border-bottom: 1px solid var(--line);
}

.drawer-section--last {
  border-bottom: none;
}

.add-row {
  display: grid;
  grid-template-columns: 1fr;
  gap: var(--s2);
}

.drive-label {
  display: flex;
  align-items: center;
  gap: var(--s2);
  font-size: var(--fs-body);
  color: var(--ink);
}

.drive-label input {
  accent-color: var(--act);
}

.section-heading {
  margin: 0 0 var(--s3);
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.loading-state,
.empty-state {
  padding: var(--s5) 0;
  text-align: center;
  font-size: var(--fs-body);
  color: var(--ink-muted);
}

.member-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: var(--s2);
}

/* 一行一个人：页底凹面 + 一根线，不做成卡 */
.member-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s3);
  padding: var(--s2) var(--s3);
  border: 1px solid var(--line);
  border-radius: var(--r-control);
  background: var(--face-page);
}

.member-name {
  font-size: var(--fs-section);
  font-weight: var(--fw-medium);
  color: var(--ink);
}

.member-meta {
  font-size: var(--fs-label);
  color: var(--ink-muted);
}
</style>
