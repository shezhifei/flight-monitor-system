import { chromium } from 'playwright';
import { createHash } from 'node:crypto';
import {
  mkdir,
  readFile,
  readdir,
  rename,
  rm,
  stat,
  writeFile,
} from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { isDeepStrictEqual } from 'node:util';

import {
  CaptureActionValidationError,
  normalizeCaptureDefinition,
  runCaptureActions,
} from './capture-actions.mjs';
import {
  LEGACY_HTML_FILES,
  LegacyRootValidationError,
  validateLegacyRoot,
} from './legacy-root.mjs';
import { extractLegacySourceContract } from './legacy-source-graph.mjs';
import { startLegacyServer } from './serve-legacy.mjs';

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const vueAppRoot = path.resolve(scriptDirectory, '..', '..');
const fixtureRoot = path.join(vueAppRoot, 'parity', 'fixtures');
const snapshotRoot = path.join(vueAppRoot, 'e2e', 'parity', 'snapshots', 'legacy');
const DEFAULT_PORT = 3100;

const VIEWPORTS = Object.freeze([
  { id: 'desktop-wide', width: 1920, height: 1080 },
  { id: 'desktop', width: 1440, height: 900 },
  { id: 'laptop', width: 1366, height: 768 },
  { id: 'tablet', width: 1024, height: 768 },
  { id: 'mobile', width: 390, height: 844 },
]);

const COMMON_HEADERS = Object.freeze({
  'Cache-Control': 'no-store',
  'X-FMS-Parity-Fixture': 'deterministic',
});

class CaptureValidationError extends Error {
  constructor(message, details = []) {
    super(details.length > 0 ? `${message}\n${details.map((detail) => `  - ${detail}`).join('\n')}` : message);
    this.name = 'CaptureValidationError';
    this.details = details;
  }
}

function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
}

async function readJson(filename) {
  return JSON.parse(await readFile(filename, 'utf8'));
}

function normalizePageId(filename) {
  return filename.replace(/\.html$/i, '');
}

const ALL_PAGE_IDS = Object.freeze(LEGACY_HTML_FILES.map(normalizePageId));

function parseArguments(argv) {
  const options = {
    mode: 'check',
    pages: [...ALL_PAGE_IDS],
  };

  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--refresh-baseline') {
      options.mode = 'refresh';
      continue;
    }
    if (argument === '--check') {
      options.mode = 'check';
      continue;
    }
    if (argument === '--page') {
      const value = argv[index + 1];
      if (!value) throw new CaptureValidationError('--page requires a comma-separated page list.');
      options.pages = value.split(',').map((page) => page.trim()).filter(Boolean);
      index += 1;
      continue;
    }
    if (argument === '--help' || argument === '-h') {
      options.help = true;
      continue;
    }
    throw new CaptureValidationError(`Unknown argument: ${argument}`);
  }

  const invalidPages = options.pages.filter((page) => !ALL_PAGE_IDS.includes(page));
  if (invalidPages.length > 0) {
    throw new CaptureValidationError('Unknown legacy page IDs.', invalidPages);
  }
  options.pages = [...new Set(options.pages)];
  return options;
}

function printHelp() {
  console.log(`Usage: node scripts/parity/capture-legacy-snapshots.mjs [options]

Options:
  --check                   Validate committed screenshot metadata and file hashes (default).
  --refresh-baseline        Render and replace approved legacy screenshot baselines.
  --page <id[,id...]>       Limit the operation to explicit legacy page IDs.
  --help                    Show this help.

Baseline files are never written without --refresh-baseline.`);
}

function encodeJwtPart(value) {
  return Buffer.from(JSON.stringify(value), 'utf8').toString('base64url');
}

function createFixtureAccessToken(user, instant) {
  const issuedAt = Math.floor(Date.parse(instant) / 1000);
  const payload = {
    sub: user.id,
    username: user.username,
    email: user.email,
    is_admin: user.is_admin,
    permissions: user.permissions,
    department: user.department,
    pv: user.permission_version,
    iat: issuedAt,
    exp: issuedAt + 3600,
    iss: 'fms-parity-fixture',
    aud: 'fms-web',
    type: 'access',
  };
  return `${encodeJwtPart({ alg: 'none', typ: 'JWT' })}.${encodeJwtPart(payload)}.fixture`;
}

function normalizeQuery(url) {
  const normalized = {};
  const keys = [...new Set(url.searchParams.keys())].sort();
  for (const key of keys) normalized[key] = url.searchParams.getAll(key).sort();
  return normalized;
}

function normalizeExpectedQuery(query = {}) {
  return Object.fromEntries(
    Object.entries(query)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, values]) => [key, [...values].sort()]),
  );
}

function parseRequestBody(rawBody) {
  if (rawBody === null || rawBody === '') return null;
  try {
    return JSON.parse(rawBody);
  } catch {
    return rawBody;
  }
}

function recordRequest(request) {
  const url = new URL(request.url());
  return {
    method: request.method().toUpperCase(),
    pathname: url.pathname,
    query: normalizeQuery(url),
    body: parseRequestBody(request.postData()),
  };
}

function fixtureMatches(fixture, request, callCount) {
  if (callCount >= (fixture.maxCalls ?? Number.POSITIVE_INFINITY)) return false;
  if (fixture.method.toUpperCase() !== request.method) return false;
  if (fixture.pathname !== request.pathname) return false;
  if (!isDeepStrictEqual(normalizeExpectedQuery(fixture.query), request.query)) return false;
  if (Object.hasOwn(fixture, 'requestBody') && !isDeepStrictEqual(fixture.requestBody, request.body)) return false;
  return true;
}

function formatSseStream(stream) {
  return stream.events.map((event, index) => {
    if (!/^[A-Za-z][A-Za-z0-9_.-]*$/.test(event.event)) {
      throw new CaptureValidationError(`Invalid named SSE event in fixture ${stream.id}: ${event.event}`);
    }
    const lines = [];
    if (event.id !== undefined) lines.push(`id: ${event.id}`);
    lines.push(`event: ${event.event}`);
    if (event.retry !== undefined) lines.push(`retry: ${event.retry}`);
    else if (index === 0) lines.push('retry: 60000');
    const data = typeof event.data === 'string' ? event.data : JSON.stringify(event.data);
    for (const line of data.split(/\r?\n/)) lines.push(`data: ${line}`);
    return `${lines.join('\n')}\n\n`;
  }).join('');
}

function sseFixtureMatches(fixture, request) {
  return request.method === 'GET'
    && fixture.pathname === request.pathname
    && isDeepStrictEqual(normalizeExpectedQuery(fixture.query), request.query);
}

async function createCapturePlan(pageId) {
  const manifestPath = path.join(fixtureRoot, 'pages', pageId, 'manifest.json');
  const manifest = await readJson(manifestPath);

  if (pageId === 'login') {
    return {
      pageId,
      htmlFile: 'login.html',
      manifest,
      scenarios: [
        {
          id: 'default',
          fixtureId: 'static-public-no-api',
          theme: 'light',
          authRole: 'public',
          routes: [],
          sseStreams: [],
          expectedPanels: ['.login-card', '#loginForm', '.demo-credentials'],
          regions: [{ id: 'login-card', selector: '.login-card' }],
        },
        {
          id: 'required-username-error',
          fixtureId: 'client-validation-no-api',
          theme: 'light',
          authRole: 'public',
          routes: [],
          sseStreams: [],
          expectedPanels: ['.login-card', '#loginForm', '.demo-credentials'],
          setup: async (page) => {
            await page.locator('#loginBtn').click();
            await page.locator('#errorMessage.show').waitFor({ state: 'visible' });
          },
          regions: [{ id: 'login-card', selector: '.login-card' }],
        },
        {
          id: 'password-visible',
          fixtureId: 'client-interaction-no-api',
          theme: 'light',
          authRole: 'public',
          routes: [],
          sseStreams: [],
          expectedPanels: ['.login-card', '#loginForm', '.demo-credentials'],
          setup: async (page) => {
            await page.locator('#username').fill('parity_admin');
            await page.locator('#password').fill('fixture-password');
            await page.locator('#passwordToggleBtn').click();
          },
          regions: [{ id: 'login-card', selector: '.login-card' }],
          captureFullPage: false,
        },
      ],
      missingRequiredScenarios: (manifest.scenarios ?? []).filter((scenario) => ![
        'default',
        'required-username-error',
        'password-visible',
      ].includes(scenario)),
      baselineObservations: [
        'The legacy login card is an internal scroll container at shorter desktop heights.',
        'Autofocus and required-field focus can scroll that container, so the title is partially clipped in some desktop/laptop/tablet error-state captures; this is preserved as legacy behavior.',
      ],
    };
  }

  if (manifest.status !== 'seeded' || !Array.isArray(manifest.scenarios) || manifest.scenarios.length === 0) {
    throw new CaptureValidationError(
      `${pageId}: deterministic screenshot capture is blocked by fixture coverage.`,
      [
        `fixture manifest status is ${JSON.stringify(manifest.status)}`,
        `declared executable scenarios: ${JSON.stringify(manifest.scenarios)}`,
        'declare at least one explicit scenario backed by strict API/SSE fixtures',
      ],
    );
  }

  const scenarioPlans = [];
  const fixtureClock = await readJson(path.join(fixtureRoot, 'common', 'clock.json'));
  for (const scenarioName of manifest.scenarios) {
    const definition = await readJson(path.join(fixtureRoot, 'pages', pageId, `${scenarioName}.json`));
    const authRole = definition.auth_role ?? 'admin';
    const authFilename = {
      admin: 'auth-admin.json',
      operator: 'auth-operator.json',
      readonly: 'auth-readonly.json',
    }[authRole];
    if (!authFilename) {
      throw new CaptureValidationError(`${pageId}/${scenarioName}: unsupported auth_role ${JSON.stringify(authRole)}.`);
    }
    const authUser = await readJson(path.join(fixtureRoot, 'common', authFilename));
    const explicitRoutes = Array.isArray(definition.routes) ? definition.routes : [];
    const commonRoutes = [
      {
        id: `common-auth-me-${authRole}`,
        method: 'GET',
        pathname: '/api/v2/auth/me',
        query: {},
        response: { status: 200, body: authUser },
      },
      {
        id: 'common-auth-heartbeat',
        method: 'POST',
        pathname: '/api/v2/auth/heartbeat',
        query: {},
        response: { status: 200, body: { success: true } },
      },
      {
        id: 'common-auth-sse-token',
        method: 'POST',
        pathname: '/api/v2/auth/sse-token',
        query: {},
        response: {
          status: 200,
          body: { token: 'parity-sse-token', expires_at: fixtureClock.instant },
        },
      },
    ].filter((common) => !explicitRoutes.some((route) => (
      route.method === common.method && route.pathname === common.pathname
    )));
    let capture;
    try {
      capture = normalizeCaptureDefinition(
        definition.capture,
        `${pageId}/${scenarioName}.capture`,
      );
    } catch (error) {
      if (error instanceof CaptureActionValidationError) {
        throw new CaptureValidationError(error.message);
      }
      throw error;
    }
    scenarioPlans.push({
      id: definition.scenario ?? `${pageId}-${scenarioName}`,
      fixtureId: definition.scenario ?? `${pageId}-${scenarioName}`,
      theme: capture.theme,
      authRole,
      authUser,
      routes: [...explicitRoutes, ...commonRoutes],
      sseStreams: Array.isArray(definition.sse_streams) ? definition.sse_streams : [],
      expectsErrorResponse: explicitRoutes.some((route) => Number(route.response?.status ?? 200) >= 400),
      approvedLegacyErrors: Array.isArray(definition.expected_legacy_errors)
        ? definition.expected_legacy_errors
        : [],
      approvedExceptionIds: Array.isArray(definition.approved_exception_ids)
        ? definition.approved_exception_ids
        : [],
      expectedPanels: capture.expectedPanels.length > 0
        ? capture.expectedPanels
        : ['body'],
      regions: capture.regions,
      captureFullPage: capture.captureFullPage,
      interactions: capture.interactions,
      blockedInteractions: capture.blockedInteractions,
    });
  }

  return {
    pageId,
    htmlFile: `${pageId}.html`,
    manifest,
    scenarios: scenarioPlans,
    missingRequiredScenarios: [],
    baselineObservations: [],
  };
}

async function installDeterminism(page, clock, authUser, suppressFixtureSseDisconnect) {
  const token = authUser ? createFixtureAccessToken(authUser, clock.instant) : null;
  const expiry = Date.parse(clock.instant) + 60 * 60 * 1000;
  await page.addInitScript(({
    fixtureClock,
    accessToken,
    expiresAt,
    shouldSuppressFixtureSseDisconnect,
  }) => {
    const fixedTimestamp = Date.parse(fixtureClock.instant);
    const NativeDate = Date;
    const FixedDate = new Proxy(NativeDate, {
      apply(target, thisArg, argumentsList) {
        if (argumentsList.length === 0) return new NativeDate(fixedTimestamp).toString();
        return Reflect.apply(target, thisArg, argumentsList);
      },
      construct(target, argumentsList, newTarget) {
        return Reflect.construct(target, argumentsList.length === 0 ? [fixedTimestamp] : argumentsList, newTarget);
      },
    });
    Object.defineProperty(FixedDate, 'now', { configurable: true, value: () => fixedTimestamp });
    Object.defineProperty(globalThis, 'Date', { configurable: true, value: FixedDate });
    try {
      Object.defineProperty(globalThis.Performance.prototype, 'now', {
        configurable: true,
        value: () => 1_000,
      });
      Object.defineProperty(globalThis.Performance.prototype, 'timeOrigin', {
        configurable: true,
        value: fixedTimestamp - 1_000,
      });
    } catch {
      // Date remains deterministic if this browser exposes a non-configurable Performance clock.
    }

    let randomState = fixtureClock.random_seed >>> 0;
    const nextRandom = () => {
      randomState = (randomState * 1664525 + 1013904223) >>> 0;
      return randomState / 0x100000000;
    };
    Object.defineProperty(Math, 'random', { configurable: true, value: nextRandom });

    if (shouldSuppressFixtureSseDisconnect && typeof EventSource === 'function') {
      const NativeEventSource = EventSource;
      class FixtureEventSource extends NativeEventSource {
        constructor(...argumentsList) {
          super(...argumentsList);
          super.addEventListener('error', (event) => event.stopImmediatePropagation());
        }
      }
      Object.defineProperty(globalThis, 'EventSource', { configurable: true, value: FixtureEventSource });
    }

    let uuidIndex = 0;
    const deterministicUuid = () => {
      const configured = fixtureClock.uuid_sequence[uuidIndex];
      uuidIndex += 1;
      if (configured) return configured;
      return `10000000-0000-4000-8000-${uuidIndex.toString(16).padStart(12, '0').slice(-12)}`;
    };
    try {
      Object.defineProperty(globalThis.crypto, 'randomUUID', { configurable: true, value: deterministicUuid });
      Object.defineProperty(globalThis.crypto, 'getRandomValues', {
        configurable: true,
        value: (array) => {
          if (!array) throw new TypeError('Expected an ArrayBuffer view');
          const bytes = new Uint8Array(array.buffer, array.byteOffset, array.byteLength);
          for (let index = 0; index < bytes.length; index += 1) bytes[index] = (index * 37 + 17) % 256;
          return array;
        },
      });
    } catch {
      // Math.random remains deterministic if this browser exposes non-configurable Crypto methods.
    }

    if (accessToken) {
      sessionStorage.setItem('access_token', accessToken);
      sessionStorage.setItem('token_type', 'bearer');
      sessionStorage.setItem('token_expires_at', String(expiresAt));
      sessionStorage.setItem('session_secret', 'parity-session-secret');
    } else {
      localStorage.clear();
      sessionStorage.clear();
    }

    document.addEventListener('DOMContentLoaded', () => {
      const style = document.createElement('style');
      style.id = 'fms-parity-snapshot-determinism';
      style.textContent = `
        html { scroll-behavior: auto !important; }
        *, *::before, *::after {
          animation: none !important;
          caret-color: transparent !important;
          scroll-behavior: auto !important;
          transition: none !important;
          -webkit-font-smoothing: antialiased !important;
          -moz-osx-font-smoothing: grayscale !important;
          text-rendering: geometricPrecision !important;
        }
        [aria-busy="true"]::before,
        [aria-busy="true"]::after,
        .loading-indicator,
        .spinner {
          animation: none !important;
        }
      `;
      document.head.append(style);
    }, { once: true });
  }, {
    fixtureClock: clock,
    accessToken: token,
    expiresAt: expiry,
    shouldSuppressFixtureSseDisconnect: suppressFixtureSseDisconnect,
  });
}

async function installNetworkFixtures(page, scenario) {
  const requestLog = [];
  const unknownRequests = [];
  const callCounts = new Map();
  let pendingRequests = 0;
  let lastActivityAt = Date.now();

  await page.route('**/api/v2/**', async (route) => {
    pendingRequests += 1;
    lastActivityAt = Date.now();
    const recorded = recordRequest(route.request());
    requestLog.push(recorded);
    try {
      const sseFixture = scenario.sseStreams.find((fixture) => sseFixtureMatches(fixture, recorded));
      if (sseFixture) {
        await new Promise((resolve) => setTimeout(resolve, 50));
        await route.fulfill({
          status: 200,
          headers: {
            ...COMMON_HEADERS,
            Connection: 'keep-alive',
            'Content-Type': 'text/event-stream; charset=utf-8',
          },
          body: formatSseStream(sseFixture),
        });
        return;
      }

      const fixture = scenario.routes.find((candidate) => fixtureMatches(
        candidate,
        recorded,
        callCounts.get(candidate.id) ?? 0,
      ));
      if (!fixture) {
        unknownRequests.push(recorded);
        await route.fulfill({
          status: 599,
          headers: COMMON_HEADERS,
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
      const response = fixture.response;
      const headers = { ...COMMON_HEADERS, ...response.headers };
      if (typeof response.body === 'string') {
        await route.fulfill({ status: response.status, headers, body: response.body });
      } else if (response.body === undefined) {
        await route.fulfill({ status: response.status, headers, body: '' });
      } else {
        await route.fulfill({ status: response.status, headers, json: response.body });
      }
    } finally {
      pendingRequests -= 1;
      lastActivityAt = Date.now();
    }
  });

  return {
    requestLog,
    unknownRequests,
    async waitForCompletion() {
      const deadline = Date.now() + 10_000;
      while (Date.now() < deadline) {
        if (pendingRequests === 0 && Date.now() - lastActivityAt >= 300) return;
        await new Promise((resolve) => setTimeout(resolve, 50));
      }
      throw new CaptureValidationError(`${scenario.id}: fixture requests did not reach a stable idle state.`);
    },
  };
}

async function waitForStablePage(page, scenario, network) {
  for (let attempt = 0; attempt < 3; attempt += 1) {
    try {
      for (const selector of scenario.expectedPanels) {
        await page.locator(selector).first().waitFor({ state: 'visible', timeout: 10_000 });
      }
      await page.evaluate(async () => {
        if (document.fonts?.ready) await document.fonts.ready;
        const images = [...document.images];
        await Promise.all(images.map(async (image) => {
          if (!image.complete) {
            await new Promise((resolve) => {
              image.addEventListener('load', resolve, { once: true });
              image.addEventListener('error', resolve, { once: true });
            });
          }
          if (typeof image.decode === 'function') {
            try {
              await image.decode();
            } catch {
              // A failed resource is reported by the response tracker below.
            }
          }
        }));
        await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      });
      await network.waitForCompletion();
      return;
    } catch (error) {
      const navigationRace = /Execution context was destroyed|Target page, context or browser has been closed/i.test(
        error instanceof Error ? error.message : String(error),
      );
      if (!scenario.expectsErrorResponse || !navigationRace || attempt === 2) throw error;
      await page.waitForLoadState('domcontentloaded', { timeout: 10_000 }).catch(() => undefined);
    }
  }
}

function relativeSnapshotPath(pageId, filename) {
  return path.posix.join(pageId, filename);
}

async function hashLegacySources(legacyRoot, htmlFile) {
  const source = await extractLegacySourceContract(legacyRoot, `html/${htmlFile}`);
  return { sourceHash: source.sourceHash, sourceFiles: source.sourceFiles };
}

async function hashFiles(root, relativePaths) {
  const files = [];
  const aggregate = createHash('sha256');
  for (const relativePath of [...relativePaths].sort()) {
    const bytes = await readFile(path.join(root, ...relativePath.split('/')));
    const digest = sha256(bytes);
    files.push({ path: relativePath, sha256: digest });
    aggregate.update(relativePath);
    aggregate.update('\0');
    aggregate.update(bytes);
    aggregate.update('\0');
  }
  return { sha256: aggregate.digest('hex'), files };
}

async function hashCaptureHarness() {
  return hashFiles(vueAppRoot, [
    'scripts/parity/capture-legacy-snapshots.mjs',
    'scripts/parity/capture-actions.mjs',
    'scripts/parity/legacy-root.mjs',
    'scripts/parity/legacy-source-graph.mjs',
    'scripts/parity/serve-legacy.mjs',
  ]);
}

async function hashPageFixtures(pageId) {
  const fixtureDirectory = path.join(fixtureRoot, 'pages', pageId);
  const manifest = await readJson(path.join(fixtureDirectory, 'manifest.json'));
  const relativePaths = [
    `parity/fixtures/pages/${pageId}/manifest.json`,
    ...(manifest.scenarios ?? []).map((scenario) => (
      `parity/fixtures/pages/${pageId}/${scenario}.json`
    )),
  ];
  return hashFiles(vueAppRoot, relativePaths);
}

function captureArtifactName(scenarioId, stateId, regionId, viewportId) {
  return [scenarioId, stateId, regionId, viewportId].filter(Boolean).join('--') + '.png';
}

async function capturePageState({
  page,
  plan,
  scenario,
  viewport,
  legacySource,
  resourceFailures,
  pageErrors,
  stateId = null,
  captureFullPage,
  regions,
  actions = [],
}) {
  const captures = [];
  const commonMetadata = {
    viewport,
    fixture: scenario.fixtureId,
    theme: scenario.theme,
    scenario: scenario.id,
    state: stateId,
    actions,
    source_sha256: legacySource.sourceHash,
    observed_resource_failures: resourceFailures,
    observed_page_errors: pageErrors,
  };

  if (captureFullPage) {
    const filename = captureArtifactName(
      scenario.id,
      stateId,
      'full-page',
      viewport.id,
    );
    const bytes = await page.screenshot({ fullPage: true, animations: 'disabled' });
    captures.push({
      bytes,
      metadata: {
        file: relativeSnapshotPath(plan.pageId, filename),
        kind: 'full-page',
        region: null,
        ...commonMetadata,
        sha256: sha256(bytes),
      },
    });
  }

  for (const region of regions) {
    const filename = captureArtifactName(scenario.id, stateId, region.id, viewport.id);
    const locator = page.locator(region.selector).first();
    await locator.waitFor({ state: 'visible' });
    const bytes = await locator.screenshot({ animations: 'disabled' });
    captures.push({
      bytes,
      metadata: {
        file: relativeSnapshotPath(plan.pageId, filename),
        kind: 'region',
        region: region.id,
        selector: region.selector,
        ...commonMetadata,
        sha256: sha256(bytes),
      },
    });
  }
  return captures;
}

function assertStableCaptureEvidence(plan, scenario, viewport, network, resourceFailures, pageErrors) {
  const errors = [];
  if (network.unknownRequests.length > 0) {
    errors.push(...network.unknownRequests.map((request) => (
      `unknown API: ${request.method} ${request.pathname} query=${JSON.stringify(request.query)} body=${JSON.stringify(request.body)}`
    )));
  }
  if (!scenario.expectsErrorResponse) {
    const isApprovedLegacyError = (failure) => (scenario.approvedLegacyErrors ?? []).some(
      (approved) => failure.includes(approved),
    );
    errors.push(...resourceFailures
      .filter((failure) => !isApprovedLegacyError(failure))
      .map((failure) => `resource failure: ${failure}`));
    errors.push(...pageErrors
      .filter((failure) => !isApprovedLegacyError(failure))
      .map((failure) => `page error: ${failure}`));
  }
  if (errors.length > 0) {
    throw new CaptureValidationError(
      `${plan.pageId}/${scenario.id}/${viewport.id}: page did not stabilize against strict fixtures.`,
      errors,
    );
  }
}

async function captureScenario(browser, baseUrl, clock, legacySource, plan, scenario, viewport) {
  const context = await browser.newContext({
    viewport: { width: viewport.width, height: viewport.height },
    deviceScaleFactor: 1,
    colorScheme: scenario.theme,
    locale: clock.locale,
    timezoneId: clock.timezone,
    reducedMotion: 'reduce',
    serviceWorkers: 'block',
  });
  const page = await context.newPage();
  const resourceFailures = [];
  const pageErrors = [];
  page.on('pageerror', (error) => pageErrors.push(error.message));
  page.on('requestfailed', (request) => resourceFailures.push(
    `${request.method()} ${new URL(request.url()).pathname}: ${request.failure()?.errorText ?? 'request failed'}`,
  ));
  page.on('response', (response) => {
    const url = new URL(response.url());
    if (url.origin === baseUrl && response.status() >= 400 && !url.pathname.startsWith('/api/v2/')) {
      resourceFailures.push(`${response.request().method()} ${url.pathname}: HTTP ${response.status()}`);
    }
  });

  try {
    await installDeterminism(page, clock, scenario.authUser, scenario.sseStreams.length > 0);
    const network = await installNetworkFixtures(page, scenario);
    await page.goto(`${baseUrl}/frontend/html/${plan.htmlFile}`, { waitUntil: 'domcontentloaded' });
    await waitForStablePage(page, scenario, network);
    if (scenario.setup) {
      await scenario.setup(page);
      await waitForStablePage(page, scenario, network);
    }

    assertStableCaptureEvidence(
      plan,
      scenario,
      viewport,
      network,
      resourceFailures,
      pageErrors,
    );

    const captures = await capturePageState({
      page,
      plan,
      scenario,
      viewport,
      legacySource,
      resourceFailures,
      pageErrors,
      captureFullPage: scenario.captureFullPage !== false,
      regions: scenario.regions,
    });

    for (const interaction of scenario.interactions ?? []) {
      await runCaptureActions(page, interaction.actions);
      await waitForStablePage(page, {
        ...scenario,
        expectedPanels: interaction.expectedPanels.length > 0
          ? interaction.expectedPanels
          : scenario.expectedPanels,
      }, network);
      assertStableCaptureEvidence(
        plan,
        scenario,
        viewport,
        network,
        resourceFailures,
        pageErrors,
      );
      captures.push(...await capturePageState({
        page,
        plan,
        scenario,
        viewport,
        legacySource,
        resourceFailures,
        pageErrors,
        stateId: interaction.id,
        captureFullPage: interaction.captureFullPage,
        regions: interaction.regions,
        actions: interaction.actions,
      }));
    }

    return { captures, requestLog: network.requestLog };
  } finally {
    await context.close();
  }
}

function assertSafeChild(parent, child) {
  const relative = path.relative(path.resolve(parent), path.resolve(child));
  if (!relative || relative.startsWith('..') || path.isAbsolute(relative)) {
    throw new CaptureValidationError(`Refusing filesystem operation outside the snapshot root: ${child}`);
  }
}

async function pathExists(candidate) {
  try {
    await stat(candidate);
    return true;
  } catch {
    return false;
  }
}

async function renameWithWindowsRetries(source, destination) {
  for (let attempt = 0; attempt < 6; attempt += 1) {
    try {
      await rename(source, destination);
      return;
    } catch (error) {
      const retryable = ['EPERM', 'EACCES', 'EBUSY'].includes(error?.code);
      if (!retryable || attempt === 5) throw error;
      await new Promise((resolve) => setTimeout(resolve, 50 * (attempt + 1)));
    }
  }
}

async function replacePageBaseline(pageId, captures, metadata) {
  await mkdir(snapshotRoot, { recursive: true });
  const target = path.join(snapshotRoot, pageId);
  const temporary = path.join(snapshotRoot, `.${pageId}.refresh-${process.pid}`);
  const backup = path.join(snapshotRoot, `.${pageId}.backup-${process.pid}`);
  assertSafeChild(snapshotRoot, target);
  assertSafeChild(snapshotRoot, temporary);
  assertSafeChild(snapshotRoot, backup);

  if (await pathExists(temporary)) await rm(temporary, { recursive: true, force: true });
  if (await pathExists(backup)) await rm(backup, { recursive: true, force: true });
  await mkdir(temporary, { recursive: true });
  try {
    for (const capture of captures) {
      const filename = path.basename(capture.metadata.file);
      await writeFile(path.join(temporary, filename), capture.bytes, { flag: 'wx' });
    }
    await writeFile(
      path.join(temporary, 'capture.metadata.json'),
      `${JSON.stringify(metadata, null, 2)}\n`,
      { encoding: 'utf8', flag: 'wx' },
    );

    const hadTarget = await pathExists(target);
    if (hadTarget) await renameWithWindowsRetries(target, backup);
    try {
      await renameWithWindowsRetries(temporary, target);
    } catch (error) {
      if (hadTarget && await pathExists(backup)) await renameWithWindowsRetries(backup, target);
      throw error;
    }
    if (hadTarget && await pathExists(backup)) await rm(backup, { recursive: true, force: true });
  } catch (error) {
    if (await pathExists(temporary)) await rm(temporary, { recursive: true, force: true });
    throw error;
  }
}

async function refreshPage(browser, baseUrl, clock, legacyRoot, pageId, browserVersion) {
  const plan = await createCapturePlan(pageId);
  const legacySource = await hashLegacySources(legacyRoot, plan.htmlFile);
  const captureHarness = await hashCaptureHarness();
  const pageFixtures = await hashPageFixtures(pageId);
  const captures = [];
  const requestEvidence = [];

  for (const scenario of plan.scenarios) {
    for (const viewport of VIEWPORTS) {
      const result = await captureScenario(
        browser,
        baseUrl,
        clock,
        legacySource,
        plan,
        scenario,
        viewport,
      );
      captures.push(...result.captures);
      requestEvidence.push({
        scenario: scenario.id,
        viewport: viewport.id,
        requests: result.requestLog,
      });
    }
  }

  if (plan.missingRequiredScenarios.length > 0) {
    throw new CaptureValidationError(
      `${pageId}: fixture manifest declares scenarios that are not executable by the capture harness.`,
      plan.missingRequiredScenarios.map((scenario) => `missing executable capture scenario: ${scenario}`),
    );
  }

  const metadata = {
    schema_version: 1,
    page: pageId,
    legacy_html: `html/${plan.htmlFile}`,
    fixture_manifest_status: plan.manifest.status,
    captured_at: clock.instant,
    browser: { engine: 'chromium', version: browserVersion },
    deterministic_environment: {
      timezone: clock.timezone,
      locale: clock.locale,
      color_scheme: 'light',
      device_scale_factor: 1,
      reduced_motion: true,
      service_workers: 'blocked',
      fonts: 'document.fonts.ready',
      timestamps: 'fixed-clock',
      performance_clock: 'fixed-monotonic-clock',
      random_values: 'seeded',
      animations: 'disabled',
      caret: 'hidden',
    },
    required_viewports: VIEWPORTS,
    source_sha256: legacySource.sourceHash,
    source_files: legacySource.sourceFiles,
    capture_harness_sha256: captureHarness.sha256,
    capture_harness_files: captureHarness.files,
    fixture_sha256: pageFixtures.sha256,
    fixture_files: pageFixtures.files,
    baseline_observations: plan.baselineObservations,
    scenarios: plan.scenarios.map((scenario) => ({
      id: scenario.id,
      fixture: scenario.fixtureId,
      theme: scenario.theme,
      auth_role: scenario.authRole,
      expects_error_response: scenario.expectsErrorResponse ?? false,
      approved_legacy_errors: scenario.approvedLegacyErrors ?? [],
      approved_exception_ids: scenario.approvedExceptionIds ?? [],
      expected_panels: scenario.expectedPanels,
      regions: scenario.regions,
      interactions: scenario.interactions ?? [],
      blocked_interactions: scenario.blockedInteractions ?? [],
    })),
    captures: captures.map((capture) => capture.metadata),
    request_evidence: requestEvidence,
  };

  await replacePageBaseline(pageId, captures, metadata);
  return { pageId, captureCount: captures.length };
}

async function validatePageBaseline(pageId, legacyRoot) {
  const directory = path.join(snapshotRoot, pageId);
  const metadataPath = path.join(directory, 'capture.metadata.json');
  if (!(await pathExists(metadataPath))) {
    throw new CaptureValidationError(`${pageId}: missing committed capture metadata: ${metadataPath}`);
  }
  const metadata = await readJson(metadataPath);
  const errors = [];
  if (metadata.page !== pageId) errors.push(`metadata page is ${JSON.stringify(metadata.page)}`);
  if (!/^[a-f0-9]{64}$/.test(metadata.source_sha256 ?? '')) errors.push('source_sha256 is missing or invalid');
  if (!isDeepStrictEqual(metadata.required_viewports, VIEWPORTS)) errors.push('required viewport matrix does not match the five approved viewports');
  if (!Array.isArray(metadata.captures) || metadata.captures.length === 0) errors.push('capture list is empty');
  const currentSource = await hashLegacySources(legacyRoot, `${pageId}.html`);
  if (metadata.source_sha256 !== currentSource.sourceHash) {
    errors.push('source_sha256 is stale for the current legacy source dependency graph');
  }
  if (!isDeepStrictEqual(metadata.source_files, currentSource.sourceFiles)) {
    errors.push('source_files do not match the current legacy source dependency graph');
  }
  const currentHarness = await hashCaptureHarness();
  if (metadata.capture_harness_sha256 !== currentHarness.sha256
    || !isDeepStrictEqual(metadata.capture_harness_files, currentHarness.files)) {
    errors.push('capture harness hash is stale');
  }
  const currentFixtures = await hashPageFixtures(pageId);
  if (metadata.fixture_sha256 !== currentFixtures.sha256
    || !isDeepStrictEqual(metadata.fixture_files, currentFixtures.files)) {
    errors.push('fixture source hash is stale');
  }

  const viewportIds = new Set(metadata.captures?.map((capture) => capture.viewport?.id));
  for (const viewport of VIEWPORTS) {
    if (!viewportIds.has(viewport.id)) errors.push(`no capture exists for viewport ${viewport.id}`);
  }

  const expectedFiles = new Set(['capture.metadata.json']);
  for (const capture of metadata.captures ?? []) {
    const filename = path.basename(capture.file ?? '');
    if (!filename || capture.file !== relativeSnapshotPath(pageId, filename)) {
      errors.push(`capture file is outside its page directory: ${JSON.stringify(capture.file)}`);
      continue;
    }
    expectedFiles.add(filename);
    if (capture.fixture === undefined) errors.push(`${filename}: missing fixture metadata`);
    if (capture.theme === undefined) errors.push(`${filename}: missing theme metadata`);
    if (capture.scenario === undefined) errors.push(`${filename}: missing scenario metadata`);
    const expectedViewport = VIEWPORTS.find((viewport) => viewport.id === capture.viewport?.id);
    if (!expectedViewport || !isDeepStrictEqual(capture.viewport, expectedViewport)) {
      errors.push(`${filename}: viewport metadata does not match an approved viewport`);
    }
    if (!isDeepStrictEqual(capture.source_sha256, metadata.source_sha256)) errors.push(`${filename}: source hash differs from page metadata`);
    const screenshotPath = path.join(directory, filename);
    if (!(await pathExists(screenshotPath))) {
      errors.push(`missing screenshot: ${screenshotPath}`);
      continue;
    }
    const bytes = await readFile(screenshotPath);
    if (sha256(bytes) !== capture.sha256) errors.push(`${filename}: screenshot hash mismatch`);
    if (bytes.length < 24 || bytes.subarray(0, 8).toString('hex') !== '89504e470d0a1a0a') {
      errors.push(`${filename}: artifact is not a valid PNG`);
      continue;
    }
    const imageWidth = bytes.readUInt32BE(16);
    const imageHeight = bytes.readUInt32BE(20);
    if (imageWidth === 0 || imageHeight === 0) errors.push(`${filename}: PNG dimensions are invalid`);
    if (capture.kind === 'full-page' && expectedViewport && (
      imageWidth < expectedViewport.width || imageHeight < expectedViewport.height
    )) {
      errors.push(`${filename}: full-page PNG dimensions are smaller than its viewport`);
    }
  }

  const actualFiles = new Set(await readdir(directory));
  for (const filename of actualFiles) {
    if (!expectedFiles.has(filename)) errors.push(`untracked baseline artifact: ${filename}`);
  }
  for (const filename of expectedFiles) {
    if (!actualFiles.has(filename)) errors.push(`metadata references missing artifact: ${filename}`);
  }
  if (errors.length > 0) throw new CaptureValidationError(`${pageId}: legacy screenshot baseline validation failed.`, errors);
  return { pageId, captureCount: metadata.captures.length };
}

async function checkBaselines(pages) {
  const validation = await validateLegacyRoot();
  const successes = [];
  const failures = [];
  for (const pageId of pages) {
    try {
      successes.push(await validatePageBaseline(pageId, validation.root));
    } catch (error) {
      failures.push(error instanceof Error ? error.message : String(error));
    }
  }
  for (const result of successes) console.log(`Validated ${result.pageId}: ${result.captureCount} captures.`);
  if (failures.length > 0) throw new CaptureValidationError('Legacy screenshot baseline coverage is incomplete.', failures);
}

async function refreshBaselines(pages) {
  const validation = await validateLegacyRoot();
  const clock = await readJson(path.join(fixtureRoot, 'common', 'clock.json'));
  const port = Number(process.env.FMS_LEGACY_FRONTEND_PORT ?? DEFAULT_PORT);
  const running = await startLegacyServer({ root: validation.root, port });
  const browser = await chromium.launch({ headless: true });
  const baseUrl = `http://${running.host}:${running.port}`;
  const successes = [];
  const failures = [];

  try {
    for (const pageId of pages) {
      try {
        const result = await refreshPage(
          browser,
          baseUrl,
          clock,
          validation.root,
          pageId,
          browser.version(),
        );
        successes.push(result);
        console.log(`Refreshed ${result.pageId}: ${result.captureCount} captures.`);
      } catch (error) {
        failures.push(error instanceof Error ? error.message : String(error));
      }
    }
  } finally {
    await browser.close();
    await new Promise((resolve, reject) => running.server.close((error) => (error ? reject(error) : resolve())));
  }

  if (failures.length > 0) {
    throw new CaptureValidationError(
      `Legacy screenshot refresh completed ${successes.length}/${pages.length} requested pages.`,
      failures,
    );
  }
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  if (options.help) {
    printHelp();
    return;
  }
  if (options.mode === 'refresh') {
    await refreshBaselines(options.pages);
    return;
  }
  await checkBaselines(options.pages);
}

try {
  await main();
} catch (error) {
  if (error instanceof CaptureValidationError || error instanceof LegacyRootValidationError) {
    console.error(error.message);
  } else if (error?.code === 'EADDRINUSE') {
    console.error(`Legacy parity server port ${process.env.FMS_LEGACY_FRONTEND_PORT ?? DEFAULT_PORT} is already in use.`);
  } else {
    console.error(error instanceof Error ? error.stack ?? error.message : String(error));
  }
  process.exitCode = 1;
}
