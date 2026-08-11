import { test, expect } from '@playwright/test';

const AI_MONITOR_URL = '/frontend/ai_monitor.html';

test.describe('AI monitor page', () => {
  test('AI entry shell mounts on the page', async ({ page }) => {
    await page.goto(AI_MONITOR_URL);

    const host = page.locator('#ai-react-root');
    await expect(host).toBeVisible();
    await expect(host).toHaveAttribute('data-ai-entry', 'ai_monitor');
    await expect(page.locator('.ai-react-entry-shell')).toBeVisible();
  });

  test('AI run surface presents a loading or error state gracefully', async ({ page }) => {
    await page.goto(AI_MONITOR_URL);

    const loading = page.locator('.ai-react-entry-shell__loading');
    const error = page.locator('.ai-react-entry-shell__error');
    await expect(loading.or(error)).toBeVisible();
  });

  test('error scenario: when AI bundle is missing an error block with retry is shown', async ({ page }) => {
    await page.goto(AI_MONITOR_URL);

    const errorBlock = page.locator('.ai-react-entry-shell__error');
    await expect(errorBlock).toBeVisible();
    const retryBtn = page.locator('.ai-react-entry-shell__retry');
    await expect(retryBtn).toBeVisible();
    await expect(retryBtn).toBeEnabled();
  });
});
