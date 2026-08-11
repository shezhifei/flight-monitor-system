import { expect, test, type Page } from '@playwright/test';
import { installSessionRoutes, PARITY_ADMIN } from '../helpers/authRoutes';

const LABELS = {
  success: true,
  data: [
    {
      label_id: 'label-priority',
      code: 'priority',
      name: '优先保障',
      color: '#ef4444',
      icon: '⭐',
      scope: 'flight',
      category: 'system',
      is_active: true,
      sort_order: 1,
      created_at: '2026-01-01T00:00:00Z',
      updated_at: '2026-07-01T00:00:00Z',
    },
    {
      label_id: 'label-vip',
      code: 'vip',
      name: '要客航班',
      color: '#8b5cf6',
      icon: null,
      scope: 'both',
      category: 'custom',
      is_active: true,
      sort_order: 2,
      created_at: '2026-02-01T00:00:00Z',
      updated_at: '2026-07-01T00:00:00Z',
    },
  ],
  error: null,
};

async function installLabelRoutes(page: Page): Promise<void> {
  await page.route('**/api/v2/labels**', (route) => {
    if (route.request().method() === 'GET') {
      return route.fulfill({ status: 200, json: LABELS });
    }
    return route.fulfill({
      status: 200,
      json: { success: true, data: LABELS.data[0], error: null },
    });
  });
}

test.describe('label_manager parity', () => {
  test('label_manager-success: renders label table from /api/v2/labels envelope', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installLabelRoutes(page);

    await page.goto('/frontend/label_manager.html');

    await expect(page.locator('.label-manager-page, .page-header').first()).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByRole('heading', { name: '标签定义管理' })).toBeVisible();
    await expect(page.getByText(/维护航班标签模板/).first()).toBeVisible();
    await expect(page.getByRole('cell', { name: '优先保障' })).toBeVisible();
    await expect(page.getByRole('cell', { name: /priority/ })).toBeVisible();
    await expect(page.getByRole('cell', { name: '要客航班' })).toBeVisible();
  });

  test('label_manager-success: admin can open create label action', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installLabelRoutes(page);

    await page.goto('/frontend/label_manager.html');
    await expect(page.getByRole('heading', { name: '标签定义管理' })).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByRole('button', { name: /新建标签/ })).toBeVisible();
  });
});
