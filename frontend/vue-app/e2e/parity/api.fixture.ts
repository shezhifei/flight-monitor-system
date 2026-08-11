import type { Page, Request, Route } from '@playwright/test';
import { isDeepStrictEqual } from 'node:util';

export type JsonPrimitive = boolean | number | string | null;
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };

export type QueryExpectation = Record<string, string[]>;

export interface ApiResponseFixture {
  status: number;
  body?: JsonValue | string;
  headers?: Record<string, string>;
}

export interface ApiRouteFixture {
  id: string;
  method: string;
  pathname: string;
  query?: QueryExpectation;
  requestBody?: JsonValue | string | null;
  maxCalls?: number;
  response: ApiResponseFixture;
}

export interface RecordedApiRequest {
  method: string;
  pathname: string;
  query: QueryExpectation;
  body: JsonValue | string | null;
}

export interface UnknownApiRequest extends RecordedApiRequest {
  reason: string;
}

export function normalizeQuery(url: URL): QueryExpectation {
  const normalized: QueryExpectation = {};
  const keys = [...new Set(url.searchParams.keys())].sort();
  for (const key of keys) {
    normalized[key] = url.searchParams.getAll(key).sort();
  }
  return normalized;
}

export function parseRequestBody(rawBody: string | null): JsonValue | string | null {
  if (rawBody === null || rawBody === '') return null;
  try {
    return JSON.parse(rawBody) as JsonValue;
  } catch {
    return rawBody;
  }
}

export function recordApiRequest(request: Request): RecordedApiRequest {
  const url = new URL(request.url());
  return {
    method: request.method().toUpperCase(),
    pathname: url.pathname,
    query: normalizeQuery(url),
    body: parseRequestBody(request.postData()),
  };
}

function normalizeExpectedQuery(query: QueryExpectation | undefined): QueryExpectation {
  if (!query) return {};
  return Object.fromEntries(
    Object.entries(query)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, values]) => [key, [...values].sort()]),
  );
}

export function apiRouteMatches(
  fixture: ApiRouteFixture,
  request: RecordedApiRequest,
  callCount = 0,
): boolean {
  const maxCalls = fixture.maxCalls ?? Number.POSITIVE_INFINITY;
  if (callCount >= maxCalls) return false;
  if (fixture.method.toUpperCase() !== request.method) return false;
  if (fixture.pathname !== request.pathname) return false;
  if (!isDeepStrictEqual(normalizeExpectedQuery(fixture.query), request.query)) return false;
  if (Object.hasOwn(fixture, 'requestBody') && !isDeepStrictEqual(fixture.requestBody, request.body)) return false;
  return true;
}

export function findApiRoute(
  fixtures: ApiRouteFixture[],
  request: RecordedApiRequest,
  callCounts: ReadonlyMap<string, number> = new Map(),
): ApiRouteFixture | undefined {
  return fixtures.find((fixture) => apiRouteMatches(fixture, request, callCounts.get(fixture.id) ?? 0));
}

function unknownReason(fixtures: ApiRouteFixture[], request: RecordedApiRequest): string {
  const samePath = fixtures.filter((fixture) => fixture.pathname === request.pathname);
  if (samePath.length === 0) return 'no fixture is registered for this pathname';
  if (!samePath.some((fixture) => fixture.method.toUpperCase() === request.method)) {
    return 'request method does not match the registered fixture';
  }
  return 'query, request body, or maximum call count does not match the registered fixture';
}

async function fulfillApiRoute(route: Route, response: ApiResponseFixture): Promise<void> {
  const headers = {
    'Cache-Control': 'no-store',
    ...response.headers,
  };
  if (typeof response.body === 'string') {
    await route.fulfill({ status: response.status, headers, body: response.body });
    return;
  }
  if (response.body === undefined) {
    await route.fulfill({ status: response.status, headers, body: '' });
    return;
  }
  await route.fulfill({ status: response.status, headers, json: response.body });
}

export async function installApiFixtures(
  page: Page,
  fixtures: ApiRouteFixture[],
  requestLog: RecordedApiRequest[],
  unknownRequests: UnknownApiRequest[],
): Promise<void> {
  const callCounts = new Map<string, number>();

  await page.route('**/api/v2/**', async (route) => {
    const recorded = recordApiRequest(route.request());
    requestLog.push(recorded);
    const fixture = findApiRoute(fixtures, recorded, callCounts);

    if (!fixture) {
      unknownRequests.push({ ...recorded, reason: unknownReason(fixtures, recorded) });
      await route.fulfill({
        status: 599,
        json: {
          success: false,
          error: {
            code: 'PARITY_UNKNOWN_API_REQUEST',
            message: `Unknown fixture request: ${recorded.method} ${recorded.pathname}`,
          },
        },
      });
      return;
    }

    callCounts.set(fixture.id, (callCounts.get(fixture.id) ?? 0) + 1);
    await fulfillApiRoute(route, fixture.response);
  });
}

export function assertNoUnknownApiRequests(unknownRequests: UnknownApiRequest[]): void {
  if (unknownRequests.length === 0) return;
  const details = unknownRequests
    .map((request) => `${request.method} ${request.pathname}: ${request.reason}`)
    .join('\n');
  throw new Error(`Parity fixture received unknown API requests:\n${details}`);
}
