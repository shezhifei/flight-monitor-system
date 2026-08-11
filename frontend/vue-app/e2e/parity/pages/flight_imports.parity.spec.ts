import { expect, test } from '@playwright/test';
import { installSessionRoutes, PARITY_ADMIN } from '../helpers/authRoutes';

/**
 * flight_imports initial render only needs auth; preview/commit require multipart upload
 * and are intentionally not fabricated in the parity shell scenario.
 */
test.describe('flight_imports parity', () => {
  test('flight_imports-success: mounts upload shell after session restore', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);

    await page.goto('/frontend/flight_imports.html');

    await expect(page.locator('.admin-container, .import-shell, .main-content').first()).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByText(/航班导入|PAYLOAD 文件导入/).first()).toBeVisible();
    await expect(page.locator('#fileInput, input[type="file"]').first()).toBeVisible();
    await expect(page.getByRole('button', { name: /上传并预览/ })).toBeVisible();
    await expect(page.getByRole('button', { name: /确认导入/ })).toBeVisible();
  });
});
