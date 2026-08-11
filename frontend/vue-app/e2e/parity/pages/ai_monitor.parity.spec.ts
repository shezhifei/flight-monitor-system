import { expect, test, type Page } from '@playwright/test';
import { installSessionRoutes, PARITY_ADMIN } from '../helpers/authRoutes';

const CAPABILITIES = {
  success: true,
  data: {
    ai_ready: true,
    ai_execute_permission: true,
    ai_chat_permission: true,
    missing_reasons: [],
  },
};

const JOBS = {
  success: true,
  data: [
    {
      job_id: 'job-5101',
      job_type: 'flight_risk',
      status: 'succeeded',
      created_at: '2026-07-14T02:20:00Z',
      updated_at: '2026-07-14T02:21:00Z',
    },
  ],
};

const JOB_STATS = {
  success: true,
  data: {
    total: 1,
    pending: 0,
    running: 0,
    succeeded: 1,
    failed: 0,
  },
};

const PENDING_ACTIONS = {
  success: true,
  data: {
    items: [],
    total: 0,
    total_count: 0,
    pagination: { limit: 200, offset: 0, has_more: false },
  },
};

const PROPOSAL_STATS = {
  success: true,
  data: {
    total: 3,
    pending: 0,
    approved: 2,
    rejected: 1,
    executed: 2,
  },
};

const EMPTY_METRICS = {
  success: true,
  data: {
    executions_total: 12,
    visible_total: 12,
    hidden_total: 0,
    requests_total: 24,
    routed_total: 24,
    fallback_total: 0,
    valid_total: 8,
    invalid_total: 0,
    validation_rate: 1,
    generated_at: '2026-07-14T02:30:00Z',
  },
};

async function installAiMonitorRoutes(page: Page): Promise<void> {
  // Catch-all first; specifics registered after win (Playwright LIFO).
  await page.route('**/api/v2/ai/**', async (route) => {
    await route.fulfill({ status: 200, json: { success: true, data: {} } });
  });
  await page.route('**/api/v2/ai/capabilities**', (route) => route.fulfill({
    status: 200,
    json: CAPABILITIES,
  }));
  await page.route('**/api/v2/ai/jobs/stats**', (route) => route.fulfill({
    status: 200,
    json: JOB_STATS,
  }));
  await page.route('**/api/v2/ai/jobs**', (route) => {
    const url = route.request().url();
    if (url.includes('/stats')) return route.fallback();
    return route.fulfill({ status: 200, json: JOBS });
  });
  await page.route('**/api/v2/ai/pending-actions**', (route) => route.fulfill({
    status: 200,
    json: PENDING_ACTIONS,
  }));
  await page.route('**/api/v2/ai/proposals/stats**', (route) => route.fulfill({
    status: 200,
    json: PROPOSAL_STATS,
  }));
  await page.route('**/api/v2/ai/metrics/**', (route) => route.fulfill({
    status: 200,
    json: EMPTY_METRICS,
  }));
  await page.route('**/api/v2/ai/events/stream**', (route) => route.fulfill({
    status: 200,
    headers: {
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache',
    },
    body: 'event: ai_execution\ndata: {"status":"idle","message":"parity"}\n\n',
  }));
}

test.describe('ai_monitor parity', () => {
  test('ai_monitor-success: Vue host shell mounts with React entry host (loaded, loading, or error)', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installAiMonitorRoutes(page);

    await page.goto('/frontend/ai_monitor.html');

    await expect(page).toHaveURL(/\/frontend\/ai_monitor\.html$/);
    await expect(page).toHaveTitle(/AI Monitor/i);

    const shell = page.locator('.ai-react-entry-shell, .ai-page').first();
    await expect(shell).toBeVisible({ timeout: 15_000 });

    const host = page.locator('#ai-react-root');
    await expect(host).toBeVisible();
    await expect(host).toHaveAttribute('data-ai-entry', 'ai_monitor');

    // If React artifacts are missing, the Vue shell shows loading/error — not a blank crash.
    const surface = page.locator(
      '.ai-react-entry-shell__loading, .ai-react-entry-shell__error, #ai-react-root[data-ai-loader="loaded"]',
    ).first();
    await expect(surface).toBeVisible({ timeout: 15_000 });

    const errorBlock = page.locator('.ai-react-entry-shell__error');
    if (await errorBlock.isVisible().catch(() => false)) {
      await expect(page.getByText(/AI 监控|暂时无法加载|AI 静态资源/).first()).toBeVisible();
      await expect(page.locator('.ai-react-entry-shell__retry')).toBeVisible();
    }
  });
});
