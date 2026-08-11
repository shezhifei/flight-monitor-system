import { expect, test } from '@playwright/test';

test.describe('login parity', () => {
  test('login-required-username-error: required-field validation focuses username', async ({ page }) => {
    await page.goto('/frontend/login.html');
    await expect(page.getByRole('heading', { name: '航班监控系统' })).toBeVisible();

    await page.locator('#loginBtn').click();
    await expect(page.getByRole('alert')).toContainText('请填写用户名');
    await expect(page.locator('#username')).toBeFocused();
    await expect(page.locator('#username')).toHaveAttribute('aria-invalid', 'true');
  });

  test('login-password-visible: password visibility toggle', async ({ page }) => {
    await page.goto('/frontend/login.html');
    await page.locator('#username').fill('parity_admin');
    await page.locator('#password').fill('fixture-password');
    await expect(page.locator('#password')).toHaveAttribute('type', 'password');
    await page.locator('#passwordToggleBtn').click();
    await expect(page.locator('#password')).toHaveAttribute('type', 'text');
  });

  test('login-default: invalid credentials surface Rust error.message and keep form usable', async ({ page }) => {
    await page.route('**/api/v2/auth/login', (route) => route.fulfill({
      status: 401,
      json: {
        success: false,
        error: { code: 'HTTP_401', message: '用户名或密码错误' },
      },
    }));
    await page.goto('/frontend/login.html');
    await page.locator('#username').fill('bad_user');
    await page.locator('#password').fill('bad_pass');
    await page.locator('#loginBtn').click();
    await expect(page.getByRole('alert')).toContainText('用户名或密码错误');
    await expect(page.locator('#loginBtn')).toBeEnabled();
    await expect(page.locator('#password')).toBeFocused();
  });

  test('login-default: successful login preserves redirect query target', async ({ page }) => {
    await page.route('**/api/v2/auth/login', (route) => route.fulfill({
      status: 200,
      json: {
        success: true,
        access_token: 'parity-access-token',
        token_type: 'bearer',
        expires_in: 3600,
        user: {
          id: '00000000-0000-4000-8000-000000000001',
          username: 'parity_admin',
          is_admin: true,
          permissions: ['*'],
        },
      },
    }));
    await page.goto('/frontend/login.html?redirect=/frontend/system_flags.html');
    await page.locator('#username').fill('parity_admin');
    await page.locator('#password').fill('ok');
    await page.locator('#loginBtn').click();
    await expect(page.getByRole('status')).toContainText('登录成功');
    await page.waitForURL(/\/frontend\/system_flags\.html$/, { timeout: 5_000 });
  });

  test('login-default: public page does not open EventSource/SSE', async ({ page }) => {
    let sseAttempts = 0;
    page.on('request', (request) => {
      if (request.url().includes('/api/v2/') && (
        request.headers().accept?.includes('text/event-stream')
        || /stream|sse|events/i.test(request.url())
      )) {
        sseAttempts += 1;
      }
    });
    await page.goto('/frontend/login.html');
    await expect(page.getByRole('heading', { name: '航班监控系统' })).toBeVisible();
    await page.waitForTimeout(300);
    expect(sseAttempts).toBe(0);
  });

  test('login-required-username-error: accessibility marks invalid username field', async ({ page }) => {
    await page.goto('/frontend/login.html');
    await page.locator('#loginBtn').click();
    await expect(page.locator('#username')).toHaveAttribute('aria-invalid', 'true');
    await expect(page.getByRole('alert')).toHaveAttribute('aria-live', 'assertive');
  });
});
