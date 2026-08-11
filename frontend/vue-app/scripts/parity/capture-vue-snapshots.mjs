/**
 * Capture deterministic Vue MPA screenshots for visual parity against legacy baselines.
 *
 * Usage:
 *   node scripts/parity/capture-vue-snapshots.mjs --page login
 *   node scripts/parity/capture-vue-snapshots.mjs --page system_flags --refresh-baseline
 *   node scripts/parity/capture-vue-snapshots.mjs --page dashboard --refresh-baseline
 *   node scripts/parity/capture-vue-snapshots.mjs --page system_status --refresh-baseline
 *   node scripts/parity/capture-vue-snapshots.mjs --page login,system_flags,dashboard,system_status --refresh-baseline
 *
 * Default mode checks existing vue snapshot metadata/hashes without writing.
 * Refresh requires an explicit flag (mirrors legacy capture policy).
 */
import { chromium } from 'playwright';
import { createHash } from 'node:crypto';
import {
  mkdir,
  readFile,
  rename,
  rm,
  writeFile,
} from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';
import { isDeepStrictEqual } from 'node:util';
import {
  CaptureActionValidationError,
  normalizeCaptureDefinition,
  runCaptureActions,
} from './capture-actions.mjs';
import {
  encodeSseEvents,
  getNetworkIdleTimeoutMs,
  getVuePanelSelectors,
  getVueRegionSelector,
  getVueSseStreams,
  isOptionalVueInteraction,
} from './capture-vue-adaptations.mjs';

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const vueAppRoot = path.resolve(scriptDirectory, '..', '..');
const fixtureRoot = path.join(vueAppRoot, 'parity', 'fixtures');
const snapshotRoot = path.join(vueAppRoot, 'e2e', 'parity', 'snapshots', 'vue');
const previewPort = 4173;
const previewOrigin = `http://127.0.0.1:${previewPort}`;
const FIXED_INSTANT = '2026-07-14T02:30:00.000Z';

const VIEWPORTS = Object.freeze([
  { id: 'desktop-wide', width: 1920, height: 1080 },
  { id: 'desktop', width: 1440, height: 900 },
  { id: 'laptop', width: 1366, height: 768 },
  { id: 'tablet', width: 1024, height: 768 },
  { id: 'mobile', width: 390, height: 844 },
]);

/** Pages with a dedicated capture implementation. Expand as visual work proceeds. */
const SUPPORTED_PAGES = Object.freeze(['login', 'system_flags', 'dashboard', 'system_status']);

const COMMON_HEADERS = Object.freeze({
  'Cache-Control': 'no-store',
  'X-FMS-Parity-Fixture': 'deterministic',
});

function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
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

async function readJson(filePath) {
  return JSON.parse(await readFile(filePath, 'utf8'));
}

function parseArgs(argv) {
  const options = { mode: 'check', pages: ['login'] };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--refresh-baseline') options.mode = 'refresh';
    else if (arg === '--check') options.mode = 'check';
    else if (arg === '--page') {
      options.pages = String(argv[++i] || '')
        .split(',')
        .map((page) => page.trim())
        .filter(Boolean);
    }
  }
  return options;
}

async function waitForPreview(url, timeoutMs = 60_000) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const response = await fetch(url);
      if (response.ok) return;
    } catch {
      // retry
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`Preview server did not become ready at ${url}`);
}

async function startPreview() {
  const viteCli = path.join(vueAppRoot, 'node_modules', 'vite', 'bin', 'vite.js');
  const child = spawn(
    process.execPath,
    [viteCli, 'preview', '--host', '127.0.0.1', '--port', String(previewPort), '--strictPort'],
    {
      cwd: vueAppRoot,
      stdio: ['ignore', 'pipe', 'pipe'],
      env: { ...process.env },
      windowsHide: true,
    },
  );
  let logs = '';
  child.stdout.on('data', (chunk) => { logs += chunk.toString(); });
  child.stderr.on('data', (chunk) => { logs += chunk.toString(); });
  try {
    await waitForPreview(`${previewOrigin}/frontend/login.html`);
  } catch (error) {
    child.kill('SIGTERM');
    throw new Error(`${error.message}\n${logs}`);
  }
  return child;
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
  if (!isDeepStrictEqual(normalizeExpectedQuery(fixture.query ?? {}), request.query)) return false;
  if (Object.hasOwn(fixture, 'requestBody') && !isDeepStrictEqual(fixture.requestBody, request.body)) {
    return false;
  }
  return true;
}

function sseFixtureMatches(fixture, request) {
  if (request.method !== 'GET' || fixture.pathname !== request.pathname) return false;
  const expectedQuery = normalizeExpectedQuery(fixture.query ?? {});
  return Object.entries(expectedQuery).every(([key, values]) => (
    isDeepStrictEqual(request.query[key] ?? [], values)
  ));
}

async function installDeterminism(page, options = {}) {
  const { accessToken = null, expiresAt = null } = options;
  await page.addInitScript(({ fixedInstant, token, expiry }) => {
    const fixed = Date.parse(fixedInstant);
    const NativeDate = Date;
    const FixedDate = new Proxy(NativeDate, {
      apply(target, thisArg, args) {
        if (args.length === 0) return new NativeDate(fixed).toString();
        return Reflect.apply(target, thisArg, args);
      },
      construct(target, args, newTarget) {
        return Reflect.construct(target, args.length === 0 ? [fixed] : args, newTarget);
      },
    });
    Object.defineProperty(FixedDate, 'now', { configurable: true, value: () => fixed });
    Object.defineProperty(globalThis, 'Date', { configurable: true, value: FixedDate });
    Object.defineProperty(Math, 'random', { configurable: true, value: () => 0.123456789 });

    // Cookie-session path uses /auth/refresh; keep a session marker so pages that
    // look for authSession do not immediately bounce.
    try {
      sessionStorage.setItem('authSession', '1');
      if (token) {
        sessionStorage.setItem('access_token', token);
        sessionStorage.setItem('token_type', 'bearer');
        sessionStorage.setItem('token_expires_at', String(expiry ?? fixed + 3_600_000));
        sessionStorage.setItem('session_secret', 'parity-session-secret');
      }
    } catch {
      // private mode / blocked storage
    }

    document.addEventListener('DOMContentLoaded', () => {
      const style = document.createElement('style');
      style.textContent = '*,*::before,*::after{animation:none!important;transition:none!important;caret-color:transparent!important;-webkit-font-smoothing:antialiased!important;-moz-osx-font-smoothing:grayscale!important;text-rendering:geometricPrecision!important;}';
      document.head.append(style);
    }, { once: true });
  }, {
    fixedInstant: FIXED_INSTANT,
    token: accessToken,
    expiry: expiresAt,
  });
}

async function installNetworkFixtures(page, routes, sseStreams = [], options = {}) {
  const { idleTimeoutMs = 10_000 } = options;
  const callCounts = new Map();
  let pendingRequests = 0;
  let lastActivityAt = Date.now();

  await page.route('**/api/v2/**', async (route) => {
    pendingRequests += 1;
    lastActivityAt = Date.now();
    const recorded = recordRequest(route.request());
    try {
      const fixture = routes.find((candidate) => fixtureMatches(
        candidate,
        recorded,
        callCounts.get(candidate.id) ?? 0,
      ));
      if (!fixture) {
        const stream = sseStreams.find((candidate) => sseFixtureMatches(candidate, recorded));
        if (stream) {
          await route.fulfill({
            status: 200,
            headers: {
              ...COMMON_HEADERS,
              'Content-Type': 'text/event-stream; charset=utf-8',
              Connection: 'keep-alive',
            },
            body: encodeSseEvents(stream.events),
          });
          return;
        }
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
      const headers = { ...COMMON_HEADERS, ...(response.headers ?? {}) };
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
    async waitForCompletion() {
      const deadline = Date.now() + idleTimeoutMs;
      while (Date.now() < deadline) {
        if (pendingRequests === 0 && Date.now() - lastActivityAt >= 300) return;
        await new Promise((resolve) => setTimeout(resolve, 50));
      }
      throw new Error('fixture requests did not reach a stable idle state');
    },
  };
}

const LOGIN_SCENARIOS = [
  {
    id: 'default',
    setup: async () => {},
    regions: [{ id: 'login-card', selector: '.login-card' }],
    captureFullPage: true,
  },
  {
    id: 'required-username-error',
    setup: async (page) => {
      await page.locator('#loginBtn').click();
      await page.locator('#errorMessage.show').waitFor({ state: 'visible' });
    },
    regions: [{ id: 'login-card', selector: '.login-card' }],
    captureFullPage: true,
  },
  {
    id: 'password-visible',
    setup: async (page) => {
      await page.locator('#username').fill('parity_admin');
      await page.locator('#password').fill('fixture-password');
      await page.locator('#passwordToggleBtn').click();
    },
    regions: [{ id: 'login-card', selector: '.login-card' }],
    captureFullPage: false,
  },
];

async function captureLogin(browser) {
  const captures = [];
  for (const scenario of LOGIN_SCENARIOS) {
    for (const viewport of VIEWPORTS) {
      const context = await browser.newContext({
        viewport: { width: viewport.width, height: viewport.height },
        deviceScaleFactor: 1,
        colorScheme: 'light',
        locale: 'zh-CN',
        timezoneId: 'Asia/Singapore',
        reducedMotion: 'reduce',
        serviceWorkers: 'block',
      });
      const page = await context.newPage();
      await installDeterminism(page);
      await page.goto(`${previewOrigin}/frontend/login.html`, { waitUntil: 'networkidle' });
      await page.evaluate(async () => {
        if (document.fonts?.ready) await document.fonts.ready;
      });
      await scenario.setup(page);
      await page.waitForTimeout(100);

      if (scenario.captureFullPage !== false) {
        const bytes = await page.screenshot({ fullPage: true, animations: 'disabled', type: 'png' });
        const file = `${scenario.id}--full-page--${viewport.id}.png`;
        captures.push({
          file,
          bytes,
          kind: 'full-page',
          scenario: scenario.id,
          viewport: viewport.id,
          sha256: sha256(bytes),
        });
      }
      for (const region of scenario.regions) {
        const locator = page.locator(region.selector).first();
        await locator.waitFor({ state: 'visible' });
        const bytes = await locator.screenshot({ animations: 'disabled', type: 'png' });
        const file = `${scenario.id}--${region.id}--${viewport.id}.png`;
        captures.push({
          file,
          bytes,
          kind: 'region',
          region: region.id,
          scenario: scenario.id,
          viewport: viewport.id,
          sha256: sha256(bytes),
        });
      }
      await context.close();
    }
  }
  return captures;
}

async function loadFixtureScenarios(pageId) {
  const manifest = await readJson(path.join(fixtureRoot, 'pages', pageId, 'manifest.json'));
  if (manifest.status !== 'seeded' || !Array.isArray(manifest.scenarios) || manifest.scenarios.length === 0) {
    throw new Error(`${pageId}: fixture manifest is not seeded for vue capture.`);
  }

  const fixtureClock = await readJson(path.join(fixtureRoot, 'common', 'clock.json'));
  const scenarios = [];
  for (const scenarioName of manifest.scenarios) {
    const definition = await readJson(path.join(fixtureRoot, 'pages', pageId, `${scenarioName}.json`));
    const authRole = definition.auth_role ?? 'admin';
    const authFilename = {
      admin: 'auth-admin.json',
      operator: 'auth-operator.json',
      readonly: 'auth-readonly.json',
    }[authRole];
    if (!authFilename) {
      throw new Error(`${pageId}/${scenarioName}: unsupported auth_role ${JSON.stringify(authRole)}`);
    }
    const authUser = await readJson(path.join(fixtureRoot, 'common', authFilename));
    const explicitRoutes = Array.isArray(definition.routes) ? definition.routes : [];
    const accessToken = createFixtureAccessToken(authUser, fixtureClock.instant);
    const commonRoutes = [
      {
        id: `common-auth-refresh-${authRole}`,
        method: 'POST',
        pathname: '/api/v2/auth/refresh',
        query: {},
        response: {
          status: 200,
          body: {
            access_token: accessToken,
            expires_in: 3600,
            token_type: 'bearer',
            session_secret: 'parity-session-secret',
          },
        },
      },
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
      {
        id: 'common-auth-logout',
        method: 'POST',
        pathname: '/api/v2/auth/logout',
        query: {},
        response: { status: 200, body: { success: true } },
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
        throw new Error(error.message);
      }
      throw error;
    }

    scenarios.push({
      id: definition.scenario ?? `${pageId}-${scenarioName}`,
      authUser,
      accessToken,
      expiresAt: Date.parse(fixtureClock.instant) + 3_600_000,
      routes: [...explicitRoutes, ...commonRoutes],
      sseStreams: getVueSseStreams(
        pageId,
        Array.isArray(definition.sse_streams) ? definition.sse_streams : [],
      ),
      expectedPanels: capture.expectedPanels,
      regions: capture.regions,
      captureFullPage: capture.captureFullPage,
      interactions: capture.interactions,
      expectsErrorResponse: explicitRoutes.some((route) => Number(route.response?.status ?? 200) >= 400),
    });
  }
  return scenarios;
}

async function captureFixturePage(browser, pageId) {
  const scenarios = await loadFixtureScenarios(pageId);
  const captures = [];

  for (const scenario of scenarios) {
    for (const viewport of VIEWPORTS) {
      const context = await browser.newContext({
        viewport: { width: viewport.width, height: viewport.height },
        deviceScaleFactor: 1,
        colorScheme: 'light',
        locale: 'zh-CN',
        timezoneId: 'Asia/Singapore',
        reducedMotion: 'reduce',
        serviceWorkers: 'block',
      });
      const page = await context.newPage();
      await installDeterminism(page, {
        accessToken: scenario.accessToken,
        expiresAt: scenario.expiresAt,
      });
      const network = await installNetworkFixtures(page, scenario.routes, scenario.sseStreams, {
        idleTimeoutMs: getNetworkIdleTimeoutMs(pageId),
      });

      await page.goto(`${previewOrigin}/frontend/${pageId}.html`, {
        waitUntil: 'domcontentloaded',
      });
      await page.evaluate(async () => {
        if (document.fonts?.ready) await document.fonts.ready;
      });

      // Prefer declared panels; fall back to main content shell for error/empty states.
      const panelSelectors = getVuePanelSelectors(pageId, scenario.expectedPanels);
      let panelReady = false;
      for (const selector of panelSelectors) {
        try {
          await page.locator(selector).first().waitFor({ state: 'visible', timeout: 8_000 });
          panelReady = true;
          break;
        } catch {
          // try next
        }
      }
      if (!panelReady && !scenario.expectsErrorResponse) {
        throw new Error(`${scenario.id}: expected panels not visible (${panelSelectors.join(', ')})`);
      }

      if (Array.isArray(scenario.interactions) && scenario.interactions.length > 0) {
        for (const interaction of scenario.interactions) {
          if (isOptionalVueInteraction(pageId, interaction.id)) {
            const actionTargetsExist = await Promise.all(
              interaction.actions.map(async ({ selector }) => (
                await page.locator(selector).first().count()
              ) > 0),
            );
            if (actionTargetsExist.some((exists) => !exists)) {
              console.warn(`${scenario.id}/${interaction.id}: Vue capture soft-skip (legacy-only interaction target).`);
              continue;
            }
          }
          await runCaptureActions(page, interaction.actions);
        }
      }

      try {
        await network.waitForCompletion();
      } catch {
        // Error/redirect scenarios may not idle cleanly; still capture whatever rendered.
      }
      await page.waitForTimeout(120);

      if (scenario.captureFullPage !== false) {
        const bytes = await page.screenshot({ fullPage: true, animations: 'disabled', type: 'png' });
        const file = `${scenario.id}--full-page--${viewport.id}.png`;
        captures.push({
          file,
          bytes,
          kind: 'full-page',
          scenario: scenario.id,
          viewport: viewport.id,
          sha256: sha256(bytes),
        });
      }

      for (const region of scenario.regions) {
        const locator = page.locator(getVueRegionSelector(pageId, region)).first();
        try {
          await locator.waitFor({ state: 'visible', timeout: 5_000 });
          const bytes = await locator.screenshot({ animations: 'disabled', type: 'png' });
          const file = `${scenario.id}--${region.id}--${viewport.id}.png`;
          captures.push({
            file,
            bytes,
            kind: 'region',
            region: region.id,
            scenario: scenario.id,
            viewport: viewport.id,
            sha256: sha256(bytes),
          });
        } catch {
          // Region may be absent in empty/error fixtures; full-page still covers them.
        }
      }

      await context.close();
    }
  }
  return captures;
}

async function writeCaptures(pageId, captures) {
  const dir = path.join(snapshotRoot, pageId);
  const tempDir = path.join(snapshotRoot, `.tmp-${pageId}-${Date.now()}`);
  await mkdir(tempDir, { recursive: true });
  for (const capture of captures) {
    await writeFile(path.join(tempDir, capture.file), capture.bytes);
  }
  const metadata = {
    schema_version: 1,
    page: pageId,
    target: 'vue',
    captured_at: FIXED_INSTANT,
    required_viewports: VIEWPORTS,
    captures: captures.map(({ bytes: _bytes, ...meta }) => ({
      ...meta,
      file: `${pageId}/${meta.file}`,
    })),
  };
  await writeFile(path.join(tempDir, 'capture.metadata.json'), `${JSON.stringify(metadata, null, 2)}\n`);
  await rm(dir, { recursive: true, force: true });
  await rename(tempDir, dir);
}

async function validateCaptures(pageId) {
  const dir = path.join(snapshotRoot, pageId);
  const metadata = JSON.parse(await readFile(path.join(dir, 'capture.metadata.json'), 'utf8'));
  const details = [];
  for (const capture of metadata.captures ?? []) {
    const filename = path.basename(capture.file);
    const bytes = await readFile(path.join(dir, filename));
    const digest = sha256(bytes);
    if (digest !== capture.sha256) {
      details.push(`${filename}: hash mismatch`);
    }
  }
  if (details.length > 0) {
    throw new Error(`Vue snapshot validation failed for ${pageId}:\n${details.map((d) => `  - ${d}`).join('\n')}`);
  }
  console.log(`Validated vue/${pageId}: ${(metadata.captures ?? []).length} captures.`);
}

async function capturePage(browser, pageId) {
  if (pageId === 'login') return captureLogin(browser);
  return captureFixturePage(browser, pageId);
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const unsupported = options.pages.filter((page) => !SUPPORTED_PAGES.includes(page));
  if (unsupported.length > 0) {
    throw new Error(
      `capture-vue unsupported page(s): ${unsupported.join(', ')}. Supported: ${SUPPORTED_PAGES.join(', ')}`,
    );
  }

  if (options.mode === 'check') {
    for (const page of options.pages) await validateCaptures(page);
    return;
  }

  const preview = await startPreview();
  const browser = await chromium.launch({ headless: true });
  try {
    for (const page of options.pages) {
      const captures = await capturePage(browser, page);
      await writeCaptures(page, captures);
      console.log(`Refreshed vue/${page}: ${captures.length} captures.`);
      await validateCaptures(page);
    }
  } finally {
    await browser.close();
    preview.kill('SIGTERM');
  }
}

await main().catch((error) => {
  console.error(error instanceof Error ? error.stack ?? error.message : String(error));
  process.exitCode = 1;
});
