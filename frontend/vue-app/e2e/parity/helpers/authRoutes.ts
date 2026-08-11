import type { Page, Route } from '@playwright/test';

export const PARITY_ADMIN = {
  id: '00000000-0000-4000-8000-000000000001',
  username: 'parity_admin',
  email: 'parity.admin@example.test',
  is_active: true,
  is_verified: true,
  is_admin: true,
  created_at: '2026-01-01T00:00:00Z',
  last_login_at: '2026-07-14T02:25:00Z',
  roles: ['system_admin'],
  permissions: ['*'],
  display_name: '基线管理员',
  effective_operator_name: '基线管理员',
  effective_operator_label: '基线管理员 · 系统管理员',
  operator_context_type: 'user',
  operator_context_id: '00000000-0000-4000-8000-000000000001',
  department: '运行控制中心',
  job_level: 9,
  job_title: '系统管理员',
  permission_version: 7,
} as const;

export const PARITY_READONLY = {
  ...PARITY_ADMIN,
  id: '00000000-0000-4000-8000-000000000003',
  username: 'parity_readonly',
  email: 'parity.readonly@example.test',
  is_admin: false,
  roles: ['operations_viewer'],
  permissions: [
    'flight:read',
    'dispatch:read',
    'dispatch:view',
    'team:view',
    'equipment:view',
    'anomaly:read',
    'system.config_read',
  ],
  display_name: '基线只读用户',
  effective_operator_name: '基线只读用户',
  effective_operator_label: '基线只读用户 · 运行观察员',
  operator_context_id: '00000000-0000-4000-8000-000000000003',
  department: '运行质量部',
  job_level: 3,
  job_title: '运行观察员',
  permission_version: 4,
} as const;

export async function installSessionRoutes(
  page: Page,
  user: typeof PARITY_ADMIN | typeof PARITY_READONLY = PARITY_ADMIN,
): Promise<void> {
  await page.route('**/api/v2/auth/refresh', (route) => route.fulfill({
    status: 200,
    json: { access_token: 'parity-access-token', expires_in: 3600 },
  }));
  await page.route('**/api/v2/auth/me', (route) => route.fulfill({
    status: 200,
    json: user,
  }));
  await page.route('**/api/v2/auth/heartbeat', (route) => route.fulfill({
    status: 200,
    json: { success: true },
  }));
  await page.route('**/api/v2/auth/sse-token', (route) => route.fulfill({
    status: 200,
    json: { token: 'parity-sse-token', expires_at: '2026-07-14T03:30:00Z' },
  }));
  await page.route('**/api/v2/auth/logout', (route) => route.fulfill({
    status: 200,
    json: { success: true },
  }));
}

/** Catch-all: anything under /api/v2 not already handled returns a safe empty envelope. */
export async function installEmptyApiFallback(page: Page): Promise<void> {
  await page.route('**/api/v2/**', async (route: Route) => {
    const request = route.request();
    // Only fulfill if no earlier more-specific route handled it.
    // Playwright runs last-registered first; register this first so specifics win.
    if (request.url().includes('/auth/')) {
      await route.fallback();
      return;
    }
    await route.fulfill({
      status: 200,
      json: { success: true, data: Array.isArray(null) ? [] : {}, error: null },
    });
  });
}
