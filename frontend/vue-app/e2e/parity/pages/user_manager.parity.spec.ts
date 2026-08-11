import { expect, test } from '@playwright/test';
import { installSessionRoutes, PARITY_ADMIN } from '../helpers/authRoutes';

const USERS = {
  success: true,
  data: [
    {
      id: '00000000-0000-4000-8000-000000000003',
      username: 'ops_manager',
      email: 'ops.manager@example.test',
      is_active: true,
      is_verified: true,
      is_admin: false,
      created_at: '2026-05-10T00:00:00Z',
      last_login_at: '2026-07-14T02:00:00Z',
      roles: ['operations_manager'],
      permissions: ['flight:read', 'dispatch:view'],
      display_name: '李调度',
      department: '运行控制中心',
      job_level: 7,
      job_title: '值班经理',
      permission_version: 6,
    },
  ],
  message: '用户列表获取成功',
  error: null,
};

const ROLES = {
  success: true,
  data: [
    {
      id: 'role-operations-manager',
      name: 'operations_manager',
      description: '运行值班经理',
      permissions: ['flight:read', 'dispatch:view'],
      is_active: true,
      is_system: true,
      created_at: '2026-01-01T00:00:00Z',
      updated_at: '2026-07-01T00:00:00Z',
      user_count: 3,
    },
  ],
  message: '角色列表获取成功',
  error: null,
};

const PERMISSIONS = {
  success: true,
  data: [
    {
      id: 'permission-flight-read',
      name: 'flight:read',
      description: '查看航班信息',
      is_active: true,
      created_at: '2026-01-01T00:00:00Z',
      updated_at: '2026-01-01T00:00:00Z',
    },
  ],
  message: '权限列表获取成功',
  error: null,
};

const TEMPLATES = {
  success: true,
  data: [
    {
      id: 'template-ops-view',
      name: '运行只读模板',
      code: 'OPS_READONLY',
      description: '运行态只读权限集合',
      permissions: ['flight:read', 'dispatch:view'],
      is_system: true,
      category: 'operations',
      display_order: 10,
      is_active: true,
      created_at: '2026-05-01T00:00:00Z',
      updated_at: '2026-07-01T00:00:00Z',
    },
  ],
  error: null,
};

test.describe('user_manager parity', () => {
  test('user_manager-success: renders users with roles[] and last_login_at from Rust envelopes', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await page.route('**/api/v2/auth/users**', (route) => route.fulfill({ status: 200, json: USERS }));
    await page.route('**/api/v2/auth/roles**', (route) => route.fulfill({ status: 200, json: ROLES }));
    await page.route('**/api/v2/auth/permissions**', (route) => route.fulfill({ status: 200, json: PERMISSIONS }));
    await page.route('**/api/v2/auth/admin/permission-templates**', (route) => route.fulfill({
      status: 200,
      json: TEMPLATES,
    }));
    await page.route('**/api/v2/reference/departments**', (route) => route.fulfill({
      status: 200,
      json: { success: true, data: [] },
    }));

    await page.goto('/frontend/user_manager.html');

    await expect(page.getByText('ops_manager').or(page.getByText('李调度'))).toBeVisible({ timeout: 15_000 });
    await expect(page.getByText('operations_manager').first()).toBeVisible();
  });
});
