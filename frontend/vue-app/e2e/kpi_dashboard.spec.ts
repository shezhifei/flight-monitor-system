import { test, expect } from '@playwright/test';

const KPI_DASHBOARD_URL = '/frontend/kpi_dashboard.html';

test.describe('KPI dashboard page', () => {
  test('KPI snapshot structure renders on load', async ({ page }) => {
    await page.goto(KPI_DASHBOARD_URL);

    await expect(page).toHaveTitle(/KPI|诊断台/i);
    await expect(page.locator('#timeRange')).toBeVisible();
    await expect(page.locator('#verdictLead')).toBeVisible();
    await expect(page.locator('#refreshBtn')).toBeVisible();
  });

  test('switching time range updates the selector and reveals custom range inputs', async ({ page }) => {
    await page.goto(KPI_DASHBOARD_URL);

    const timeRange = page.locator('#timeRange');
    await timeRange.selectOption('this_week');
    await expect(timeRange).toHaveValue('this_week');

    await timeRange.selectOption('custom');
    await expect(timeRange).toHaveValue('custom');
    const customGroup = page.locator('#customRangeGroup');
    await expect(customGroup).toBeVisible();
    await expect(page.locator('#startDate')).toBeEnabled();
    await expect(page.locator('#endDate')).toBeEnabled();
  });

  test('refresh button triggers a data refresh without throwing', async ({ page }) => {
    await page.goto(KPI_DASHBOARD_URL);

    const refreshBtn = page.locator('#refreshBtn');
    await expect(refreshBtn).toBeVisible();
    await expect(refreshBtn).toBeEnabled();
    await refreshBtn.click();
    await expect(refreshBtn).toBeVisible();
  });

  test('error scenario: failed KPI fetch surfaces an inline error alert', async ({ page }) => {
    await page.goto(KPI_DASHBOARD_URL);

    const inlineError = page.locator('.inline-error');
    await expect(inlineError).toBeVisible();
    await expect(inlineError).toHaveRole('alert');
    await expect(inlineError).toContainText(/失败|加载失败|Error|HTTP/i);
  });
});
