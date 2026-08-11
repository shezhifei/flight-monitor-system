import { expect, test, type Page } from '@playwright/test';
import { installSessionRoutes, PARITY_ADMIN } from '../helpers/authRoutes';

const DEPARTMENTS = {
  success: true,
  data: [
    {
      id: 'dept-ground',
      name: '地服保障部',
      code: 'GROUND',
      terminal: 'T1',
      is_active: true,
    },
  ],
  error: null,
};

const TASK_TYPES = {
  success: true,
  data: [
    {
      id: 'tt-boarding',
      code: 'boarding',
      name: '登机保障',
      default_department_id: 'dept-ground',
      category: 'passenger',
      sequence_order: 10,
      default_duration_minutes: 30,
      trigger_offset_minutes: -20,
      trigger_type: 'scheduled_departure',
      description: '登机节点保障',
      is_active: true,
    },
  ],
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
      is_active: true,
    },
  ],
  error: null,
};

const EMPTY_LIST = { success: true, data: [], error: null };

async function installDispatchRuleRoutes(page: Page): Promise<void> {
  await page.route('**/api/v2/dispatch/resources/departments**', (route) =>
    route.fulfill({ status: 200, json: DEPARTMENTS }),
  );
  await page.route('**/api/v2/dispatch/task-types**', (route) =>
    route.fulfill({ status: 200, json: TASK_TYPES }),
  );
  await page.route('**/api/v2/dispatch/resources/equipment-types**', (route) =>
    route.fulfill({ status: 200, json: EQUIPMENT_TYPES }),
  );
  await page.route('**/api/v2/dispatch/rules/departments/**', (route) =>
    route.fulfill({ status: 200, json: EMPTY_LIST }),
  );
}

test.describe('dispatch_rule_center parity', () => {
  test('dispatch_rule_center-success: renders workbench shell with department and task type fixtures', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installDispatchRuleRoutes(page);

    await page.goto('/frontend/dispatch_rule_center.html');

    await expect(page.locator('.page-shell, .workbench, .topbar').first()).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByRole('heading', { name: '派工规则配置工作台' })).toBeVisible();
    await expect(page.getByText(/按科室管理任务类型/).first()).toBeVisible();
    await expect(page.getByText('登机保障').first()).toBeVisible();
    await expect(page.getByRole('button', { name: '导出' })).toBeVisible();
  });

  test('dispatch_rule_center-success: selecting a task type reveals rule configuration tabs', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installDispatchRuleRoutes(page);

    await page.goto('/frontend/dispatch_rule_center.html');
    await expect(page.getByRole('heading', { name: '派工规则配置工作台' })).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByRole('heading', { name: '任务类型' })).toBeVisible();

    // Rule sub-tabs (生成规则/调整规则/…) only render after a task type is selected.
    await page.getByRole('button', { name: /登机保障/ }).click();
    await expect(page.getByRole('tab', { name: '生成规则' })).toBeVisible({ timeout: 10_000 });
    await expect(page.getByRole('tab', { name: '调整规则' })).toBeVisible();
    // "资质要求" appears both as TaskTypePanel tab and rule workbench tab.
    await expect(page.getByRole('tab', { name: '资质要求' }).first()).toBeVisible();
    await expect(page.getByRole('tab', { name: '规则预览' })).toBeVisible();
  });
});
