import { expect, test, type Page } from '@playwright/test';
import { installSessionRoutes, PARITY_ADMIN } from '../helpers/authRoutes';

const ONTOLOGY_OBJECTS = {
  success: true,
  data: [
    {
      name: 'Flight',
      description: '航班主数据对象',
      object_id_strategy: 'flight_id',
      fields: {
        flight_number: {
          name: 'flight_number',
          field_type: 'string',
          required: true,
          description: '航班号',
        },
      },
      relations: {},
      actions: { update_status: {} },
      is_active: true,
    },
  ],
};

const ONTOLOGY_ACTIONS = {
  success: true,
  data: [
    {
      object: 'Flight',
      action: 'update_status',
      is_active: true,
      definition: {
        name: 'update_status',
        description: '更新航班状态',
        category: 'operations',
        risk_level: 'MEDIUM',
        approval_policy: 'none',
        parameters: {
          status: {
            name: 'status',
            field_type: 'string',
            required: true,
            description: '目标状态',
          },
        },
      },
    },
  ],
};

const ENTITIES = {
  success: true,
  data: {
    entities: [{ id: 'default' }],
  },
};

const MODELS = {
  success: true,
  data: {
    models: [
      {
        id: 'model-gpt-4o-mini',
        name: 'gpt-4o-mini',
        provider: 'openai',
      },
    ],
  },
};

const TOOL_CATEGORIES = {
  success: true,
  data: {
    categories: [
      {
        name: 'operations',
        tools: ['flight_lookup'],
      },
    ],
  },
};

const TOOLS = {
  success: true,
  data: [
    {
      name: 'flight_lookup',
      category: 'operations',
      description: '查询航班状态',
      enabled: true,
    },
  ],
};

async function installAiConfigRoutes(page: Page): Promise<void> {
  // Register catch-all first so later, more-specific routes win (Playwright LIFO).
  await page.route('**/api/v2/ai/**', async (route) => {
    await route.fulfill({ status: 200, json: { success: true, data: {} } });
  });
  await page.route('**/api/v2/ai/ontology/objects**', (route) => route.fulfill({
    status: 200,
    json: ONTOLOGY_OBJECTS,
  }));
  await page.route('**/api/v2/ai/ontology/actions**', (route) => route.fulfill({
    status: 200,
    json: ONTOLOGY_ACTIONS,
  }));
  await page.route('**/api/v2/ai/entities**', (route) => {
    const url = route.request().url();
    // Detail/sub-resources under /entities/:id — empty success envelope is enough for shell.
    if (/\/entities\/[^/]+/.test(url)) {
      return route.fulfill({
        status: 200,
        json: { success: true, data: { id: 'default', config_version: 1 } },
      });
    }
    return route.fulfill({ status: 200, json: ENTITIES });
  });
  await page.route('**/api/v2/ai/models**', (route) => route.fulfill({
    status: 200,
    json: MODELS,
  }));
  await page.route('**/api/v2/ai/tools/categories**', (route) => route.fulfill({
    status: 200,
    json: TOOL_CATEGORIES,
  }));
  await page.route('**/api/v2/ai/tools**', (route) => {
    const url = route.request().url();
    if (url.includes('/categories')) return route.fallback();
    return route.fulfill({ status: 200, json: TOOLS });
  });
}

test.describe('ai_config_center parity', () => {
  test('ai_config_center-success: mounts Vue AI config shell and renders ontology objects from Rust envelopes', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await installAiConfigRoutes(page);

    await page.goto('/frontend/ai_config_center.html');

    await expect(page.locator('.admin-container, .admin-sidebar, .main-content').first()).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByText('AI 配置').first()).toBeVisible();
    await expect(page.getByText('对象定义').first()).toBeVisible();
    await expect(page.getByText(/AIP Ontology|Ontology 只读视图/).first()).toBeVisible();
    // Fixture object name surfaces in the default objects table.
    await expect(page.getByText('Flight').first()).toBeVisible({ timeout: 10_000 });
    await expect(page.getByText(/航班主数据对象|flight_id/).first()).toBeVisible();
  });
});
