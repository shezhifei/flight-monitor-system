import { expect, test, type Page } from '@playwright/test';
import { installSessionRoutes, PARITY_ADMIN, PARITY_READONLY } from '../helpers/authRoutes';

const TEAMS = {
  success: true,
  data: [
    {
      id: 'team-ground-01',
      name: '地服一组',
      team_type_id: 'team-type-driver',
      code: 'GROUND-01',
      leader_id: '00000000-0000-4000-8000-000000000002',
      leader_name: '张航',
      terminal: 'T1',
      current_status: 'available',
      current_stand_id: 'stand-a12',
      member_count: 3,
      is_active: true,
    },
  ],
  message: '班组列表获取成功',
  error: null,
};

const TEAM_TYPES = {
  success: true,
  data: [
    {
      id: 'team-type-driver',
      name: '特车驾驶班组',
      code: 'DRV',
      department_id: 'dept-ground',
      description: '负责特种车辆驾驶',
      color: '#1677ff',
      is_driver_type: true,
      task_types: ['aircraft_tow'],
      is_active: true,
      team_count: 1,
    },
  ],
  message: '班组类型列表获取成功',
  error: null,
};

const EQUIPMENT = {
  success: true,
  data: [
    {
      id: 'equipment-tug-01',
      code: 'TUG-01',
      equipment_type_id: 'equipment-type-tug',
      equipment_type_name: '牵引车',
      name: '一号牵引车',
      license_plate: '粤A·FMS01',
      terminal: 'T1',
      status: 'available',
      current_stand_id: 'stand-a12',
      next_maintenance_date: '2026-08-01',
      is_active: true,
    },
  ],
  message: '设备列表获取成功',
  error: null,
};

const EQUIPMENT_TYPES = {
  success: true,
  data: [
    {
      id: 'equipment-type-tug',
      name: '牵引车',
      code: 'TUG',
      category: 'vehicle',
      requires_driver: true,
      driver_team_type_id: 'team-type-driver',
      icon: 'tractor',
      description: '航空器牵引设备',
      is_active: true,
      equipment_count: 1,
    },
  ],
  message: '设备类型列表获取成功',
  error: null,
};

const USERS = {
  success: true,
  data: [
    {
      id: '00000000-0000-4000-8000-000000000002',
      username: 'parity_operator',
      email: 'operator@example.test',
      is_active: true,
      is_verified: true,
      is_admin: false,
      display_name: '张航',
      roles: ['operator'],
      permissions: ['dispatch:view', 'team:view', 'equipment:view'],
    },
  ],
  message: '用户列表获取成功',
  error: null,
};

async function installResourceManagerRoutes(page: Page): Promise<{ userDirectoryRequests: string[] }> {
  const userDirectoryRequests: string[] = [];
  await page.route('**/api/v2/dispatch/teams**', (route) => {
    const url = route.request().url();
    if (url.includes('/members')) {
      return route.fulfill({
        status: 200,
        json: { success: true, data: [], error: null },
      });
    }
    return route.fulfill({ status: 200, json: TEAMS });
  });
  await page.route('**/api/v2/dispatch/team-types**', (route) => route.fulfill({
    status: 200,
    json: TEAM_TYPES,
  }));
  await page.route('**/api/v2/dispatch/equipment-types**', (route) => route.fulfill({
    status: 200,
    json: EQUIPMENT_TYPES,
  }));
  await page.route('**/api/v2/dispatch/equipment**', (route) => route.fulfill({
    status: 200,
    json: EQUIPMENT,
  }));
  await page.route('**/api/v2/auth/users**', (route) => {
    userDirectoryRequests.push(route.request().url());
    return route.fulfill({
      status: 200,
      json: USERS,
    });
  });
  return { userDirectoryRequests };
}

test.describe('resource_manager parity', () => {
  test('resource_manager-success: renders teams shell with Rust dispatch team envelopes', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installResourceManagerRoutes(page);

    await page.goto('/frontend/resource_manager.html');

    await expect(page.locator('.admin-container, .main-content').first()).toBeVisible({ timeout: 15_000 });
    await expect(page.getByText('班组管理').first()).toBeVisible();
    await expect(page.getByRole('cell', { name: /地服一组/ })).toBeVisible();
    await expect(page.getByRole('cell', { name: /GROUND-01|特车驾驶班组/ }).first()).toBeVisible();
  });

  test('resource_manager-success: equipment section loads equipment fixtures', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installResourceManagerRoutes(page);

    await page.goto('/frontend/resource_manager.html');
    await expect(page.getByRole('cell', { name: /地服一组/ })).toBeVisible({ timeout: 15_000 });

    await page.getByRole('button', { name: '设备管理' }).click();
    await expect(page.getByRole('cell', { name: /TUG-01|一号牵引车/ }).first()).toBeVisible({
      timeout: 10_000,
    });
  });

  test('resource_manager-readonly: preserves view access but hides team/equipment mutations', async ({ page }) => {
    await installSessionRoutes(page, PARITY_READONLY);
    const { userDirectoryRequests } = await installResourceManagerRoutes(page);

    await page.goto('/frontend/resource_manager.html');
    await expect(page.getByRole('cell', { name: /地服一组/ })).toBeVisible({ timeout: 15_000 });
    await expect(page.getByRole('button', { name: '新建班组' })).toHaveCount(0);
    await expect(page.getByRole('button', { name: '编辑' })).toHaveCount(0);
    await expect(page.getByRole('button', { name: '删除' })).toHaveCount(0);

    await page.getByRole('button', { name: '设备管理' }).click();
    await expect(page.getByRole('cell', { name: /TUG-01|一号牵引车/ }).first()).toBeVisible();
    await expect(page.getByRole('button', { name: '新建设备' })).toHaveCount(0);
    await expect(page.getByRole('button', { name: '状态' })).toHaveCount(0);
    expect(userDirectoryRequests).toHaveLength(0);
  });
});
