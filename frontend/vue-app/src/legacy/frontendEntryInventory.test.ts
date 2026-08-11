import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

import { PRODUCTION_HTML_ENTRIES } from '@/shared/production-html-entries';
import { pageRegistry } from '@/shared/page-registry';

const __dirname = dirname(fileURLToPath(import.meta.url));
const vueAppRoot = resolve(__dirname, '../..');

function listRootHtmlFiles(): string[] {
  return readdirSync(vueAppRoot)
    .filter((name) => name.endsWith('.html'))
    .filter((name) => statSync(join(vueAppRoot, name)).isFile());
}

describe('frontend entry inventory', () => {
  it('every Vite input maps to an existing HTML file in the project root', () => {
    const missing = PRODUCTION_HTML_ENTRIES.filter(
      (name) => !existsSync(join(vueAppRoot, `${name}.html`)),
    );
    expect(missing).toEqual([]);
  });

  it('every root HTML file is in the Vite input allowlist', () => {
    const onDisk = listRootHtmlFiles().map((name) => name.replace(/\.html$/, ''));
    const orphaned = onDisk.filter(
      (name) => !PRODUCTION_HTML_ENTRIES.includes(name as (typeof PRODUCTION_HTML_ENTRIES)[number]),
    );
    expect(orphaned).toEqual([]);
  });

  it('test.html is deleted and absent from the project root', () => {
    expect(existsSync(join(vueAppRoot, 'test.html'))).toBe(false);
    expect(PRODUCTION_HTML_ENTRIES).not.toContain('test' as never);
  });

  it('every non-redirect entry has a matching page-registry definition', () => {
    const entriesWithoutRegistry: string[] = [];
    for (const name of PRODUCTION_HTML_ENTRIES) {
      if (name === 'index') continue;
      if (!pageRegistry[name]) entriesWithoutRegistry.push(name);
    }
    expect(entriesWithoutRegistry).toEqual([]);
  });

  it('every page-registry entry that has a /frontend/<page>.html url is in the Vite input', () => {
    const missing: string[] = [];
    for (const def of Object.values(pageRegistry)) {
      if (!def.url) continue;
      const match = def.url.match(/^\/frontend\/([^/]+)\.html$/);
      if (!match) continue;
      const name = match[1];
      if (!PRODUCTION_HTML_ENTRIES.includes(name as (typeof PRODUCTION_HTML_ENTRIES)[number])) {
        missing.push(`${def.id}: ${def.url}`);
      }
    }
    expect(missing).toEqual([]);
  });

  it('index.html is a redirect-only stub, not a Vue bundle host', () => {
    const indexHtml = readFileSync(join(vueAppRoot, 'index.html'), 'utf8');
    expect(indexHtml).toMatch(/url=\/frontend\/dashboard\.html/);
    expect(indexHtml).toMatch(/href="\/frontend\/dashboard\.html"/);
    expect(indexHtml).not.toMatch(/\/src\/main\.ts/);
    expect(indexHtml).not.toMatch(/\/src\/entries\//);
  });

  it('no root HTML loads the legacy ai_entry_loader script', () => {
    const offenders: string[] = [];
    for (const name of listRootHtmlFiles()) {
      const text = readFileSync(join(vueAppRoot, name), 'utf8');
      if (text.includes('/frontend/js/ai_entry_loader.js')) offenders.push(name);
    }
    expect(offenders).toEqual([]);
  });

  it('no root HTML uses the legacy /frontend/html/ path for navigation', () => {
    const offenders: string[] = [];
    for (const name of listRootHtmlFiles()) {
      const text = readFileSync(join(vueAppRoot, name), 'utf8');
      if (/\/frontend\/html\/[^"'\s]+\.html/.test(text)) offenders.push(name);
    }
    expect(offenders).toEqual([]);
  });
});
