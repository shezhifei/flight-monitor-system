<script setup lang="ts">
import { computed, watch } from 'vue';
import { pageUrl } from '@/shared/page-routes';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import SvgIcon from '@/components/ui/SvgIcon.vue';
import UiButton from '@/components/ui/UiButton.vue';
import UiModal from '@/components/ui/UiModal.vue';
import UiPill from '@/components/ui/UiPill.vue';
import UiSearch from '@/components/ui/UiSearch.vue';
import UiSelect from '@/components/ui/UiSelect.vue';
import { useResourceManager } from '@/composables/useResourceManager';
import type { Department } from '@/composables/useResourceManager';
import { useTerminalDirectory } from '@/composables/useTerminalDirectory';
import type { BaggageCarousel, Gate, Stand, Terminal } from '@/composables/useTerminalDirectory';
import { useQualificationCatalog } from '@/composables/useQualificationCatalog';
import type { QualificationCatalog } from '@/composables/useQualificationCatalog';
import { hasUserPermission, useAuth } from '@/composables/useAuth';
import TeamMemberDrawer from './TeamMemberDrawer.vue';
import DepartmentsSection from './DepartmentsSection.vue';
import QualificationsSection from './QualificationsSection.vue';
import TerminalDirectorySection from './TerminalDirectorySection.vue';
import EquipmentTypeModal from './EquipmentTypeModal.vue';
import EquipmentStatusModal from './EquipmentStatusModal.vue';

const auth = useAuth();
const canManageTeams = computed(() => hasUserPermission(auth.getUser(), 'team:manage'));
const canManageEquipment = computed(() => hasUserPermission(auth.getUser(), 'equipment:manage'));
const canManageDispatch = computed(() => hasUserPermission(auth.getUser(), 'dispatch:manage'));
const rm = useResourceManager({ loadAssignableUsers: canManageTeams.value });
const td = useTerminalDirectory();
const qc = useQualificationCatalog();

function handleLogout() { auth.logout(); }

const teamModalShow = computed(() => rm.modal.value.kind === 'team');
const equipmentModalShow = computed(() => rm.modal.value.kind === 'equipment');
const equipmentTypeModalShow = computed(() => rm.modal.value.kind === 'equipment-type');
const equipmentStatusModalShow = computed(() => rm.modal.value.kind === 'equipment-status');
const teamMembersDrawerShow = computed(() => rm.modal.value.kind === 'team-members');

const editingTeam = computed(() => rm.modal.value.kind === 'team' ? rm.modal.value.item ?? null : null);
const editingEquipment = computed(() => rm.modal.value.kind === 'equipment' ? rm.modal.value.item ?? null : null);
const editingEquipmentType = computed(() => rm.modal.value.kind === 'equipment-type' ? rm.modal.value.item ?? null : null);
const editingEquipmentStatus = computed(() => rm.modal.value.kind === 'equipment-status' ? rm.modal.value.item : null);
const activeMemberTeam = computed(() => rm.modal.value.kind === 'team-members' ? rm.modal.value.team : null);

type PillTone = 'act' | 'ok' | 'warn' | 'danger' | 'mute';

function teamStatusLabel(s: string | null | undefined) {
  if (s === 'on_duty') return '在岗';
  if (s === 'off_duty') return '离岗';
  if (s === 'break') return '休息';
  if (s === 'available') return '可用';
  return s || '-';
}

function teamStatusTone(s: string | null | undefined): PillTone {
  if (s === 'on_duty' || s === 'available') return 'ok';
  if (s === 'break') return 'warn';
  return 'mute';
}

function equipmentStatusLabel(s: string | null | undefined) {
  if (s === 'available') return '可用';
  if (s === 'in_use') return '使用中';
  if (s === 'maintenance') return '维护中';
  if (s === 'retired') return '已报废';
  return s || '-';
}

function equipmentStatusTone(s: string | null | undefined): PillTone {
  if (s === 'available') return 'ok';
  if (s === 'in_use') return 'act';
  if (s === 'maintenance') return 'warn';
  if (s === 'retired') return 'danger';
  return 'mute';
}

function userLabel(u: { display_name?: string; username?: string; id: string }) {
  return u.display_name || u.username || u.id;
}

/* 工具栏筛选与模态表单的下拉选项 */
const teamTypeFilterOptions = computed(() => [
  { value: '', label: '全部类型' },
  ...rm.rawTeamTypes.value.map((tt) => ({ value: tt.id, label: tt.name })),
]);

const teamStatusFilterOptions = [
  { value: '', label: '全部状态' },
  { value: 'on_duty', label: '在岗' },
  { value: 'off_duty', label: '离岗' },
  { value: 'break', label: '休息' },
];

const equipmentTypeFilterOptions = computed(() => [
  { value: '', label: '全部类型' },
  ...rm.rawEquipmentTypes.value.map((et) => ({ value: et.id, label: et.name })),
]);

const equipmentStatusFilterOptions = [
  { value: '', label: '全部状态' },
  { value: 'available', label: '可用' },
  { value: 'in_use', label: '使用中' },
  { value: 'maintenance', label: '维护中' },
  { value: 'retired', label: '已报废' },
];

const departmentOptions = computed(() => [
  { value: '', label: '请选择科室' },
  ...rm.rawDepartments.value
    .filter((d) => d.is_active !== false)
    .map((d) => ({ value: d.id, label: d.name })),
]);

const leaderOptions = computed(() => [
  { value: '', label: '请选择班组长' },
  ...rm.assignableUsers.value.map((u) => ({ value: u.id, label: userLabel(u) })),
]);

const teamStatusOptions = [
  { value: 'available', label: '可用' },
  { value: 'on_duty', label: '在岗' },
  { value: 'off_duty', label: '离岗' },
  { value: 'break', label: '休息' },
];

const equipmentTypeOptions = computed(() => [
  { value: '', label: '请选择...' },
  ...rm.rawEquipmentTypes.value.map((et) => ({ value: et.id, label: et.name })),
]);

const equipmentStatusOptions = [
  { value: 'available', label: '可用' },
  { value: 'in_use', label: '使用中' },
  { value: 'maintenance', label: '维护中' },
  { value: 'retired', label: '已报废' },
];

async function confirmDeleteTeam(id: string, name: string) {
  if (!canManageTeams.value) return;
  if (!window.confirm(`确认删除班组「${name}」吗？`)) return;
  await rm.deleteTeam(id);
}
async function confirmDeleteEquipment(id: string, name: string) {
  if (!canManageEquipment.value) return;
  if (!window.confirm(`确认删除设备「${name}」吗？`)) return;
  await rm.deleteEquipment(id);
}
async function confirmDeleteEquipmentType(id: string, name: string) {
  if (!canManageEquipment.value) return;
  if (!window.confirm(`确认删除设备类型「${name}」吗？`)) return;
  await rm.deleteEquipmentType(id);
}

async function onAddMember() {
  if (!canManageTeams.value) return;
  const team = activeMemberTeam.value;
  if (!team) return;
  await rm.addTeamMember(team.id);
}
async function onRemoveMember(userId: string) {
  if (!canManageTeams.value) return;
  const team = activeMemberTeam.value;
  if (!team) return;
  if (!window.confirm('确认移除该成员吗？')) return;
  await rm.removeTeamMember(team.id, userId);
}

async function onSaveTeam() {
  if (!canManageTeams.value) return;
  const m = rm.modal.value;
  if (m.kind !== 'team') return;
  const ok = m.item ? await rm.updateTeam(m.item.id, rm.teamForm.value) : await rm.createTeam(rm.teamForm.value);
  if (ok) rm.closeModal();
}

async function onSaveEquipment() {
  if (!canManageEquipment.value) return;
  const m = rm.modal.value;
  if (m.kind !== 'equipment') return;
  const ok = m.item
    ? await rm.updateEquipment(m.item.id, rm.equipmentForm.value)
    : await rm.createEquipment(rm.equipmentForm.value);
  if (ok) rm.closeModal();
}

// ------------- 科室目录 ----------------------------------

const departmentModalShow = computed(() => rm.modal.value.kind === 'department');
const editingDepartment = computed(() =>
  rm.modal.value.kind === 'department' ? rm.modal.value.item ?? null : null,
);

const departmentManagerOptions = computed(() => [
  { value: '', label: '暂不指定' },
  ...rm.assignableUsers.value.map((u) => ({ value: u.id, label: userLabel(u) })),
]);

async function onSaveDepartment() {
  if (!canManageTeams.value) return;
  const m = rm.modal.value;
  if (m.kind !== 'department') return;
  const ok = m.item
    ? await rm.updateDepartment(m.item.id, rm.departmentForm.value)
    : await rm.createDepartment(rm.departmentForm.value);
  if (ok) rm.closeModal();
}

async function onToggleDepartmentActive(dept: Department) {
  if (!canManageTeams.value) return;
  const active = dept.is_active !== false;
  if (!window.confirm(`确认${active ? '停用' : '启用'}科室「${dept.name}」吗？`)) return;
  await rm.setDepartmentActive(dept.id, !active);
}

// ------------- 资质目录 ----------------------------------

const qualificationDepartmentOptions = computed(() => qc.departmentOptions(rm.rawDepartments.value));

watch(
  () => rm.activeSection.value,
  (section) => {
    if (section === 'qualifications' && !qc.selectedDepartmentId.value && rm.rawDepartments.value[0]) {
      void qc.selectDepartment(rm.rawDepartments.value[0].id);
    }
  },
);

async function onSelectQualificationDepartment(id: string) {
  await qc.selectDepartment(id);
}

async function onToggleQualificationActive(item: QualificationCatalog) {
  if (!canManageDispatch.value) return;
  const next = !item.is_active;
  if (!window.confirm(`确认${next ? '启用' : '停用'}资质「${item.qualification_name}」吗？`)) return;
  await qc.setCatalogActive(item, next);
}

// ------------- 空间目录（楼/口/转盘/机位挂楼） -------------

/* 切到空间目录板块时按需装载楼列表 */
watch(
  () => rm.activeSection.value,
  (section) => {
    if (section === 'terminals' && td.terminals.value.length === 0) {
      void td.fetchTerminals();
    }
  },
  { immediate: true },
);

async function onDeactivateTerminal(t: Terminal) {
  if (!canManageDispatch.value) return;
  if (!window.confirm(`确认停用航站楼「${t.name}」吗？存在未结束占用时会被拒绝。`)) return;
  await td.deactivateTerminal(t.terminal_id);
}

async function onDetachStand(s: Stand) {
  if (!canManageDispatch.value) return;
  if (!window.confirm(`确认把机位「${s.code}」从本楼移出吗？存在未结束占用时会被拒绝。`)) return;
  await td.detachStand(s.id);
}

async function onDetachGate(g: Gate) {
  if (!canManageDispatch.value) return;
  if (!window.confirm(`确认把登机口「${g.code}」从本楼移出吗？`)) return;
  await td.detachGate(g.gate_id);
}

async function onDetachCarousel(c: BaggageCarousel) {
  if (!canManageDispatch.value) return;
  if (!window.confirm(`确认把转盘「${c.code}」从本楼移出吗？`)) return;
  await td.detachCarousel(c.carousel_id);
}

async function onDeactivateGate(g: Gate) {
  if (!canManageDispatch.value) return;
  if (!window.confirm(`确认停用登机口「${g.code}」吗？存在未结束分配时会被拒绝。`)) return;
  await td.deactivateGate(g.gate_id);
}

async function onDeactivateCarousel(c: BaggageCarousel) {
  if (!canManageDispatch.value) return;
  if (!window.confirm(`确认停用转盘「${c.code}」吗？存在未结束分配时会被拒绝。`)) return;
  await td.deactivateCarousel(c.carousel_id);
}

async function onDeactivateStand(s: Stand) {
  if (!canManageDispatch.value) return;
  if (!window.confirm(`确认停用机位「${s.code}」吗？存在未结束占用时会被拒绝。`)) return;
  await td.deactivateStand(s.id);
}
</script>

<template>
  <div class="admin-container resource-manager-page">
    <!-- Sidebar -->
    <aside class="admin-sidebar">
      <div class="sidebar-header">
        <div class="sidebar-logo">
          <SvgIcon src="/frontend/icons/users.svg" :size="20" />
          <span>资源管理</span>
        </div>
      </div>

      <nav class="sidebar-nav">
        <div class="nav-section">
          <div class="nav-section-title">
            班组
          </div>
          <button
            type="button"
            class="nav-item"
            :class="{ active: rm.activeSection.value === 'teams' }"
            @click="rm.switchSection('teams')"
          >
            <span class="nav-item-icon"><SvgIcon src="/frontend/icons/users.svg" /></span>
            <span>班组管理</span>
          </button>
          <button
            type="button"
            class="nav-item"
            :class="{ active: rm.activeSection.value === 'team-types' }"
            @click="rm.switchSection('team-types')"
          >
            <span class="nav-item-icon"><SvgIcon src="/frontend/icons/detail.svg" /></span>
            <span>班组类型（只读）</span>
          </button>
        </div>
        <div class="nav-section">
          <div class="nav-section-title">
            目录
          </div>
          <button
            type="button"
            class="nav-item"
            :class="{ active: rm.activeSection.value === 'departments' }"
            @click="rm.switchSection('departments')"
          >
            <span class="nav-item-icon"><SvgIcon src="/frontend/icons/folder.svg" /></span>
            <span>科室目录</span>
          </button>
          <button
            type="button"
            class="nav-item"
            :class="{ active: rm.activeSection.value === 'qualifications' }"
            @click="rm.switchSection('qualifications')"
          >
            <span class="nav-item-icon"><SvgIcon src="/frontend/icons/detail.svg" /></span>
            <span>资质目录</span>
          </button>
          <button
            type="button"
            class="nav-item"
            :class="{ active: rm.activeSection.value === 'terminals' }"
            @click="rm.switchSection('terminals')"
          >
            <span class="nav-item-icon"><SvgIcon src="/frontend/icons/storage.svg" /></span>
            <span>空间目录</span>
          </button>
        </div>
        <div class="nav-section">
          <div class="nav-section-title">
            设备
          </div>
          <button
            type="button"
            class="nav-item"
            :class="{ active: rm.activeSection.value === 'equipment' }"
            @click="rm.switchSection('equipment')"
          >
            <span class="nav-item-icon"><SvgIcon src="/frontend/icons/plane.svg" /></span>
            <span>设备管理</span>
          </button>
          <button
            type="button"
            class="nav-item"
            :class="{ active: rm.activeSection.value === 'equipment-types' }"
            @click="rm.switchSection('equipment-types')"
          >
            <span class="nav-item-icon"><SvgIcon src="/frontend/icons/settings.svg" /></span>
            <span>设备类型</span>
          </button>
        </div>
      </nav>

      <div class="sidebar-footer">
        <div class="user-info">
          <div class="user-avatar">
            {{ rm.sidebarUser.value.initial }}
          </div>
          <div class="user-details">
            <div class="user-name">
              {{ rm.sidebarUser.value.username }}
            </div>
            <div class="user-role">
              {{ rm.sidebarUser.value.role }}
            </div>
          </div>
        </div>
        <div class="sidebar-footer-actions">
          <ThemeToggle />
          <button
            class="logout-btn"
            type="button"
            title="退出"
            aria-label="退出"
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

    <main class="main-content">
      <!-- ========== Teams Section ========== -->
      <section class="section-content" :class="{ active: rm.activeSection.value === 'teams' }">
        <div class="content-header">
          <div class="content-heading">
            <div class="content-title">
              班组管理
            </div>
            <div class="content-subtitle">
              维护班组、成员状态与当前作业位置。
            </div>
          </div>
        </div>

        <div class="content-body">
          <div class="stats-grid">
            <div class="stat-card">
              <div class="stat-label">
                班组总数
              </div>
              <div class="stat-value blue">
                {{ rm.teamsTotal.value }}
              </div>
            </div>
            <div class="stat-card">
              <div class="stat-label">
                在岗班组
              </div>
              <div class="stat-value green">
                {{ rm.rawTeams.value.filter(t => t.current_status === 'on_duty').length }}
              </div>
            </div>
            <div class="stat-card">
              <div class="stat-label">
                班组成员
              </div>
              <div class="stat-value orange">
                {{ rm.rawTeams.value.reduce((sum, t) => sum + (t.member_count || 0), 0) }}
              </div>
            </div>
          </div>

          <div class="section-toolbar">
            <div class="filter-group">
              <UiSearch
                v-model="rm.teamSearch.value"
                label="搜索班组"
                placeholder="搜索班组..."
              />
              <UiSelect
                v-model="rm.teamTypeFilter.value"
                :options="teamTypeFilterOptions"
                label="按班组类型筛选"
              />
              <UiSelect
                v-model="rm.teamStatusFilter.value"
                :options="teamStatusFilterOptions"
                label="按班组状态筛选"
              />
            </div>
            <UiButton
              v-if="canManageTeams"
              variant="primary"
              size="md"
              @click="rm.openTeamModal()"
            >
              <SvgIcon src="/frontend/icons/add.svg" :size="14" /> 新建班组
            </UiButton>
          </div>

          <div class="table-container">
            <table>
              <thead>
                <tr>
                  <th>班组名称</th>
                  <th>类型</th>
                  <th>科室</th>
                  <th>班组长</th>
                  <th>成员</th>
                  <th>状态</th>
                  <th>当前位置</th>
                  <th class="col-actions">
                    操作
                  </th>
                </tr>
              </thead>
              <tbody>
                <tr v-if="rm.loading.value">
                  <td colspan="8" class="empty-state">
                    <div class="loading-spinner" />
                    <p>加载中...</p>
                  </td>
                </tr>
                <tr v-else-if="rm.teams.value.length === 0">
                  <td colspan="8" class="empty-state">
                    暂无班组数据
                  </td>
                </tr>
                <tr v-for="team in rm.teams.value" :key="team.id">
                  <td>
                    <strong>{{ team.name }}</strong>
                    <template v-if="team.code">
                      <br><small class="muted-code">{{ team.code }}</small>
                    </template>
                  </td>
                  <td>
                    <template v-if="team.team_type_name">
                      <span
                        class="team-type-dot"
                        :style="{ background: team.team_type_color ?? undefined }"
                      />
                      {{ team.team_type_name }}
                    </template>
                    <template v-else>
                      -
                    </template>
                  </td>
                  <td>{{ team.department_name || '-' }}</td>
                  <td>{{ team.leader_name || '-' }}</td>
                  <td>
                    <div class="cell-stack">
                      <UiPill tone="act">
                        {{ team.member_count || 0 }} 人
                      </UiPill>
                      <UiButton @click="rm.openTeamMembersDrawer(team)">
                        {{ canManageTeams ? '管理成员' : '查看成员' }}
                      </UiButton>
                    </div>
                  </td>
                  <td>
                    <UiPill :tone="teamStatusTone(team.current_status)">
                      {{ teamStatusLabel(team.current_status) }}
                    </UiPill>
                  </td>
                  <td>{{ team.current_stand_id || '-' }}</td>
                  <td>
                    <div class="row-actions">
                      <UiButton v-if="canManageTeams" @click="rm.openTeamModal(team)">
                        编辑
                      </UiButton>
                      <UiButton v-if="canManageTeams" variant="danger" @click="confirmDeleteTeam(team.id, team.name)">
                        删除
                      </UiButton>
                    </div>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
          <div class="pagination">
            <span class="pagination-info">共 {{ rm.teamsTotal.value }} 条</span>
          </div>
        </div>
      </section>

      <!-- ========== Team Types Section ========== -->
      <section class="section-content" :class="{ active: rm.activeSection.value === 'team-types' }">
        <div class="content-header">
          <div class="content-heading">
            <div class="content-title">
              班组类型
              <UiPill tone="mute" class="readonly-pill">
                已下线 · 只读历史
              </UiPill>
            </div>
            <div class="content-subtitle">
              班组类型已降为只读历史目录（PR2 起写接口返回 410），班组改为直接挂科室；此处仅展示存量数据。
            </div>
          </div>
        </div>
        <div class="content-body">
          <div class="section-toolbar">
            <div class="filter-group">
              <UiSearch
                v-model="rm.teamTypeSearch.value"
                label="搜索班组类型"
                placeholder="搜索班组类型..."
              />
            </div>
          </div>

          <div class="table-container">
            <table>
              <thead>
                <tr>
                  <th>名称</th>
                  <th>代码</th>
                  <th>可作业类型</th>
                  <th>关联班组</th>
                </tr>
              </thead>
              <tbody>
                <tr v-if="rm.teamTypes.value.length === 0">
                  <td colspan="4" class="empty-state">
                    暂无班组类型
                  </td>
                </tr>
                <tr v-for="tt in rm.teamTypes.value" :key="tt.id">
                  <td>
                    <span
                      v-if="tt.color"
                      class="team-type-dot"
                      :style="{ background: tt.color }"
                    />
                    {{ tt.name }}
                    <UiPill v-if="tt.is_driver_type" tone="act" class="driver-pill">
                      司机
                    </UiPill>
                  </td>
                  <td>{{ tt.code || '-' }}</td>
                  <td>{{ (tt.task_types ?? []).join(', ') || '-' }}</td>
                  <td>{{ tt.team_count ?? '-' }}</td>
                </tr>
              </tbody>
            </table>
          </div>
          <div class="pagination">
            <span class="pagination-info">共 {{ rm.teamTypesTotal.value }} 条</span>
          </div>
        </div>
      </section>

      <!-- ========== Equipment Section ========== -->
      <section class="section-content" :class="{ active: rm.activeSection.value === 'equipment' }">
        <div class="content-header">
          <div class="content-heading">
            <div class="content-title">
              设备管理
            </div>
            <div class="content-subtitle">
              维护车辆与设备的状态、位置与下次保养计划。
            </div>
          </div>
        </div>
        <div class="content-body">
          <div class="stats-grid">
            <div class="stat-card">
              <div class="stat-label">
                设备总数
              </div>
              <div class="stat-value blue">
                {{ rm.equipmentTotal.value }}
              </div>
            </div>
            <div class="stat-card">
              <div class="stat-label">
                可用
              </div>
              <div class="stat-value green">
                {{ rm.rawEquipment.value.filter(e => e.status === 'available').length }}
              </div>
            </div>
            <div class="stat-card">
              <div class="stat-label">
                使用中
              </div>
              <div class="stat-value blue">
                {{ rm.rawEquipment.value.filter(e => e.status === 'in_use').length }}
              </div>
            </div>
            <div class="stat-card">
              <div class="stat-label">
                维护中
              </div>
              <div class="stat-value orange">
                {{ rm.rawEquipment.value.filter(e => e.status === 'maintenance').length }}
              </div>
            </div>
          </div>

          <div class="section-toolbar">
            <div class="filter-group">
              <UiSearch
                v-model="rm.equipmentSearch.value"
                label="搜索设备"
                placeholder="搜索设备..."
              />
              <UiSelect
                v-model="rm.equipmentTypeFilter.value"
                :options="equipmentTypeFilterOptions"
                label="按设备类型筛选"
              />
              <UiSelect
                v-model="rm.equipmentStatusFilter.value"
                :options="equipmentStatusFilterOptions"
                label="按设备状态筛选"
              />
            </div>
            <UiButton
              v-if="canManageEquipment"
              variant="primary"
              size="md"
              @click="rm.openEquipmentModal()"
            >
              <SvgIcon src="/frontend/icons/add.svg" :size="14" /> 新建设备
            </UiButton>
          </div>

          <div class="table-container">
            <table>
              <thead>
                <tr>
                  <th>设备名称</th>
                  <th>类型</th>
                  <th>科室</th>
                  <th>车牌/编号</th>
                  <th>状态</th>
                  <th>当前位置</th>
                  <th>下次保养</th>
                  <th class="col-actions">
                    操作
                  </th>
                </tr>
              </thead>
              <tbody>
                <tr v-if="rm.loading.value">
                  <td colspan="8" class="empty-state">
                    <div class="loading-spinner" />
                    <p>加载中...</p>
                  </td>
                </tr>
                <tr v-else-if="rm.equipment.value.length === 0">
                  <td colspan="8" class="empty-state">
                    暂无设备数据
                  </td>
                </tr>
                <tr v-for="eq in rm.equipment.value" :key="eq.id">
                  <td>
                    <strong>{{ eq.code }}</strong>
                    <template v-if="eq.name">
                      <br><small class="muted-code">{{ eq.name }}</small>
                    </template>
                  </td>
                  <td>{{ eq.equipment_type_name || '-' }}</td>
                  <td>{{ eq.department_name || '-' }}</td>
                  <td>{{ eq.license_plate || '-' }}</td>
                  <td>
                    <UiPill :tone="equipmentStatusTone(eq.status)">
                      {{ equipmentStatusLabel(eq.status) }}
                    </UiPill>
                  </td>
                  <td>{{ eq.current_stand_id || '-' }}</td>
                  <td>{{ eq.next_maintenance_date || '-' }}</td>
                  <td>
                    <div class="row-actions">
                      <UiButton v-if="canManageEquipment" @click="rm.openEquipmentModal(eq)">
                        编辑
                      </UiButton>
                      <UiButton v-if="canManageEquipment" @click="rm.openEquipmentStatusModal(eq)">
                        状态
                      </UiButton>
                      <UiButton v-if="canManageEquipment" variant="danger" @click="confirmDeleteEquipment(eq.id, eq.name || eq.code)">
                        删除
                      </UiButton>
                    </div>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
          <div class="pagination">
            <span class="pagination-info">共 {{ rm.equipmentTotal.value }} 条</span>
          </div>
        </div>
      </section>

      <!-- ========== Equipment Types Section ========== -->
      <section class="section-content" :class="{ active: rm.activeSection.value === 'equipment-types' }">
        <div class="content-header">
          <div class="content-heading">
            <div class="content-title">
              设备类型
            </div>
            <div class="content-subtitle">
              维护设备分类及其司机要求。
            </div>
          </div>
        </div>
        <div class="content-body">
          <div class="section-toolbar">
            <div class="filter-group">
              <UiSearch
                v-model="rm.equipmentTypeSearch.value"
                label="搜索设备类型"
                placeholder="搜索设备类型..."
              />
            </div>
            <UiButton
              v-if="canManageEquipment"
              variant="primary"
              size="md"
              @click="rm.openEquipmentTypeModal()"
            >
              <SvgIcon src="/frontend/icons/add.svg" :size="14" /> 新建类型
            </UiButton>
          </div>

          <div class="table-container">
            <table>
              <thead>
                <tr>
                  <th>名称</th>
                  <th>代码</th>
                  <th>分类</th>
                  <th>需要司机</th>
                  <th>关联设备</th>
                  <th class="col-actions">
                    操作
                  </th>
                </tr>
              </thead>
              <tbody>
                <tr v-if="rm.equipmentTypes.value.length === 0">
                  <td colspan="6" class="empty-state">
                    暂无设备类型
                  </td>
                </tr>
                <tr v-for="et in rm.equipmentTypes.value" :key="et.id">
                  <td>{{ et.name }}</td>
                  <td>{{ et.code || '-' }}</td>
                  <td>{{ et.category || '-' }}</td>
                  <td>{{ et.requires_driver ? '是' : '否' }}</td>
                  <td>{{ et.equipment_count ?? '-' }}</td>
                  <td>
                    <div class="row-actions">
                      <UiButton v-if="canManageEquipment" @click="rm.openEquipmentTypeModal(et)">
                        编辑
                      </UiButton>
                      <UiButton v-if="canManageEquipment" variant="danger" @click="confirmDeleteEquipmentType(et.id, et.name)">
                        删除
                      </UiButton>
                    </div>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
          <div class="pagination">
            <span class="pagination-info">共 {{ rm.equipmentTypesTotal.value }} 条</span>
          </div>
        </div>
      </section>

      <!-- ========== Departments Section ========== -->
      <DepartmentsSection
        :active="rm.activeSection.value === 'departments'"
        :can-manage="canManageTeams"
        :departments="rm.departments.value"
        :total="rm.departmentsTotal.value"
        :search="rm.departmentSearch.value"
        :saving="rm.saving.value"
        :modal-open="departmentModalShow"
        :editing="editingDepartment"
        :form="rm.departmentForm.value"
        :manager-options="departmentManagerOptions"
        @update:search="rm.departmentSearch.value = $event"
        @update:form="rm.departmentForm.value = $event"
        @open="rm.openDepartmentModal($event)"
        @close="rm.closeModal()"
        @save="onSaveDepartment"
        @toggle-active="onToggleDepartmentActive"
      />

      <!-- ========== Qualification Catalog Section ========== -->
      <QualificationsSection
        :active="rm.activeSection.value === 'qualifications'"
        :can-manage="canManageDispatch"
        :selected-department-id="qc.selectedDepartmentId.value"
        :catalogs="qc.catalogs.value"
        :search="qc.search.value"
        :loading="qc.loading.value"
        :saving="qc.saving.value"
        :modal="qc.modal.value"
        :form="qc.form.value"
        :level-form="qc.levelForm.value"
        :department-options="qualificationDepartmentOptions"
        :levels-for="qc.levelsFor"
        @update:selected-department-id="onSelectQualificationDepartment"
        @update:search="qc.search.value = $event"
        @update:form="qc.form.value = $event"
        @update:level-form="qc.levelForm.value = $event"
        @open="qc.openQualificationModal($event)"
        @open-level="qc.openLevelModal($event)"
        @close="qc.closeModal()"
        @save="qc.saveCurrentModal()"
        @toggle-active="onToggleQualificationActive"
      />

      <!-- ========== Terminal Directory Section ========== -->
      <TerminalDirectorySection
        :active="rm.activeSection.value === 'terminals'"
        :can-manage="canManageDispatch"
        :terminals="td.terminals.value"
        :loading="td.loading.value"
        :saving="td.saving.value"
        :terminal-search="td.terminalSearch.value"
        :selected-terminal-id="td.selectedTerminalId.value"
        :directory="td.directory.value"
        :context-loading="td.contextLoading.value"
        :attachable-stands="td.attachableStands.value"
        :attach-stand-id="td.attachStandId.value"
        :modal="td.modal.value"
        :terminal-form="td.terminalForm.value"
        :gate-form="td.gateForm.value"
        :carousel-form="td.carouselForm.value"
        :stand-form="td.standForm.value"
        @update:terminal-search="td.terminalSearch.value = $event"
        @update:attach-stand-id="td.attachStandId.value = $event"
        @update:terminal-form="td.terminalForm.value = $event"
        @update:gate-form="td.gateForm.value = $event"
        @update:carousel-form="td.carouselForm.value = $event"
        @update:stand-form="td.standForm.value = $event"
        @select="td.selectTerminal($event)"
        @open-terminal="td.openTerminalModal($event)"
        @open-gate="td.openGateModal($event)"
        @open-carousel="td.openCarouselModal($event)"
        @open-stand="td.openStandModal($event)"
        @open-attach-stand="td.openAttachStandModal()"
        @close="td.closeModal()"
        @save="td.saveCurrentModal()"
        @deactivate-terminal="onDeactivateTerminal"
        @detach-stand="onDetachStand"
        @detach-gate="onDetachGate"
        @detach-carousel="onDetachCarousel"
        @deactivate-gate="onDeactivateGate"
        @deactivate-carousel="onDeactivateCarousel"
        @deactivate-stand="onDeactivateStand"
        @reactivate-gate="td.reactivateGate($event.gate_id)"
        @reactivate-carousel="td.reactivateCarousel($event.carousel_id)"
        @reactivate-stand="td.reactivateStand($event.id)"
      />
    </main>

    <!-- Modals & Drawer：帽幕关一律走库件 -->
    <UiModal
      :open="teamModalShow"
      :title="editingTeam ? '编辑班组' : '新建班组'"
      :width="480"
      @close="rm.closeModal()"
    >
      <div class="form-group">
        <label for="t-name">名称 <span class="required">*</span></label>
        <input
          id="t-name"
          v-model="rm.teamForm.value.name"
          type="text"
          placeholder="例如：地服一组"
        >
      </div>
      <div class="form-group">
        <label for="t-code">代码</label>
        <input
          id="t-code"
          v-model="rm.teamForm.value.code"
          type="text"
          placeholder="例如：GROUND-01"
        >
      </div>
      <div class="form-group">
        <UiSelect
          v-model="rm.teamForm.value.department_id"
          :options="departmentOptions"
          label="所属科室（必填）"
          min-width="100%"
        />
      </div>
      <div class="form-group">
        <UiSelect
          v-model="rm.teamForm.value.leader_id"
          :options="leaderOptions"
          label="班组长"
          min-width="100%"
        />
      </div>
      <div class="form-group">
        <UiSelect
          v-model="rm.teamForm.value.current_status"
          :options="teamStatusOptions"
          label="班组状态"
          min-width="100%"
        />
      </div>
      <template #footer>
        <UiButton size="md" @click="rm.closeModal()">
          取消
        </UiButton>
        <UiButton
          size="md"
          variant="primary"
          :disabled="!rm.teamForm.value.name.trim() || !rm.teamForm.value.department_id || rm.saving.value"
          @click="onSaveTeam"
        >
          {{ rm.saving.value ? '保存中...' : '保存' }}
        </UiButton>
      </template>
    </UiModal>

    <UiModal
      :open="equipmentModalShow"
      :title="editingEquipment ? '编辑设备' : '新建设备'"
      :width="480"
      @close="rm.closeModal()"
    >
      <div class="form-group">
        <label for="e-code">代码 <span class="required">*</span></label>
        <input
          id="e-code"
          v-model="rm.equipmentForm.value.code"
          type="text"
          placeholder="例如：TUG-01"
        >
      </div>
      <div class="form-group">
        <label for="e-name">名称</label>
        <input
          id="e-name"
          v-model="rm.equipmentForm.value.name"
          type="text"
          placeholder="例如：一号牵引车"
        >
      </div>
      <div class="form-group">
        <UiSelect
          v-model="rm.equipmentForm.value.equipment_type_id"
          :options="equipmentTypeOptions"
          label="设备类型"
          min-width="100%"
        />
      </div>
      <div class="form-group">
        <UiSelect
          v-model="rm.equipmentForm.value.department_id"
          :options="departmentOptions"
          label="所属科室（必填）"
          min-width="100%"
        />
      </div>
      <div class="form-group">
        <label for="e-plate">车牌</label>
        <input
          id="e-plate"
          v-model="rm.equipmentForm.value.license_plate"
          type="text"
          placeholder="可选"
        >
      </div>
      <div class="form-group">
        <UiSelect
          v-model="rm.equipmentForm.value.status"
          :options="equipmentStatusOptions"
          label="设备状态"
          min-width="100%"
        />
      </div>
      <div class="form-group">
        <label for="e-next">下次保养</label>
        <input id="e-next" v-model="rm.equipmentForm.value.next_maintenance_date" type="date">
      </div>
      <template #footer>
        <UiButton size="md" @click="rm.closeModal()">
          取消
        </UiButton>
        <UiButton
          size="md"
          variant="primary"
          :disabled="!rm.equipmentForm.value.code.trim() || !rm.equipmentForm.value.department_id || rm.saving.value"
          @click="onSaveEquipment"
        >
          {{ rm.saving.value ? '保存中...' : '保存' }}
        </UiButton>
      </template>
    </UiModal>

    <EquipmentTypeModal
      :show="equipmentTypeModalShow"
      :editing="editingEquipmentType"
      :form="rm.equipmentTypeForm.value"
      :saving="rm.saving.value"
      @close="rm.closeModal()"
      @save="rm.saveCurrentModal()"
      @update:form="rm.equipmentTypeForm.value = $event"
    />

    <EquipmentStatusModal
      :show="equipmentStatusModalShow"
      :equipment="editingEquipmentStatus"
      :form="rm.equipmentStatusForm.value"
      :saving="rm.saving.value"
      @close="rm.closeModal()"
      @save="rm.saveCurrentModal()"
      @update:form="rm.equipmentStatusForm.value = $event"
    />

    <TeamMemberDrawer
      :show="teamMembersDrawerShow"
      :team="activeMemberTeam"
      :members="rm.teamMembers.value"
      :loading="rm.teamMembersLoading.value"
      :add="rm.teamMemberAdd.value"
      :add-busy="rm.teamMemberAddBusy.value"
      :assignable-users="rm.filteredAssignableUsers.value"
      :search="rm.memberSearch.value"
      :can-manage="canManageTeams"
      @close="rm.closeModal()"
      @add="onAddMember"
      @remove="onRemoveMember"
      @update:add="rm.teamMemberAdd.value = $event"
      @update:search="rm.memberSearch.value = $event"
    />
  </div>
</template>

<style scoped>
/* 壳层复用 admin-layout / admin-page；帽幕关归 UiModal，按钮归 UiButton，
   状态章归 UiPill，搜索归 UiSearch，下拉归 UiSelect。 */

.section-content {
  display: none;
  flex: 1;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}

.section-content.active {
  display: flex;
}

.section-content .content-header {
  flex-shrink: 0;
}

.section-content .content-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

.stat-value.blue { color: var(--act); }
.stat-value.green { color: var(--ok); }
.stat-value.orange { color: var(--warn); }

.cell-stack {
  display: inline-flex;
  align-items: center;
  gap: var(--s2);
}

.row-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--s1);
}

/* 操作列表头右对齐，与行内 .row-actions 同一方向 */
.col-actions {
  text-align: right;
}

.driver-pill {
  margin-left: var(--s2);
}

.readonly-pill {
  margin-left: var(--s2);
  vertical-align: middle;
}

.loading-spinner {
  display: inline-block;
  width: 20px;
  height: 20px;
  border: 2px solid var(--line-strong);
  border-top-color: var(--act);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
  margin-bottom: var(--s2);
}

.muted-code {
  color: var(--ink-muted);
  font-size: var(--fs-label);
}

.team-type-dot {
  display: inline-block;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  margin-right: var(--s2);
  vertical-align: middle;
  /* 后端未给色时的底声：动蓝，不再在模板里写死 hex 兕底 */
  background: var(--act);
}

/* UiModal 身内表单 */
.form-group {
  margin-bottom: var(--s3);
}

.form-group > label {
  display: block;
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  margin-bottom: var(--s1);
  color: var(--ink-subtle);
}

.required {
  color: var(--danger);
}

.form-group input {
  width: 100%;
  height: var(--h-md);
  padding: 0 var(--s3);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-control);
  font-size: var(--fs-body);
  box-sizing: border-box;
  background: var(--face-page);
  color: var(--ink);
  font-family: inherit;
}

.form-group input:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}

/* 侧栏 button.nav-item 去掉默认 button 样式 */
:deep(.admin-sidebar button.nav-item) {
  width: 100%;
  border: none;
  background: transparent;
  font: inherit;
  text-align: left;
  color: inherit;
}
</style>
