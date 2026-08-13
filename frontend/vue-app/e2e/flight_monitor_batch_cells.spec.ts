import { test, expect, type Page, type Route } from '@playwright/test';
import {
  installSessionRoutes,
  PARITY_ADMIN,
  PARITY_READONLY,
} from './parity/helpers/authRoutes';
import {
  installLegacyAuthStorage,
  type AuthUserFixture,
} from './parity/auth.fixture';

/**
 * Batch cell selection + submit flows with fixture auth and mocked APIs.
 * Does not require a live backend; validates UI selection, permissions, and
 * the atomic PATCH /flights/batch-cells client contract.
 */

const MANAGE_OPERATOR = {
  ...PARITY_ADMIN,
  id: '00000000-0000-4000-8000-000000000010',
  username: 'parity_manager',
  email: 'parity.manager@example.test',
  is_admin: false,
  roles: ['flight_manager'],
  permissions: ['flight.update', 'flight:read'],
  display_name: '航班管理员',
  effective_operator_name: '航班管理员',
  effective_operator_label: '航班管理员',
  operator_context_id: '00000000-0000-4000-8000-000000000010',
  job_level: 6,
  job_title: '航班管理员',
  permission_version: 3,
} as const;

function makeFlight(index: number, overrides: Record<string, unknown> = {}) {
  const n = String(index).padStart(2, '0');
  return {
    flight_id: `flight-batch-${n}`,
    flight_number: `CZ31${n}`,
    airline_code: 'CZ',
    registration: `B-10${n}`,
    aircraft_type_detail: 'A320',
    status: 'boarding',
    scheduled_departure: '2026-07-14T03:00:00Z',
    scheduled_arrival: '2026-07-14T05:15:00Z',
    estimated_departure: '2026-07-14T03:10:00Z',
    estimated_arrival: '2026-07-14T05:25:00Z',
    actual_departure: null,
    actual_arrival: null,
    stand: `A1${n}`,
    gate: `${10 + index}`,
    terminal: 'T1',
    position: `A1${n}`,
    baggage_carousel: null,
    cobt_time: `2026-07-14T02:${30 + index}:00Z`,
    boarding_allowed_time: null,
    start_boarding_time: null,
    end_boarding_time: null,
    on_blocks_time: null,
    off_blocks_time: null,
    flight_remarks: `remark-${n}`,
    is_quick_turnaround: false,
    is_commercial_signed: false,
    inbound_leg: {
      leg_type: 'inbound',
      flight_no: `CZ30${n}`,
      flight_type: 'domestic',
      mission: 20,
      is_vip: false,
      origin_stations: [{ code: 'CAN', name: '广州' }],
      destination_stations: [{ code: 'PVG', name: '上海浦东' }],
    },
    outbound_leg: {
      leg_type: 'outbound',
      flight_no: `CZ31${n}`,
      flight_type: 'domestic',
      mission: 20,
      is_vip: false,
      origin_stations: [{ code: 'PVG', name: '上海浦东' }],
      destination_stations: [{ code: 'PEK', name: '北京' }],
    },
    anomaly_summary: {
      has_open_anomaly: false,
      open_count: 0,
      acknowledged_count: 0,
      total: 0,
      open: 0,
      critical: 0,
      highest_severity: null,
    },
    labels: [],
    business_cases: [],
    created_at: '2026-07-14T01:00:00Z',
    updated_at: '2026-07-14T02:28:00Z',
    version: 10 + index,
    ...overrides,
  };
}

const FLIGHTS = {
  success: true,
  data: [1, 2, 3, 4, 5].map((i) => makeFlight(i)),
};

const AIRPORT_CONTEXT = {
  code: 'PVG',
  display_name: '上海浦东国际机场',
  name_aliases: ['浦东机场', '上海浦东'],
};

async function installCommonRoutes(page: Page): Promise<void> {
  await page.route('**/api/v2/flights**', async (route: Route) => {
    const request = route.request();
    if (request.method() === 'PATCH' && request.url().includes('/batch-cells')) {
      // Let specific batch-cells handlers take over if registered later.
      await route.fallback();
      return;
    }
    if (request.method() === 'GET') {
      await route.fulfill({ status: 200, json: FLIGHTS });
      return;
    }
    await route.fulfill({ status: 200, json: { success: true, data: {} } });
  });
  await page.route('**/api/v2/system/airport-context**', (route) =>
    route.fulfill({ status: 200, json: AIRPORT_CONTEXT }),
  );
  await page.route('**/api/v2/business-case-types**', (route) =>
    route.fulfill({ status: 200, json: { success: true, data: [] } }),
  );
  await page.route('**/api/v2/notifications/unread-count**', (route) =>
    route.fulfill({ status: 200, json: { unread_count: 0 } }),
  );
  await page.route('**/api/v2/ai/capabilities**', (route) =>
    route.fulfill({
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
    }),
  );
  await page.route('**/api/v2/sse/stream**', (route) =>
    route.fulfill({
      status: 200,
      headers: {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
      },
      body: 'event: connected\ndata: {"status":"connected"}\n\n',
    }),
  );
  await page.route('**/api/v2/labels**', (route) =>
    route.fulfill({ status: 200, json: { success: true, data: [] } }),
  );
  await page.route('**/api/v2/reference/**', (route) =>
    route.fulfill({ status: 200, json: { success: true, data: [] } }),
  );
  await page.route('**/api/v2/dispatch/collaboration/**', (route) =>
    route.fulfill({ status: 200, json: { success: true, data: [] } }),
  );
}

async function installBatchColumnStorage(page: Page): Promise<void> {
  await page.addInitScript(() => {
    localStorage.setItem(
      'flight_monitor_columns',
      JSON.stringify({
        stand: true,
        cobt_time: true,
        start_boarding_time: true,
        remarks: true,
      }),
    );
    localStorage.setItem(
      'flight_monitor_columns_order',
      JSON.stringify([
        'flight_number',
        'stand',
        'cobt_time',
        'start_boarding_time',
        'remarks',
      ]),
    );
  });
}

async function openMonitorAs(
  page: Page,
  user: typeof PARITY_ADMIN | typeof PARITY_READONLY | typeof MANAGE_OPERATOR,
): Promise<void> {
  await installBatchColumnStorage(page);
  // Seed JWT-like storage so hasUserPermission / is_admin work before /auth/me resolves.
  const authUser = user as unknown as AuthUserFixture;
  await installLegacyAuthStorage(page, authUser, '2026-07-14T02:30:00Z');
  await installSessionRoutes(page, user as typeof PARITY_ADMIN);
  await installCommonRoutes(page);
  await page.goto('/frontend/flight_monitor.html');
  await expect(page.locator('#flight-list-main')).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText('CZ3101').first()).toBeVisible({ timeout: 15_000 });

  // Batch selection is table-view only; default page mode is card.
  await page.getByRole('button', { name: '表格视图' }).click();
  const table = page.locator('#flightTable');
  await expect(table).toBeVisible({ timeout: 10_000 });
  await expect(table.locator('tbody tr[data-flight-id]').first()).toBeVisible({ timeout: 10_000 });
}

function remarksCell(page: Page, flightId: string) {
  return page.locator(`td[data-field="flight_remarks"][data-flight-id="${flightId}"]`);
}

function cobtCell(page: Page, flightId: string) {
  return page.locator(`td[data-field="cobt_time"][data-flight-id="${flightId}"]`);
}

function standCell(page: Page, flightId: string) {
  return page.locator(`td[data-field="stand"][data-flight-id="${flightId}"]`);
}

function startBoardingCell(page: Page, flightId: string) {
  return page.locator(`td[data-field="start_boarding_time"][data-flight-id="${flightId}"]`);
}

async function selectCell(
  cell: ReturnType<typeof remarksCell>,
  modifiers: Array<'Control' | 'Shift' | 'Meta'> = [],
): Promise<void> {
  await cell.scrollIntoViewIfNeeded();
  await cell.click({ modifiers });
}

/** Real pointer drag: mouse down on start, move through intermediate, up on end. */
async function dragSelectCells(
  page: Page,
  from: ReturnType<typeof remarksCell>,
  to: ReturnType<typeof remarksCell>,
): Promise<void> {
  await from.scrollIntoViewIfNeeded();
  await to.scrollIntoViewIfNeeded();
  const fromBox = await from.boundingBox();
  const toBox = await to.boundingBox();
  if (!fromBox || !toBox) {
    throw new Error('Unable to resolve cell bounding boxes for drag select');
  }
  const startX = fromBox.x + fromBox.width / 2;
  const startY = fromBox.y + fromBox.height / 2;
  const endX = toBox.x + toBox.width / 2;
  const endY = toBox.y + toBox.height / 2;
  await page.mouse.move(startX, startY);
  await page.mouse.down();
  // Step through intermediate positions so elementFromPoint hits each row.
  const steps = 8;
  for (let step = 1; step <= steps; step += 1) {
    const t = step / steps;
    await page.mouse.move(startX + (endX - startX) * t, startY + (endY - startY) * t, { steps: 2 });
  }
  await page.mouse.up();
}

interface BatchCellsBody {
  field?: string;
  value?: unknown;
  client_action_id?: string;
  targets?: Array<{ flight_id: string; expected_version?: number; expected_value?: unknown }>;
}

test.describe('Flight monitor batch cells (fixture auth)', () => {
  test('admin real mouse-drag selects remarks range and submits atomic batch PATCH', async ({ page }) => {
    const batchBodies: BatchCellsBody[] = [];
    await page.route('**/api/v2/flights/batch-cells', async (route) => {
      const body = route.request().postDataJSON() as BatchCellsBody;
      batchBodies.push(body);
      const targets = Array.isArray(body?.targets) ? body.targets : [];
      await route.fulfill({
        status: 200,
        json: {
          success: true,
          message: `批量更新成功：${targets.length} 条`,
          data: {
            batch_id: body?.client_action_id || 'BATCH-TEST',
            field: body?.field,
            updated_count: targets.length,
            results: targets.map((t) => ({
              flight_id: t.flight_id,
              version: (t.expected_version ?? 10) + 1,
              value: body?.value,
              timeline_id: null,
            })),
          },
        },
      });
    });

    await openMonitorAs(page, PARITY_ADMIN);

    const first = remarksCell(page, 'flight-batch-01');
    const third = remarksCell(page, 'flight-batch-03');
    await expect(first).toBeVisible();

    // True drag (mouse down/move/up), not Shift-click.
    await dragSelectCells(page, first, third);

    await expect(page.locator('.batch-cell-selection-bar')).toBeVisible();
    await expect(page.locator('.batch-cell-selection-bar')).toContainText(/已选 3 个/);

    await first.click({ button: 'right' });
    await expect(page.locator('#flightCellContextMenu')).toBeVisible();
    await page.locator('#flightCellContextMenu button', { hasText: /批量修改/ }).click();

    const modal = page.locator('#flightBatchEditModal');
    await expect(modal).toBeVisible({ timeout: 5_000 });
    const valueInput = modal.locator('textarea.form-control, textarea');
    await expect(valueInput).toBeVisible({ timeout: 5_000 });
    await valueInput.fill('batch-note-ok');

    await modal.getByRole('button', { name: /应用到\s*\d+\s*项/ }).click();

    await expect.poll(() => batchBodies.length).toBe(1);
    const body = batchBodies[0];
    expect(body.field).toBe('flight_remarks');
    expect(body.value).toBe('batch-note-ok');
    expect(body.targets).toHaveLength(3);
    expect((body.targets ?? []).map((t) => t.flight_id).sort()).toEqual([
      'flight-batch-01',
      'flight-batch-02',
      'flight-batch-03',
    ]);
    expect(body.client_action_id).toBeTruthy();
    // Snapshot fields must carry expected_version.
    expect((body.targets ?? []).every((t) => typeof t.expected_version === 'number')).toBe(true);

    await expect(page.locator('.batch-cell-selection-bar')).toHaveCount(0, { timeout: 10_000 });
  });

  test('Ctrl multi-select appends discontinuous remarks cells', async ({ page }) => {
    await openMonitorAs(page, PARITY_ADMIN);

    await selectCell(remarksCell(page, 'flight-batch-01'));
    await selectCell(remarksCell(page, 'flight-batch-03'), ['Control']);
    await selectCell(remarksCell(page, 'flight-batch-05'), ['Control']);

    await expect(page.locator('.batch-cell-selection-bar')).toContainText(/已选 3 个/);
    await expect(remarksCell(page, 'flight-batch-01')).toHaveClass(/cell-batch-selected/);
    await expect(remarksCell(page, 'flight-batch-02')).not.toHaveClass(/cell-batch-selected/);
    await expect(remarksCell(page, 'flight-batch-03')).toHaveClass(/cell-batch-selected/);
  });

  test('Ctrl cross-column append is rejected and original selection kept', async ({ page }) => {
    await openMonitorAs(page, PARITY_ADMIN);

    await selectCell(remarksCell(page, 'flight-batch-01'));
    await expect(page.locator('.batch-cell-selection-bar')).toContainText(/航班备注|备注/);

    // Try to add a stand cell with Ctrl — should not switch column or clear remarks.
    await selectCell(standCell(page, 'flight-batch-02'), ['Control']);

    await expect(remarksCell(page, 'flight-batch-01')).toHaveClass(/cell-batch-selected/);
    await expect(standCell(page, 'flight-batch-02')).not.toHaveClass(/cell-batch-selected/);
  });

  test('non-admin flight.update can select remarks but not stand (adminOnly)', async ({ page }) => {
    await openMonitorAs(page, MANAGE_OPERATOR);

    // Remarks is allowed for ordinary manage users.
    await selectCell(remarksCell(page, 'flight-batch-01'));
    await expect(page.locator('.batch-cell-selection-bar')).toBeVisible();
    await expect(remarksCell(page, 'flight-batch-01')).toHaveClass(/cell-batch-selected/);

    // Stand is sync-locked / adminOnly — should not become batch-editable.
    await page.keyboard.press('Escape');
    await expect(standCell(page, 'flight-batch-01')).not.toHaveClass(/cell-batch-editable/);
  });

  test('readonly user has no batch selection chrome on editable columns', async ({ page }) => {
    await openMonitorAs(page, PARITY_READONLY);

    await expect(page.locator('.batch-cell-selection-bar')).toHaveCount(0);
    // Cells may render but without batch-editable class / selection handlers.
    const remarks = remarksCell(page, 'flight-batch-01');
    if (await remarks.count()) {
      await remarks.click({ button: 'right' });
      await expect(page.locator('#flightCellContextMenu')).toHaveCount(0);
    }
  });

  test('version conflict 409 keeps selection and shows error', async ({ page }) => {
    await page.route('**/api/v2/flights/batch-cells', async (route) => {
      await route.fulfill({
        status: 409,
        json: {
          success: false,
          error: {
            code: 'FLIGHT_BATCH_CONFLICT',
            message: '1 个航班的数据已被其他用户更新，本次批量修改未执行',
            details: {
              code: 'FLIGHT_BATCH_CONFLICT',
              conflicts: [
                {
                  flight_id: 'flight-batch-01',
                  reason: 'version_changed',
                  expected_version: 11,
                  current_version: 12,
                },
              ],
            },
            type: 'conflict_error',
          },
        },
      });
    });

    await openMonitorAs(page, PARITY_ADMIN);
    await selectCell(remarksCell(page, 'flight-batch-01'));
    await page.locator('.batch-cell-selection-bar button', { hasText: /批量修改/ }).click();

    const modal = page.locator('#flightBatchEditModal');
    await expect(modal).toBeVisible();
    await modal.locator('textarea').fill('will-conflict');
    await modal.getByRole('button', { name: /应用到\s*\d+\s*项/ }).click();

    // Selection should remain for retry.
    await expect(page.locator('.batch-cell-selection-bar')).toBeVisible({ timeout: 8_000 });
    await expect(page.getByText(/FLIGHT_BATCH_CONFLICT|冲突|失败/).first()).toBeVisible({ timeout: 8_000 });
  });

  test('admin can batch-edit COBT via same interface', async ({ page }) => {
    let captured: BatchCellsBody | null = null;
    await page.route('**/api/v2/flights/batch-cells', async (route) => {
      captured = route.request().postDataJSON() as BatchCellsBody;
      const targets = Array.isArray(captured?.targets) ? captured.targets : [];
      await route.fulfill({
        status: 200,
        json: {
          success: true,
          message: `批量更新成功：${targets.length} 条`,
          data: {
            batch_id: 'COBT-BATCH',
            field: 'cobt_time',
            updated_count: targets.length,
            results: targets.map((t) => ({
              flight_id: t.flight_id,
              version: 20,
              value: captured?.value,
            })),
          },
        },
      });
    });

    await openMonitorAs(page, PARITY_ADMIN);
    await selectCell(cobtCell(page, 'flight-batch-01'));
    await selectCell(cobtCell(page, 'flight-batch-02'), ['Control']);
    await expect(page.locator('.batch-cell-selection-bar')).toContainText(/COBT/);
    await page.locator('.batch-cell-selection-bar button', { hasText: /批量修改/ }).click();

    const modal = page.locator('#flightBatchEditModal');
    await expect(modal).toBeVisible({ timeout: 5_000 });
    const datetimeInput = modal.locator('input[type="datetime-local"]');
    await expect(datetimeInput).toBeVisible({ timeout: 5_000 });
    await datetimeInput.fill('2026-07-14T15:30');
    await modal.getByRole('button', { name: /应用到\s*\d+\s*项/ }).click();

    await expect.poll(() => captured !== null).toBeTruthy();
    const body = captured as unknown as BatchCellsBody;
    expect(body.field).toBe('cobt_time');
    expect(body.targets?.length).toBe(2);
  });

  test('admin can batch-edit existing start_boarding_time and sends non-null expected_value', async ({ page }) => {
    // Existing timeline values must not be submitted as null (would 409 against real API).
    const existingA = '2026-07-14T03:20:00Z';
    const existingB = '2026-07-14T03:25:00Z';
    const flightsWithBoarding = {
      success: true,
      data: [
        makeFlight(1, { start_boarding_time: existingA }),
        makeFlight(2, { start_boarding_time: existingB }),
        makeFlight(3),
        makeFlight(4),
        makeFlight(5),
      ],
    };

    await page.route('**/api/v2/flights**', async (route: Route) => {
      const request = route.request();
      if (request.method() === 'PATCH' && request.url().includes('/batch-cells')) {
        await route.fallback();
        return;
      }
      if (request.method() === 'GET') {
        await route.fulfill({ status: 200, json: flightsWithBoarding });
        return;
      }
      await route.fulfill({ status: 200, json: { success: true, data: {} } });
    });

    let captured: BatchCellsBody | null = null;
    await page.route('**/api/v2/flights/batch-cells', async (route) => {
      captured = route.request().postDataJSON() as BatchCellsBody;
      const targets = Array.isArray(captured?.targets) ? captured.targets : [];
      // Assert server would not see null expected_value for populated cells.
      for (const t of targets) {
        expect(t.expected_value).not.toBeNull();
        expect(t.expected_value).toBeTruthy();
      }
      await route.fulfill({
        status: 200,
        json: {
          success: true,
          message: `批量更新成功：${targets.length} 条`,
          data: {
            batch_id: 'BOARD-BATCH',
            field: 'start_boarding_time',
            updated_count: targets.length,
            results: targets.map((t) => ({
              flight_id: t.flight_id,
              version: 12,
              value: captured?.value,
              timeline_id: `tl-${t.flight_id}`,
            })),
          },
        },
      });
    });

    // openMonitorAs installs routes first; re-register flights override after would be overridden.
    // So install session + common manually, then flights override above must be registered AFTER openMonitorAs.
    await installBatchColumnStorage(page);
    await installSessionRoutes(page, PARITY_ADMIN as typeof PARITY_ADMIN);
    await installLegacyAuthStorage(page, PARITY_ADMIN as unknown as AuthUserFixture, '2026-07-14T02:30:00Z');
    await installCommonRoutes(page);
    // Override flights GET with populated boarding times (registered after common so it wins).
    await page.route('**/api/v2/flights?**', async (route: Route) => {
      if (route.request().method() === 'GET') {
        await route.fulfill({ status: 200, json: flightsWithBoarding });
        return;
      }
      await route.fallback();
    });
    await page.route('**/api/v2/flights', async (route: Route) => {
      if (route.request().method() === 'GET') {
        await route.fulfill({ status: 200, json: flightsWithBoarding });
        return;
      }
      await route.fallback();
    });

    await page.goto('/frontend/flight_monitor.html');
    await expect(page.locator('#flight-list-main')).toBeVisible({ timeout: 15_000 });
    await expect(page.getByText('CZ3101').first()).toBeVisible({ timeout: 15_000 });
    await page.getByRole('button', { name: '表格视图' }).click();
    await expect(page.locator('#flightTable')).toBeVisible({ timeout: 10_000 });

    await selectCell(startBoardingCell(page, 'flight-batch-01'));
    await selectCell(startBoardingCell(page, 'flight-batch-02'), ['Control']);
    await expect(page.locator('.batch-cell-selection-bar')).toContainText(/开始登机/);
    await page.locator('.batch-cell-selection-bar button', { hasText: /批量修改/ }).click();

    const modal = page.locator('#flightBatchEditModal');
    await expect(modal).toBeVisible({ timeout: 5_000 });
    await modal.locator('input[type="datetime-local"]').fill('2026-07-14T16:00');
    await modal.getByRole('button', { name: /应用到\s*\d+\s*项/ }).click();

    await expect.poll(() => captured !== null).toBeTruthy();
    const body = captured as unknown as BatchCellsBody;
    expect(body.field).toBe('start_boarding_time');
    expect(body.targets?.length).toBe(2);
    const expectedValues = (body.targets ?? []).map((t) => String(t.expected_value));
    expect(expectedValues).toEqual(expect.arrayContaining([existingA, existingB]));
  });

  test('single-cell context menu opens batch modal (N=1 path)', async ({ page }) => {
    await openMonitorAs(page, PARITY_ADMIN);
    const cell = remarksCell(page, 'flight-batch-01');
    await cell.scrollIntoViewIfNeeded();
    await cell.click({ button: 'right' });
    await expect(page.locator('#flightCellContextMenu')).toBeVisible();
    await page.locator('#flightCellContextMenu button', { hasText: /修改「航班备注」|修改此单元格/ }).click();
    const modal = page.locator('#flightBatchEditModal');
    await expect(modal).toBeVisible({ timeout: 5_000 });
    await expect(modal).toContainText(/应用到\s*1\s*项|批量修改 航班备注/);
  });
});
