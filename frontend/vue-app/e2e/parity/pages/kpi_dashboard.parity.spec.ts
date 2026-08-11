import { expect, test, type Page } from '@playwright/test';
import { installSessionRoutes, PARITY_ADMIN } from '../helpers/authRoutes';

const SNAPSHOT = {
  success: true,
  data: {
    calculated_at: '2026-07-14T02:30:00Z',
    turnaround_time_p90_minutes: 18,
    on_time_departure_rate: 0.873,
    service_node_compliance_rate: 0.961,
    equipment_utilization_rate: 0.76,
    abnormal_flight_ratio: 0.042,
    on_time_trend: [
      { date: '2026-07-13', value: 0.91 },
      { date: '2026-07-14', value: 0.873 },
    ],
    hourly_flight_volume: [
      { hour_label: '06:00', count: 12 },
      { hour_label: '07:00', count: 24 },
    ],
    turnaround_distribution: [
      { bucket: '0-30', count: 4 },
      { bucket: '30-60', count: 8 },
    ],
  },
  error: null,
};

const TREND = {
  success: true,
  data: {
    metric: 'on_time_rate',
    items: [
      { date: '2026-07-13', value: 0.91 },
      { date: '2026-07-14', value: 0.873 },
    ],
  },
  error: null,
};

const SERVICE_NODES = {
  success: true,
  data: {
    date: '2026-07-14',
    items: [
      { node: 'cleaning', rate: 0.93 },
      { node: 'loading', rate: 0.81 },
      { node: 'boarding', rate: 1 },
    ],
  },
  error: null,
};

async function installKpiRoutes(page: Page): Promise<void> {
  await page.route('**/api/v2/kpi/snapshot**', (route) => route.fulfill({
    status: 200,
    json: SNAPSHOT,
  }));
  await page.route('**/api/v2/kpi/trend?**', (route) => route.fulfill({
    status: 200,
    json: TREND,
  }));
  await page.route('**/api/v2/kpi/service-nodes**', (route) => route.fulfill({
    status: 200,
    json: SERVICE_NODES,
  }));
}

test.describe('kpi_dashboard parity', () => {
  test('kpi_dashboard-success: renders diagnostic shell from snapshot/trend/service-nodes envelopes', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installKpiRoutes(page);

    await page.goto('/frontend/kpi_dashboard.html');

    await expect(page.locator('.kpi-dashboard-page, .workspace-page').first()).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.locator('.page-title').filter({ hasText: 'KPI 诊断台' })).toBeVisible();
    await expect(page.locator('#timeRange')).toBeVisible();
    await expect(page.locator('#refreshBtn')).toBeVisible();
    await expect(page.locator('#verdictLead')).toContainText(/出港准点率|等待/);
    await expect(page.locator('#decisionAttainment')).toContainText(/达标|未达标|待评估/);
    await expect(page.getByText('3 秒判断', { exact: true })).toBeVisible();
  });

  test('kpi_dashboard-success: time range control stays interactive after load', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installKpiRoutes(page);

    await page.goto('/frontend/kpi_dashboard.html');
    await expect(page.locator('#timeRange')).toBeVisible({ timeout: 15_000 });

    await page.locator('#timeRange').selectOption('this_week');
    await expect(page.locator('#timeRange')).toHaveValue('this_week');
  });
});
