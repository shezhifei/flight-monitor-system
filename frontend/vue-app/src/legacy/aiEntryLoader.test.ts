/**
 * @vitest-environment jsdom
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  EntryNotFoundError,
  EntryBootstrapError,
  MANIFEST_URL,
  ManifestLoadError,
  __resetAiEntryLoaderForTests,
  loadAiReactEntry,
  waitForAiEntryBootstrap,
} from './aiEntryLoader';

type FetchMock = ReturnType<typeof vi.fn>;

function buildHost(entryName: string): HTMLElement {
  const host = document.createElement('div');
  host.id = 'ai-react-root';
  host.setAttribute('data-ai-entry', entryName);
  document.body.appendChild(host);
  return host;
}

function mockFetchOk(body: unknown): FetchMock {
  const fn = vi.fn().mockResolvedValue({
    ok: true,
    status: 200,
    json: () => Promise.resolve(body),
  });
  vi.stubGlobal('fetch', fn);
  return fn;
}

function mockFetchError(status: number): FetchMock {
  const fn = vi.fn().mockResolvedValue({
    ok: false,
    status,
    json: () => Promise.resolve({}),
  });
  vi.stubGlobal('fetch', fn);
  return fn;
}

beforeEach(() => {
  document.head.innerHTML = '';
  document.body.innerHTML = '';
  __resetAiEntryLoaderForTests();
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('aiEntryLoader (TypeScript)', () => {
  it('fetches the manifest from the canonical URL and injects matching CSS', async () => {
    const fetchMock = mockFetchOk({
      'src/entries/nl_query.tsx': {
        file: 'assets/nl_query-abc.js',
        isEntry: true,
        css: ['assets/nl_query-abc.css'],
      },
    });
    const host = buildHost('nl_query');

    // jsdom cannot execute a real `import(<httpUrl>)`, so the import step
    // rejects — we assert the side effects that occur strictly BEFORE the
    // import: manifest fetch and stylesheet injection.
    await loadAiReactEntry(host, 'nl_query').catch(() => {});

    expect(fetchMock).toHaveBeenCalledWith(MANIFEST_URL, { credentials: 'same-origin' });
    const link = document.head.querySelector(
      'link[href="/frontend/static/ai/assets/nl_query-abc.css"]',
    );
    expect(link).not.toBeNull();
  });

  it('throws ManifestLoadError when the manifest request fails', async () => {
    mockFetchError(404);
    const host = buildHost('ai_monitor');

    await expect(loadAiReactEntry(host, 'ai_monitor')).rejects.toBeInstanceOf(ManifestLoadError);
    expect(host.getAttribute('data-ai-loader')).toBe('error');
  });

  it('throws EntryNotFoundError when manifest does not contain the requested entry', async () => {
    mockFetchOk({});
    const host = buildHost('ai_monitor');

    await expect(loadAiReactEntry(host, 'ai_monitor')).rejects.toBeInstanceOf(EntryNotFoundError);
    expect(host.getAttribute('data-ai-loader')).toBe('error');
  });

  it('does not render any "功能正在开发中" placeholder', async () => {
    mockFetchError(404);
    const host = buildHost('llm_eval_lab');

    await loadAiReactEntry(host, 'llm_eval_lab').catch(() => {});

    expect(document.body.innerHTML).not.toMatch(/功能正在开发中/);
    expect(document.body.innerHTML).not.toMatch(/legacy-dev-placeholder/);
  });

  it('resolves entries by key, src, or hashed file pattern', async () => {
    mockFetchOk({
      'src/entries/dispatch_board_ai.tsx': {
        file: 'assets/dispatch_board_ai-xyz.js',
        isEntry: true,
        css: ['assets/dispatch_board_ai-xyz.css'],
      },
    });
    const host = buildHost('dispatch_board_ai');
    await loadAiReactEntry(host, 'dispatch_board_ai').catch(() => {});
    expect(
      document.head.querySelector(
        'link[href="/frontend/static/ai/assets/dispatch_board_ai-xyz.css"]',
      ),
    ).not.toBeNull();
  });

  it('rejects manifest assets that point outside the static AI asset directory', async () => {
    mockFetchOk({
      'src/entries/nl_query.tsx': {
        file: 'https://evil.example/nl_query.js',
        isEntry: true,
        css: ['//evil.example/nl_query.css'],
      },
    });
    const host = buildHost('nl_query');

    await expect(loadAiReactEntry(host, 'nl_query')).rejects.toThrow(/same-origin static asset path|static\/ai/);

    expect(host.getAttribute('data-ai-loader')).toBe('error');
    expect(document.head.querySelector('link[href^="https://evil.example"]')).toBeNull();
  });

  it('waits until React reports an authenticated commit', async () => {
    const host = buildHost('nl_query');
    const completion = waitForAiEntryBootstrap(host, 'nl_query', 100);

    host.setAttribute('data-ai-bootstrap', 'bootstrapping');
    host.setAttribute('data-ai-mounted', 'true');
    host.setAttribute('data-ai-bootstrap', 'ready');

    await expect(completion).resolves.toBeUndefined();
  });

  it('rejects React bootstrap errors and timeouts instead of marking the entry loaded', async () => {
    const errorHost = buildHost('ai_monitor');
    const errorCompletion = waitForAiEntryBootstrap(errorHost, 'ai_monitor', 100);
    errorHost.setAttribute('data-ai-bootstrap', 'error');

    await expect(errorCompletion).rejects.toBeInstanceOf(EntryBootstrapError);

    const timeoutHost = buildHost('llm_eval_lab');
    await expect(waitForAiEntryBootstrap(timeoutHost, 'llm_eval_lab', 1))
      .rejects.toThrow(/timed out/);
  });
});
