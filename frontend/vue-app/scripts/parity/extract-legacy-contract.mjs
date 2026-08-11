import { readFile, stat, writeFile, mkdir } from 'node:fs/promises';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { chromium } from '@playwright/test';
import ts from 'typescript';
import { LEGACY_HTML_FILES, validateLegacyRoot } from './legacy-root.mjs';
import { extractLegacySourceContract } from './legacy-source-graph.mjs';
import { createLegacyServer } from './serve-legacy.mjs';

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const appRoot = path.resolve(scriptDirectory, '..', '..');
const contractsRoot = path.join(appRoot, 'parity', 'contracts');
const fixturesRoot = path.join(appRoot, 'parity', 'fixtures');
const fixedCaptureViewport = Object.freeze({ width: 1440, height: 900 });

async function loadRuntimeContractSchema() {
  const schemaPath = path.join(appRoot, 'src', 'legacy', 'legacyContractSchema.ts');
  const source = await readFile(schemaPath, 'utf8');
  const transpiled = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ESNext,
      target: ts.ScriptTarget.ES2022,
    },
    fileName: schemaPath,
    reportDiagnostics: true,
  });
  const errors = (transpiled.diagnostics ?? []).filter(
    (diagnostic) => diagnostic.category === ts.DiagnosticCategory.Error,
  );
  if (errors.length > 0) {
    throw new Error(`Unable to load legacy contract schema:\n${errors.map((diagnostic) => (
      ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n')
    )).join('\n')}`);
  }
  const encoded = Buffer.from(transpiled.outputText, 'utf8').toString('base64');
  return import(`data:text/javascript;base64,${encoded}`);
}

const {
  LEGACY_CONTRACT_PAGES,
  LEGACY_CONTRACT_VERSION,
  assertLegacyContract,
} = await loadRuntimeContractSchema();

async function fileExists(candidate) {
  try {
    return (await stat(candidate)).isFile();
  } catch {
    return false;
  }
}

function normalizeText(value) {
  return String(value ?? '').replace(/\s+/g, ' ').trim();
}

function compareCodePoints(left, right) {
  return left === right ? 0 : left < right ? -1 : 1;
}

async function loadJson(candidate) {
  return JSON.parse(await readFile(candidate, 'utf8'));
}

async function loadScenarioDefinitions(page) {
  const pageFixturesRoot = path.join(fixturesRoot, 'pages', page);
  const manifest = await loadJson(path.join(pageFixturesRoot, 'manifest.json'));
  const hasExplicitScenarios = Array.isArray(manifest.scenarios) && manifest.scenarios.length > 0;
  const scenarioNames = hasExplicitScenarios
    ? manifest.scenarios
    : ['default'];

  return Promise.all(scenarioNames.map(async (scenarioName) => {
    const fixturePath = path.join(pageFixturesRoot, `${scenarioName}.json`);
    return {
      id: `${page}-${scenarioName}`,
      fixture: await fileExists(fixturePath)
        ? `parity/fixtures/pages/${page}/${scenarioName}.json`
        : `parity/fixtures/pages/${page}/manifest.json#default`,
      definition: await fileExists(fixturePath) ? await loadJson(fixturePath) : {},
      coverageGaps: hasExplicitScenarios
        ? []
        : ['page fixture manifest has no explicit scenario list; only the initial deterministic render was observed'],
    };
  }));
}

function normalizeQuery(url) {
  const query = {};
  for (const key of [...new Set(url.searchParams.keys())].sort()) {
    query[key] = url.searchParams.getAll(key).sort();
  }
  return query;
}

function canonicalizeJson(value) {
  if (Array.isArray(value)) return value.map(canonicalizeJson);
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value)
        .sort(([left], [right]) => compareCodePoints(left, right))
        .map(([key, nested]) => [key, canonicalizeJson(nested)]),
    );
  }
  return value;
}

function sortObservedEvidence(items) {
  const unique = new Map();
  for (const item of items) {
    const canonical = JSON.stringify(canonicalizeJson(item));
    if (!unique.has(canonical)) unique.set(canonical, item);
  }
  return [...unique.entries()]
    .sort(([left], [right]) => compareCodePoints(left, right))
    .map(([, item]) => item);
}

function parseBody(rawBody) {
  if (rawBody === null || rawBody === '') return null;
  try {
    return JSON.parse(rawBody);
  } catch {
    return rawBody;
  }
}

function fixtureRouteMatches(fixture, method, url, body) {
  if (String(fixture.method ?? '').toUpperCase() !== method) return false;
  if (fixture.pathname !== url.pathname) return false;
  if (Object.hasOwn(fixture, 'query')) {
    if (JSON.stringify(fixture.query ?? {}) !== JSON.stringify(normalizeQuery(url))) return false;
  }
  if (Object.hasOwn(fixture, 'requestBody')) {
    if (JSON.stringify(fixture.requestBody) !== JSON.stringify(body)) return false;
  }
  return true;
}

function formatSseStream(events) {
  const source = Array.isArray(events) && events.length > 0
    ? events
    : [{ event: 'parity_contract_event', data: { captured: true }, id: 'contract-1' }];
  return source.map((event, index) => {
    const lines = [];
    if (event.id !== undefined) lines.push(`id: ${event.id}`);
    lines.push(`event: ${event.event}`);
    if (event.retry !== undefined) lines.push(`retry: ${event.retry}`);
    else if (index === 0) lines.push('retry: 60000');
    const data = typeof event.data === 'string' ? event.data : JSON.stringify(event.data);
    data.split(/\r?\n/).forEach((line) => lines.push(`data: ${line}`));
    return `${lines.join('\n')}\n\n`;
  }).join('');
}

function buildDashboardScenarioResponse(scenarioDefinition) {
  if (!Object.hasOwn(scenarioDefinition, 'data')) return null;
  return {
    status: 200,
    body: {
      success: true,
      data: scenarioDefinition.data,
      message: 'dashboard workbench loaded',
    },
  };
}

function deterministicCompatibilityBody(pathname, clock) {
  if (pathname === '/api/v2/auth/heartbeat') return { success: true };
  if (pathname === '/api/v2/auth/sse-token') {
    return { token: 'parity-sse-token', expires_at: clock.instant };
  }
  if ([
    '/api/v2/auth/users',
    '/api/v2/auth/roles',
    '/api/v2/auth/permissions',
    '/api/v2/dispatch/team-types',
    '/api/v2/dispatch/equipment-types',
    '/api/v2/dispatch/teams',
    '/api/v2/dispatch/equipment',
  ].includes(pathname)) return [];
  if (pathname === '/api/v2/system/flags') {
    return { success: true, data: { flags: [] } };
  }
  if (pathname === '/api/v2/reference/business-case-statuses') {
    return {
      success: true,
      data: [{ code: 'draft', label: '草稿', terminal: false, sort_order: 10 }],
    };
  }
  return {
    success: true,
    data: { items: [], total: 0, generated_at: clock.instant },
    message: 'synthetic response for legacy contract surface capture; see fixtureGaps',
  };
}

async function installBrowserDeterminism(page, clock, authUser, installAuth, suppressFixtureSseDisconnect) {
  const tokenPayload = {
    sub: authUser.id,
    username: authUser.username,
    email: authUser.email,
    is_admin: authUser.is_admin,
    permissions: authUser.permissions,
    department: authUser.department,
    pv: authUser.permission_version,
    iat: Math.floor(Date.parse(clock.instant) / 1000),
    exp: Math.floor(Date.parse(clock.instant) / 1000) + 3600,
    iss: 'fms-parity-fixture',
    aud: 'fms-web',
    type: 'access',
  };
  const encode = (value) => Buffer.from(JSON.stringify(value), 'utf8').toString('base64url');
  const token = `${encode({ alg: 'none', typ: 'JWT' })}.${encode(tokenPayload)}.fixture`;

  await page.addInitScript(({
    fixtureClock,
    accessToken,
    expiry,
    shouldInstallAuth,
    shouldSuppressFixtureSseDisconnect,
  }) => {
    globalThis.__fmsParityStorageMutations = [];
    globalThis.__fmsParityUrlChanges = [];

    const NativeDate = Date;
    const fixedTimestamp = Date.parse(fixtureClock.instant);
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

    let randomState = fixtureClock.random_seed >>> 0;
    const nextRandom = () => {
      randomState = (randomState * 1664525 + 1013904223) >>> 0;
      return randomState / 0x100000000;
    };
    Object.defineProperty(Math, 'random', { configurable: true, value: nextRandom });
    let uuidIndex = 0;
    try {
      Object.defineProperty(globalThis.crypto, 'randomUUID', {
        configurable: true,
        value: () => {
          const configured = fixtureClock.uuid_sequence[uuidIndex];
          uuidIndex += 1;
          return configured ?? `10000000-0000-4000-8000-${uuidIndex.toString(16).padStart(12, '0')}`;
        },
      });
      Object.defineProperty(globalThis.crypto, 'getRandomValues', {
        configurable: true,
        value: (array) => {
          const bytes = new Uint8Array(array.buffer, array.byteOffset, array.byteLength);
          for (let index = 0; index < bytes.length; index += 1) bytes[index] = (index * 37 + 17) % 256;
          return array;
        },
      });
    } catch {
      // Math.random remains deterministic when a browser exposes non-configurable Crypto methods.
    }

    const storageSetItem = Storage.prototype.setItem;
    const storageRemoveItem = Storage.prototype.removeItem;
    const storageClear = Storage.prototype.clear;
    const storageName = (storage) => storage === localStorage ? 'localStorage' : 'sessionStorage';
    Storage.prototype.setItem = function setItem(key, value) {
      globalThis.__fmsParityStorageMutations.push({
        storage: storageName(this), operation: 'set', key: String(key), value: String(value),
      });
      return storageSetItem.call(this, key, value);
    };
    Storage.prototype.removeItem = function removeItem(key) {
      globalThis.__fmsParityStorageMutations.push({
        storage: storageName(this), operation: 'remove', key: String(key), value: null,
      });
      return storageRemoveItem.call(this, key);
    };
    Storage.prototype.clear = function clear() {
      globalThis.__fmsParityStorageMutations.push({
        storage: storageName(this), operation: 'clear', key: null, value: null,
      });
      return storageClear.call(this);
    };

    const recordUrlChange = (kind, before) => {
      queueMicrotask(() => {
        const after = location.href;
        if (after !== before) globalThis.__fmsParityUrlChanges.push({ kind, from: before, to: after });
      });
    };
    const nativePushState = history.pushState.bind(history);
    const nativeReplaceState = history.replaceState.bind(history);
    history.pushState = (...args) => {
      const before = location.href;
      const result = nativePushState(...args);
      recordUrlChange('pushState', before);
      return result;
    };
    history.replaceState = (...args) => {
      const before = location.href;
      const result = nativeReplaceState(...args);
      recordUrlChange('replaceState', before);
      return result;
    };
    addEventListener('hashchange', (event) => {
      globalThis.__fmsParityUrlChanges.push({ kind: 'hashchange', from: event.oldURL, to: event.newURL });
    });

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

    if (shouldInstallAuth) {
      sessionStorage.setItem('access_token', accessToken);
      sessionStorage.setItem('token_type', 'bearer');
      sessionStorage.setItem('token_expires_at', String(expiry));
      sessionStorage.setItem('session_secret', 'parity-session-secret');
      globalThis.__fmsParityStorageMutations.length = 0;
    }

    document.addEventListener('DOMContentLoaded', () => {
      const style = document.createElement('style');
      style.id = 'fms-parity-contract-determinism';
      style.textContent = '*,*::before,*::after{animation:none!important;caret-color:transparent!important;scroll-behavior:auto!important;transition:none!important;}';
      document.head.append(style);
    }, { once: true });
  }, {
    fixtureClock: clock,
    accessToken: token,
    expiry: Date.parse(clock.instant) + 3600000,
    shouldInstallAuth: installAuth,
    shouldSuppressFixtureSseDisconnect: suppressFixtureSseDisconnect,
  });
}

async function observeScenario(browser, baseUrl, pageName, scenario, sharedFixtures) {
  const context = await browser.newContext({
    viewport: fixedCaptureViewport,
    locale: sharedFixtures.clock.locale,
    timezoneId: sharedFixtures.clock.timezone,
    colorScheme: 'light',
    reducedMotion: 'reduce',
    serviceWorkers: 'block',
  });
  const page = await context.newPage();
  const apiRequests = [];
  const fixtureGaps = [];
  const sseSubscriptions = [];
  const consoleErrors = [];
  const pageErrors = [];
  const documentNavigations = [];
  let lastMainFrameUrl = 'about:blank';
  const pendingApiRequests = new Set();
  let lastFixtureActivity = Date.now();
  const installAuth = pageName !== 'login';
  await installBrowserDeterminism(
    page,
    sharedFixtures.clock,
    sharedFixtures.authUser,
    installAuth,
    (scenario.definition.sse_streams ?? []).length > 0,
  );

  const isApiRequest = (request) => new URL(request.url()).pathname.startsWith('/api/v2/');
  page.on('request', (request) => {
    if (!isApiRequest(request)) return;
    pendingApiRequests.add(request);
    lastFixtureActivity = Date.now();
  });
  const finishApiRequest = (request) => {
    if (!isApiRequest(request)) return;
    pendingApiRequests.delete(request);
    lastFixtureActivity = Date.now();
  };
  page.on('requestfinished', finishApiRequest);
  page.on('requestfailed', finishApiRequest);
  page.on('framenavigated', (frame) => {
    if (frame !== page.mainFrame()) return;
    const nextUrl = frame.url();
    if (lastMainFrameUrl !== 'about:blank' && nextUrl !== lastMainFrameUrl) {
      documentNavigations.push({ kind: 'navigation', from: lastMainFrameUrl, to: nextUrl });
    }
    lastMainFrameUrl = nextUrl;
  });

  page.on('console', (message) => {
    if (message.type() === 'error') consoleErrors.push(normalizeText(message.text()));
  });
  page.on('pageerror', (error) => pageErrors.push(normalizeText(error.message)));

  await page.route('**/*', async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const method = request.method().toUpperCase();
    const body = parseBody(request.postData());

    if (url.pathname.startsWith('/api/v2/')) {
      apiRequests.push({ method, pathname: url.pathname, query: normalizeQuery(url), body });

      if (method === 'GET' && url.pathname === '/api/v2/auth/me') {
        await route.fulfill({ status: 200, json: sharedFixtures.authUser });
        return;
      }

      if (url.pathname === '/api/v2/auth/heartbeat' || url.pathname === '/api/v2/auth/sse-token') {
        await route.fulfill({ status: 200, json: deterministicCompatibilityBody(url.pathname, sharedFixtures.clock) });
        return;
      }

      const sseFixture = (scenario.definition.sse_streams ?? []).find((fixture) => (
        fixture.pathname === url.pathname
        && JSON.stringify(fixture.query ?? {}) === JSON.stringify(normalizeQuery(url))
      ));
      const isSseRequest = Boolean(sseFixture)
        || request.headers().accept?.includes('text/event-stream')
        || /(?:sse|stream|events)/i.test(url.pathname);
      if (isSseRequest) {
        sseSubscriptions.push({
          url: `${url.pathname}${url.search}`,
          pathname: url.pathname,
          query: normalizeQuery(url),
          fixtureId: sseFixture?.id ?? null,
          eventTypes: [...new Set((sseFixture?.events ?? []).map((event) => event.event))].sort(compareCodePoints),
        });
        await new Promise((resolve) => setTimeout(resolve, 50));
        await route.fulfill({
          status: 200,
          headers: {
            'Cache-Control': 'no-cache, no-store',
            'Content-Type': 'text/event-stream; charset=utf-8',
          },
          body: formatSseStream(sseFixture?.events),
        });
        return;
      }

      const fixture = (scenario.definition.routes ?? []).find((candidate) => (
        fixtureRouteMatches(candidate, method, url, body)
      ));
      if (fixture) {
        const response = fixture.response ?? {};
        await route.fulfill({
          status: response.status ?? 200,
          headers: response.headers,
          body: typeof response.body === 'string' ? response.body : undefined,
          json: typeof response.body === 'string' || response.body === undefined ? undefined : response.body,
        });
        return;
      }

      if (pageName === 'dashboard' && url.pathname === '/api/v2/dashboard/workbench') {
        const dashboardResponse = buildDashboardScenarioResponse(scenario.definition);
        if (dashboardResponse) {
          await route.fulfill({ status: dashboardResponse.status, json: dashboardResponse.body });
          return;
        }
      }

      fixtureGaps.push({
        method,
        pathname: url.pathname,
        query: normalizeQuery(url),
        body,
        reason: 'no explicit common or page scenario fixture matched; a synthetic response was used only to expose the legacy surface',
      });
      await route.fulfill({
        status: 200,
        json: deterministicCompatibilityBody(url.pathname, sharedFixtures.clock),
      });
      return;
    }

    if (url.origin !== baseUrl) {
      await route.fulfill({ status: 204, body: '' });
      return;
    }

    await route.continue();
  });

  const requestedUrl = `${baseUrl}/frontend/html/${pageName}.html`;
  let navigationError = null;
  try {
    await page.goto(requestedUrl, { waitUntil: 'domcontentloaded', timeout: 15000 });
  } catch (error) {
    navigationError = normalizeText(error instanceof Error ? error.message : String(error));
  }

  try {
    await page.evaluate(async () => {
      if (document.fonts?.ready) await document.fonts.ready;
    });
    const expectedPanels = Array.isArray(scenario.definition.expected_panels)
      ? scenario.definition.expected_panels
      : [];
    for (const selector of expectedPanels) {
      await page.locator(selector).first().waitFor({ state: 'attached', timeout: 5000 });
    }

    const fixtureDeadline = Date.now() + 15000;
    while (pendingApiRequests.size > 0 || Date.now() - lastFixtureActivity < 500) {
      if (Date.now() >= fixtureDeadline) {
        throw new Error(`fixture requests did not become idle; ${pendingApiRequests.size} request(s) remain`);
      }
      await page.waitForTimeout(50);
    }
    await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(
      () => requestAnimationFrame(resolve),
    )));
  } catch (error) {
    pageErrors.push(normalizeText(error instanceof Error ? error.message : String(error)));
  }

  const observation = await page.evaluate(() => {
    const compactText = (value) => String(value ?? '').replace(/\s+/g, ' ').trim().slice(0, 500);
    const visible = (element) => {
      const style = getComputedStyle(element);
      const rect = element.getBoundingClientRect();
      return style.display !== 'none' && style.visibility !== 'hidden' && rect.width > 0 && rect.height > 0;
    };
    const selectorFor = (element) => {
      if (element.id) return `#${CSS.escape(element.id)}`;
      const testId = element.getAttribute('data-testid');
      if (testId) return `[data-testid="${CSS.escape(testId)}"]`;
      const aiEntry = element.getAttribute('data-ai-entry');
      if (aiEntry) return `[data-ai-entry="${CSS.escape(aiEntry)}"]`;
      const name = element.getAttribute('name');
      if (name && document.querySelectorAll(`${element.tagName.toLowerCase()}[name="${CSS.escape(name)}"]`).length === 1) {
        return `${element.tagName.toLowerCase()}[name="${CSS.escape(name)}"]`;
      }
      const parts = [];
      let current = element;
      while (current && current !== document.documentElement) {
        const tag = current.tagName.toLowerCase();
        const siblings = current.parentElement
          ? [...current.parentElement.children].filter((candidate) => candidate.tagName === current.tagName)
          : [];
        const suffix = siblings.length > 1 ? `:nth-of-type(${siblings.indexOf(current) + 1})` : '';
        parts.unshift(`${tag}${suffix}`);
        current = current.parentElement;
      }
      return parts.join(' > ');
    };
    const controlFor = (element) => ({
      selector: selectorFor(element),
      tag: element.tagName.toLowerCase(),
      type: element.getAttribute('type') ?? element.getAttribute('role') ?? '',
      text: compactText(element.textContent || element.value || element.getAttribute('placeholder')),
      name: element.getAttribute('name'),
      ariaLabel: element.getAttribute('aria-label'),
      disabled: Boolean(element.disabled || element.getAttribute('aria-disabled') === 'true'),
    });
    const overlayFor = (element) => ({
      selector: selectorFor(element),
      title: compactText(element.querySelector('h1,h2,h3,.modal-title,.drawer-title')?.textContent),
      visible: visible(element),
    });

    const regions = [...document.querySelectorAll('header,nav,main,aside,section,footer,[role="region"]')]
      .map((element) => ({
        selector: selectorFor(element),
        tag: element.tagName.toLowerCase(),
        role: element.getAttribute('role'),
        text: compactText(element.getAttribute('aria-label') || element.querySelector('h1,h2,h3')?.textContent),
        visible: visible(element),
      }));
    const headings = [...document.querySelectorAll('h1,h2,h3,h4,h5,h6')]
      .filter(visible)
      .map((element) => ({
        selector: selectorFor(element),
        level: Number(element.tagName.slice(1)),
        text: compactText(element.textContent),
      }));
    const controls = [...document.querySelectorAll('button,input,select,textarea,[role="button"],[contenteditable="true"]')]
      .filter(visible)
      .map(controlFor);
    const labels = [...document.querySelectorAll('label')]
      .filter(visible)
      .map((element) => ({
        selector: selectorFor(element),
        text: compactText(element.textContent),
        for: element.getAttribute('for'),
      }));
    const forms = [...document.querySelectorAll('form')].map((element) => ({
      selector: selectorFor(element),
      method: (element.getAttribute('method') ?? 'get').toUpperCase(),
      action: element.getAttribute('action') ?? '',
      fields: [...element.querySelectorAll('input,select,textarea,button')].map(selectorFor),
    }));
    const tables = [...document.querySelectorAll('table')].map((element) => ({
      selector: selectorFor(element),
      columns: [...element.querySelectorAll('thead th')].map((heading) => compactText(heading.textContent)),
    }));
    const tabs = [...document.querySelectorAll('[role="tab"],button.tab,button.page-tab,.tabs button')]
      .filter(visible)
      .map(controlFor);
    const dialogs = [...document.querySelectorAll('dialog,[role="dialog"],.modal')].map(overlayFor);
    const drawers = [...document.querySelectorAll('.drawer,.side-drawer,.offcanvas,[data-drawer]')].map(overlayFor);
    const links = [...document.querySelectorAll('a[href]')]
      .filter(visible)
      .map((element) => ({
        selector: selectorFor(element),
        text: compactText(element.textContent),
        href: element.getAttribute('href') ?? '',
      }));
    const stableSelectors = [...document.querySelectorAll('[id],[data-testid],[data-ai-entry]')]
      .map((element) => {
        if (element.id) return `#${CSS.escape(element.id)}`;
        if (element.hasAttribute('data-testid')) return `[data-testid="${CSS.escape(element.getAttribute('data-testid'))}"]`;
        return `[data-ai-entry="${CSS.escape(element.getAttribute('data-ai-entry'))}"]`;
      });
    const permissionAttributes = ['data-permission', 'data-permissions', 'data-require-permission', 'data-admin-only'];
    const permissionRules = [];
    for (const attribute of permissionAttributes) {
      document.querySelectorAll(`[${attribute}]`).forEach((element) => permissionRules.push({
        selector: selectorFor(element),
        attribute,
        value: element.getAttribute(attribute) ?? '',
      }));
    }
    return {
      surface: {
        regions,
        headings,
        controls,
        labels,
        forms,
        tables,
        tabs,
        dialogs,
        drawers,
        links,
        stableSelectors: [...new Set(stableSelectors)].sort(),
        permissionRules,
      },
      storageMutations: globalThis.__fmsParityStorageMutations ?? [],
      urlChanges: globalThis.__fmsParityUrlChanges ?? [],
    };
  });

  const requestedPathname = `/frontend/html/${pageName}.html`;
  const redirected = new URL(page.url()).pathname !== requestedPathname;
  if (redirected) {
    observation.surface = {
      regions: [], headings: [], controls: [], labels: [], forms: [], tables: [], tabs: [],
      dialogs: [], drawers: [], links: [], stableSelectors: [], permissionRules: [],
    };
    observation.storageMutations = [];
  }
  observation.urlChanges = [...observation.urlChanges, ...documentNavigations];

  const finalConsoleErrors = [...consoleErrors, ...pageErrors]
    .filter((message) => !redirected || !/Execution context was destroyed/i.test(message));
  if (navigationError) finalConsoleErrors.push(navigationError);
  for (const subscription of sseSubscriptions) {
    const matched = (scenario.definition.sse_streams ?? []).some((fixture) => (
      fixture.pathname === subscription.pathname
      && JSON.stringify(fixture.query ?? {}) === JSON.stringify(subscription.query)
    ));
    if (!matched) {
      fixtureGaps.push({
        method: 'GET',
        pathname: subscription.pathname,
        query: subscription.query,
        body: null,
        reason: 'no explicit named SSE fixture matched this observed EventSource subscription',
      });
    }
  }
  const normalizeCapturedOrigin = (message) => message.replaceAll(baseUrl, '<legacy-origin>');
  await context.close();

  return {
    surface: observation.surface,
    scenario: {
      id: scenario.id,
      fixture: scenario.fixture,
      captureStatus: finalConsoleErrors.length > 0
        ? 'rendered-with-errors'
        : fixtureGaps.length > 0 || scenario.coverageGaps.length > 0
          ? 'rendered-with-fixture-gaps'
          : 'rendered',
      coverageGaps: scenario.coverageGaps,
      expectedHttpStatuses: [...new Set((scenario.definition.routes ?? [])
        .map((route) => Number(route.response?.status ?? 200))
        .filter((status) => status >= 400 && status <= 599))].sort((left, right) => left - right),
      expectedLegacyErrors: Array.isArray(scenario.definition.expected_legacy_errors)
        ? [...new Set(scenario.definition.expected_legacy_errors)].sort(compareCodePoints)
        : [],
      apiRequests: sortObservedEvidence(apiRequests),
      fixtureGaps: sortObservedEvidence(fixtureGaps),
      storageMutations: sortObservedEvidence(observation.storageMutations),
      urlChanges: sortObservedEvidence(observation.urlChanges.map((change) => ({
        ...change,
        from: normalizeCapturedOrigin(change.from),
        to: normalizeCapturedOrigin(change.to),
      }))),
      sseSubscriptions: sortObservedEvidence(sseSubscriptions),
      consoleErrors: [...new Set(finalConsoleErrors.map(normalizeCapturedOrigin))].sort().slice(0, 100),
    },
  };
}

function mergeSurfaceContracts(surfaces) {
  const keys = [
    'regions',
    'headings',
    'controls',
    'labels',
    'forms',
    'tables',
    'tabs',
    'dialogs',
    'drawers',
    'links',
    'stableSelectors',
    'permissionRules',
  ];
  return Object.fromEntries(keys.map((key) => {
    const seen = new Set();
    const merged = [];
    for (const surface of surfaces) {
      for (const item of surface[key]) {
        const identity = typeof item === 'string' ? item : JSON.stringify(item);
        if (seen.has(identity)) continue;
        seen.add(identity);
        merged.push(item);
      }
    }
    return [key, merged];
  }));
}

async function listenOnEphemeralPort(server) {
  await new Promise((resolve, reject) => {
    const onError = (error) => reject(error);
    server.once('error', onError);
    server.listen(0, '127.0.0.1', () => {
      server.off('error', onError);
      resolve();
    });
  });
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('Failed to resolve legacy capture server port.');
  return `http://127.0.0.1:${address.port}`;
}

function closeServer(server) {
  return new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
}

function selectedPages(argumentsList) {
  if (argumentsList.length === 0) return [...LEGACY_CONTRACT_PAGES];
  const requested = argumentsList.map((argument) => argument.replace(/\.html$/i, ''));
  for (const page of requested) {
    if (!LEGACY_CONTRACT_PAGES.includes(page)) {
      throw new Error(`Unknown legacy contract page "${page}". Expected one of the fixed 21-page inventory.`);
    }
  }
  return requested;
}

async function main() {
  const validation = await validateLegacyRoot();
  const htmlPageNames = LEGACY_HTML_FILES.map((filename) => filename.replace(/\.html$/i, ''));
  if (JSON.stringify(htmlPageNames) !== JSON.stringify([...LEGACY_CONTRACT_PAGES])) {
    throw new Error('Legacy root inventory and TypeScript contract inventory have diverged.');
  }

  const pages = selectedPages(process.argv.slice(2));
  const clock = await loadJson(path.join(fixturesRoot, 'common', 'clock.json'));
  const authUser = await loadJson(path.join(fixturesRoot, 'common', 'auth-admin.json'));
  const server = createLegacyServer(validation.root);
  const baseUrl = await listenOnEphemeralPort(server);
  const browser = await chromium.launch({ headless: true });
  await mkdir(contractsRoot, { recursive: true });

  try {
    for (const pageName of pages) {
      const { sourceFiles: _sourceFiles, ...source } = await extractLegacySourceContract(
        validation.root,
        `html/${pageName}.html`,
      );
      const scenarioDefinitions = await loadScenarioDefinitions(pageName);
      const observations = [];
      for (const scenario of scenarioDefinitions) {
        observations.push(await observeScenario(browser, baseUrl, pageName, scenario, { clock, authUser }));
      }

      const contract = {
        contractVersion: LEGACY_CONTRACT_VERSION,
        page: pageName,
        generatedAt: clock.instant,
        source,
        surface: mergeSurfaceContracts(observations.map((observation) => observation.surface)),
        scenarios: observations.map((observation) => observation.scenario),
        approvedExceptions: [...new Set(scenarioDefinitions.flatMap((scenario) => (
          Array.isArray(scenario.definition.approved_exception_ids)
            ? scenario.definition.approved_exception_ids
            : []
        )))].sort(compareCodePoints),
      };
      assertLegacyContract(contract);
      const outputPath = path.join(contractsRoot, `${pageName}.json`);
      await writeFile(outputPath, `${JSON.stringify(contract, null, 2)}\n`, 'utf8');
      console.log(`Captured ${pageName}: ${contract.scenarios.length} scenario(s), ${contract.surface.controls.length} visible control(s).`);
    }
  } finally {
    await browser.close();
    await closeServer(server);
  }

  console.log(`Validated and wrote ${pages.length} legacy contract file(s) to ${contractsRoot}.`);
}

await main().catch((error) => {
  console.error(error instanceof Error ? error.stack ?? error.message : String(error));
  process.exitCode = 1;
});
