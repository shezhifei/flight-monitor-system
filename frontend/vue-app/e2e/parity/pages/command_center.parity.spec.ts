import { expect, test, type Page } from '@playwright/test';
import { installSessionRoutes, PARITY_ADMIN } from '../helpers/authRoutes';

const FLIGHTS = {
  success: true,
  data: [
    {
      flight_id: 'flight-mu5101',
      flight_number: 'MU5101',
      airline_code: 'MU',
      registration: 'B-2026',
      aircraft_type_detail: 'A320',
      status: 'boarding',
      scheduled_departure: '2026-07-14T03:00:00Z',
      scheduled_arrival: '2026-07-14T05:15:00Z',
      estimated_departure: '2026-07-14T03:10:00Z',
      estimated_arrival: '2026-07-14T05:25:00Z',
      delay_minutes: 45,
      stand: 'A12',
      gate: '12',
      terminal: 'T1',
      position: 'A12',
      inbound_leg: {
        leg_type: 'inbound',
        flight_no: 'MU5100',
        flight_type: 'domestic',
        origin_stations: [{ code: 'SHA', name: '上海虹桥' }],
        destination_stations: [{ code: 'PVG', name: '上海浦东' }],
      },
      outbound_leg: {
        leg_type: 'outbound',
        flight_no: 'MU5101',
        flight_type: 'domestic',
        origin_stations: [{ code: 'PVG', name: '上海浦东' }],
        destination_stations: [{ code: 'PEK', name: '北京' }],
      },
      anomaly_summary: {
        total: 1,
        open: 1,
        critical: 0,
        highest_severity: 'high',
      },
    },
  ],
};

const ANOMALIES = {
  success: true,
  data: {
    items: [
      {
        anomaly_id: 'anomaly-5101',
        flight_id: 'flight-mu5101',
        flight_number: 'MU5101',
        anomaly_type: 'service_node_timeout',
        severity: 'high',
        status: 'open',
        escalation_level: 1,
        title: '登机保障节点即将超时',
        description: '登机节点剩余 15 分钟',
        detected_at: '2026-07-14T02:26:00Z',
        resolved_at: null,
        created_at: '2026-07-14T02:26:00Z',
        updated_at: '2026-07-14T02:28:00Z',
      },
    ],
    total: 1,
    limit: 500,
    offset: 0,
  },
};

const DISPATCH_ORDERS = {
  success: true,
  data: [
    {
      order_id: 'order-5101',
      flight_id: 'flight-mu5101',
      flight_number: 'MU5101',
      task_type: 'boarding',
      task_name: '登机保障',
      status: 'pending',
      blocked: false,
      team_name: '地服一组',
      assignee_name: '张航',
      terminal: 'T1',
      stand: 'A12',
    },
  ],
};

async function installCommandCenterRoutes(page: Page): Promise<void> {
  await page.route('**/api/v2/flights**', (route) => route.fulfill({
    status: 200,
    json: FLIGHTS,
  }));
  await page.route('**/api/v2/anomalies**', (route) => {
    const url = route.request().url();
    if (url.includes('/stats')) {
      return route.fulfill({
        status: 200,
        json: {
          success: true,
          data: {
            total: 1,
            open: 1,
            acknowledged: 0,
            resolved: 0,
            critical: 0,
            escalated: 1,
          },
        },
      });
    }
    return route.fulfill({ status: 200, json: ANOMALIES });
  });
  await page.route('**/api/v2/dispatch-orders**', (route) => route.fulfill({
    status: 200,
    json: DISPATCH_ORDERS,
  }));
  await page.route('**/api/v2/sse/stream**', (route) => route.fulfill({
    status: 200,
    headers: {
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache',
    },
    body: 'event: connected\ndata: {"status":"connected"}\n\n',
  }));
}

test.describe('command_center parity', () => {
  test('command_center-success: renders verdict shell and KPIs from flights/anomalies/dispatch envelopes', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installCommandCenterRoutes(page);

    await page.goto('/frontend/command_center.html');

    await expect(page.locator('.command-center-page, .command-verdict-strip').first()).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.locator('#opsVerdictTitle, #opsVerdictChip').first()).toBeVisible();
    await expect(page.locator('#opsVerdictTitle')).toContainText(/运行正常|需关注|高压力态势/);
    await expect(page.getByText('未闭环异常').first()).toBeVisible();
    await expect(page.locator('#metricOpenAnomalies')).toHaveText('1');
    await expect(page.locator('#metricDecisionCount')).toHaveText('1');
    // Priority queue surfaces open anomaly / risk flight labels from fixture data
    await expect(page.getByText(/MU5101|登机节点剩余 15 分钟|延误/).first()).toBeVisible();
  });
});
