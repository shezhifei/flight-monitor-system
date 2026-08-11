import { test as base, expect } from '@playwright/test';
import { createRequire } from 'node:module';

import {
  assertNoUnknownApiRequests,
  installApiFixtures,
  type ApiRouteFixture,
  type RecordedApiRequest,
  type UnknownApiRequest,
} from './api.fixture';
import {
  AUTH_USERS,
  createAuthMeRoute,
  installLegacyAuthStorage,
  type AuthRole,
  type AuthUserFixture,
} from './auth.fixture';
import { installSseFixtures, type SseStreamFixture } from './sse.fixture';

const require = createRequire(import.meta.url);
const clockJson = require('../../parity/fixtures/common/clock.json') as unknown;

export interface ClockFixture {
  instant: string;
  timezone: string;
  locale: string;
  random_seed: number;
  uuid_sequence: string[];
}

interface ParityOptions {
  authRole: AuthRole;
  installAuthStorage: boolean;
  apiFixtureSet: { routes: ApiRouteFixture[] };
  sseFixtureSet: { streams: SseStreamFixture[] };
}

interface ParityFixtures {
  authUser: AuthUserFixture;
  clock: ClockFixture;
  requestLog: RecordedApiRequest[];
  unknownApiRequests: UnknownApiRequest[];
}

const clock = clockJson as ClockFixture;

async function installDeterministicBrowser(page: Parameters<typeof installApiFixtures>[0]): Promise<void> {
  await page.addInitScript((fixtureClock: ClockFixture) => {
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

    let uuidIndex = 0;
    const deterministicUuid = () => {
      const configured = fixtureClock.uuid_sequence[uuidIndex];
      uuidIndex += 1;
      if (configured) return configured;
      const suffix = uuidIndex.toString(16).padStart(12, '0').slice(-12);
      return `10000000-0000-4000-8000-${suffix}`;
    };
    try {
      Object.defineProperty(globalThis.crypto, 'randomUUID', {
        configurable: true,
        value: deterministicUuid,
      });
      Object.defineProperty(globalThis.crypto, 'getRandomValues', {
        configurable: true,
        value: <T extends ArrayBufferView | null>(array: T): T => {
          if (!array) throw new TypeError('Expected an ArrayBuffer view');
          const bytes = new Uint8Array(array.buffer, array.byteOffset, array.byteLength);
          for (let index = 0; index < bytes.length; index += 1) bytes[index] = Math.floor(nextRandom() * 256);
          return array;
        },
      });
    } catch {
      // Older browser engines may expose non-configurable Crypto methods; Math.random remains deterministic.
    }

    document.addEventListener('DOMContentLoaded', () => {
      const style = document.createElement('style');
      style.id = 'fms-parity-determinism';
      style.textContent = `
        *, *::before, *::after {
          animation-delay: 0s !important;
          animation-duration: 0s !important;
          animation-iteration-count: 1 !important;
          caret-color: transparent !important;
          scroll-behavior: auto !important;
          transition-delay: 0s !important;
          transition-duration: 0s !important;
        }
      `;
      document.head.append(style);
    }, { once: true });
  }, clock);
}

export const test = base.extend<ParityFixtures & ParityOptions>({
  timezoneId: clock.timezone,
  locale: clock.locale,
  colorScheme: 'light',
  contextOptions: { reducedMotion: 'reduce' },
  serviceWorkers: 'block',
  authRole: ['admin', { option: true }],
  installAuthStorage: [true, { option: true }],
  apiFixtureSet: [{ routes: [] }, { option: true }],
  sseFixtureSet: [{ streams: [] }, { option: true }],
  clock: async ({ browserName: _browserName }, use) => {
    await use(clock);
  },
  authUser: async ({ authRole }, use) => {
    await use(AUTH_USERS[authRole]);
  },
  unknownApiRequests: async ({ browserName: _browserName }, use) => {
    await use([]);
  },
  requestLog: [async ({
    page,
    authUser,
    installAuthStorage: shouldInstallAuthStorage,
    apiFixtureSet,
    sseFixtureSet,
    unknownApiRequests,
  }, use) => {
    await installDeterministicBrowser(page);
    if (shouldInstallAuthStorage) {
      await installLegacyAuthStorage(page, authUser, clock.instant);
    }
    const requests: RecordedApiRequest[] = [];
    const configuredApiRoutes = apiFixtureSet.routes;
    const configuredSseStreams = sseFixtureSet.streams;
    await installApiFixtures(
      page,
      [createAuthMeRoute(authUser), ...configuredApiRoutes],
      requests,
      unknownApiRequests,
    );
    await installSseFixtures(page, configuredSseStreams, requests);
    await use(requests);
    assertNoUnknownApiRequests(unknownApiRequests);
  }, { auto: true }],
});

export { expect };
