import { expect, test } from '@playwright/test';
import { installSessionRoutes, PARITY_ADMIN, PARITY_READONLY } from '../helpers/authRoutes';

async function installDashboardRoutes(page: import('@playwright/test').Page): Promise<void> {
  await page.route('**/api/v2/dashboard/workbench**', (route) => route.fulfill({
    status: 200,
    json: {
      success: true,
      data: {
        summary: { pending_orders: 2, open_anomalies: 1, unread_notifications: 0 },
        modules: [],
        cards: [],
      },
    },
  }));
  await page.route('**/api/v2/mobile/workbench**', (route) => route.fulfill({
    status: 200,
    json: {
      success: true,
      data: {
        notification_unread_count: 0,
        chat_unread_total: 0,
        pending_sync_action_count: 0,
        critical_alerts: [],
      },
    },
  }));
  await page.route('**/api/v2/dispatch-orders/**', (route) => route.fulfill({
    status: 200,
    json: { success: true, data: [] },
  }));
  await page.route('**/api/v2/ai/**', (route) => route.fulfill({
    status: 200,
    json: { success: true, data: {} },
    headers: route.request().headers().accept?.includes('text/event-stream')
      ? { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache' }
      : undefined,
    body: route.request().headers().accept?.includes('text/event-stream')
      ? 'event: ai_execution\ndata: {"status":"idle"}\n\n'
      : undefined,
  }));
  await page.route('**/api/v2/**', async (route) => {
    if (route.request().url().includes('/auth/')) {
      await route.fallback();
      return;
    }
    await route.fulfill({ status: 200, json: { success: true, data: [] } });
  });
}

test.describe('dashboard parity', () => {
  test('dashboard-success: admin session mounts the workbench shell', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installDashboardRoutes(page);
    await page.goto('/frontend/dashboard.html');

    await expect(page.locator('#app, .dashboard-page, body').first()).toBeVisible();
    // Chinese workbench heading or welcome content from legacy/Vue shell
    await expect(page.getByText(/工作台|欢迎|基线管理员|parity_admin/i).first()).toBeVisible({
      timeout: 15_000,
    });
  });

  test('dashboard-success: readonly session does not hard-fail the shell', async ({ page }) => {
    await installSessionRoutes(page, PARITY_READONLY);
    await installDashboardRoutes(page);
    await page.goto('/frontend/dashboard.html');
    await expect(page).toHaveURL(/\/frontend\/dashboard\.html$/);
    await expect(page.locator('body')).not.toBeEmpty();
  });
});
