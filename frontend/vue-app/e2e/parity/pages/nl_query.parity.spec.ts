import { expect, test, type Page } from '@playwright/test';
import { installSessionRoutes, PARITY_ADMIN } from '../helpers/authRoutes';

const SUGGESTIONS = {
  success: true,
  data: {
    suggestions: [
      {
        label: '延误航班',
        text: '查询未来两小时预计延误的航班',
      },
      {
        label: '机位冲突',
        text: '列出当前机位冲突及影响航班',
      },
    ],
  },
  message: '查询建议获取成功',
};

const HISTORY = {
  success: true,
  data: {
    items: [],
    total: 0,
  },
};

async function installNlQueryRoutes(page: Page): Promise<void> {
  // Catch-all first; specifics registered after win (Playwright LIFO).
  await page.route('**/api/v2/ai/**', async (route) => {
    await route.fulfill({ status: 200, json: { success: true, data: {} } });
  });
  await page.route('**/api/v2/ai/nl-query/suggestions**', (route) => route.fulfill({
    status: 200,
    json: SUGGESTIONS,
  }));
  await page.route('**/api/v2/ai/nl-query/history**', (route) => route.fulfill({
    status: 200,
    json: HISTORY,
  }));
  await page.route('**/api/v2/ai/nl-query/**', (route) => {
    const url = route.request().url();
    if (url.includes('/suggestions') || url.includes('/history')) return route.fallback();
    return route.fulfill({ status: 200, json: { success: true, data: {} } });
  });
  await page.route('**/api/v2/ai/events/stream**', (route) => route.fulfill({
    status: 200,
    headers: {
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache',
    },
    body: 'event: ai_execution\ndata: {"event_type":"ai_execution","payload":{"status":"idle"}}\n\n',
  }));
}

test.describe('nl_query parity', () => {
  test('nl_query-success: Vue host shell mounts with React entry host (loaded, loading, or error)', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installNlQueryRoutes(page);

    await page.goto('/frontend/nl_query.html');

    await expect(page).toHaveURL(/\/frontend\/nl_query\.html$/);
    await expect(page).toHaveTitle(/NL Query/i);

    const shell = page.locator('.ai-react-entry-shell, .nl-query-page, .workspace-page').first();
    await expect(shell).toBeVisible({ timeout: 15_000 });

    const host = page.locator('#ai-react-root');
    await expect(host).toBeVisible();
    await expect(host).toHaveAttribute('data-ai-entry', 'nl_query');

    const surface = page.locator(
      '.ai-react-entry-shell__loading, .ai-react-entry-shell__error, #ai-react-root[data-ai-loader="loaded"]',
    ).first();
    await expect(surface).toBeVisible({ timeout: 15_000 });

    const errorBlock = page.locator('.ai-react-entry-shell__error');
    if (await errorBlock.isVisible().catch(() => false)) {
      await expect(page.getByText(/自然语言查询|暂时无法加载|AI 静态资源/).first()).toBeVisible();
      await expect(page.locator('.ai-react-entry-shell__retry')).toBeVisible();
    }
  });
});
