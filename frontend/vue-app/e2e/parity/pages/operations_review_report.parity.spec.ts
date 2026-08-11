import { expect, test, type Page } from '@playwright/test';
import { installSessionRoutes, PARITY_ADMIN } from '../helpers/authRoutes';

const BASELINE = {
  success: true,
  data: {
    target_date: '2026-07-14',
    weather_category: 'normal',
    items: [
      {
        hour: '08:00',
        actual_volume: 24,
        actual_on_time_rate: 0.78,
        baseline_volume: 20,
        baseline_on_time_rate: 0.9,
        threshold_margin: -0.12,
        is_abnormal: true,
      },
      {
        hour: '09:00',
        actual_volume: 18,
        actual_on_time_rate: 0.92,
        baseline_volume: 18,
        baseline_on_time_rate: 0.9,
        threshold_margin: 0.02,
        is_abnormal: false,
      },
    ],
  },
  error: null,
};

const TREND_WITH_ANOMALIES = {
  success: true,
  data: {
    metric: 'on_time_rate',
    items: [
      { date: '2026-07-13', value: 0.91, anomaly_count: 0 },
      { date: '2026-07-14', value: 0.84, anomaly_count: 2, is_abnormal: true },
    ],
  },
  error: null,
};

const KPI_COMPARE = {
  success: true,
  data: {
    comparison: [
      {
        metric: 'on_time_departure_rate',
        name: '出港准点率',
        baseline: 0.9,
        compare: 0.87,
        change: -0.03,
      },
    ],
  },
  error: null,
};

async function installOpsReviewRoutes(page: Page): Promise<void> {
  await page.route('**/api/v2/kpi/baseline-compare**', (route) => route.fulfill({
    status: 200,
    json: BASELINE,
  }));
  await page.route('**/api/v2/kpi/trend-with-anomalies**', (route) => route.fulfill({
    status: 200,
    json: TREND_WITH_ANOMALIES,
  }));
  await page.route('**/api/v2/kpi/compare**', (route) => route.fulfill({
    status: 200,
    json: KPI_COMPARE,
  }));
}

test.describe('operations_review_report parity', () => {
  test('operations_review_report-success: renders review shell from baseline-compare and trend envelopes', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installOpsReviewRoutes(page);

    await page.goto('/frontend/operations_review_report.html');

    await expect(page.locator('.operations-review-page, .workspace-page').first()).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.locator('.page-title').filter({ hasText: '运行复盘' })).toBeVisible();
    await expect(page.getByRole('heading', { name: '回放控制台' })).toBeVisible();
    await expect(page.getByRole('heading', { name: '基线偏离诊断' })).toBeVisible();
    await expect(page.locator('#summaryEventTotal')).toHaveText('42');
    await expect(page.locator('#summaryAnomalyEvents')).toHaveText('1');
    await expect(page.locator('#refreshAllBtn')).toBeVisible();
  });

  test('operations_review_report-success: replay control buttons remain visible after baseline load', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installOpsReviewRoutes(page);

    await page.goto('/frontend/operations_review_report.html');
    await expect(page.locator('#reloadReplayBtn')).toBeVisible({ timeout: 15_000 });
    await expect(page.getByRole('button', { name: '拉取事件' })).toBeVisible();
    await expect(page.locator('#playBtn')).toBeVisible();
  });
});
