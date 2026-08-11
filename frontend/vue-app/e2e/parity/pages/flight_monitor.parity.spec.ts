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
      actual_departure: null,
      actual_arrival: null,
      stand: 'A12',
      gate: '12',
      terminal: 'T1',
      position: 'A12',
      is_quick_turnaround: true,
      is_commercial_signed: false,
      inbound_leg: {
        leg_type: 'inbound',
        flight_no: 'MU5100',
        flight_type: 'domestic',
        mission: 20,
        is_vip: false,
        origin_stations: [{ code: 'SHA', name: '上海虹桥' }],
        destination_stations: [{ code: 'PVG', name: '上海浦东' }],
      },
      outbound_leg: {
        leg_type: 'outbound',
        flight_no: 'MU5101',
        flight_type: 'domestic',
        mission: 20,
        is_vip: false,
        origin_stations: [{ code: 'PVG', name: '上海浦东' }],
        destination_stations: [{ code: 'PEK', name: '北京' }],
      },
      anomaly_summary: {
        has_open_anomaly: true,
        open_count: 1,
        acknowledged_count: 0,
        total: 1,
        open: 1,
        critical: 0,
        highest_severity: 'high',
      },
      labels: ['priority'],
      business_cases: [],
      created_at: '2026-07-14T01:00:00Z',
      updated_at: '2026-07-14T02:28:00Z',
      version: 7,
    },
  ],
};

const AIRPORT_CONTEXT = {
  code: 'PVG',
  display_name: '上海浦东国际机场',
  name_aliases: ['浦东机场', '上海浦东'],
};

const BUSINESS_CASE_TYPES = {
  success: true,
  data: [
    {
      code: 'flight_delay',
      name: '航班延误',
      description: '航班延误处置事项',
      visibility_scope: 'department',
      is_active: true,
      created_at: '2026-07-01T00:00:00Z',
      updated_at: '2026-07-14T00:00:00Z',
    },
  ],
};

async function installFlightMonitorRoutes(page: Page): Promise<void> {
  await page.route('**/api/v2/flights**', (route) => route.fulfill({
    status: 200,
    json: FLIGHTS,
  }));
  await page.route('**/api/v2/system/airport-context**', (route) => route.fulfill({
    status: 200,
    json: AIRPORT_CONTEXT,
  }));
  await page.route('**/api/v2/business-case-types**', (route) => route.fulfill({
    status: 200,
    json: BUSINESS_CASE_TYPES,
  }));
  await page.route('**/api/v2/notifications/unread-count**', (route) => route.fulfill({
    status: 200,
    json: { unread_count: 0 },
  }));
  await page.route('**/api/v2/ai/capabilities**', (route) => route.fulfill({
    status: 200,
    json: {
      success: true,
      data: {
        ai_ready: true,
        ai_execute_permission: true,
        ai_chat_permission: true,
        missing_reasons: [],
      },
    },
  }));
  await page.route('**/api/v2/sse/stream**', (route) => route.fulfill({
    status: 200,
    headers: {
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache',
    },
    body: 'event: connected\ndata: {"status":"connected"}\n\n',
  }));
  // Secondary detail-panel calls after auto-selecting the first flight.
  await page.route('**/api/v2/labels**', (route) => route.fulfill({
    status: 200,
    json: { success: true, data: [] },
  }));
  await page.route('**/api/v2/reference/**', (route) => route.fulfill({
    status: 200,
    json: { success: true, data: [] },
  }));
  await page.route('**/api/v2/dispatch/collaboration/**', (route) => route.fulfill({
    status: 200,
    json: { success: true, data: [] },
  }));
}

test.describe('flight_monitor parity', () => {
  test('flight_monitor-success: renders flight list shell with GET /api/v2/flights fixture data', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installFlightMonitorRoutes(page);

    await page.goto('/frontend/flight_monitor.html');

    const listRegion = page.locator('#flight-list-main');
    await expect(listRegion).toBeVisible({ timeout: 15_000 });
    await expect(listRegion).toHaveAttribute('aria-label', '实时航班列表');
    await expect(page.locator('[data-role="flight-workbar"], #refreshBtn').first()).toBeVisible();

    await expect(page.getByText('MU5101').first()).toBeVisible({ timeout: 15_000 });
    await expect(page.getByText('MU5100').or(page.locator('[data-flight-id="flight-mu5101"]')).first()).toBeVisible();
  });
});
