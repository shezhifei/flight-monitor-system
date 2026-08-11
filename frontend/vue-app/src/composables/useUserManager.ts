import { computed, getCurrentInstance, onMounted, ref } from 'vue';
import { useApi } from './useApi';
import { useToast } from './useToast';

export type UserSection = 'users' | 'roles' | 'permissions' | 'templates';

export interface Permission {
  id: string;
  /** Permission code / identifier returned by Rust as `name` (e.g. flight:read). */
  name: string;
  description?: string;
  is_active?: boolean;
  /** @deprecated legacy alias — prefer `name` */
  code?: string;
  status?: string;
}

export interface UserRole {
  id: string;
  name: string;
  description?: string;
  permissions?: string[] | Permission[];
  is_active?: boolean;
  is_system?: boolean;
  user_count?: number;
}

export interface PermissionTemplate {
  id: string;
  name: string;
  code?: string;
  category?: string;
  description?: string;
  /** Permission names (Rust DTO). */
  permissions?: string[] | Permission[];
  /** @deprecated legacy field — prefer `permissions` */
  permission_ids?: string[];
  is_system?: boolean;
  is_active?: boolean;
}

export interface ManagedUser {
  id: string;
  username: string;
  email?: string;
  is_admin?: boolean;
  is_active?: boolean;
  is_verified?: boolean;
  /** Role names as returned by Rust UserResponse. */
  roles?: UserRole[] | string[];
  role?: string;
  status?: string;
  last_login_at?: string;
  /** @deprecated prefer last_login_at */
  last_login?: string;
  lastLogin?: string;
  display_name?: string;
  department?: string;
  job_level?: number | null;
  job_title?: string;
  permission_version?: number;
  [key: string]: unknown;
}

export interface SidebarUser {
  username: string;
  display_name?: string;
  role?: string;
  is_admin?: boolean;
  initial: string;
}

export interface UserFormState {
  username: string;
  email: string;
  password: string;
  is_admin: boolean;
  is_active: boolean;
  /** Selected role names (matches Rust UserCreate/UserAdminUpdate). */
  roles: string[];
  department: string;
  job_level: number;
  job_title: string;
}

export interface RoleFormState {
  name: string;
  description: string;
  /** Permission names selected for the role. */
  permission_codes: string[];
}

export interface TemplateFormState {
  name: string;
  code: string;
  category: string;
  description: string;
  /** Permission names (Rust PermissionTemplateCreate/Update). */
  permissions: string[];
}

export type TemplateApplyMode = 'replace' | 'append' | 'clear';

function emptyUserForm(): UserFormState {
  return {
    username: '',
    email: '',
    password: '',
    is_admin: false,
    is_active: true,
    roles: [],
    department: '',
    job_level: 1,
    job_title: '',
  };
}

function emptyRoleForm(): RoleFormState {
  return { name: '', description: '', permission_codes: [] };
}

function emptyTemplateForm(): TemplateFormState {
  return { name: '', code: '', category: '', description: '', permissions: [] };
}

function deriveInitial(name: string | undefined): string {
  const trimmed = String(name ?? '').trim();
  if (!trimmed) return 'U';
  return trimmed.charAt(0).toUpperCase();
}

/** Unwrap ApiResponse envelopes / list containers into a plain array. */
function extractList<T>(payload: unknown): T[] {
  if (Array.isArray(payload)) return payload as T[];
  if (payload && typeof payload === 'object') {
    const obj = payload as Record<string, unknown>;
    // Prefer explicit list keys first (including nested ApiResponse.data).
    for (const key of ['data', 'items', 'results', 'list']) {
      const value = obj[key];
      if (Array.isArray(value)) return value as T[];
      // Nested envelope: { data: { items: [...] } }
      if (value && typeof value === 'object' && !Array.isArray(value)) {
        const nested = value as Record<string, unknown>;
        for (const nestedKey of ['items', 'results', 'list', 'data']) {
          const nestedValue = nested[nestedKey];
          if (Array.isArray(nestedValue)) return nestedValue as T[];
        }
      }
    }
  }
  return [];
}

function extractErrorMessage(payload: unknown, fallback: string): string {
  if (payload && typeof payload === 'object') {
    const obj = payload as Record<string, unknown>;
    for (const key of ['message', 'detail', 'error']) {
      const value = obj[key];
      if (typeof value === 'string' && value.trim()) return value.trim();
      if (value && typeof value === 'object') {
        const nested = value as Record<string, unknown>;
        if (typeof nested.message === 'string' && nested.message.trim()) {
          return nested.message.trim();
        }
      }
    }
  }
  return fallback;
}

export function permissionNameOf(permission: Permission | string | null | undefined): string {
  if (!permission) return '';
  if (typeof permission === 'string') return permission;
  return String(permission.name || permission.code || '').trim();
}

export function roleNamesOf(user: ManagedUser | null | undefined): string[] {
  if (!user || !Array.isArray(user.roles)) return [];
  return (user.roles as Array<UserRole | string>)
    .map((r) => (typeof r === 'string' ? r : r.name))
    .filter((name): name is string => Boolean(name));
}

export function useUserManager() {
  const api = useApi();
  const toast = useToast();

  const activeSection = ref<UserSection>('users');
  const loading = ref(false);
  const savingUser = ref(false);
  const savingRole = ref(false);
  const savingTemplate = ref(false);

  const users = ref<ManagedUser[]>([]);
  const roles = ref<UserRole[]>([]);
  const permissions = ref<Permission[]>([]);
  const templates = ref<PermissionTemplate[]>([]);
  const departmentSuggestions = ref<string[]>([]);

  const sidebarUser = ref<SidebarUser | null>(null);

  const searchQuery = ref('');
  const roleSearch = ref('');
  const permissionSearch = ref('');
  const templateSearch = ref('');

  // User modal
  const showUserModal = ref(false);
  const editingUser = ref<ManagedUser | null>(null);
  const userForm = ref<UserFormState>(emptyUserForm());

  // Role modal
  const showRoleModal = ref(false);
  const editingRole = ref<UserRole | null>(null);
  const roleForm = ref<RoleFormState>(emptyRoleForm());

  // Template modal
  const showTemplateModal = ref(false);
  const editingTemplate = ref<PermissionTemplate | null>(null);
  const templateForm = ref<TemplateFormState>(emptyTemplateForm());

  const filteredUsers = computed(() => {
    const q = searchQuery.value.trim().toLowerCase();
    if (!q) return users.value;
    return users.value.filter((u) =>
      (u.username ?? '').toLowerCase().includes(q)
      || (u.email ?? '').toLowerCase().includes(q)
      || (u.display_name ?? '').toLowerCase().includes(q)
      || (u.department ?? '').toLowerCase().includes(q),
    );
  });

  const filteredRoles = computed(() => {
    const q = roleSearch.value.trim().toLowerCase();
    if (!q) return roles.value;
    return roles.value.filter((r) => (r.name ?? '').toLowerCase().includes(q));
  });

  const filteredPermissions = computed(() => {
    const q = permissionSearch.value.trim().toLowerCase();
    if (!q) return permissions.value;
    return permissions.value.filter((p) =>
      permissionNameOf(p).toLowerCase().includes(q)
      || (p.description ?? '').toLowerCase().includes(q),
    );
  });

  const filteredTemplates = computed(() => {
    const q = templateSearch.value.trim().toLowerCase();
    if (!q) return templates.value;
    return templates.value.filter((t) =>
      (t.name ?? '').toLowerCase().includes(q)
      || (t.code ?? '').toLowerCase().includes(q),
    );
  });

  function permissionCodesOf(role: UserRole | null | undefined): string[] {
    if (!role || !role.permissions) return [];
    return (role.permissions as Array<string | Permission>)
      .map((p) => permissionNameOf(p))
      .filter((c): c is string => Boolean(c));
  }

  function permissionNamesOf(template: PermissionTemplate | null | undefined): string[] {
    if (!template) return [];
    if (Array.isArray(template.permissions)) {
      return (template.permissions as Array<string | Permission>)
        .map((p) => permissionNameOf(p))
        .filter((name): name is string => Boolean(name));
    }
    // Legacy fallback: permission_ids may contain either ids or names.
    if (Array.isArray(template.permission_ids)) {
      const byId = new Map(permissions.value.map((p) => [p.id, permissionNameOf(p)]));
      return template.permission_ids
        .map((id) => byId.get(String(id)) ?? String(id))
        .filter((name): name is string => Boolean(name));
    }
    return [];
  }

  function normalizeDepartmentName(value: unknown): string {
    return String(value ?? '').trim();
  }

  async function fetchMe(): Promise<void> {
    const res = await api.get<Record<string, unknown>>('/api/v2/auth/me');
    if (!res.ok || !res.data) {
      sidebarUser.value = null;
      return;
    }
    const data = res.data as Record<string, unknown>;
    const username = String(data.username ?? data.name ?? data.display_name ?? '用户');
    sidebarUser.value = {
      username,
      display_name: typeof data.display_name === 'string' ? data.display_name : undefined,
      role: typeof data.role === 'string' ? data.role : undefined,
      is_admin: Boolean(data.is_admin),
      initial: deriveInitial(username),
    };
  }

  async function fetchUsers(): Promise<void> {
    loading.value = true;
    try {
      const res = await api.get<unknown>('/api/v2/auth/users?page_size=200');
      if (!res.ok) {
        toast.showToast('error', extractErrorMessage(res.data, '加载用户列表失败'));
        return;
      }
      users.value = extractList<ManagedUser>(res.data);
    } finally {
      loading.value = false;
    }
  }

  async function fetchRoles(): Promise<void> {
    const res = await api.get<unknown>('/api/v2/auth/roles');
    if (!res.ok) {
      toast.showToast('error', extractErrorMessage(res.data, '加载角色失败'));
      return;
    }
    roles.value = extractList<UserRole>(res.data);
  }

  async function fetchPermissions(): Promise<void> {
    const res = await api.get<unknown>('/api/v2/auth/permissions?page_size=500');
    if (!res.ok) {
      toast.showToast('error', extractErrorMessage(res.data, '加载权限失败'));
      return;
    }
    permissions.value = extractList<Permission>(res.data).map((p) => ({
      ...p,
      // Normalize so UI can always read `name`.
      name: permissionNameOf(p) || p.id,
    }));
  }

  async function fetchTemplates(): Promise<void> {
    const res = await api.get<unknown>('/api/v2/auth/admin/permission-templates');
    if (!res.ok) {
      toast.showToast('error', extractErrorMessage(res.data, '加载权限模板失败'));
      return;
    }
    templates.value = extractList<PermissionTemplate>(res.data);
  }

  async function fetchDepartmentSuggestions(): Promise<void> {
    try {
      const res = await api.get<unknown>('/api/v2/reference/departments?page_size=200');
      if (!res.ok) return;
      const list = extractList<Record<string, unknown>>(res.data);
      const names = list
        .map((item) => normalizeDepartmentName(item?.name ?? item?.department_name ?? item))
        .filter(Boolean);
      // Also include departments already present on users.
      for (const user of users.value) {
        const dept = normalizeDepartmentName(user.department);
        if (dept) names.push(dept);
      }
      departmentSuggestions.value = Array.from(new Set(names)).sort((a, b) => a.localeCompare(b, 'zh-CN'));
    } catch {
      // Non-fatal: datalist simply stays empty / user-derived.
      const fromUsers = users.value
        .map((u) => normalizeDepartmentName(u.department))
        .filter(Boolean);
      departmentSuggestions.value = Array.from(new Set(fromUsers)).sort((a, b) => a.localeCompare(b, 'zh-CN'));
    }
  }

  async function ensureRolesLoaded(): Promise<void> {
    if (roles.value.length === 0) {
      await fetchRoles();
    }
  }

  async function ensurePermissionsLoaded(): Promise<void> {
    if (permissions.value.length === 0) {
      await fetchPermissions();
    }
  }

  async function ensureTemplatesLoaded(): Promise<void> {
    if (templates.value.length === 0) {
      await fetchTemplates();
    }
  }

  // -------- Users --------

  function buildUserWritePayload(form: Partial<UserFormState>): Record<string, unknown> {
    const payload: Record<string, unknown> = {
      username: form.username,
      email: form.email,
      is_admin: form.is_admin,
      is_active: form.is_active,
      roles: form.roles ?? [],
      department: normalizeDepartmentName(form.department) || undefined,
      job_level: typeof form.job_level === 'number' ? form.job_level : 1,
      job_title: normalizeDepartmentName(form.job_title) || undefined,
    };
    if (form.password) {
      payload.password = form.password;
    }
    return payload;
  }

  async function createUser(data: UserFormState): Promise<boolean> {
    const res = await api.post<unknown>('/api/v2/auth/register', buildUserWritePayload(data));
    if (!res.ok) {
      toast.showToast('error', extractErrorMessage(res.data, '用户创建失败'));
      return false;
    }
    toast.showToast('success', '用户创建成功');
    await fetchUsers();
    return true;
  }

  async function updateUser(id: string, data: Partial<UserFormState>): Promise<boolean> {
    const payload = buildUserWritePayload(data);
    if (!payload.password) delete payload.password;
    const res = await api.put<unknown>(`/api/v2/auth/users/${encodeURIComponent(id)}`, payload);
    if (!res.ok) {
      toast.showToast('error', extractErrorMessage(res.data, '用户更新失败'));
      return false;
    }
    toast.showToast('success', '用户更新成功');
    await fetchUsers();
    return true;
  }

  async function deleteUser(id: string): Promise<boolean> {
    if (typeof window !== 'undefined' && !window.confirm('确定删除该用户？此操作不可撤销。')) {
      return false;
    }
    const res = await api.delete<unknown>(`/api/v2/auth/users/${encodeURIComponent(id)}`);
    if (!res.ok) {
      toast.showToast('error', extractErrorMessage(res.data, '用户删除失败'));
      return false;
    }
    toast.showToast('success', '用户删除成功');
    await fetchUsers();
    return true;
  }

  async function openCreateUserModal(): Promise<void> {
    editingUser.value = null;
    userForm.value = emptyUserForm();
    await Promise.all([ensureRolesLoaded(), fetchDepartmentSuggestions()]);
    showUserModal.value = true;
  }

  async function openEditUserModal(user: ManagedUser): Promise<void> {
    editingUser.value = user;
    await Promise.all([ensureRolesLoaded(), fetchDepartmentSuggestions()]);
    userForm.value = {
      username: user.username ?? '',
      email: user.email ?? '',
      password: '',
      is_admin: Boolean(user.is_admin),
      is_active: user.is_active !== false,
      roles: roleNamesOf(user),
      department: user.department ?? '',
      job_level: typeof user.job_level === 'number' ? user.job_level : 1,
      job_title: user.job_title ?? '',
    };
    showUserModal.value = true;
  }

  function closeUserModal(): void {
    showUserModal.value = false;
    editingUser.value = null;
  }

  async function saveUser(): Promise<void> {
    if (savingUser.value) return;
    if (!userForm.value.username.trim()) {
      toast.showToast('warning', '请填写用户名');
      return;
    }
    if (!userForm.value.email.trim()) {
      toast.showToast('warning', '请填写邮箱');
      return;
    }
    savingUser.value = true;
    try {
      const ok = editingUser.value
        ? await updateUser(editingUser.value.id, userForm.value)
        : await createUser(userForm.value);
      if (ok) closeUserModal();
    } finally {
      savingUser.value = false;
    }
  }

  // -------- Roles --------

  async function openCreateRoleModal(): Promise<void> {
    editingRole.value = null;
    roleForm.value = emptyRoleForm();
    await Promise.all([ensurePermissionsLoaded(), ensureTemplatesLoaded()]);
    showRoleModal.value = true;
  }

  async function openEditRoleModal(role: UserRole): Promise<void> {
    editingRole.value = role;
    roleForm.value = {
      name: role.name ?? '',
      description: role.description ?? '',
      permission_codes: permissionCodesOf(role),
    };
    await Promise.all([ensurePermissionsLoaded(), ensureTemplatesLoaded()]);
    showRoleModal.value = true;
  }

  function closeRoleModal(): void {
    showRoleModal.value = false;
    editingRole.value = null;
  }

  /**
   * Apply a permission template to the in-progress role form (replace / append / clear).
   * Matches legacy client-side apply behavior; server-side apply is also available via API.
   */
  function applyTemplateToRoleForm(templateId: string, mode: TemplateApplyMode): boolean {
    if (mode === 'clear') {
      roleForm.value = { ...roleForm.value, permission_codes: [] };
      toast.showToast('info', '已清空所有权限');
      return true;
    }
    const template = templates.value.find((t) => t.id === templateId);
    if (!template) {
      toast.showToast('warning', '请先选择模板');
      return false;
    }
    const names = permissionNamesOf(template);
    if (mode === 'replace') {
      roleForm.value = { ...roleForm.value, permission_codes: [...names] };
      toast.showToast('success', `已替换模板 "${template.name}" 的权限`);
      return true;
    }
    // append
    roleForm.value = {
      ...roleForm.value,
      permission_codes: Array.from(new Set([...roleForm.value.permission_codes, ...names])),
    };
    toast.showToast('success', `已追加模板 "${template.name}" 的权限`);
    return true;
  }

  async function createRole(data: RoleFormState): Promise<boolean> {
    const payload = {
      name: data.name.trim(),
      description: data.description.trim() || undefined,
      permissions: data.permission_codes,
    };
    const res = await api.post<unknown>('/api/v2/auth/roles', payload);
    if (!res.ok) {
      toast.showToast('error', extractErrorMessage(res.data, '角色创建失败'));
      return false;
    }
    toast.showToast('success', '角色创建成功');
    await fetchRoles();
    return true;
  }

  async function updateRole(id: string, data: RoleFormState): Promise<boolean> {
    const payload = {
      name: data.name.trim(),
      description: data.description.trim() || undefined,
      permissions: data.permission_codes,
    };
    const res = await api.put<unknown>(`/api/v2/auth/roles/${encodeURIComponent(id)}`, payload);
    if (!res.ok) {
      toast.showToast('error', extractErrorMessage(res.data, '角色更新失败'));
      return false;
    }
    toast.showToast('success', '角色更新成功');
    await fetchRoles();
    return true;
  }

  async function deleteRole(id: string): Promise<boolean> {
    const role = roles.value.find((r) => r.id === id);
    if (role?.is_system) {
      toast.showToast('warning', '系统角色不可删除');
      return false;
    }
    if (typeof window !== 'undefined' && !window.confirm('确定删除该角色？关联用户将失去此角色。')) {
      return false;
    }
    const res = await api.delete<unknown>(`/api/v2/auth/roles/${encodeURIComponent(id)}`);
    if (!res.ok) {
      toast.showToast('error', extractErrorMessage(res.data, '角色删除失败'));
      return false;
    }
    toast.showToast('success', '角色已删除');
    await fetchRoles();
    return true;
  }

  async function saveRole(): Promise<void> {
    if (savingRole.value) return;
    if (!roleForm.value.name.trim()) {
      toast.showToast('warning', '请填写角色名称');
      return;
    }
    savingRole.value = true;
    try {
      const ok = editingRole.value
        ? await updateRole(editingRole.value.id, roleForm.value)
        : await createRole(roleForm.value);
      if (ok) closeRoleModal();
    } finally {
      savingRole.value = false;
    }
  }

  // -------- Templates --------

  async function openCreateTemplateModal(): Promise<void> {
    editingTemplate.value = null;
    templateForm.value = emptyTemplateForm();
    await ensurePermissionsLoaded();
    showTemplateModal.value = true;
  }

  async function openEditTemplateModal(template: PermissionTemplate): Promise<void> {
    editingTemplate.value = template;
    await ensurePermissionsLoaded();
    templateForm.value = {
      name: template.name ?? '',
      code: template.code ?? '',
      category: template.category ?? '',
      description: template.description ?? '',
      permissions: permissionNamesOf(template),
    };
    showTemplateModal.value = true;
  }

  function closeTemplateModal(): void {
    showTemplateModal.value = false;
    editingTemplate.value = null;
  }

  async function createTemplate(data: TemplateFormState): Promise<boolean> {
    const payload = {
      name: data.name.trim(),
      code: data.code.trim() || undefined,
      category: data.category.trim() || undefined,
      description: data.description.trim() || undefined,
      permissions: data.permissions,
    };
    const res = await api.post<unknown>('/api/v2/auth/admin/permission-templates', payload);
    if (!res.ok) {
      toast.showToast('error', extractErrorMessage(res.data, '模板创建失败'));
      return false;
    }
    toast.showToast('success', '模板创建成功');
    await fetchTemplates();
    return true;
  }

  async function updateTemplate(id: string, data: TemplateFormState): Promise<boolean> {
    const payload = {
      name: data.name.trim(),
      code: data.code.trim() || undefined,
      category: data.category.trim() || undefined,
      description: data.description.trim() || undefined,
      permissions: data.permissions,
    };
    const res = await api.put<unknown>(
      `/api/v2/auth/admin/permission-templates/${encodeURIComponent(id)}`,
      payload,
    );
    if (!res.ok) {
      toast.showToast('error', extractErrorMessage(res.data, '模板更新失败'));
      return false;
    }
    toast.showToast('success', '模板更新成功');
    await fetchTemplates();
    return true;
  }

  async function deleteTemplate(id: string): Promise<boolean> {
    const template = templates.value.find((t) => t.id === id);
    if (template?.is_system) {
      toast.showToast('warning', '系统模板不可删除');
      return false;
    }
    if (typeof window !== 'undefined' && !window.confirm('确定删除该权限模板？')) {
      return false;
    }
    const res = await api.delete<unknown>(
      `/api/v2/auth/admin/permission-templates/${encodeURIComponent(id)}`,
    );
    if (!res.ok) {
      toast.showToast('error', extractErrorMessage(res.data, '模板删除失败'));
      return false;
    }
    toast.showToast('success', '模板已删除');
    await fetchTemplates();
    return true;
  }

  async function saveTemplate(): Promise<void> {
    if (savingTemplate.value) return;
    const name = templateForm.value.name.trim();
    const code = templateForm.value.code.trim();
    if (!name || !code) {
      toast.showToast('warning', '请填写模板名称与代码');
      return;
    }
    savingTemplate.value = true;
    try {
      const ok = editingTemplate.value
        ? await updateTemplate(editingTemplate.value.id, templateForm.value)
        : await createTemplate(templateForm.value);
      if (ok) closeTemplateModal();
    } finally {
      savingTemplate.value = false;
    }
  }

  // -------- Section switching --------

  function switchSection(section: UserSection): void {
    activeSection.value = section;
    if (section === 'users') void fetchUsers();
    else if (section === 'roles') void fetchRoles();
    else if (section === 'permissions') void fetchPermissions();
    else if (section === 'templates') {
      void fetchTemplates();
      if (permissions.value.length === 0) void fetchPermissions();
    }
  }

  async function bootstrap(): Promise<void> {
    await Promise.all([fetchMe(), fetchUsers(), fetchRoles()]);
  }

  if (getCurrentInstance()) {
    onMounted(() => {
      void bootstrap();
    });
  }

  return {
    activeSection,
    loading,
    savingUser,
    savingRole,
    savingTemplate,
    users,
    roles,
    permissions,
    templates,
    departmentSuggestions,
    filteredUsers,
    filteredRoles,
    filteredPermissions,
    filteredTemplates,
    sidebarUser,
    searchQuery,
    roleSearch,
    permissionSearch,
    templateSearch,
    showUserModal,
    editingUser,
    userForm,
    showRoleModal,
    editingRole,
    roleForm,
    showTemplateModal,
    editingTemplate,
    templateForm,
    switchSection,
    bootstrap,
    fetchMe,
    fetchUsers,
    fetchRoles,
    fetchPermissions,
    fetchTemplates,
    fetchDepartmentSuggestions,
    openCreateUserModal,
    openEditUserModal,
    closeUserModal,
    saveUser,
    createUser,
    updateUser,
    deleteUser,
    openCreateRoleModal,
    openEditRoleModal,
    closeRoleModal,
    saveRole,
    createRole,
    updateRole,
    deleteRole,
    applyTemplateToRoleForm,
    openCreateTemplateModal,
    openEditTemplateModal,
    closeTemplateModal,
    saveTemplate,
    createTemplate,
    updateTemplate,
    deleteTemplate,
    permissionCodesOf,
    permissionNamesOf,
    roleNamesOf,
  };
}
