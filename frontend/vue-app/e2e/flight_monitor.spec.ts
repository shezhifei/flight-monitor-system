import { test, expect } from '@playwright/test';

const FLIGHT_MONITOR_URL = '/frontend/flight_monitor.html';

test.describe('Flight monitor page', () => {
  test('flight list region and workbar render on load', async ({ page }) => {
    await page.goto(FLIGHT_MONITOR_URL);

    const flightListRegion = page.locator('#flight-list-main');
    await expect(flightListRegion).toBeVisible();
    await expect(flightListRegion).toHaveAttribute('aria-label', '实时航班列表');

    const workbar = page.locator('[data-role="flight-workbar"]');
    await expect(workbar).toBeVisible();
    await expect(page.locator('#refreshBtn')).toBeVisible();
  });

  test('filter by delay status updates the business filter control', async ({ page }) => {
    await page.goto(FLIGHT_MONITOR_URL);

    const delayFilter = page.locator('#delayFilter');
    await expect(delayFilter).toBeVisible();
    await delayFilter.selectOption('only');
    await expect(delayFilter).toHaveValue('only');
  });

  test('anomaly status filter is reflected in the filter control', async ({ page }) => {
    await page.goto(FLIGHT_MONITOR_URL);

    const anomalyFilter = page.locator('#anomalyFilter');
    await expect(anomalyFilter).toBeVisible();
    await anomalyFilter.selectOption('only');
    await expect(anomalyFilter).toHaveValue('only');

    const connectionPill = page.locator('#connectionStatusPill');
    await expect(connectionPill).toBeVisible();
  });

  test('error scenario: shows empty/failed state when no flights are available', async ({ page }) => {
    await page.goto(FLIGHT_MONITOR_URL);

    const emptyState = page.locator('.empty-state');
    await expect(emptyState).toBeVisible();
    await expect(emptyState).toContainText(/暂无航班数据|没有匹配的航班|实时航班数据加载失败/);
  });
});
