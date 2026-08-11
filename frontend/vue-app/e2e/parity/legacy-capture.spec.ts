import { createRequire } from 'node:module';

import type { ApiRouteFixture } from './api.fixture';
import { test, expect } from './fixtures';
import type { SseStreamFixture } from './sse.fixture';

const require = createRequire(import.meta.url);
const dashboardSuccessJson = require('../../parity/fixtures/pages/dashboard/success.json') as unknown;
const dashboardScenario = dashboardSuccessJson as unknown as {
  routes: ApiRouteFixture[];
  sse_streams: SseStreamFixture[];
};
const dashboardApiRoute = dashboardScenario.routes.find(
  (route) => route.id === 'dashboard-burden-metrics-success',
);
const dashboardSseStream = dashboardScenario.sse_streams.find(
  (stream) => stream.id === 'dashboard-ai-events-success',
);
if (!dashboardApiRoute || !dashboardSseStream) {
  throw new Error('Dashboard success fixture is missing its canonical API or SSE route.');
}

function fixtureUrl(pathname: string, query: Record<string, string[]> = {}): string {
  const url = new URL(pathname, 'http://fixture.invalid');
  for (const [key, values] of Object.entries(query)) {
    for (const value of values) url.searchParams.append(key, value);
  }
  return `${url.pathname}${url.search}`;
}

test.use({
  baseURL: 'http://127.0.0.1:3100',
  installAuthStorage: false,
  apiFixtureSet: { routes: dashboardScenario.routes },
  sseFixtureSet: { streams: dashboardScenario.sse_streams },
});

test.describe('legacy baseline server', () => {
  test('serves dashboard HTML and every directly referenced archive asset', async ({ request }) => {
    const dashboardResponse = await request.get('/frontend/html/dashboard.html');
    expect(dashboardResponse.status()).toBe(200);
    const html = await dashboardResponse.text();
    const references = [...html.matchAll(
      /(?:src|href)=["'](\/frontend\/(?:html|js|css|static|vendor|icons|images|fonts)\/[^"']+|\/favicon(?:-full\.jpg|\.ico))["']/g,
    )].map((match) => match[1]);
    const uniqueReferences = [...new Set(references)];
    expect(uniqueReferences.length).toBeGreaterThan(0);

    for (const reference of uniqueReferences) {
      const response = await request.get(reference);
      expect(response.status(), reference).toBe(200);
    }
  });

  test('uses deterministic API, auth, time, random, and named SSE fixtures', async ({ page, requestLog, clock }) => {
    await page.goto('/frontend/html/login.html');

    const browserDeterminism = await page.evaluate(() => ({
      now: Date.now(),
      performanceNow: performance.now(),
      firstRandom: Math.random(),
      firstUuid: crypto.randomUUID(),
      timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
      deterministicStyle: Boolean(document.querySelector('#fms-parity-determinism')),
    }));
    expect(browserDeterminism.now).toBe(Date.parse(clock.instant));
    expect(browserDeterminism.performanceNow).toBe(1000);
    expect(browserDeterminism.firstRandom).toBeCloseTo(0.32495401822961867, 12);
    expect(browserDeterminism.firstUuid).toBe(clock.uuid_sequence[0]);
    expect(browserDeterminism.timezone).toBe(clock.timezone);
    expect(browserDeterminism.deterministicStyle).toBe(true);

    const responses = await page.evaluate(async ({ apiUrl, sseUrl }) => {
      const [authResponse, dashboardResponse, sseResponse] = await Promise.all([
        fetch('/api/v2/auth/me').then((response) => response.json()),
        fetch(apiUrl).then((response) => response.json()),
        fetch(sseUrl).then((response) => response.text()),
      ]);
      return { authResponse, dashboardResponse, sseResponse };
    }, {
      apiUrl: fixtureUrl(dashboardApiRoute.pathname, dashboardApiRoute.query),
      sseUrl: fixtureUrl(dashboardSseStream.pathname, dashboardSseStream.query),
    });

    expect(responses.authResponse).toMatchObject({ username: 'parity_admin', permission_version: 7 });
    expect(responses.dashboardResponse.data).toMatchObject({
      generated_at: '2026-07-14T02:30:00Z',
      blocked_completion_count: 1,
      open_soft_followups: 1,
    });
    expect(responses.sseResponse).toContain('event: ai_execution');
    expect(requestLog).toEqual(expect.arrayContaining([
      expect.objectContaining({ method: 'GET', pathname: '/api/v2/auth/me', query: {} }),
      expect.objectContaining({
        method: dashboardApiRoute.method,
        pathname: dashboardApiRoute.pathname,
        query: dashboardApiRoute.query,
      }),
      expect.objectContaining({
        method: 'GET',
        pathname: dashboardSseStream.pathname,
        query: dashboardSseStream.query,
      }),
    ]));
  });
});
