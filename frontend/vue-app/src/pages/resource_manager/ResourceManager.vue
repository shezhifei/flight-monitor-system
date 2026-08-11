<script setup lang="ts">
import { computed } from 'vue';
import { pageUrl } from '@/shared/page-routes';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import SvgIcon from '@/components/ui/SvgIcon.vue';
import { useResourceManager } from '@/composables/useResourceManager';
import { hasUserPermission, useAuth } from '@/composables/useAuth';
import TeamMemberDrawer from './TeamMemberDrawer.vue';
import TeamTypeModal from './TeamTypeModal.vue';
import EquipmentTypeModal from './EquipmentTypeModal.vue';
import EquipmentStatusModal from './EquipmentStatusModal.vue';

const auth = useAuth();
const canManageTeams = computed(() => hasUserPermission(auth.getUser(), 'team:manage'));
const canManageEquipment = computed(() => hasUserPermission(auth.getUser(), 'equipment:manage'));
const rm = useResourceManager({ loadAssignableUsers: canManageTeams.value });

function handleLogout() { auth.logout(); }

const teamModalShow = computed(() => rm.modal.value.kind === 'team');
const equipmentModalShow = computed(() => rm.modal.value.kind === 'equipment');
const teamTypeModalShow = computed(() => rm.modal.value.kind === 'team-type');
const equipmentTypeModalShow = computed(() => rm.modal.value.kind === 'equipment-type');
const equipmentStatusModalShow = computed(() => rm.modal.value.kind === 'equipment-status');
const teamMembersDrawerShow = computed(() => rm.modal.value.kind === 'team-members');

const editingTeam = computed(() => rm.modal.value.kind === 'team' ? rm.modal.value.item ?? null : null);
const editingEquipment = computed(() => rm.modal.value.kind === 'equipment' ? rm.modal.value.item ?? null : null);
const editingTeamType = computed(() => rm.modal.value.kind === 'team-type' ? rm.modal.value.item ?? null : null);
const editingEquipmentType = computed(() => rm.modal.value.kind === 'equipment-type' ? rm.modal.value.item ?? null : null);
const editingEquipmentStatus = computed(() => rm.modal.value.kind === 'equipment-status' ? rm.modal.value.item : null);
const activeMemberTeam = computed(() => rm.modal.value.kind === 'team-members' ? rm.modal.value.team : null);

function teamStatusLabel(s: string | null | undefined) {
  if (s === 'on_duty') return '在岗';
  if (s === 'off_duty') return '离岗';
  if (s === 'break') return '休息';
  if (s === 'available') return '可用';
  return s || '-';
}

function teamStatusClass(s: string | null | undefined) {
  if (s === 'on_duty') return 'badge badge-on-duty';
  if (s === 'break') return 'badge badge-break';
  if (s === 'available') return 'badge badge-available';
  return 'badge badge-off-duty';
}

function equipmentStatusLabel(s: string | null | undefined) {
  if (s === 'available') return '可用';
  if (s === 'in_use') return '使用中';
  if (s === 'maintenance') return '维护中';
  if (s === 'retired') return '已报废';
  return s || '-';
}

function equipmentStatusClass(s: string | null | undefined) {
  if (s === 'available') return 'badge badge-available';
  if (s === 'in_use') return 'badge badge-in-use';
  if (s === 'maintenance') return 'badge badge-maintenance';
  if (s === 'retired') return 'badge badge-retired';
  return 'badge';
}

function userLabel(u: { display_name?: string; username?: string; id: string }) {
  return u.display_name || u.username || u.id;
}

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
async function confirmDeleteTeamType(id: string, name: string) {
  if (!canManageTeams.value) return;
  if (!window.confirm(`确认删除班组类型「${name}」吗？`)) return;
  await rm.deleteTeamType(id);
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
            <span>班组类型</span>
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
              <div class="search-group">
                <span class="search-icon"><SvgIcon src="/frontend/icons/search.svg" :size="16" /></span>
                <input
                  v-model="rm.teamSearch.value"
                  type="text"
                  class="search-input"
                  placeholder="搜索班组..."
                >
              </div>
              <select v-model="rm.teamTypeFilter.value" class="filter-select">
                <option value="">
                  全部类型
                </option>
                <option v-for="tt in rm.rawTeamTypes.value" :key="tt.id" :value="tt.id">
                  {{ tt.name }}
                </option>
              </select>
              <select v-model="rm.teamStatusFilter.value" class="filter-select">
                <option value="">
                  全部状态
                </option>
                <option value="on_duty">
                  在岗
                </option>
                <option value="off_duty">
                  离岗
                </option>
                <option value="break">
                  休息
                </option>
              </select>
            </div>
            <button v-if="canManageTeams" class="btn btn-primary" type="button" @click="rm.openTeamModal()">
              <span><SvgIcon src="/frontend/icons/add.svg" :size="14" /></span> 新建班组
            </button>
          </div>

          <div class="table-container">
            <table>
              <thead>
                <tr>
                  <th>班组名称</th>
                  <th>类型</th>
                  <th>班组长</th>
                  <th>成员</th>
                  <th>状态</th>
                  <th>当前位置</th>
                  <th style="text-align: right;">
                    操作
                  </th>
                </tr>
              </thead>
              <tbody>
                <tr v-if="rm.loading.value">
                  <td colspan="7" class="empty-state">
                    <div class="loading-spinner" />
                    <p>加载中...</p>
                  </td>
                </tr>
                <tr v-else-if="rm.teams.value.length === 0">
                  <td colspan="7" class="empty-state">
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
                        :style="{ background: team.team_type_color || '#1677ff' }"
                      />
                      {{ team.team_type_name }}
                    </template>
                    <template v-else>
                      -
                    </template>
                  </td>
                  <td>{{ team.leader_name || '-' }}</td>
                  <td>
                    <div class="cell-stack">
                      <span class="badge badge-info">{{ team.member_count || 0 }} 人</span>
                      <button type="button" class="btn btn-secondary btn-sm" @click="rm.openTeamMembersDrawer(team)">
                        {{ canManageTeams ? '管理成员' : '查看成员' }}
                      </button>
                    </div>
                  </td>
                  <td>
                    <span :class="teamStatusClass(team.current_status)">
                      {{ teamStatusLabel(team.current_status) }}
                    </span>
                  </td>
                  <td>{{ team.current_stand_id || team.terminal || '-' }}</td>
                  <td style="text-align: right;">
                    <button v-if="canManageTeams" type="button" class="btn btn-secondary btn-sm" @click="rm.openTeamModal(team)">
                      编辑
                    </button>
                    <button v-if="canManageTeams" type="button" class="btn btn-secondary btn-sm danger" @click="confirmDeleteTeam(team.id, team.name)">
                      删除
                    </button>
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
            </div>
            <div class="content-subtitle">
              维护班组类型及其可执行的作业类型。
            </div>
          </div>
        </div>
        <div class="content-body">
          <div class="section-toolbar">
            <div class="filter-group">
              <div class="search-group">
                <span class="search-icon"><SvgIcon src="/frontend/icons/search.svg" :size="16" /></span>
                <input
                  v-model="rm.teamTypeSearch.value"
                  type="text"
                  class="search-input"
                  placeholder="搜索班组类型..."
                >
              </div>
            </div>
            <button v-if="canManageTeams" class="btn btn-primary" type="button" @click="rm.openTeamTypeModal()">
              <span><SvgIcon src="/frontend/icons/add.svg" :size="14" /></span> 新建类型
            </button>
          </div>

          <div class="table-container">
            <table>
              <thead>
                <tr>
                  <th>名称</th>
                  <th>代码</th>
                  <th>可作业类型</th>
                  <th>关联班组</th>
                  <th style="text-align: right;">
                    操作
                  </th>
                </tr>
              </thead>
              <tbody>
                <tr v-if="rm.teamTypes.value.length === 0">
                  <td colspan="5" class="empty-state">
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
                    <span v-if="tt.is_driver_type" class="badge badge-info" style="margin-left:6px;">司机</span>
                  </td>
                  <td>{{ tt.code || '-' }}</td>
                  <td>{{ (tt.task_types ?? []).join(', ') || '-' }}</td>
                  <td>{{ tt.team_count ?? '-' }}</td>
                  <td style="text-align: right;">
                    <button v-if="canManageTeams" type="button" class="btn btn-secondary btn-sm" @click="rm.openTeamTypeModal(tt)">
                      编辑
                    </button>
                    <button v-if="canManageTeams" type="button" class="btn btn-secondary btn-sm danger" @click="confirmDeleteTeamType(tt.id, tt.name)">
                      删除
                    </button>
                  </td>
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
              <div class="search-group">
                <span class="search-icon"><SvgIcon src="/frontend/icons/search.svg" :size="16" /></span>
                <input
                  v-model="rm.equipmentSearch.value"
                  type="text"
                  class="search-input"
                  placeholder="搜索设备..."
                >
              </div>
              <select v-model="rm.equipmentTypeFilter.value" class="filter-select">
                <option value="">
                  全部类型
                </option>
                <option v-for="et in rm.rawEquipmentTypes.value" :key="et.id" :value="et.id">
                  {{ et.name }}
                </option>
              </select>
              <select v-model="rm.equipmentStatusFilter.value" class="filter-select">
                <option value="">
                  全部状态
                </option>
                <option value="available">
                  可用
                </option>
                <option value="in_use">
                  使用中
                </option>
                <option value="maintenance">
                  维护中
                </option>
                <option value="retired">
                  已报废
                </option>
              </select>
            </div>
            <button v-if="canManageEquipment" class="btn btn-primary" type="button" @click="rm.openEquipmentModal()">
              <span><SvgIcon src="/frontend/icons/add.svg" :size="14" /></span> 新建设备
            </button>
          </div>

          <div class="table-container">
            <table>
              <thead>
                <tr>
                  <th>设备名称</th>
                  <th>类型</th>
                  <th>车牌/编号</th>
                  <th>状态</th>
                  <th>当前位置</th>
                  <th>下次保养</th>
                  <th style="text-align: right;">
                    操作
                  </th>
                </tr>
              </thead>
              <tbody>
                <tr v-if="rm.loading.value">
                  <td colspan="7" class="empty-state">
                    <div class="loading-spinner" />
                    <p>加载中...</p>
                  </td>
                </tr>
                <tr v-else-if="rm.equipment.value.length === 0">
                  <td colspan="7" class="empty-state">
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
                  <td>{{ eq.license_plate || '-' }}</td>
                  <td>
                    <span :class="equipmentStatusClass(eq.status)">
                      {{ equipmentStatusLabel(eq.status) }}
                    </span>
                  </td>
                  <td>{{ eq.current_stand_id || eq.terminal || '-' }}</td>
                  <td>{{ eq.next_maintenance_date || '-' }}</td>
                  <td style="text-align: right;">
                    <button v-if="canManageEquipment" type="button" class="btn btn-secondary btn-sm" @click="rm.openEquipmentModal(eq)">
                      编辑
                    </button>
                    <button v-if="canManageEquipment" type="button" class="btn btn-secondary btn-sm" @click="rm.openEquipmentStatusModal(eq)">
                      状态
                    </button>
                    <button v-if="canManageEquipment" type="button" class="btn btn-secondary btn-sm danger" @click="confirmDeleteEquipment(eq.id, eq.name || eq.code)">
                      删除
                    </button>
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
              <div class="search-group">
                <span class="search-icon"><SvgIcon src="/frontend/icons/search.svg" :size="16" /></span>
                <input
                  v-model="rm.equipmentTypeSearch.value"
                  type="text"
                  class="search-input"
                  placeholder="搜索设备类型..."
                >
              </div>
            </div>
            <button v-if="canManageEquipment" class="btn btn-primary" type="button" @click="rm.openEquipmentTypeModal()">
              <span><SvgIcon src="/frontend/icons/add.svg" :size="14" /></span> 新建类型
            </button>
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
                  <th style="text-align: right;">
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
                  <td style="text-align: right;">
                    <button v-if="canManageEquipment" type="button" class="btn btn-secondary btn-sm" @click="rm.openEquipmentTypeModal(et)">
                      编辑
                    </button>
                    <button v-if="canManageEquipment" type="button" class="btn btn-secondary btn-sm danger" @click="confirmDeleteEquipmentType(et.id, et.name)">
                      删除
                    </button>
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
    </main>

    <!-- Modals & Drawer -->
    <Teleport to="body">
      <div v-if="teamModalShow" class="modal-overlay" @click.self="rm.closeModal()">
        <div
          class="modal-content"
          role="dialog"
          aria-modal="true"
          aria-labelledby="team-modal-title"
        >
          <header class="modal-header">
            <h3 id="team-modal-title">
              {{ editingTeam ? '编辑班组' : '新建班组' }}
            </h3>
            <button
              class="modal-close"
              type="button"
              aria-label="关闭"
              @click="rm.closeModal()"
            >
              ×
            </button>
          </header>
          <div class="modal-body">
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
              <label for="t-type">类型</label>
              <select id="t-type" v-model="rm.teamForm.value.team_type_id">
                <option value="">
                  请选择...
                </option>
                <option v-for="tt in rm.rawTeamTypes.value" :key="tt.id" :value="tt.id">
                  {{ tt.name }}
                </option>
              </select>
            </div>
            <div class="form-group">
              <label for="t-leader">班组长</label>
              <select id="t-leader" v-model="rm.teamForm.value.leader_id">
                <option value="">
                  请选择班组长
                </option>
                <option v-for="u in rm.assignableUsers.value" :key="u.id" :value="u.id">
                  {{ userLabel(u) }}
                </option>
              </select>
            </div>
            <div class="form-group">
              <label for="t-terminal">航站楼</label>
              <input
                id="t-terminal"
                v-model="rm.teamForm.value.terminal"
                type="text"
                placeholder="例如：T1"
              >
            </div>
            <div class="form-group">
              <label for="t-status">状态</label>
              <select id="t-status" v-model="rm.teamForm.value.current_status">
                <option value="available">
                  可用
                </option>
                <option value="on_duty">
                  在岗
                </option>
                <option value="off_duty">
                  离岗
                </option>
                <option value="break">
                  休息
                </option>
              </select>
            </div>
          </div>
          <footer class="modal-footer">
            <button type="button" class="btn btn-secondary" @click="rm.closeModal()">
              取消
            </button>
            <button
              type="button"
              class="btn btn-primary"
              :disabled="!rm.teamForm.value.name.trim() || rm.saving.value"
              @click="onSaveTeam"
            >
              {{ rm.saving.value ? '保存中...' : '保存' }}
            </button>
          </footer>
        </div>
      </div>
    </Teleport>

    <Teleport to="body">
      <div v-if="equipmentModalShow" class="modal-overlay" @click.self="rm.closeModal()">
        <div
          class="modal-content"
          role="dialog"
          aria-modal="true"
          aria-labelledby="equipment-modal-title"
        >
          <header class="modal-header">
            <h3 id="equipment-modal-title">
              {{ editingEquipment ? '编辑设备' : '新建设备' }}
            </h3>
            <button
              class="modal-close"
              type="button"
              aria-label="关闭"
              @click="rm.closeModal()"
            >
              ×
            </button>
          </header>
          <div class="modal-body">
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
              <label for="e-type">类型</label>
              <select id="e-type" v-model="rm.equipmentForm.value.equipment_type_id">
                <option value="">
                  请选择...
                </option>
                <option v-for="et in rm.rawEquipmentTypes.value" :key="et.id" :value="et.id">
                  {{ et.name }}
                </option>
              </select>
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
              <label for="e-terminal">航站楼</label>
              <input
                id="e-terminal"
                v-model="rm.equipmentForm.value.terminal"
                type="text"
                placeholder="例如：T1"
              >
            </div>
            <div class="form-group">
              <label for="e-status">状态</label>
              <select id="e-status" v-model="rm.equipmentForm.value.status">
                <option value="available">
                  可用
                </option>
                <option value="in_use">
                  使用中
                </option>
                <option value="maintenance">
                  维护中
                </option>
                <option value="retired">
                  已报废
                </option>
              </select>
            </div>
            <div class="form-group">
              <label for="e-next">下次保养</label>
              <input id="e-next" v-model="rm.equipmentForm.value.next_maintenance_date" type="date">
            </div>
          </div>
          <footer class="modal-footer">
            <button type="button" class="btn btn-secondary" @click="rm.closeModal()">
              取消
            </button>
            <button
              type="button"
              class="btn btn-primary"
              :disabled="!rm.equipmentForm.value.code.trim() || rm.saving.value"
              @click="onSaveEquipment"
            >
              {{ rm.saving.value ? '保存中...' : '保存' }}
            </button>
          </footer>
        </div>
      </div>
    </Teleport>

    <TeamTypeModal
      :show="teamTypeModalShow"
      :editing="editingTeamType"
      :form="rm.teamTypeForm.value"
      :saving="rm.saving.value"
      @close="rm.closeModal()"
      @save="rm.saveCurrentModal()"
      @update:form="rm.teamTypeForm.value = $event"
    />

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
/* 壳层复用 admin-layout / admin-page；仅保留本页分区与业务控件 */

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

.stat-value.blue { color: var(--ws-primary, var(--system-blue)); }
.stat-value.green { color: var(--ws-success, var(--system-green)); }
.stat-value.orange { color: var(--ws-warn, var(--system-orange)); }

.btn-secondary.danger { color: var(--ws-danger, var(--system-red)); }
.btn-sm { margin-left: 4px; }
.btn-sm:first-child { margin-left: 0; }

.cell-stack { display: inline-flex; align-items: center; gap: 6px; }
.empty-state { text-align: center; padding: 40px 20px; color: var(--admin-text-muted, var(--text-tertiary)); }
.loading-spinner {
  display: inline-block;
  width: 20px;
  height: 20px;
  border: 2px solid var(--admin-border, var(--border-light));
  border-top-color: var(--ws-primary, var(--system-blue));
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
  margin-bottom: 8px;
}
@keyframes spin { to { transform: rotate(360deg); } }

.muted-code { color: var(--admin-text-muted, var(--text-tertiary)); font-size: 12px; }
.team-type-dot {
  display: inline-block;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  margin-right: 6px;
  vertical-align: middle;
}

.badge-on-duty,
.badge-available { background: var(--dh-signal-ok-soft); color: var(--ws-success); }
.badge-off-duty { background: var(--ws-surface-muted); color: var(--admin-text-muted); }
.badge-break { background: var(--dh-signal-warn-soft); color: var(--ws-warn); }
.badge-in-use { background: var(--system-blue-subtle); color: var(--ws-primary); }
.badge-maintenance { background: var(--dh-signal-warn-soft); color: var(--ws-warn); }
.badge-retired { background: var(--error-bg-subtle); color: var(--ws-danger, var(--system-red)); }

/* Inline modals */
.modal-overlay {
  position: fixed;
  inset: 0;
  background: var(--bg-modal, rgba(15, 23, 42, 0.55));
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2100;
}
.modal-content {
  width: 480px;
  max-width: 95vw;
  background: var(--admin-card-bg, var(--bg-card));
  color: var(--admin-text, var(--text-primary));
  border: 1px solid var(--admin-border, var(--border-light));
  border-radius: 12px;
  display: flex;
  flex-direction: column;
  box-shadow: var(--ws-shadow-md, 0 20px 40px rgba(0, 0, 0, 0.28));
}
.modal-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 20px;
  border-bottom: 1px solid var(--admin-border, var(--border-light));
}
.modal-header h3 { margin: 0; font-size: 16px; font-weight: 600; }
.modal-close {
  background: none;
  border: none;
  font-size: 24px;
  line-height: 1;
  cursor: pointer;
  color: var(--admin-text-muted, var(--text-tertiary));
}
.modal-body { padding: 20px; max-height: 60vh; overflow-y: auto; }
.form-group { margin-bottom: 16px; }
.form-group label {
  display: block;
  font-size: 13px;
  font-weight: 500;
  margin-bottom: 6px;
  color: var(--admin-text, var(--text-primary));
}
.required { color: var(--ws-danger, var(--system-red)); }
.form-group input,
.form-group textarea,
.form-group select {
  width: 100%;
  padding: 8px 12px;
  border: 1px solid var(--admin-border, var(--border-light));
  border-radius: 6px;
  font-size: 14px;
  box-sizing: border-box;
  background: var(--ws-surface-muted, var(--bg-input));
  color: var(--admin-text, var(--text-primary));
}
.form-group textarea { min-height: 60px; resize: vertical; }
.modal-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 14px 20px;
  border-top: 1px solid var(--admin-border, var(--border-light));
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
