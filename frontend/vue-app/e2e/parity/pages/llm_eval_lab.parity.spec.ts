import { expect, test, type Page } from '@playwright/test';
import { installSessionRoutes, PARITY_ADMIN } from '../helpers/authRoutes';

const EVAL_JOBS = {
  success: true,
  data: {
    items: [
      {
        job_id: 'eval-job-20260714-001',
        status: 'completed',
        created_at: '2026-07-14T02:05:00Z',
        started_at: '2026-07-14T02:05:05Z',
        finished_at: '2026-07-14T02:07:30Z',
        suite: 'quick',
        options: {
          profile_concurrency: 2,
          case_concurrency: 2,
          enable_tool_routing: true,
        },
        progress: {
          total_attempts: 12,
          completed_attempts: 12,
          percentage: 100,
        },
        ranking: [
          {
            profile_id: 'profile-primary',
            rank: 1,
            score: 0.93,
          },
        ],
        error_message: null,
        profiles: [
          {
            profile_id: 'profile-primary',
            name: '主模型',
            model: 'gpt-parity',
            status: 'completed',
            progress: {
              total_attempts: 6,
              completed_attempts: 6,
              percentage: 100,
            },
            metrics: {
              accuracy: 0.93,
              latency_p95_ms: 820,
            },
            error_message: null,
          },
        ],
      },
    ],
  },
  message: '评测任务列表获取成功',
};

async function installLlmEvalRoutes(page: Page): Promise<void> {
  // Catch-all first; specifics registered after win (Playwright LIFO).
  await page.route('**/api/v2/ai/**', async (route) => {
    await route.fulfill({ status: 200, json: { success: true, data: {} } });
  });
  await page.route('**/api/v2/ai/eval/jobs**', (route) => route.fulfill({
    status: 200,
    json: EVAL_JOBS,
  }));
  await page.route('**/api/v2/ai/eval/**', (route) => {
    const url = route.request().url();
    if (url.includes('/jobs')) return route.fallback();
    return route.fulfill({ status: 200, json: { success: true, data: {} } });
  });
  await page.route('**/api/v2/ai/events/stream**', (route) => route.fulfill({
    status: 200,
    headers: {
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache',
    },
    body: 'event: ai_execution\ndata: {"status":"idle"}\n\n',
  }));
}

test.describe('llm_eval_lab parity', () => {
  test('llm_eval_lab-success: Vue host shell mounts with React entry host (loaded, loading, or error)', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installLlmEvalRoutes(page);

    await page.goto('/frontend/llm_eval_lab.html');

    await expect(page).toHaveURL(/\/frontend\/llm_eval_lab\.html$/);
    await expect(page).toHaveTitle(/LLM Eval Lab/i);

    const shell = page.locator('.ai-react-entry-shell, .ai-page').first();
    await expect(shell).toBeVisible({ timeout: 15_000 });

    const host = page.locator('#ai-react-root');
    await expect(host).toBeVisible();
    await expect(host).toHaveAttribute('data-ai-entry', 'llm_eval_lab');

    const surface = page.locator(
      '.ai-react-entry-shell__loading, .ai-react-entry-shell__error, #ai-react-root[data-ai-loader="loaded"]',
    ).first();
    await expect(surface).toBeVisible({ timeout: 15_000 });

    const errorBlock = page.locator('.ai-react-entry-shell__error');
    if (await errorBlock.isVisible().catch(() => false)) {
      await expect(page.getByText(/LLM 评测|暂时无法加载|AI 静态资源/).first()).toBeVisible();
      await expect(page.locator('.ai-react-entry-shell__retry')).toBeVisible();
    }
  });
});
