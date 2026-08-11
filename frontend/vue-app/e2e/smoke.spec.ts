import { test, expect } from '@playwright/test';

test.describe('Smoke tests', () => {
  test('login form validates required fields and toggles password visibility', async ({ page }) => {
    await page.goto('/frontend/login.html');
    await expect(page).toHaveTitle(/登录|login/i);
    await expect(page.getByRole('heading', { name: '航班监控系统' })).toBeVisible();

    await page.locator('#loginBtn').click();
    await expect(page.getByRole('alert')).toContainText('请填写用户名');
    await expect(page.locator('#username')).toHaveAttribute('aria-invalid', 'true');

    await page.locator('#username').fill('dispatcher');
    await page.locator('#loginBtn').click();
    await expect(page.getByRole('alert')).toContainText('请填写密码');
    await expect(page.locator('#password')).toHaveAttribute('aria-invalid', 'true');

    await expect(page.locator('#password')).toHaveAttribute('type', 'password');
    await page.locator('#passwordToggleBtn').click();
    await expect(page.locator('#password')).toHaveAttribute('type', 'text');
  });

  test('health endpoint responds', async ({ request }) => {
    const response = await request.get('/api/v2/health/ping');
    expect(response.ok()).toBeTruthy();
  });
});
