import { expect, test, type Page } from '@playwright/test';
import { installSessionRoutes, PARITY_ADMIN } from '../helpers/authRoutes';

const HEALTH = {
  success: true,
  status: 'healthy',
  database: {
    flights: 128,
  },
  errors_count: 1,
  recent_errors: [
    {
      error_id: 'error-20260714-001',
      timestamp: '2026-07-14T02:24:00Z',
      error_type: 'SSE_RECONNECT',
      severity: 'medium',
      message: '客户端已自动重连',
      emitted_at_ms: 1783995840000,
    },
  ],
  buffer_status: {
    total_connections: 7,
    max_connections: 1000,
    status: 'active',
    topics: {
      flight_updates: 3,
      global_status: 2,
      status_changes: 2,
    },
  },
  services: {
    api_server: {
      status: 'healthy',
      detail: 'API 服务在线',
      uptime_seconds: 86400,
    },
    postgres: {
      status: 'healthy',
      detail: '连接池正常',
    },
    redis: {
      status: 'healthy',
      detail: '延迟 2.4 ms',
    },
    auth: {
      status: 'healthy',
      detail: '认证服务正常',
    },
  },
  runtime: {
    started_at: '2026-07-13T02:30:00Z',
    uptime_seconds: 86400,
    uptime_human: '1d',
    timestamp: '2026-07-14T02:30:00Z',
  },
};

const PERFORMANCE = {
  success: true,
  data: {
    db_pool: {
      active: 4,
      idle: 12,
      max: 20,
      usage_pct: 20,
    },
    redis: {
      latency_ms: 2.4,
      connected: true,
    },
    sse: {
      connections: 7,
      max: 1000,
      usage_pct: 0.7,
    },
    requests: {
      p50: 18,
      p95: 74,
      p99: 120,
      avg: 29,
      count: 2400,
    },
    timestamp: 1783996200,
  },
  message: '性能指标获取成功',
  error: null,
};

const ERRORS = {
  success: true,
  data: [
    {
      error_id: 'error-20260714-001',
      timestamp: '2026-07-14T02:24:00Z',
      error_type: 'SSE_RECONNECT',
      severity: 'medium',
      message: '客户端已自动重连',
      emitted_at_ms: 1783995840000,
    },
  ],
};

const SSE_STATS = {
  success: true,
  data: {
    active_connections: 7,
    total_connections: 7,
    max_connections: 1000,
    topics: {
      flight_updates: 3,
      global_status: 2,
      status_changes: 2,
    },
    connection_breakdown: {
      connected: 7,
      inactive: 0,
    },
  },
};

async function installSystemStatusRoutes(page: Page): Promise<void> {
  await page.route('**/api/v2/health/performance**', (route) => route.fulfill({
    status: 200,
    json: PERFORMANCE,
  }));
  await page.route('**/api/v2/health/errors/clear**', (route) => route.fulfill({
    status: 200,
    json: { success: true, message: 'cleared' },
  }));
  await page.route('**/api/v2/health/errors**', (route) => route.fulfill({
    status: 200,
    json: ERRORS,
  }));
  await page.route('**/api/v2/system/runtime/streaming/sse-stats**', (route) => route.fulfill({
    status: 200,
    json: SSE_STATS,
  }));
  // Flat health payload (not always envelope-wrapped).
  await page.route('**/api/v2/health**', (route) => {
    const url = route.request().url();
    if (url.includes('/performance') || url.includes('/errors') || url.includes('/stream')) {
      return route.fallback();
    }
    return route.fulfill({ status: 200, json: HEALTH });
  });
  await page.route('**/api/v2/sse/stream**', (route) => route.fulfill({
    status: 200,
    headers: {
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache',
    },
    body: 'event: connected\ndata: {"status":"connected"}\n\n',
  }));
}

test.describe('system_status parity', () => {
  test('system_status-success: renders health metrics from Rust health/performance envelopes', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installSystemStatusRoutes(page);

    await page.goto('/frontend/system_status.html');

    await expect(page.locator('#pageTitle, .status-card, .dashboard-container').first()).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByText('系统整体状态')).toBeVisible();
    await expect(page.locator('#statusText')).toContainText(/运行正常|性能降级|服务不可用|状态未知/);
    await expect(page.locator('#countFlights')).toHaveText('128');
    await expect(page.getByText(/客户端已自动重连|SSE_RECONNECT/).first()).toBeVisible();
    await expect(page.getByText('核心基础设施').first()).toBeVisible();
    await expect(page.getByText('API Server').first()).toBeVisible();
  });
});
