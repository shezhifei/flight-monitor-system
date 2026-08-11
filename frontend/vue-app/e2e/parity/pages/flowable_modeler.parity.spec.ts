import { expect, test, type Page } from '@playwright/test';
import { installSessionRoutes, PARITY_ADMIN } from '../helpers/authRoutes';

const CASE_TYPES = {
  success: true,
  data: [
    {
      id: 'case-delay',
      code: 'flight_delay',
      name: '航班延误处置',
      description: '延误处置流程',
      bpmn_xml: null,
      is_active: true,
    },
    {
      id: 'case-vip',
      code: 'vip_service',
      name: '要客保障',
      description: '要客保障流程',
      bpmn_xml: null,
      is_active: true,
    },
  ],
  error: null,
};

const USER_CONTEXT = {
  success: true,
  data: {
    user_context: {
      display_name: '基线管理员',
      username: 'parity_admin',
      role: '系统管理员',
      department: '运行控制中心',
      department_id: 'dept-ops',
    },
  },
  error: null,
};

const AI_CAPABILITIES = {
  success: true,
  data: {
    chat: true,
    generate_draft: false,
    tools: [],
  },
  error: null,
};

async function installFlowableRoutes(page: Page): Promise<void> {
  await page.route('**/api/v2/auth/user-context**', (route) => route.fulfill({
    status: 200,
    json: USER_CONTEXT,
  }));
  await page.route('**/api/v2/business-case-types**', (route) => {
    const url = route.request().url();
    if (/\/business-case-types\/[^/?]+/.test(url) && !url.includes('?')) {
      return route.fulfill({
        status: 200,
        json: { success: true, data: CASE_TYPES.data[0], error: null },
      });
    }
    return route.fulfill({ status: 200, json: CASE_TYPES });
  });
  await page.route('**/api/v2/ai/capabilities**', (route) => route.fulfill({
    status: 200,
    json: AI_CAPABILITIES,
  }));
  await page.route('**/api/v2/ai/**', (route) => route.fulfill({
    status: 200,
    json: { success: true, data: {}, error: null },
  }));
}

test.describe('flowable_modeler parity', () => {
  test('flowable_modeler-success: renders process design shell and case type list', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installFlowableRoutes(page);

    await page.goto('/frontend/flowable_modeler.html');

    await expect(page.locator('.main-container, .admin-sidebar').first()).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByText('流程设计', { exact: true })).toBeVisible();
    await expect(page.getByText('事项类型', { exact: true })).toBeVisible();
    await expect(page.getByText('航班延误处置').first()).toBeVisible();
    await expect(page.getByText('选择一个业务事项类型').first()).toBeVisible();
  });

  test('flowable_modeler-empty: empty editor state is shown before selecting a case type', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installFlowableRoutes(page);

    await page.goto('/frontend/flowable_modeler.html');
    await expect(page.locator('.empty-state-title')).toContainText('选择一个业务事项类型', {
      timeout: 15_000,
    });
    await expect(page.getByText('部署作用域', { exact: true })).toBeVisible();
  });
});
