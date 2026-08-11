import { test, expect } from '@playwright/test';

const DISPATCH_BOARD_URL = '/frontend/dispatch_board.html';

test.describe('Dispatch board page', () => {
  test('board toolbar and gantt area render on load', async ({ page }) => {
    await page.goto(DISPATCH_BOARD_URL);

    await expect(page.locator('#opsDock')).toBeVisible();
    await expect(page.locator('.gantt-shell')).toBeVisible();
    await expect(page.locator('#openAiFloatingBtn')).toBeVisible();
  });

  test('opening status panel lists orders and exposes the batch action', async ({ page }) => {
    await page.goto(DISPATCH_BOARD_URL);

    await page.locator('#openStatusFloatingBtn').click();
    const statusPanel = page.locator('#statusPanel');
    await expect(statusPanel).toBeVisible();

    const statusItems = page.locator('#statusCounts .status-count-item');
    await expect(statusItems).toBeVisible();
    expect(await statusItems.count()).toBeGreaterThan(0);

    const batchBtn = page.locator('#statusBatchOpenBtn');
    await expect(batchBtn).toBeVisible();
    await expect(batchBtn).toBeDisabled();
  });

  test('assignment: selecting a status highlights it in the status locator', async ({ page }) => {
    await page.goto(DISPATCH_BOARD_URL);

    await page.locator('#openStatusFloatingBtn').click();
    await expect(page.locator('#statusPanel')).toBeVisible();

    const firstStatus = page.locator('#statusCounts .status-count-item').first();
    await firstStatus.click();
    await expect(firstStatus).toHaveClass(/active/);
  });

  test('error scenario: search with no match shows an empty result message', async ({ page }) => {
    await page.goto(DISPATCH_BOARD_URL);

    const searchInput = page.locator('#timelineSearchInput');
    await expect(searchInput).toBeVisible();
    await searchInput.fill('__no_such_flight_zzz__');
    await page.locator('#timelineSearchBtn').click();

    const searchResults = page.locator('#timelineSearchResults');
    await expect(searchResults).toBeVisible();
    await expect(searchResults).toContainText('无匹配结果');
  });
});
