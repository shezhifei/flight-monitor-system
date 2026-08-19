/**
 * Typed loader for React AI entries built under `frontend/ai-react`.
 *
 * The Vue shell owns its HTML; the React AI bundle is built separately and
 * published under `/frontend/static/ai/`. This loader is the canonical
 * production replacement for the retired legacy `ai_entry_loader.js` script.
 *
 * On manifest or asset failure, callers receive a precise typed error so the
 * surrounding Vue surface can render an actionable diagnostic. The loader
 * itself never renders a legacy "in development" placeholder.
 */

import { assertStaticAssetUrl } from '@/lib/url-guard';

export type AiEntryName =
  | 'ai_monitor'
  | 'nl_query'
  | 'dispatch_board_ai';

export const AI_ENTRY_LABELS: Record<AiEntryName, string> = {
  ai_monitor: 'AI 监控',
  nl_query: '自然语言查询',
  dispatch_board_ai: '派工 AI 助手',
};

export const MANIFEST_URL = '/frontend/static/ai/manifest.json';
const ASSET_BASE = '/frontend/static/ai/';

export class ManifestLoadError extends Error {
  public readonly status?: number;
  constructor(message: string, status?: number) {
    super(message);
    this.name = 'ManifestLoadError';
    this.status = status;
  }
}

export class EntryNotFoundError extends Error {
  public readonly entryName: string;
  constructor(entryName: string) {
    super(`AI entry "${entryName}" was not found in the manifest.`);
    this.name = 'EntryNotFoundError';
    this.entryName = entryName;
  }
}

export class EntryBootstrapError extends Error {
  public readonly entryName: string;
  constructor(entryName: string, reason: string) {
    super(`AI entry "${entryName}" did not complete bootstrap: ${reason}`);
    this.name = 'EntryBootstrapError';
    this.entryName = entryName;
  }
}

type ManifestEntry = {
  file?: string;
  src?: string;
  isEntry?: boolean;
  imports?: string[];
  css?: string[];
};

type Manifest = Record<string, ManifestEntry>;

const loadedJs = new Set<string>();
const loadedCss = new Set<string>();
let manifestPromise: Promise<Manifest> | null = null;

function normalizeAssetPath(path: string): string {
  const text = String(path || '').trim().replace(/\\/g, '/');
  if (!text) return '';
  if (/^[a-z][a-z0-9+.-]*:/i.test(text) || text.startsWith('//')) {
    throw new Error('AI asset path must be a same-origin static asset path.');
  }
  const candidate = text.startsWith('/')
    ? text
    : text.startsWith(ASSET_BASE.replace(/^\//, ''))
      ? `/${text}`
      : `${ASSET_BASE}${text.replace(/^\/+/, '')}`;
  return assertStaticAssetUrl(candidate, ASSET_BASE);
}

function loadManifest(force = false): Promise<Manifest> {
  if (force) {
    manifestPromise = null;
  }
  if (!manifestPromise) {
    manifestPromise = fetch(MANIFEST_URL, { credentials: 'same-origin' })
      .then((response) => {
        if (!response.ok) {
          throw new ManifestLoadError(`Manifest request failed: ${response.status}`, response.status);
        }
        return response.json() as Promise<Manifest>;
      })
      .catch((error: unknown) => {
        manifestPromise = null;
        if (error instanceof ManifestLoadError) throw error;
        throw new ManifestLoadError(
          error instanceof Error ? error.message : 'Manifest network error',
        );
      });
  }
  return manifestPromise;
}

function resolveEntry(manifest: Manifest, entryName: string): ManifestEntry | null {
  const directKeys = [
    entryName,
    `${entryName}.js`,
    `src/entries/${entryName}.tsx`,
    `src/entries/${entryName}.ts`,
  ];
  for (const key of directKeys) {
    if (manifest[key]) return manifest[key];
  }
  for (const [key, item] of Object.entries(manifest)) {
    if (!item || typeof item !== 'object') continue;
    if (!item.isEntry) continue;
    const src = String(item.src || '');
    const file = String(item.file || '');
    if (
      key.includes(entryName) ||
      src.includes(entryName) ||
      file.includes(`${entryName}-`) ||
      file.includes(`${entryName}.`)
    ) {
      return item;
    }
  }
  return null;
}

function appendStyles(manifest: Manifest, entry: ManifestEntry, visited = new Set<string>()): void {
  const id = String(entry.file || '');
  if (id) {
    if (visited.has(id)) return;
    visited.add(id);
  }

  const imports = Array.isArray(entry.imports) ? entry.imports : [];
  for (const key of imports) {
    const dep = manifest[key];
    if (dep) appendStyles(manifest, dep, visited);
  }

  const cssFiles = Array.isArray(entry.css) ? entry.css : [];
  for (const cssFile of cssFiles) {
    const href = normalizeAssetPath(String(cssFile || ''));
    if (!href || loadedCss.has(href)) continue;
    const link = document.createElement('link');
    link.rel = 'stylesheet';
    link.href = href;
    document.head.appendChild(link);
    loadedCss.add(href);
  }
}

export type LoadAiEntryOptions = {
  /** Reload manifest from network instead of using the in-memory cache. */
  force?: boolean;
  /** Maximum time to wait for authenticated React commit. */
  bootstrapTimeoutMs?: number;
};

const DEFAULT_BOOTSTRAP_TIMEOUT_MS = 15_000;

export function waitForAiEntryBootstrap(
  host: HTMLElement,
  entryName: string,
  timeoutMs = DEFAULT_BOOTSTRAP_TIMEOUT_MS,
): Promise<void> {
  const inspect = (): 'ready' | 'error' | null => {
    if (
      host.getAttribute('data-ai-mounted') === 'true'
      && host.getAttribute('data-ai-bootstrap') === 'ready'
    ) {
      return 'ready';
    }
    return host.getAttribute('data-ai-bootstrap') === 'error' ? 'error' : null;
  };

  const initialState = inspect();
  if (initialState === 'ready') {
    return Promise.resolve();
  }
  if (initialState === 'error') {
    return Promise.reject(new EntryBootstrapError(entryName, 'React reported an error'));
  }

  return new Promise<void>((resolve, reject) => {
    let settled = false;
    const finish = (error?: EntryBootstrapError) => {
      if (settled) return;
      settled = true;
      window.clearTimeout(timeoutId);
      observer.disconnect();
      if (error) reject(error);
      else resolve();
    };
    const observer = new MutationObserver(() => {
      const state = inspect();
      if (state === 'ready') finish();
      else if (state === 'error') {
        finish(new EntryBootstrapError(entryName, 'React reported an error'));
      }
    });
    const timeoutId = window.setTimeout(() => {
      finish(new EntryBootstrapError(entryName, `timed out after ${timeoutMs}ms`));
    }, timeoutMs);

    observer.observe(host, {
      attributes: true,
      attributeFilter: ['data-ai-mounted', 'data-ai-bootstrap'],
    });
  });
}

export async function loadAiReactEntry(
  host: HTMLElement,
  entryName: string,
  options: LoadAiEntryOptions = {},
): Promise<void> {
  host.setAttribute('data-ai-loader', 'loading');
  let manifest: Manifest;
  try {
    manifest = await loadManifest(options.force);
  } catch (error) {
    host.setAttribute('data-ai-loader', 'error');
    throw error;
  }

  const entry = resolveEntry(manifest, entryName);
  if (!entry || !entry.file) {
    host.setAttribute('data-ai-loader', 'error');
    throw new EntryNotFoundError(entryName);
  }

  try {
    appendStyles(manifest, entry);
    const moduleUrl = normalizeAssetPath(String(entry.file));
    if (!loadedJs.has(moduleUrl)) {
      await import(/* @vite-ignore */ moduleUrl);
      loadedJs.add(moduleUrl);
    }
    await waitForAiEntryBootstrap(host, entryName, options.bootstrapTimeoutMs);
  } catch (error) {
    host.setAttribute('data-ai-loader', 'error');
    throw error instanceof Error
      ? error
      : new Error(`Failed to import AI entry module: ${entryName}`);
  }

  host.setAttribute('data-ai-loader', 'loaded');
}

/** Reset internal caches; intended for tests only. */
export function __resetAiEntryLoaderForTests(): void {
  loadedJs.clear();
  loadedCss.clear();
  manifestPromise = null;
}
