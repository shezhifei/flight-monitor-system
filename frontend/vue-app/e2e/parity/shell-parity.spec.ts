import { existsSync } from 'node:fs';
import { isAbsolute, relative, resolve } from 'node:path';

import { expect, test, type Page, type Request } from '@playwright/test';

const AI_STATIC_ROOT = resolve(process.cwd(), '..', 'static', 'ai');

const READONLY_USER = {
  id: '00000000-0000-4000-8000-000000000003',
  username: 'parity_readonly',
  display_name: '基线只读用户',
  is_admin: false,
  roles: ['operations_viewer'],
  permissions: ['flight:read', 'dispatch:read', 'system.config_read'],
  department: '运行质量部',
};

interface AuthRequestCounts {
  heartbeat: number;
  logout: number;
  me: number;
  refresh: number;
}

async function installAuthenticatedShellRoutes(page: Page): Promise<AuthRequestCounts> {
  const counts: AuthRequestCounts = { heartbeat: 0, logout: 0, me: 0, refresh: 0 };

  await page.route('**/api/v2/**', async (route) => {
    const request = route.request();
    const pathname = new URL(request.url()).pathname;

    if (pathname === '/api/v2/auth/refresh') {
      counts.refresh += 1;
      await route.fulfill({
        status: 200,
        json: { access_token: 'shell-parity-access-token', expires_in: 3600 },
      });
      return;
    }
    if (pathname === '/api/v2/auth/me') {
      counts.me += 1;
      await route.fulfill({ status: 200, json: READONLY_USER });
      return;
    }
    if (pathname === '/api/v2/auth/heartbeat') {
      counts.heartbeat += 1;
      await route.fulfill({ status: 200, json: { success: true } });
      return;
    }
    if (pathname === '/api/v2/auth/sse-token') {
      await route.fulfill({
        status: 200,
        json: { sse_token: 'shell-parity-sse-token', sse_expires_in: 3600 },
      });
      return;
    }
    if (pathname === '/api/v2/auth/logout') {
      counts.logout += 1;
      await route.fulfill({ status: 200, json: { success: true } });
      return;
    }

    await route.fulfill({
      status: 403,
      json: { success: false, error: { code: 'FORBIDDEN', message: '只读账号无此权限' } },
    });
  });

  return counts;
}

function requestPath(request: Request): string {
  return new URL(request.url()).pathname;
}

async function installBuiltAiAssetRoutes(page: Page): Promise<void> {
  await page.route('**/frontend/static/ai/**', async (route) => {
    const pathname = decodeURIComponent(new URL(route.request().url()).pathname);
    const assetPath = pathname.slice('/frontend/static/ai/'.length);
    const candidate = resolve(AI_STATIC_ROOT, assetPath);
    const relativePath = relative(AI_STATIC_ROOT, candidate);
    if (!relativePath || relativePath.startsWith('..') || isAbsolute(relativePath) || !existsSync(candidate)) {
      await route.fulfill({ status: 404, body: 'AI build artifact not found' });
      return;
    }
    const contentType = candidate.endsWith('.json')
      ? 'application/json'
      : candidate.endsWith('.css')
        ? 'text/css'
        : candidate.endsWith('.js')
          ? 'text/javascript'
          : candidate.endsWith('.ttf')
            ? 'font/ttf'
            : 'application/octet-stream';
    await route.fulfill({ path: candidate, contentType });
  });
}

test.describe('protected MPA shell parity', () => {
  test('redirects unauthenticated direct navigation before mounting protected content', async ({ page }) => {
    const protectedRequests: string[] = [];
    page.on('request', (request) => {
      const pathname = requestPath(request);
      if (pathname.startsWith('/api/v2/') && pathname !== '/api/v2/auth/refresh') {
        protectedRequests.push(pathname);
      }
    });
    await page.route('**/api/v2/auth/refresh', (route) => route.fulfill({
      status: 401,
      json: { success: false, error: { code: 'UNAUTHORIZED', message: '未登录' } },
    }));

    await page.goto('/frontend/label_manager.html');

    await expect(page).toHaveURL(/\/frontend\/login\.html$/);
    await expect(page.getByRole('heading', { name: '标签定义管理' })).toHaveCount(0);
    expect(protectedRequests).toEqual([]);
  });

  test('fails an expired cookie session closed before protected business requests', async ({ page }) => {
    let refreshCalls = 0;
    const businessRequests: string[] = [];
    page.on('request', (request) => {
      const pathname = requestPath(request);
      if (pathname.startsWith('/api/v2/') && !pathname.startsWith('/api/v2/auth/')) {
        businessRequests.push(pathname);
      }
    });
    await page.route('**/api/v2/auth/refresh', async (route) => {
      refreshCalls += 1;
      if (refreshCalls === 1) {
        await route.fulfill({
          status: 200,
          json: { access_token: 'expired-shell-token', expires_in: 3600 },
        });
        return;
      }
      await route.fulfill({
        status: 401,
        json: { success: false, error: { code: 'SESSION_EXPIRED', message: '会话已过期' } },
      });
    });
    await page.route('**/api/v2/auth/me', (route) => route.fulfill({
      status: 401,
      json: { success: false, error: { code: 'UNAUTHORIZED', message: '访问令牌已失效' } },
    }));

    await page.goto('/frontend/label_manager.html');

    await expect(page).toHaveURL(/\/frontend\/login\.html$/);
    expect(refreshCalls).toBe(2);
    expect(businessRequests).toEqual([]);
  });

  test('restores readonly identity and permissions across full-page navigation', async ({ page }) => {
    const counts = await installAuthenticatedShellRoutes(page);

    await page.goto('/frontend/label_manager.html');
    await expect(page.getByRole('heading', { name: '标签定义管理' })).toBeVisible();
    await expect(page.getByRole('button', { name: '新建标签' })).toHaveCount(0);

    await page.goto('/frontend/system_flags.html');
    await expect(page.getByText('parity_readonly')).toBeVisible();
    await expect(page).toHaveURL(/\/frontend\/system_flags\.html$/);

    expect(counts.refresh).toBe(2);
    expect(counts.me).toBe(2);
    expect(counts.heartbeat).toBeGreaterThanOrEqual(2);
  });

  test('reconnects the React SSE stream through the authenticated Vue transport', async ({ page }) => {
    await installAuthenticatedShellRoutes(page);
    await installBuiltAiAssetRoutes(page);
    let streamRequests = 0;
    await page.route('**/api/v2/ai/events/stream**', async (route) => {
      streamRequests += 1;
      await route.fulfill({
        status: 200,
        headers: {
          'Cache-Control': 'no-cache',
          'Content-Type': 'text/event-stream',
        },
        body: 'event: ai_execution\ndata: {"event":"tool_end","message":"parity"}\n\n',
      });
    });

    await page.goto('/frontend/ai_monitor.html');

    await expect(page.locator('#ai-react-root')).toHaveAttribute('data-ai-loader', 'loaded');
    await expect.poll(() => streamRequests, { timeout: 10_000 }).toBeGreaterThanOrEqual(2);
  });

  test('logout clears the protected shell and returns to the public login page', async ({ page }) => {
    const counts = await installAuthenticatedShellRoutes(page);

    await page.goto('/frontend/system_flags.html');
    await expect(page.getByText('parity_readonly')).toBeVisible();
    await page.getByTitle('退出登录').click();

    await expect(page).toHaveURL(/\/frontend\/login\.html$/);
    expect(counts.logout).toBe(1);
  });
});
