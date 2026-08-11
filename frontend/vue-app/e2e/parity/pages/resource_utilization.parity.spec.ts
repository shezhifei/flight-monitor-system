import { expect, test, type Page } from '@playwright/test';
import { installSessionRoutes, PARITY_ADMIN } from '../helpers/authRoutes';

const SUMMARY = {
  success: true,
  data: [
    { name: '机位 A12', dimension: 'stand', utilization: 0.96 },
    { name: '地服一组', dimension: 'team', utilization: 0.72 },
    { name: '牵引车 TUG-01', dimension: 'equipment', utilization: 0.88 },
  ],
  error: null,
};

const STANDS = {
  success: true,
  data: [{ name: '机位 A12', dimension: 'stand', utilization: 0.96 }],
  error: null,
};

const TEAMS = {
  success: true,
  data: [{ name: '地服一组', dimension: 'team', utilization: 0.72 }],
  error: null,
};

const EQUIPMENT = {
  success: true,
  data: [{ name: '牵引车 TUG-01', dimension: 'equipment', utilization: 0.88 }],
  error: null,
};

async function installUtilizationRoutes(page: Page): Promise<void> {
  await page.route('**/api/v2/dispatch/analytics/resource-utilization/summary**', (route) =>
    route.fulfill({ status: 200, json: SUMMARY }),
  );
  await page.route('**/api/v2/dispatch/analytics/resource-utilization/stands**', (route) =>
    route.fulfill({ status: 200, json: STANDS }),
  );
  await page.route('**/api/v2/dispatch/analytics/resource-utilization/teams**', (route) =>
    route.fulfill({ status: 200, json: TEAMS }),
  );
  await page.route('**/api/v2/dispatch/analytics/resource-utilization/equipment**', (route) =>
    route.fulfill({ status: 200, json: EQUIPMENT }),
  );
}

test.describe('resource_utilization parity', () => {
  test('resource_utilization-success: renders bottleneck shell from utilization summary envelope', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installUtilizationRoutes(page);

    await page.goto('/frontend/resource_utilization.html');

    await expect(page.locator('.resource-utilization-page, .workspace-page').first()).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.locator('.page-title').filter({ hasText: '资源利用率' })).toBeVisible();
    await expect(page.getByRole('heading', { name: '资源瓶颈台' })).toBeVisible();
    await expect(page.getByText('当前瓶颈', { exact: true })).toBeVisible();
    await expect(page.locator('#metricBottleneckDimension')).toContainText(/机位 A12|牵引车/);
    await expect(page.getByText('机位 A12').first()).toBeVisible();
    await expect(page.locator('#refreshBtn')).toBeVisible();
  });

  test('resource_utilization-success: object ranking lists fixture utilization rows', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installUtilizationRoutes(page);

    await page.goto('/frontend/resource_utilization.html');
    await expect(page.getByRole('heading', { name: '对象排序' })).toBeVisible({ timeout: 15_000 });
    await expect(page.locator('#bottleneckLeaderboard')).toContainText('地服一组');
    await expect(page.locator('#leaderboardMeta')).toContainText(/3 个对象/);
  });
});
