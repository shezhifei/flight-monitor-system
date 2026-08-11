import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

interface CapturedCall {
  method: string;
  url: string;
  body?: unknown;
}

const apiMocks = vi.hoisted(() => {
  const calls: CapturedCall[] = [];
  const okResult = (data: unknown = null) => ({
    ok: true,
    status: 200,
    data,
    response: new Response(JSON.stringify(data), { status: 200 }),
  });
  const errorResult = (status: number, data: unknown = null) => ({
    ok: false,
    status,
    data,
    response: new Response(null, { status }),
  });
  return {
    calls,
    okResult,
    errorResult,
    nextGet: null as null | ReturnType<typeof okResult>,
    nextPost: null as null | ReturnType<typeof okResult>,
    nextPut: null as null | ReturnType<typeof okResult>,
    nextDelete: null as null | ReturnType<typeof okResult>,
    getImpl: null as null | ((url: string) => ReturnType<typeof okResult> | Promise<ReturnType<typeof okResult>>),
    get: vi.fn(async (url: string) => {
      calls.push({ method: 'GET', url });
      if (apiMocks.getImpl) return apiMocks.getImpl(url);
      return apiMocks.nextGet ?? okResult([]);
    }),
    post: vi.fn(async (url: string, body?: unknown) => {
      calls.push({ method: 'POST', url, body });
      return apiMocks.nextPost ?? okResult({ id: 'new-id' });
    }),
    put: vi.fn(async (url: string, body?: unknown) => {
      calls.push({ method: 'PUT', url, body });
      return apiMocks.nextPut ?? okResult({});
    }),
    patch: vi.fn(async (url: string, body?: unknown) => {
      calls.push({ method: 'PATCH', url, body });
      return okResult({});
    }),
    del: vi.fn(async (url: string) => {
      calls.push({ method: 'DELETE', url });
      return apiMocks.nextDelete ?? okResult({});
    }),
  };
});

const toastMocks = vi.hoisted(() => ({
  showToast: vi.fn(),
  show: vi.fn(),
  dismissToast: vi.fn(),
  pauseToast: vi.fn(),
  resumeToast: vi.fn(),
  clearToasts: vi.fn(),
}));

vi.mock('@/composables/useApi', () => ({
  useApi: () => ({
    get: apiMocks.get,
    post: apiMocks.post,
    put: apiMocks.put,
    patch: apiMocks.patch,
    delete: apiMocks.del,
    raw: vi.fn(),
    request: vi.fn(),
  }),
}));

vi.mock('@/composables/useToast', () => ({
  useToast: () => toastMocks,
}));

// Import after mocks are registered.
import { useUserManager } from '@/composables/useUserManager';

function resetMocks(): void {
  apiMocks.calls.length = 0;
  apiMocks.get.mockClear();
  apiMocks.post.mockClear();
  apiMocks.put.mockClear();
  apiMocks.patch.mockClear();
  apiMocks.del.mockClear();
  apiMocks.nextGet = null;
  apiMocks.nextPost = null;
  apiMocks.nextPut = null;
  apiMocks.nextDelete = null;
  apiMocks.getImpl = null;
  toastMocks.showToast.mockClear();
}

function findCall(method: string, urlMatches: (url: string) => boolean): CapturedCall | undefined {
  return apiMocks.calls.find((c) => c.method === method && urlMatches(c.url));
}

describe('useUserManager — role workflows', () => {
  beforeEach(() => {
    resetMocks();
    // Stub window.confirm to always allow destructive actions.
    vi.stubGlobal('confirm', vi.fn(() => true));
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('createRole posts the expected payload and refreshes roles', async () => {
    const manager = useUserManager();
    const ok = await manager.createRole({
      name: '调度主管',
      description: '负责派工指挥',
      permission_codes: ['dispatch:read', 'dispatch:manage'],
    });

    expect(ok).toBe(true);
    const post = findCall('POST', (u) => u === '/api/v2/auth/roles');
    expect(post).toBeDefined();
    expect(post?.body).toEqual({
      name: '调度主管',
      description: '负责派工指挥',
      permissions: ['dispatch:read', 'dispatch:manage'],
    });
    // Refresh fetch must have been triggered.
    expect(findCall('GET', (u) => u === '/api/v2/auth/roles')).toBeDefined();
    expect(toastMocks.showToast).toHaveBeenCalledWith('success', '角色创建成功');
  });

  it('createRole returns false and toasts error when api fails', async () => {
    apiMocks.nextPost = apiMocks.errorResult(409, { message: '角色已存在' });
    const manager = useUserManager();
    const ok = await manager.createRole({
      name: '调度主管',
      description: '',
      permission_codes: [],
    });
    expect(ok).toBe(false);
    expect(toastMocks.showToast).toHaveBeenCalledWith('error', '角色已存在');
  });

  it('updateRole puts to the role endpoint', async () => {
    const manager = useUserManager();
    const ok = await manager.updateRole('role-1', {
      name: '调度员',
      description: '',
      permission_codes: ['dispatch:read'],
    });
    expect(ok).toBe(true);
    const put = findCall('PUT', (u) => u === '/api/v2/auth/roles/role-1');
    expect(put).toBeDefined();
    expect(put?.body).toMatchObject({
      name: '调度员',
      permissions: ['dispatch:read'],
    });
  });

  it('deleteRole confirms and deletes', async () => {
    const manager = useUserManager();
    const ok = await manager.deleteRole('role-9');
    expect(ok).toBe(true);
    expect(findCall('DELETE', (u) => u === '/api/v2/auth/roles/role-9')).toBeDefined();
    expect(toastMocks.showToast).toHaveBeenCalledWith('success', '角色已删除');
  });

  it('deleteRole blocks system roles', async () => {
    const manager = useUserManager();
    manager.roles.value = [
      { id: 'sys-1', name: 'admin', is_system: true, permissions: ['*'] },
    ];
    const ok = await manager.deleteRole('sys-1');
    expect(ok).toBe(false);
    expect(findCall('DELETE', (u) => u.includes('sys-1'))).toBeUndefined();
    expect(toastMocks.showToast).toHaveBeenCalledWith('warning', '系统角色不可删除');
  });

  it('deleteRole bails out when user declines confirmation', async () => {
    vi.stubGlobal('confirm', vi.fn(() => false));
    const manager = useUserManager();
    const ok = await manager.deleteRole('role-9');
    expect(ok).toBe(false);
    expect(findCall('DELETE', (u) => u === '/api/v2/auth/roles/role-9')).toBeUndefined();
  });

  it('applyTemplateToRoleForm supports replace, append, and clear', () => {
    const manager = useUserManager();
    manager.templates.value = [
      {
        id: 'tmpl-ops',
        name: '运行只读',
        code: 'OPS_READONLY',
        permissions: ['flight:read', 'dispatch:view'],
      },
    ];
    manager.roleForm.value = {
      name: 'ops',
      description: '',
      permission_codes: ['team:view'],
    };

    manager.applyTemplateToRoleForm('tmpl-ops', 'append');
    expect(manager.roleForm.value.permission_codes).toEqual(
      expect.arrayContaining(['team:view', 'flight:read', 'dispatch:view']),
    );

    manager.applyTemplateToRoleForm('tmpl-ops', 'replace');
    expect(manager.roleForm.value.permission_codes).toEqual(['flight:read', 'dispatch:view']);

    manager.applyTemplateToRoleForm('', 'clear');
    expect(manager.roleForm.value.permission_codes).toEqual([]);
  });
});

describe('useUserManager — template workflows', () => {
  beforeEach(() => {
    resetMocks();
    vi.stubGlobal('confirm', vi.fn(() => true));
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('createTemplate posts name/code/category/permissions (not permission_ids)', async () => {
    const manager = useUserManager();
    const ok = await manager.createTemplate({
      name: '调度操作员',
      code: 'dispatch_operator',
      category: '调度',
      description: '',
      permissions: ['dispatch:read', 'dispatch:manage'],
    });
    expect(ok).toBe(true);
    const post = findCall('POST', (u) => u === '/api/v2/auth/admin/permission-templates');
    expect(post).toBeDefined();
    expect(post?.body).toEqual({
      name: '调度操作员',
      code: 'dispatch_operator',
      category: '调度',
      description: undefined,
      permissions: ['dispatch:read', 'dispatch:manage'],
    });
    expect(findCall('GET', (u) => u === '/api/v2/auth/admin/permission-templates')).toBeDefined();
  });

  it('updateTemplate puts permissions by name', async () => {
    const manager = useUserManager();
    const ok = await manager.updateTemplate('tmpl-1', {
      name: '调度操作员',
      code: 'dispatch_operator',
      category: '',
      description: '更新',
      permissions: ['dispatch:view'],
    });
    expect(ok).toBe(true);
    const put = findCall('PUT', (u) => u === '/api/v2/auth/admin/permission-templates/tmpl-1');
    expect(put).toBeDefined();
    expect(put?.body).toMatchObject({
      name: '调度操作员',
      code: 'dispatch_operator',
      description: '更新',
      permissions: ['dispatch:view'],
    });
    expect(put?.body).not.toHaveProperty('permission_ids');
  });

  it('deleteTemplate confirms and deletes', async () => {
    const manager = useUserManager();
    const ok = await manager.deleteTemplate('tmpl-5');
    expect(ok).toBe(true);
    expect(
      findCall('DELETE', (u) => u === '/api/v2/auth/admin/permission-templates/tmpl-5'),
    ).toBeDefined();
    expect(toastMocks.showToast).toHaveBeenCalledWith('success', '模板已删除');
  });

  it('deleteTemplate blocks system templates', async () => {
    const manager = useUserManager();
    manager.templates.value = [
      { id: 'tmpl-sys', name: '系统模板', code: 'SYS', is_system: true, permissions: [] },
    ];
    const ok = await manager.deleteTemplate('tmpl-sys');
    expect(ok).toBe(false);
    expect(findCall('DELETE', (u) => u.includes('tmpl-sys'))).toBeUndefined();
  });

  it('saveTemplate refuses to submit when name or code is missing', async () => {
    const manager = useUserManager();
    manager.templateForm.value = {
      name: '',
      code: '',
      category: '',
      description: '',
      permissions: [],
    };
    await manager.saveTemplate();
    expect(findCall('POST', (u) => u.includes('permission-templates'))).toBeUndefined();
    expect(toastMocks.showToast).toHaveBeenCalledWith('warning', '请填写模板名称与代码');
  });

  it('openEditTemplateModal restores permission names from template.permissions', async () => {
    apiMocks.getImpl = (url) => {
      if (url.includes('/permissions')) {
        return apiMocks.okResult({
          success: true,
          data: [
            { id: 'p1', name: 'flight:read', description: '读航班', is_active: true },
            { id: 'p2', name: 'dispatch:view', description: '读调度', is_active: true },
          ],
        });
      }
      return apiMocks.okResult([]);
    };
    const manager = useUserManager();
    await manager.openEditTemplateModal({
      id: 'template-ops-view',
      name: '运行只读模板',
      code: 'OPS_READONLY',
      permissions: ['flight:read', 'dispatch:view'],
      is_system: true,
      category: 'operations',
    });
    expect(manager.templateForm.value.permissions).toEqual(['flight:read', 'dispatch:view']);
    expect(manager.showTemplateModal.value).toBe(true);
  });
});

describe('useUserManager — fetch + me + users', () => {
  beforeEach(() => {
    resetMocks();
  });

  it('fetchMe hydrates sidebarUser with initial uppercase', async () => {
    apiMocks.nextGet = apiMocks.okResult({
      username: 'alice',
      is_admin: true,
      role: 'admin',
    });
    const manager = useUserManager();
    await manager.fetchMe();
    expect(manager.sidebarUser.value).toMatchObject({
      username: 'alice',
      is_admin: true,
      initial: 'A',
    });
  });

  it('fetchRoles unwraps an ApiResponse envelope', async () => {
    apiMocks.nextGet = apiMocks.okResult({
      success: true,
      data: [
        {
          id: 'role-operations-manager',
          name: 'operations_manager',
          permissions: ['flight:read', 'dispatch:view'],
          is_system: true,
          user_count: 3,
        },
      ],
      message: '角色列表获取成功',
    });
    const manager = useUserManager();
    await manager.fetchRoles();
    expect(manager.roles.value).toHaveLength(1);
    expect(manager.roles.value[0]?.name).toBe('operations_manager');
    expect(manager.roles.value[0]?.user_count).toBe(3);
  });

  it('fetchPermissions maps name (not code) and is_active', async () => {
    apiMocks.nextGet = apiMocks.okResult({
      success: true,
      data: [
        {
          id: 'permission-flight-read',
          name: 'flight:read',
          description: '查看航班信息',
          is_active: true,
        },
      ],
    });
    const manager = useUserManager();
    await manager.fetchPermissions();
    expect(manager.permissions.value).toHaveLength(1);
    expect(manager.permissions.value[0]?.name).toBe('flight:read');
    expect(manager.permissions.value[0]?.is_active).toBe(true);
    expect(manager.filteredPermissions.value[0]?.name).toBe('flight:read');
  });

  it('fetchUsers unwraps envelope and keeps last_login_at / roles string[]', async () => {
    apiMocks.nextGet = apiMocks.okResult({
      success: true,
      data: [
        {
          id: '00000000-0000-4000-8000-000000000003',
          username: 'ops_manager',
          email: 'ops.manager@example.test',
          is_active: true,
          is_admin: false,
          last_login_at: '2026-07-14T02:00:00Z',
          roles: ['operations_manager'],
          department: '运行控制中心',
          job_level: 7,
          job_title: '值班经理',
        },
      ],
    });
    const manager = useUserManager();
    await manager.fetchUsers();
    expect(manager.users.value).toHaveLength(1);
    expect(manager.users.value[0]?.last_login_at).toBe('2026-07-14T02:00:00Z');
    expect(manager.users.value[0]?.roles).toEqual(['operations_manager']);
    expect(manager.users.value[0]?.department).toBe('运行控制中心');
  });

  it('openCreateUserModal loads roles before showing the dialog', async () => {
    apiMocks.getImpl = (url) => {
      if (url.includes('/auth/roles')) {
        return apiMocks.okResult({
          success: true,
          data: [{ id: 'r1', name: 'operations_manager', permissions: [] }],
        });
      }
      if (url.includes('/reference/departments')) {
        return apiMocks.okResult({
          success: true,
          data: [{ id: 'd1', name: '运行控制中心' }],
        });
      }
      return apiMocks.okResult([]);
    };
    const manager = useUserManager();
    expect(manager.roles.value).toHaveLength(0);
    await manager.openCreateUserModal();
    expect(manager.roles.value).toHaveLength(1);
    expect(manager.showUserModal.value).toBe(true);
    expect(manager.departmentSuggestions.value).toContain('运行控制中心');
  });

  it('openEditUserModal maps role names and org fields onto the form', async () => {
    apiMocks.getImpl = (url) => {
      if (url.includes('/auth/roles')) {
        return apiMocks.okResult({
          success: true,
          data: [
            { id: 'role-ops', name: 'operations_manager', permissions: [] },
            { id: 'role-view', name: 'viewer', permissions: [] },
          ],
        });
      }
      if (url.includes('/reference/departments')) {
        return apiMocks.okResult({ success: true, data: [] });
      }
      return apiMocks.okResult([]);
    };
    const manager = useUserManager();
    await manager.openEditUserModal({
      id: 'u1',
      username: 'ops_manager',
      email: 'ops@example.test',
      is_admin: false,
      is_active: true,
      roles: ['operations_manager'],
      department: '运行控制中心',
      job_level: 7,
      job_title: '值班经理',
      last_login_at: '2026-07-14T02:00:00Z',
    });
    expect(manager.userForm.value).toMatchObject({
      username: 'ops_manager',
      email: 'ops@example.test',
      roles: ['operations_manager'],
      department: '运行控制中心',
      job_level: 7,
      job_title: '值班经理',
    });
    expect(manager.showUserModal.value).toBe(true);
  });

  it('createUser posts roles/department/job fields', async () => {
    const manager = useUserManager();
    const ok = await manager.createUser({
      username: 'new_user',
      email: 'new@example.test',
      password: 'secret',
      is_admin: false,
      is_active: true,
      roles: ['operations_manager'],
      department: '运行控制中心',
      job_level: 3,
      job_title: '主管',
    });
    expect(ok).toBe(true);
    const post = findCall('POST', (u) => u === '/api/v2/auth/register');
    expect(post?.body).toMatchObject({
      username: 'new_user',
      email: 'new@example.test',
      password: 'secret',
      roles: ['operations_manager'],
      department: '运行控制中心',
      job_level: 3,
      job_title: '主管',
    });
  });
});
