import { defineConfig, devices } from '@playwright/test';

const localPreviewBase = 'http://127.0.0.1:4173';
const defaultBaseUrl =
  process.env.BASE_URL
  || (process.env.CI ? 'http://localhost:18443' : localPreviewBase);

/**
 * Local default targets Vite preview (built MPA at /frontend/*) so parity e2e
 * can run without docker. Override with BASE_URL for full-stack smoke.
 */
export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: 'html',
  use: {
    baseURL: defaultBaseUrl,
    ignoreHTTPSErrors: true,
    trace: 'on-first-retry',
  },
  webServer: process.env.BASE_URL
    ? undefined
    : [
        {
          command: 'npx vite preview --host 127.0.0.1 --port 4173 --strictPort',
          url: `${localPreviewBase}/frontend/login.html`,
          reuseExistingServer: !process.env.CI,
          timeout: 180_000,
        },
        {
          command: 'npm run parity:serve-legacy',
          url: 'http://127.0.0.1:3100/frontend/html/login.html',
          reuseExistingServer: !process.env.CI,
          timeout: 180_000,
        },
      ],
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
});
