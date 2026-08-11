import { describe, expect, it } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';

import clockJson from '../../parity/fixtures/common/clock.json';
import httpErrorsJson from '../../parity/fixtures/common/http-errors.json';
import {
  apiRouteMatches,
  assertNoUnknownApiRequests,
  findApiRoute,
  type ApiRouteFixture,
  type RecordedApiRequest,
} from '../../e2e/parity/api.fixture';
import {
  AUTH_USERS,
  createAuthMeRoute,
  createFixtureAccessToken,
} from '../../e2e/parity/auth.fixture';
import { buildSseStream, formatSseEvent, validateSseBlock } from '../../e2e/parity/sse.fixture';
import { normalizeCaptureDefinition } from '../../scripts/parity/capture-actions.mjs';

const rustUserResponseFields = [
  'created_at',
  'department',
  'display_name',
  'effective_operator_label',
  'effective_operator_name',
  'email',
  'id',
  'is_active',
  'is_admin',
  'is_verified',
  'job_level',
  'job_title',
  'last_login_at',
  'operator_context_id',
  'operator_context_type',
  'permission_version',
  'permissions',
  'roles',
  'username',
];

const productionPages = [
  'ai_config_center',
  'ai_monitor',
  'anomaly_monitor',
  'command_center',
  'dashboard',
  'dispatch_board',
  'dispatch_rule_center',
  'flight_imports',
  'flight_monitor',
  'flowable_modeler',
  'kpi_dashboard',
  'label_manager',
  'llm_eval_lab',
  'login',
  'nl_query',
  'operations_review_report',
  'resource_manager',
  'resource_utilization',
  'system_flags',
  'system_status',
  'user_manager',
];

describe('parity API fixtures', () => {
  it('matches exact method, pathname, query, body, and call budget', () => {
    const fixture: ApiRouteFixture = {
      id: 'resolve-anomaly',
      method: 'POST',
      pathname: '/api/v2/anomalies/anomaly-1/resolve',
      query: { notify: ['true'] },
      requestBody: { note: '已复核', create_todo: false },
      maxCalls: 1,
      response: { status: 200, body: { success: true, data: { anomaly_id: 'anomaly-1' } } },
    };
    const request: RecordedApiRequest = {
      method: 'POST',
      pathname: '/api/v2/anomalies/anomaly-1/resolve',
      query: { notify: ['true'] },
      body: { note: '已复核', create_todo: false },
    };

    expect(apiRouteMatches(fixture, request)).toBe(true);
    expect(findApiRoute([fixture], request)?.id).toBe('resolve-anomaly');
    expect(apiRouteMatches(fixture, request, 1)).toBe(false);
    expect(findApiRoute([fixture], { ...request, query: {} })).toBeUndefined();
    expect(findApiRoute([fixture], { ...request, body: { note: '错误内容' } })).toBeUndefined();
  });

  it('fails with a precise error for unknown API requests', () => {
    expect(() => assertNoUnknownApiRequests([{
      method: 'GET',
      pathname: '/api/v2/unregistered',
      query: {},
      body: null,
      reason: 'no fixture is registered for this pathname',
    }])).toThrow(/GET \/api\/v2\/unregistered/);
  });

  it('provides the required Rust-shaped HTTP status variants', () => {
    const variants = httpErrorsJson as Array<{ status: number; body: { success: boolean; error: { code: string } } }>;
    expect(variants.map((variant) => variant.status)).toEqual([401, 403, 409, 422, 500]);
    for (const variant of variants) {
      expect(variant.body.success).toBe(false);
      expect(variant.body.error.code).toBe(`HTTP_${variant.status}`);
    }
  });

  it.each(['success', 'empty', 'partial'])('provides a Rust-shaped dashboard workbench route for %s', (scenario) => {
    const fixturePath = path.resolve(
      process.cwd(),
      'parity',
      'fixtures',
      'pages',
      'dashboard',
      `${scenario}.json`,
    );
    const fixture = JSON.parse(readFileSync(fixturePath, 'utf8')) as {
      routes: Array<{
        method: string;
        pathname: string;
        response: { status: number; body?: unknown };
      }>;
    };
    const route = fixture.routes.find(({ method, pathname }) => (
      method === 'GET' && pathname === '/api/v2/dashboard/workbench'
    ));
    expect(route, `${scenario} dashboard fixture is missing the Vue workbench endpoint`).toBeDefined();
    expect(route?.response.status).toBe(scenario === 'partial' ? 500 : 200);

    if (scenario !== 'partial') {
      expect(route?.response.body).toMatchObject({
        success: true,
        data: {
          generated_at: expect.any(String),
          user_context: {
            user_id: expect.any(String),
            is_admin: expect.any(Boolean),
            permissions: expect.any(Array),
          },
          role_hint: expect.any(String),
          attention_items: expect.any(Array),
          risk_summary: {
            unresolved_anomalies: expect.any(Number),
            high_risk_flights: expect.any(Number),
            dispatch_conflicts: expect.any(Number),
            stale_data_indicators: expect.any(Array),
            high_risk_flight_refs: expect.any(Array),
            dispatch_conflict_refs: expect.any(Array),
          },
          recent_changes: expect.any(Array),
          quick_links: expect.any(Array),
          module_status: expect.any(Array),
          degraded_sources: expect.any(Array),
        },
      });
    }
  });
});

describe('parity auth and clock fixtures', () => {
  it.each(Object.entries(AUTH_USERS))('%s uses the Rust UserResponse field names', (_role, user) => {
    expect(Object.keys(user).sort()).toEqual(rustUserResponseFields);
  });

  it('provides admin, operator, and read-only permission profiles', () => {
    expect(AUTH_USERS.admin.is_admin).toBe(true);
    expect(AUTH_USERS.operator.permissions).toContain('dispatch:write');
    expect(AUTH_USERS.readonly.permissions).not.toContain('dispatch:write');
  });

  it('creates a deterministic legacy-compatible access token and auth route', () => {
    const clock = clockJson as { instant: string };
    const token = createFixtureAccessToken(AUTH_USERS.operator, clock.instant);
    const payload = JSON.parse(Buffer.from(token.split('.')[1], 'base64url').toString('utf8')) as {
      sub: string;
      exp: number;
      iat: number;
    };
    expect(payload.sub).toBe(AUTH_USERS.operator.id);
    expect(payload.exp - payload.iat).toBe(3600);
    expect(createAuthMeRoute(AUTH_USERS.operator)).toMatchObject({
      method: 'GET',
      pathname: '/api/v2/auth/me',
      query: {},
      response: { status: 200 },
    });
  });

  it('tracks an explicit fixture manifest directory for all 21 production pages', () => {
    const pagesRoot = path.resolve(process.cwd(), 'parity', 'fixtures', 'pages');
    const directories = readdirSync(pagesRoot, { withFileTypes: true })
      .filter((entry) => entry.isDirectory())
      .map((entry) => entry.name)
      .sort();
    expect(directories).toEqual(productionPages);

    for (const page of productionPages) {
      const manifest = JSON.parse(readFileSync(path.join(pagesRoot, page, 'manifest.json'), 'utf8')) as {
        page: string;
        status: string;
      };
      expect(manifest.page).toBe(page);
      expect(['seeded', 'awaiting-contract-capture']).toContain(manifest.status);
    }
  });

  it('declares critical regions, executable interactions, or explicit blockers for every protected page', () => {
    const pagesRoot = path.resolve(process.cwd(), 'parity', 'fixtures', 'pages');
    for (const page of productionPages.filter((pageId) => pageId !== 'login')) {
      const fixture = JSON.parse(
        readFileSync(path.join(pagesRoot, page, 'success.json'), 'utf8'),
      ) as { capture?: unknown };
      const capture = normalizeCaptureDefinition(fixture.capture, `${page}/success.capture`);

      expect(
        capture.regions.length + capture.interactions.length + capture.blockedInteractions.length,
        `${page} has no critical visual or interaction evidence`,
      ).toBeGreaterThan(0);
      for (const interaction of capture.interactions) {
        expect(interaction.regions.length + Number(interaction.captureFullPage)).toBeGreaterThan(0);
      }
    }
  });
});

describe('parity SSE fixtures', () => {
  it('formats and validates named SSE events', () => {
    const stream = buildSseStream([
      { id: '1', event: 'anomaly_created', data: { anomaly_id: 'anomaly-1' } },
      { id: '2', event: 'error_log', retry: 1000, data: { error_id: 'error-1' } },
    ]);
    expect(stream).toContain('event: anomaly_created');
    expect(stream).toContain('event: error_log');
    expect(stream.match(/\n\n/g)).toHaveLength(2);
  });

  it('rejects malformed or unnamed SSE blocks', () => {
    expect(() => validateSseBlock('event: anomaly_created\ndata: {}\n')).toThrow(/blank line/);
    expect(() => validateSseBlock('data: {}\n\n')).toThrow(/named event/);
    expect(() => validateSseBlock('event: anomaly_created\nunknown: value\ndata: {}\n\n')).toThrow(/Unsupported/);
    expect(() => formatSseEvent({ event: 'bad event', data: {} })).toThrow(/Invalid named SSE event/);
  });
});
