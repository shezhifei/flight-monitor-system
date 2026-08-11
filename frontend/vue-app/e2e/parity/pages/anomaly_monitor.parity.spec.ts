import { expect, test, type Page } from '@playwright/test';
import { installSessionRoutes, PARITY_ADMIN, PARITY_READONLY } from '../helpers/authRoutes';

const ANOMALY_LIST = {
  success: true,
  data: {
    items: [
      {
        anomaly_id: 'anomaly-5101',
        flight_id: 'flight-mu5101',
        anomaly_type: 'service_node_timeout',
        severity: 'high',
        status: 'open',
        title: '登机保障节点即将超时',
        description: '登机节点剩余 15 分钟',
        detected_at: '2026-07-14T02:26:00Z',
        resolved_at: null,
        escalation_level: 1,
        last_escalated_at: null,
        linked_todo_id: null,
        rule_id: null,
        context_data: {},
        created_at: '2026-07-14T02:26:00Z',
        updated_at: '2026-07-14T02:28:00Z',
      },
    ],
    total: 1,
    limit: 100,
    offset: 0,
  },
};

const ANOMALY_STATS = {
  success: true,
  data: {
    total: 1,
    open: 1,
    acknowledged: 0,
    resolved: 0,
    critical: 0,
    escalated: 1,
  },
};

async function installAnomalyRoutes(
  page: Page,
  user: typeof PARITY_ADMIN | typeof PARITY_READONLY = PARITY_ADMIN,
): Promise<{ mutationRequests: string[]; resolveBodies: unknown[] }> {
  const mutationRequests: string[] = [];
  const resolveBodies: unknown[] = [];
  await installSessionRoutes(page, user);
  await page.route('**/api/v2/anomalies/stats**', (route) => route.fulfill({
    status: 200,
    json: ANOMALY_STATS,
  }));
  await page.route('**/api/v2/anomalies?**', (route) => route.fulfill({
    status: 200,
    json: ANOMALY_LIST,
  }));
  await page.route('**/api/v2/anomalies/*/resolve', async (route) => {
    mutationRequests.push(route.request().url());
    resolveBodies.push(route.request().postDataJSON());
    await route.fulfill({
      status: 200,
      json: { success: true, data: { anomaly_id: 'anomaly-5101', todo_resolved: false } },
    });
  });
  await page.route('**/api/v2/anomalies/*/acknowledge', (route) => {
    mutationRequests.push(route.request().url());
    return route.fulfill({
      status: 200,
      json: { success: true, data: { anomaly_id: 'anomaly-5101' } },
    });
  });
  await page.route('**/api/v2/anomalies/stream**', (route) => route.fulfill({
    status: 200,
    headers: {
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache',
    },
    body: 'event: initial\ndata: {"items":[]}\n\n',
  }));
  return { mutationRequests, resolveBodies };
}

test.describe('anomaly_monitor parity', () => {
  test('anomaly_monitor-success: unwraps Rust list envelope and renders anomaly_id fields', async ({ page }) => {
    await installAnomalyRoutes(page);
    await page.goto('/frontend/anomaly_monitor.html');

    await expect(page.locator('#statsGrid, .metrics-strip').first()).toBeVisible();
    await expect(page.getByText('登机保障节点即将超时')).toBeVisible();
    await expect(page.getByRole('cell', { name: '节点超时' })).toBeVisible();
  });

  test('anomaly_monitor-success: resolve posts AnomalyResolveRequest body', async ({ page }) => {
    const { resolveBodies } = await installAnomalyRoutes(page);
    await page.goto('/frontend/anomaly_monitor.html');
    await expect(page.getByText('登机保障节点即将超时')).toBeVisible();

    const resolveButton = page.getByRole('button', { name: /解决|确认解决|resolve/i }).first();
    await expect(resolveButton).toBeVisible();
    await resolveButton.click();
    await expect.poll(() => resolveBodies.length).toBeGreaterThan(0);
    const body = resolveBodies[0] as { resolve_todo?: boolean };
    expect(body).toMatchObject({ resolve_todo: expect.any(Boolean) });
  });

  test('anomaly_monitor-readonly: hides mutation actions and sends no mutation request', async ({ page }) => {
    const { mutationRequests } = await installAnomalyRoutes(page, PARITY_READONLY);
    await page.goto('/frontend/anomaly_monitor.html');

    await expect(page.getByText('登机保障节点即将超时')).toBeVisible();
    await expect(page.getByText('只读', { exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: /确认|解决/ })).toHaveCount(0);
    expect(mutationRequests).toHaveLength(0);
  });
});
