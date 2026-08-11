import { expect, test, type Page } from '@playwright/test';
import { installSessionRoutes, PARITY_ADMIN, PARITY_READONLY } from '../helpers/authRoutes';

/**
 * Vue system_flags parity scenarios against Rust-shaped fixtures.
 */
const SYSTEM_FLAGS_FIXTURES = {
  flags: {
    success: true,
    data: {
      flags: [
        {
          path: 'feature_flags.dispatch_chat_v1.enabled',
          value: true,
          type: 'boolean',
          category: 'feature_flags',
          label: 'Enabled',
          description: 'Configuration for feature_flags.dispatch_chat_v1.enabled',
          masked: false,
        },
        {
          path: 'database.password',
          value: '***REDACTED***',
          type: 'string',
          category: 'database',
          label: 'Password',
          description: 'Configuration for database.password',
          masked: true,
        },
      ],
    },
  },
};

async function installFlagsRoutes(page: Page, options: { allowPatch?: boolean } = {}): Promise<{ patchBodies: unknown[] }> {
  const patchBodies: unknown[] = [];
  await page.route('**/api/v2/system/flags', async (route) => {
    if (route.request().method() === 'GET') {
      if (options.allowPatch === false) {
        await route.fulfill({
          status: 403,
          json: { success: false, error: { code: 'HTTP_403', message: '缺少权限: system:config' } },
        });
        return;
      }
      await route.fulfill({ status: 200, json: SYSTEM_FLAGS_FIXTURES.flags });
      return;
    }
    if (route.request().method() === 'PATCH') {
      const body = route.request().postDataJSON();
      patchBodies.push(body);
      if (options.allowPatch === false) {
        await route.fulfill({
          status: 403,
          json: { success: false, error: { code: 'HTTP_403', message: '缺少权限: system:config' } },
        });
        return;
      }
      const typed = body as { path?: string; value?: unknown };
      expect(typed.path).toBeTruthy();
      expect(typed.path).not.toBe('');
      await route.fulfill({
        status: 200,
        json: {
          success: true,
          data: { path: typed.path, value: typed.value, masked: false, success: true },
        },
      });
      return;
    }
    await route.fulfill({ status: 405, json: { success: false } });
  });
  return { patchBodies };
}

test.describe('system_flags parity', () => {
  test('system_flags-success: renders path/label flags and keeps masked values read-only', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installFlagsRoutes(page);
    await page.goto('/frontend/system_flags.html');

    await expect(page.locator('#content-area, .main-content').first()).toBeVisible();
    await expect(page.getByText('feature_flags.dispatch_chat_v1.enabled', { exact: true })).toBeVisible();
    await expect(page.getByText('database.password', { exact: true })).toBeVisible();

    const maskedInput = page.locator('input[type="password"], input[disabled]').first();
    if (await maskedInput.count()) {
      await expect(maskedInput).toBeDisabled();
    }
  });

  test('system_flags-success: PATCH update sends dotted path (not blank path)', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    const { patchBodies } = await installFlagsRoutes(page);
    await page.goto('/frontend/system_flags.html');
    await expect(page.getByText('feature_flags.dispatch_chat_v1.enabled', { exact: true })).toBeVisible();

    // Legacy control is a select (已启用/已禁用), not a toggle button.
    const control = page.getByLabel('切换 feature_flags.dispatch_chat_v1.enabled');
    await expect(control).toBeVisible();
    await control.selectOption('false');
    await expect.poll(() => patchBodies.length).toBeGreaterThan(0);
    const body = patchBodies[0] as { path?: string };
    expect(body.path).toBe('feature_flags.dispatch_chat_v1.enabled');
  });

  test('system_flags-forbidden: readonly session sees a persistent permission error and no actions', async ({ page }) => {
    await installSessionRoutes(page, PARITY_READONLY);
    await installFlagsRoutes(page, { allowPatch: false });
    await page.goto('/frontend/system_flags.html');
    await expect(page).toHaveURL(/\/frontend\/system_flags\.html$/);
    await expect(page.getByText('缺少权限: system:config')).toBeVisible();
    await expect(page.getByRole('button', { name: /导出配置/ })).toHaveCount(0);
    await expect(page.getByLabel(/切换 feature_flags/)).toHaveCount(0);
  });
});
