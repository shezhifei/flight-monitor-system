<script setup lang="ts">
import { pageUrl } from '@/shared/page-routes';
import { useUserManager } from '@/composables/useUserManager';
import { useAuth } from '@/composables/useAuth';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import SvgIcon from '@/components/ui/SvgIcon.vue';
import UserModal from './UserModal.vue';
import RoleModal from './RoleModal.vue';
import TemplateModal from './TemplateModal.vue';

const {
  activeSection,
  loading,
  savingUser,
  savingRole,
  savingTemplate,
  filteredUsers,
  filteredRoles,
  filteredPermissions,
  filteredTemplates,
  permissions,
  roles,
  templates,
  departmentSuggestions,
  sidebarUser,
  searchQuery,
  roleSearch,
  permissionSearch,
  templateSearch,
  switchSection,
  fetchUsers,
  fetchRoles,
  fetchPermissions,
  fetchTemplates,
  // user modal
  showUserModal,
  editingUser,
  userForm,
  openCreateUserModal,
  openEditUserModal,
  closeUserModal,
  saveUser,
  deleteUser,
  // role modal
  showRoleModal,
  editingRole,
  roleForm,
  openCreateRoleModal,
  openEditRoleModal,
  closeRoleModal,
  saveRole,
  deleteRole,
  applyTemplateToRoleForm,
  // template modal
  showTemplateModal,
  editingTemplate,
  templateForm,
  openCreateTemplateModal,
  openEditTemplateModal,
  closeTemplateModal,
  saveTemplate,
  deleteTemplate,
  roleNamesOf,
} = useUserManager();

const auth = useAuth();
function handleLogout() { auth.logout(); }

function refreshCurrent(): void {
  if (activeSection.value === 'users') void fetchUsers();
  else if (activeSection.value === 'roles') void fetchRoles();
  else if (activeSection.value === 'permissions') void fetchPermissions();
  else if (activeSection.value === 'templates') void fetchTemplates();
}

function formatLastLogin(user: { last_login_at?: string; last_login?: string; lastLogin?: string }): string {
  const raw = user.last_login_at || user.last_login || user.lastLogin;
  if (!raw) return '—';
  const ts = Date.parse(raw);
  if (Number.isNaN(ts)) return raw;
  return new Date(ts).toLocaleString('zh-CN');
}

function permissionLabel(perm: { name?: string; code?: string }): string {
  return perm.name || perm.code || '—';
}

function permissionActive(perm: { is_active?: boolean; status?: string }): boolean {
  if (typeof perm.is_active === 'boolean') return perm.is_active;
  if (typeof perm.status === 'string') {
    return !['disabled', 'inactive', 'false'].includes(perm.status.toLowerCase());
  }
  return true;
}

function templatePermissionCount(tmpl: {
  permissions?: unknown[];
  permission_ids?: unknown[];
}): number {
  if (Array.isArray(tmpl.permissions)) return tmpl.permissions.length;
  if (Array.isArray(tmpl.permission_ids)) return tmpl.permission_ids.length;
  return 0;
}
</script>

<template>
  <div class="admin-container">
    <!-- Sidebar -->
    <aside class="admin-sidebar">
      <div class="sidebar-header">
        <div class="sidebar-logo">
          <SvgIcon src="/frontend/icons/users.svg" :size="20" />
          <span>用户管理</span>
        </div>
      </div>

      <div class="sidebar-nav">
        <div class="nav-section">
          <div class="nav-section-title">
            用户与权限
          </div>
          <div
            class="nav-item"
            :class="{ active: activeSection === 'users' }"
            data-section="users"
            @click="switchSection('users')"
          >
            <span class="nav-item-icon"><SvgIcon src="/frontend/icons/users.svg" /></span>
            <span>用户列表</span>
          </div>
          <div
            class="nav-item"
            :class="{ active: activeSection === 'roles' }"
            data-section="roles"
            @click="switchSection('roles')"
          >
            <span class="nav-item-icon"><SvgIcon src="/frontend/icons/lock.svg" /></span>
            <span>角色管理</span>
          </div>
          <div
            class="nav-item"
            :class="{ active: activeSection === 'permissions' }"
            data-section="permissions"
            @click="switchSection('permissions')"
          >
            <span class="nav-item-icon"><SvgIcon src="/frontend/icons/security.svg" /></span>
            <span>权限列表</span>
          </div>
        </div>

        <div class="nav-section">
          <div class="nav-section-title">
            模板
          </div>
          <div
            class="nav-item"
            :class="{ active: activeSection === 'templates' }"
            data-section="templates"
            @click="switchSection('templates')"
          >
            <span class="nav-item-icon"><SvgIcon src="/frontend/icons/detail.svg" /></span>
            <span>模板管理</span>
          </div>
        </div>
      </div>

      <div class="sidebar-footer">
        <div class="user-info">
          <div class="user-avatar">
            {{ sidebarUser?.initial ?? '·' }}
          </div>
          <div class="user-details">
            <div class="user-name">
              {{ sidebarUser?.username ?? '加载中...' }}
            </div>
            <div class="user-role">
              <span v-if="sidebarUser?.is_admin" class="badge badge-admin">管理员</span>
              <span v-else-if="sidebarUser?.role">{{ sidebarUser.role }}</span>
              <span v-else>—</span>
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

    <!-- Main Content -->
    <main class="main-content">
      <header class="content-header">
        <div class="content-heading">
          <div class="content-title">
            <template v-if="activeSection === 'users'">
              用户列表
            </template>
            <template v-else-if="activeSection === 'roles'">
              角色管理
            </template>
            <template v-else-if="activeSection === 'permissions'">
              权限列表
            </template>
            <template v-else>
              模板管理
            </template>
          </div>
          <div class="content-subtitle">
            查看账号状态、分配角色权限，并维护权限模板。
          </div>
        </div>
        <div class="header-actions">
          <button type="button" class="btn btn-secondary" @click="refreshCurrent">
            <SvgIcon src="/frontend/icons/refresh.svg" :size="14" /> 刷新
          </button>
          <button
            v-if="activeSection === 'users'"
            type="button"
            class="btn btn-primary"
            @click="openCreateUserModal"
          >
            <SvgIcon src="/frontend/icons/add.svg" :size="14" /> 添加用户
          </button>
          <button
            v-else-if="activeSection === 'roles'"
            type="button"
            class="btn btn-primary"
            @click="openCreateRoleModal"
          >
            <SvgIcon src="/frontend/icons/add.svg" :size="14" /> 添加角色
          </button>
          <button
            v-else-if="activeSection === 'templates'"
            type="button"
            class="btn btn-primary"
            @click="openCreateTemplateModal"
          >
            <SvgIcon src="/frontend/icons/add.svg" :size="14" /> 添加模板
          </button>
        </div>
      </header>

      <div class="content-body">
        <!-- Users Section -->
        <div class="section-content" :class="{ active: activeSection === 'users' }">
          <div class="section-toolbar">
            <div class="search-group">
              <span class="search-icon"><SvgIcon src="/frontend/icons/search.svg" :size="16" /></span>
              <input
                v-model="searchQuery"
                type="search"
                class="search-input"
                placeholder="搜索用户名或邮箱..."
              >
            </div>
          </div>

          <div class="table-container">
            <table>
              <thead>
                <tr>
                  <th>用户名</th>
                  <th>邮箱</th>
                  <th>角色</th>
                  <th>状态</th>
                  <th>上次登录</th>
                  <th>操作</th>
                </tr>
              </thead>
              <tbody>
                <tr v-if="loading">
                  <td colspan="6" class="empty-placeholder">
                    数据加载中...
                  </td>
                </tr>
                <tr v-else-if="filteredUsers.length === 0">
                  <td colspan="6" class="empty-placeholder">
                    暂无用户数据
                  </td>
                </tr>
                <tr v-for="user in filteredUsers" :key="user.id">
                  <td>
                    <strong>{{ user.username }}</strong>
                    <span v-if="user.is_admin" class="badge badge-admin">Admin</span>
                  </td>
                  <td>{{ user.email || '—' }}</td>
                  <td>
                    <div class="role-tags">
                      <span
                        v-for="roleName in roleNamesOf(user)"
                        :key="roleName"
                        class="role-tag"
                        :class="{ admin: roleName === 'admin' }"
                      >{{ roleName }}</span>
                      <span v-if="roleNamesOf(user).length === 0">—</span>
                    </div>
                  </td>
                  <td>
                    <span v-if="user.is_active === false" class="badge badge-muted">已停用</span>
                    <span v-else class="badge badge-active">启用</span>
                  </td>
                  <td>{{ formatLastLogin(user) }}</td>
                  <td>
                    <button type="button" class="btn btn-secondary btn-sm" @click="openEditUserModal(user)">
                      编辑
                    </button>
                    <button type="button" class="btn btn-secondary btn-sm" @click="deleteUser(user.id)">
                      删除
                    </button>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>

          <div class="pagination-info">
            共 {{ filteredUsers.length }} 条
          </div>
        </div>

        <!-- Roles Section -->
        <div class="section-content" :class="{ active: activeSection === 'roles' }">
          <div class="section-toolbar">
            <div class="search-group">
              <span class="search-icon"><SvgIcon src="/frontend/icons/search.svg" :size="16" /></span>
              <input
                v-model="roleSearch"
                type="search"
                class="search-input"
                placeholder="搜索角色..."
              >
            </div>
          </div>

          <div class="table-container">
            <table>
              <thead>
                <tr>
                  <th>角色名</th>
                  <th>描述</th>
                  <th>权限数</th>
                  <th>用户数</th>
                  <th>类型</th>
                  <th>操作</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="role in filteredRoles" :key="role.id">
                  <td><strong>{{ role.name }}</strong></td>
                  <td>{{ role.description || '—' }}</td>
                  <td>{{ Array.isArray(role.permissions) ? role.permissions.length : 0 }}</td>
                  <td>{{ role.user_count ?? 0 }}</td>
                  <td>
                    <span v-if="role.is_system" class="badge badge-info">系统</span>
                    <span v-else class="badge badge-muted">自定义</span>
                  </td>
                  <td>
                    <button type="button" class="btn btn-secondary btn-sm" @click="openEditRoleModal(role)">
                      编辑
                    </button>
                    <button
                      type="button"
                      class="btn btn-secondary btn-sm"
                      :disabled="role.is_system"
                      :title="role.is_system ? '系统角色不可删除' : '删除角色'"
                      @click="deleteRole(role.id)"
                    >
                      删除
                    </button>
                  </td>
                </tr>
                <tr v-if="!filteredRoles.length">
                  <td colspan="6" class="empty-placeholder">
                    暂无角色
                  </td>
                </tr>
              </tbody>
            </table>
          </div>

          <div class="pagination-info">
            共 {{ filteredRoles.length }} 条
          </div>
        </div>

        <!-- Permissions Section -->
        <div class="section-content" :class="{ active: activeSection === 'permissions' }">
          <div class="section-toolbar">
            <div class="search-group">
              <span class="search-icon"><SvgIcon src="/frontend/icons/search.svg" :size="16" /></span>
              <input
                v-model="permissionSearch"
                type="search"
                class="search-input"
                placeholder="搜索权限名称..."
              >
            </div>
          </div>

          <div class="table-container">
            <table>
              <thead>
                <tr>
                  <th>权限名称</th>
                  <th>描述</th>
                  <th>状态</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="perm in filteredPermissions" :key="perm.id || permissionLabel(perm)">
                  <td><code>{{ permissionLabel(perm) }}</code></td>
                  <td>{{ perm.description || '—' }}</td>
                  <td>
                    <span v-if="permissionActive(perm)" class="badge badge-active">启用</span>
                    <span v-else class="badge badge-muted">禁用</span>
                  </td>
                </tr>
                <tr v-if="!filteredPermissions.length">
                  <td colspan="3" class="empty-placeholder">
                    暂无权限
                  </td>
                </tr>
              </tbody>
            </table>
          </div>

          <div class="pagination-info">
            共 {{ filteredPermissions.length }} 条
          </div>
        </div>

        <!-- Templates Section -->
        <div class="section-content" :class="{ active: activeSection === 'templates' }">
          <div class="section-toolbar">
            <div class="search-group">
              <span class="search-icon"><SvgIcon src="/frontend/icons/search.svg" :size="16" /></span>
              <input
                v-model="templateSearch"
                type="search"
                class="search-input"
                placeholder="搜索模板名称..."
              >
            </div>
          </div>

          <div class="table-container">
            <table>
              <thead>
                <tr>
                  <th>模板名称</th>
                  <th>模板代码</th>
                  <th>分类</th>
                  <th>权限数量</th>
                  <th>操作</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="tmpl in filteredTemplates" :key="tmpl.id">
                  <td>{{ tmpl.name }}</td>
                  <td>{{ tmpl.code || '—' }}</td>
                  <td>{{ tmpl.category || '—' }}</td>
                  <td>{{ templatePermissionCount(tmpl) }}</td>
                  <td>
                    <span v-if="tmpl.is_system" class="badge badge-info">系统</span>
                    <span v-else class="badge badge-muted">自定义</span>
                    <button type="button" class="btn btn-secondary btn-sm" @click="openEditTemplateModal(tmpl)">
                      编辑
                    </button>
                    <button
                      v-if="!tmpl.is_system"
                      type="button"
                      class="btn btn-secondary btn-sm"
                      @click="deleteTemplate(tmpl.id)"
                    >
                      删除
                    </button>
                  </td>
                </tr>
                <tr v-if="!filteredTemplates.length">
                  <td colspan="5" class="empty-placeholder">
                    暂无模板
                  </td>
                </tr>
              </tbody>
            </table>
          </div>

          <div class="pagination-info">
            共 {{ filteredTemplates.length }} 条
          </div>
        </div>
      </div>
    </main>

    <UserModal
      :show="showUserModal"
      :editing="editingUser"
      :form="userForm"
      :roles="roles"
      :department-suggestions="departmentSuggestions"
      :saving="savingUser"
      @close="closeUserModal"
      @save="saveUser"
      @update:form="userForm = $event"
    />

    <RoleModal
      :show="showRoleModal"
      :editing="editingRole"
      :form="roleForm"
      :permissions="permissions"
      :templates="templates"
      :saving="savingRole"
      @close="closeRoleModal"
      @save="saveRole"
      @update:form="roleForm = $event"
      @apply-template="(id, mode) => applyTemplateToRoleForm(id, mode)"
    />

    <TemplateModal
      :show="showTemplateModal"
      :editing="editingTemplate"
      :form="templateForm"
      :permissions="permissions"
      :saving="savingTemplate"
      @close="closeTemplateModal"
      @save="saveTemplate"
      @update:form="templateForm = $event"
    />
  </div>
</template>

<style scoped>
/* 壳层 / 侧栏 / 顶栏 / 表格 / 搜索 / 按钮 全部复用 admin-layout + admin-page */

.section-content {
  display: none;
}

.section-content.active {
  display: block;
}

/* 身份用动蓝（管理员是身份不是警告）；事态用四声其衬；停用走中性墨 */
.badge-admin {
  background: var(--act-soft);
  color: var(--act);
}

/* 表内 Admin 章紧跟用户名，间距走梯 */
td .badge-admin {
  margin-left: var(--s1);
}

/* 侧栏徽标图标与文字的间距：SvgIcon 已自带 em 兑基线，这里只补右距 */
.sidebar-logo .svg-icon {
  margin-right: var(--s2);
}

.badge-active {
  background: var(--ok-soft);
  color: var(--ok);
}

.badge-muted {
  background: color-mix(in srgb, var(--ink) 8%, transparent);
  color: var(--ink-muted);
}

.role-tags {
  display: flex;
  flex-wrap: wrap;
  gap: var(--s1);
}

.role-tag {
  display: inline-block;
  padding: 2px var(--s2);
  border-radius: var(--r-pill);
  font-size: var(--fs-label);
  background: var(--act-soft);
  color: var(--act);
}

.empty-placeholder {
  text-align: center;
  padding: var(--s5) var(--s4);
  color: var(--ink-muted);
}

.pagination-info {
  margin-top: var(--s4);
  padding: var(--s2) var(--s1);
  font-size: var(--fs-body);
  color: var(--ink-subtle);
}
</style>
