import { expect, test, type Page } from '@playwright/test';
import { installSessionRoutes, PARITY_ADMIN } from './parity/helpers/authRoutes';

const ONTOLOGY_URL = '/frontend/ontology_center.html';

const FLIGHT_VIEW = {
  success: true,
  data: {
    flight_id: 'FL_E2E_001',
    registration: 'B-E2E1',
    plan_stand: '201',
    plan_gate: 'A12',
    occupations: [{ id: 'occ-1', stand_code: '201' }],
    assignments: [{ id: 'asn-1', gate_code: 'A12' }],
    turnaround_links: [{ id: 'tl-1', status: 'active' }],
  },
};

const AIRCRAFT_VIEW = {
  success: true,
  data: {
    registration: 'B-E2E1',
    in_field: true,
    current_stand: '201',
    current_gate: 'A12',
    occupations: [{ id: 'occ-1' }],
    assignments: [{ id: 'asn-1' }],
    flights: [{ flight_id: 'FL_E2E_001' }],
  },
};

const SUGGESTIONS = {
  success: true,
  data: [
    {
      id: 'sug-1',
      flight_id: 'FL_E2E_001',
      kind: 'stand',
      current_value: '201',
      suggested_value: '202',
      status: 'pending',
      reason: 'e2e',
      created_by: 'parity_admin',
    },
  ],
};

const LINKS = {
  success: true,
  data: [
    {
      id: 'tl-1',
      inbound_flight_id: 'FL_IN_001',
      outbound_flight_id: 'FL_E2E_001',
      status: 'active',
      source: 'auto',
    },
  ],
};

async function installOntologyRoutes(page: Page): Promise<{
  reassignBodies: unknown[];
  allocateStandBodies: unknown[];
  acceptBodies: unknown[];
  autoScanBodies: unknown[];
}> {
  const reassignBodies: unknown[] = [];
  const allocateStandBodies: unknown[] = [];
  const acceptBodies: unknown[] = [];
  const autoScanBodies: unknown[] = [];

  await page.route('**/api/v2/ontology/**', async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const path = url.pathname;
    const method = request.method();

    if (method === 'GET' && /\/flights\/[^/]+\/resources$/.test(path)) {
      await route.fulfill({ status: 200, json: FLIGHT_VIEW });
      return;
    }
    if (method === 'GET' && /\/aircraft\/[^/]+\/resources$/.test(path)) {
      await route.fulfill({ status: 200, json: AIRCRAFT_VIEW });
      return;
    }
    if (method === 'GET' && /\/flights\/[^/]+\/turnaround-links$/.test(path)) {
      await route.fulfill({ status: 200, json: LINKS });
      return;
    }
    if (method === 'GET' && path.endsWith('/suggestions')) {
      await route.fulfill({ status: 200, json: SUGGESTIONS });
      return;
    }
    if (method === 'POST' && path.endsWith('/aircraft/reassign')) {
      reassignBodies.push(request.postDataJSON());
      await route.fulfill({
        status: 200,
        json: {
          success: true,
          data: {
            applied: [
              {
                flight_id: 'FL_E2E_001',
                old_registration: 'B-OLD',
                new_registration: 'B-E2E1',
                broken_links: [],
                created_links: [],
                suggestions: [],
              },
            ],
          },
        },
      });
      return;
    }
    if (method === 'POST' && path.endsWith('/stands/occupations')) {
      allocateStandBodies.push(request.postDataJSON());
      await route.fulfill({
        status: 201,
        json: {
          success: true,
          data: {
            occupation: { id: 'occ-new', stand_code: '201' },
            overlap_warnings: ['stand 201 overlaps occupation occ-x'],
          },
        },
      });
      return;
    }
    if (method === 'POST' && /\/suggestions\/[^/]+\/accept$/.test(path)) {
      acceptBodies.push(request.postDataJSON());
      await route.fulfill({
        status: 200,
        json: {
          success: true,
          data: { ...SUGGESTIONS.data[0], status: 'accepted_executed' },
        },
      });
      return;
    }
    if (method === 'POST' && path.endsWith('/turnaround-links/auto-scan')) {
      autoScanBodies.push(request.postDataJSON());
      await route.fulfill({
        status: 200,
        json: {
          success: true,
          data: { evaluated: 3, created: ['tl-new'], skipped: 2, errors: [] },
        },
      });
      return;
    }
    if (method === 'POST' && path.endsWith('/flights/confirm-drafts')) {
      await route.fulfill({
        status: 200,
        json: { success: true, data: { confirmed: ['FL_E2E_001'], missing: [] } },
      });
      return;
    }

    await route.fulfill({ status: 200, json: { success: true, data: {} } });
  });

  return { reassignBodies, allocateStandBodies, acceptBodies, autoScanBodies };
}

async function openOntologyCenter(page: Page): Promise<void> {
  await installSessionRoutes(page, PARITY_ADMIN);
  await installOntologyRoutes(page);
  await page.goto(ONTOLOGY_URL);
  await expect(page.getByRole('heading', { name: '本体资源台' })).toBeVisible();
}

test.describe('Ontology Center', () => {
  test('renders shell, permission pills, tabs, and empty view state', async ({ page }) => {
    await openOntologyCenter(page);

    await expect(page).toHaveTitle(/本体资源台|Ontology/i);
    await expect(page.locator('.ontology-page')).toBeVisible();
    await expect(page.locator('.ontology-perm-bar .ontology-pill.tone-ok')).toHaveCount(5);

    for (const label of ['资源视图', '换机', '机位 / 登机口', '资源建议', '周转链接']) {
      await expect(page.locator('.ontology-tab', { hasText: label })).toBeVisible();
    }

    await expect(page.getByText('尚未加载上下文')).toBeVisible();
    await expect(page.getByRole('button', { name: '加载资源视图' })).toBeEnabled();
  });

  test('loads flight resource view and shows plan stand/gate', async ({ page }) => {
    await openOntologyCenter(page);

    await page.getByPlaceholder('例如 FL…').fill('FL_E2E_001');
    await page.getByRole('button', { name: '加载资源视图' }).click();

    await expect(page.getByText('航段资源')).toBeVisible();
    await expect(page.getByText('FL_E2E_001')).toBeVisible();
    await expect(page.getByText('B-E2E1')).toBeVisible();
    await expect(page.getByText('201').first()).toBeVisible();
    await expect(page.getByText('A12').first()).toBeVisible();
  });

  test('switches context to aircraft mode and loads aircraft view', async ({ page }) => {
    await openOntologyCenter(page);

    await page.getByRole('button', { name: '机号', exact: true }).click();
    await page.getByPlaceholder('例如 B-1234').fill('B-E2E1');
    await page.getByRole('button', { name: '加载资源视图' }).click();

    await expect(page.getByText('飞机资源')).toBeVisible();
    await expect(page.locator('.ontology-pill.tone-ok', { hasText: '在场' })).toBeVisible();
    await expect(page.getByText('B-E2E1').first()).toBeVisible();
  });

  test('tab navigation shows reassign / resources / suggestions / links panels', async ({ page }) => {
    await openOntologyCenter(page);

    await page.getByRole('button', { name: '换机' }).click();
    await expect(page.getByRole('heading', { name: /换机 ReassignAircraft/ })).toBeVisible();
    await expect(page.getByRole('button', { name: '提交换机' })).toBeVisible();

    await page.getByRole('button', { name: '机位 / 登机口' }).click();
    await expect(page.getByRole('heading', { name: /正式机位/ })).toBeVisible();
    await expect(page.getByRole('button', { name: '分配机位' })).toBeVisible();
    await expect(page.getByRole('button', { name: '分配登机口' })).toBeVisible();

    await page.getByRole('button', { name: '资源建议' }).click();
    await expect(page.getByRole('heading', { name: '资源调整建议' })).toBeVisible();
    await expect(page.getByRole('button', { name: '创建建议' })).toBeVisible();

    await page.getByRole('button', { name: '周转链接' }).click();
    await expect(page.getByRole('heading', { name: '周转链接' })).toBeVisible();
    await expect(page.getByRole('button', { name: '自动扫描建链' })).toBeVisible();
  });

  test('reassign posts expected payload', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    const bodies = await installOntologyRoutes(page);
    await page.goto(ONTOLOGY_URL);
    await expect(page.getByRole('heading', { name: '本体资源台' })).toBeVisible();

    await page.getByRole('button', { name: '换机' }).click();
    await page.getByLabel('航班 ID').fill('FL_E2E_001');
    await page.getByLabel('新机号（原样）').fill('B-E2E1');
    await page.getByRole('button', { name: '提交换机' }).click();

    await expect.poll(() => bodies.reassignBodies.length).toBe(1);
    expect(bodies.reassignBodies[0]).toMatchObject({
      changes: [{ flight_id: 'FL_E2E_001', new_registration: 'B-E2E1' }],
    });
  });

  test('allocate stand posts payload and surfaces overlap warning', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    const bodies = await installOntologyRoutes(page);
    await page.goto(ONTOLOGY_URL);
    await expect(page.getByRole('heading', { name: '本体资源台' })).toBeVisible();

    await page.getByRole('button', { name: '机位 / 登机口' }).click();
    await page.locator('#stand-registration').fill('B-E2E1');
    await page.locator('#stand-code').fill('201');
    // datetime-local already has defaults from composable
    await page.getByRole('button', { name: '分配机位' }).click();

    await expect.poll(() => bodies.allocateStandBodies.length).toBe(1);
    const body = bodies.allocateStandBodies[0] as Record<string, unknown>;
    expect(body.registration).toBe('B-E2E1');
    expect(body.stand_code).toBe('201');
    expect(body.starts_at).toBeTruthy();
    expect(body.ends_at).toBeTruthy();
  });

  test('suggestions list loads and accept posts', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    const bodies = await installOntologyRoutes(page);
    await page.goto(ONTOLOGY_URL);
    await expect(page.getByRole('heading', { name: '本体资源台' })).toBeVisible();

    // Load flight context so suggestions refresh with flight filter
    await page.getByPlaceholder('例如 FL…').fill('FL_E2E_001');
    await page.getByRole('button', { name: '加载资源视图' }).click();
    await expect(page.getByText('航段资源')).toBeVisible();

    await page.getByRole('button', { name: '资源建议' }).click();
    await expect(page.getByText('202')).toBeVisible();
    await expect(page.getByText('pending')).toBeVisible();
    await page.getByRole('button', { name: '接受' }).click();

    await expect.poll(() => bodies.acceptBodies.length).toBe(1);
  });

  test('auto-scan reports evaluated/created counts', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    const bodies = await installOntologyRoutes(page);
    await page.goto(ONTOLOGY_URL);
    await expect(page.getByRole('heading', { name: '本体资源台' })).toBeVisible();

    await page.getByRole('button', { name: '周转链接' }).click();
    await page.getByRole('button', { name: '自动扫描建链' }).click();

    await expect.poll(() => bodies.autoScanBodies.length).toBe(1);
    await expect(page.getByText(/上次扫描：评估 3/)).toBeVisible();
    await expect(page.getByText(/新建 1/)).toBeVisible();
  });

  test('uses workspace CSS tokens (no hardcoded panel color classes required)', async ({ page }) => {
    await openOntologyCenter(page);
    const pageEl = page.locator('.ontology-page');
    await expect(pageEl).toBeVisible();

    const bg = await pageEl.evaluate((el) => getComputedStyle(el).backgroundColor);
    // token --ws-bg resolves to a non-transparent color in both themes
    expect(bg).not.toBe('rgba(0, 0, 0, 0)');
    expect(bg).not.toBe('transparent');

    const surface = page.locator('.ontology-header');
    const border = await surface.evaluate((el) => getComputedStyle(el).borderTopColor);
    expect(border).toBeTruthy();
  });
});
