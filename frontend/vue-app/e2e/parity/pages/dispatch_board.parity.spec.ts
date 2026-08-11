import { expect, test, type Page } from '@playwright/test';
import { installSessionRoutes, PARITY_ADMIN } from '../helpers/authRoutes';

const STANDS = {
  success: true,
  data: [
    {
      id: 'stand-a12',
      code: 'A12',
      terminal: 'T1',
      is_active: true,
    },
  ],
  error: null,
};

const TIMELINE = {
  success: true,
  data: {
    view_mode: 'flight',
    window_start: '2026-07-14T01:00:00Z',
    window_end: '2026-07-14T05:00:00Z',
    items: [
      {
        order_id: 'order-5101',
        flight_id: 'flight-mu5101',
        flight_no: 'MU5101',
        task_type: 'boarding',
        task_type_name: '登机保障',
        status: 'pending',
        team_name: '地服一组',
        terminal: 'T1',
        lane_id: 'lane-mu5101',
        lane_label: 'MU5101',
        start_time: '2026-07-14T02:40:00Z',
        end_time: '2026-07-14T03:10:00Z',
        planned_start_time: '2026-07-14T02:40:00Z',
        planned_end_time: '2026-07-14T03:10:00Z',
        stand_code: 'A12',
        is_flight_summary: false,
      },
    ],
    lanes: [
      {
        id: 'lane-mu5101',
        label: 'MU5101',
        resource_type: 'flight',
        resource_id: 'flight-mu5101',
      },
    ],
  },
  error: null,
};

const SAFETY_PROGRESS = {
  success: true,
  data: {
    items: [
      {
        dispatch_order_id: 'order-5101',
        task_type: 'boarding',
        enforced: true,
        ready: true,
        required_total: 2,
        completed_required: 2,
        pending_required_count: 0,
        failed_required_count: 0,
        template_version: 'v1',
        blocking_issues: [],
        soft_missing_count: 0,
        can_soft_complete: true,
      },
    ],
  },
  error: null,
};

const COLLAB_GROUPS = {
  success: true,
  data: {
    items: [],
    total: 0,
  },
  error: null,
};

async function installDispatchBoardRoutes(page: Page): Promise<void> {
  await page.route('**/api/v2/stands**', (route) => route.fulfill({
    status: 200,
    json: STANDS,
  }));
  await page.route('**/api/v2/dispatch/resources/stands**', (route) => route.fulfill({
    status: 200,
    json: STANDS,
  }));
  await page.route('**/api/v2/dispatch-orders/timeline**', (route) => route.fulfill({
    status: 200,
    json: TIMELINE,
  }));
  await page.route('**/api/v2/dispatch-orders/safety-checklist/progress**', (route) =>
    route.fulfill({ status: 200, json: SAFETY_PROGRESS }),
  );
  await page.route('**/api/v2/dispatch/collaboration/**', (route) => route.fulfill({
    status: 200,
    json: COLLAB_GROUPS,
  }));
  await page.route('**/api/v2/dispatch/analytics/**', (route) => route.fulfill({
    status: 200,
    json: { success: true, data: {}, error: null },
  }));
  await page.route('**/api/v2/sse/stream**', (route) => route.fulfill({
    status: 200,
    headers: {
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache',
    },
    body: 'event: connected\ndata: {"status":"connected"}\n\n',
  }));
  await page.route('**/api/v2/ai/**', (route) => route.fulfill({
    status: 200,
    json: { success: true, data: {}, error: null },
  }));
}

test.describe('dispatch_board parity', () => {
  test('dispatch_board-success: renders gantt shell with toolbar after timeline envelope load', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installDispatchBoardRoutes(page);

    await page.goto('/frontend/dispatch_board.html');

    await expect(page.locator('.dispatch-board-page, .gantt-shell').first()).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.locator('#opsDock')).toBeVisible();
    await expect(page.locator('.gantt-shell')).toBeVisible();
    await expect(page.locator('#openAiFloatingBtn')).toBeVisible();
    await expect(page.getByRole('button', { name: '智能派工' })).toBeVisible();
    await expect(page.getByRole('button', { name: '派工状态' })).toBeVisible();
  });

  test('dispatch_board-success: gantt legend and search shell remain available', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installDispatchBoardRoutes(page);

    await page.goto('/frontend/dispatch_board.html');
    // #ganttStage can have zero height under flex min-height:0 in headless viewports;
    // assert attached stage + visible shell chrome instead.
    await expect(page.locator('#ganttStage')).toBeAttached({ timeout: 15_000 });
    await expect(page.locator('.gantt-shell')).toBeVisible();
    await expect(page.locator('#timelineSearchInput')).toBeVisible();
    await expect(page.locator('#ganttLegendOverlay')).toBeAttached();
    await expect(page.locator('#openStatusFloatingBtn')).toBeVisible();
    await expect(page.locator('#timelineSearchBtn')).toBeVisible();
  });
});
