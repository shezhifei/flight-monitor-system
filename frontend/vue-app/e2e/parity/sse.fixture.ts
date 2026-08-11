import type { Page, Route } from '@playwright/test';

import {
  normalizeQuery,
  recordApiRequest,
  type JsonValue,
  type QueryExpectation,
  type RecordedApiRequest,
} from './api.fixture';

export interface NamedSseEventFixture {
  event: string;
  data: JsonValue | string;
  id?: string;
  retry?: number;
}

export interface SseStreamFixture {
  id: string;
  pathname: string;
  query?: QueryExpectation;
  events: NamedSseEventFixture[];
}

const eventNamePattern = /^[A-Za-z][A-Za-z0-9_.-]*$/;

function assertSingleLine(value: string, label: string): void {
  if (value.includes('\r') || value.includes('\n')) {
    throw new Error(`${label} must not contain line breaks`);
  }
}

export function formatSseEvent(event: NamedSseEventFixture): string {
  if (!eventNamePattern.test(event.event)) {
    throw new Error(`Invalid named SSE event: ${event.event}`);
  }
  assertSingleLine(event.event, 'SSE event name');
  if (event.id !== undefined) assertSingleLine(event.id, 'SSE event id');
  if (event.retry !== undefined && (!Number.isInteger(event.retry) || event.retry < 0)) {
    throw new Error(`Invalid SSE retry value: ${event.retry}`);
  }

  const data = typeof event.data === 'string' ? event.data : JSON.stringify(event.data);
  const lines = [];
  if (event.id !== undefined) lines.push(`id: ${event.id}`);
  lines.push(`event: ${event.event}`);
  if (event.retry !== undefined) lines.push(`retry: ${event.retry}`);
  for (const dataLine of data.split(/\r?\n/)) lines.push(`data: ${dataLine}`);
  return `${lines.join('\n')}\n\n`;
}

export function validateSseBlock(block: string): void {
  const normalized = block.replace(/\r\n/g, '\n');
  if (!normalized.endsWith('\n\n')) throw new Error('SSE block must end with a blank line');

  const lines = normalized.slice(0, -2).split('\n');
  let eventName: string | undefined;
  let dataLines = 0;
  for (const line of lines) {
    if (line.startsWith(':')) continue;
    const separator = line.indexOf(':');
    if (separator < 0) throw new Error(`Malformed SSE field: ${line}`);
    const field = line.slice(0, separator);
    const value = line.slice(separator + 1).replace(/^ /, '');
    if (!['event', 'data', 'id', 'retry'].includes(field)) {
      throw new Error(`Unsupported SSE field: ${field}`);
    }
    if (field === 'event') eventName = value;
    if (field === 'data') dataLines += 1;
    if (field === 'retry' && !/^\d+$/.test(value)) throw new Error(`Invalid SSE retry field: ${value}`);
  }
  if (!eventName || !eventNamePattern.test(eventName)) throw new Error('SSE block requires a valid named event');
  if (dataLines === 0) throw new Error('SSE block requires at least one data field');
}

export function buildSseStream(events: NamedSseEventFixture[]): string {
  if (events.length === 0) throw new Error('SSE fixture stream must contain at least one named event');
  return events.map((event) => {
    const block = formatSseEvent(event);
    validateSseBlock(block);
    return block;
  }).join('');
}

function sseRouteMatches(fixture: SseStreamFixture, request: RecordedApiRequest): boolean {
  if (request.method !== 'GET' || request.pathname !== fixture.pathname) return false;
  const expected = new URL('http://fixture.invalid');
  for (const [key, values] of Object.entries(fixture.query ?? {})) {
    for (const value of values) expected.searchParams.append(key, value);
  }
  return JSON.stringify(normalizeQuery(expected)) === JSON.stringify(request.query);
}

async function fulfillSseRoute(route: Route, fixture: SseStreamFixture): Promise<void> {
  await route.fulfill({
    status: 200,
    headers: {
      'Cache-Control': 'no-cache, no-store',
      Connection: 'keep-alive',
      'Content-Type': 'text/event-stream; charset=utf-8',
    },
    body: buildSseStream(fixture.events),
  });
}

export async function installSseFixtures(
  page: Page,
  fixtures: SseStreamFixture[],
  requestLog: RecordedApiRequest[],
): Promise<void> {
  await page.route('**/api/v2/**', async (route) => {
    const recorded = recordApiRequest(route.request());
    const fixture = fixtures.find((candidate) => sseRouteMatches(candidate, recorded));
    if (!fixture) {
      await route.fallback();
      return;
    }
    requestLog.push(recorded);
    await fulfillSseRoute(route, fixture);
  });
}
