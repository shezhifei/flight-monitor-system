import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const __dirname = dirname(fileURLToPath(import.meta.url));
const vueAppRoot = resolve(__dirname, '../..');
const repoRoot = resolve(vueAppRoot, '../..');
const styles = join(vueAppRoot, 'src/styles');

function readCss(name: string): string {
  const path = join(styles, name);
  expect(existsSync(path), `missing ${name}`).toBe(true);
  return readFileSync(path, 'utf8');
}

function stripCommentsAndDataUris(css: string): string {
  return css
    .replace(/\/\*[\s\S]*?\*\//g, ' ')
    .replace(/url\(\s*["']?data:[^)]+\)/gi, 'url()');
}

function chromeColorHits(css: string): string[] {
  const body = stripCommentsAndDataUris(css);
  const hits: string[] = [];
  for (const [i, line] of body.split(/\r?\n/).entries()) {
    const hex = line.match(/#[0-9a-fA-F]{3,8}\b/g);
    const rgba = line.match(/rgba?\(/g);
    if (hex || rgba) {
      hits.push(`L${i + 1}: ${line.trim()}`);
    }
  }
  return hits;
}

describe('signal token migration gates', () => {
  it('workspace_unified_theme.css has no --ws- token names', () => {
    const text = readCss('workspace_unified_theme.css');
    const hits = [...text.matchAll(/--ws-/g)];
    expect(hits.map((m) => m[0])).toEqual([]);
  });

  it('global sheets no longer define or consume listed legacy tokens', () => {
    const files = [
      'components.css',
      'tables.css',
      'variables.css',
      'theme-tokens.css',
      'apple-theme.css',
    ] as const;
    const banned = [
      '--spacing-md',
      '--font-size-base',
      '--btn-primary',
      '--btn-secondary',
      '--btn-danger',
    ] as const;
    const hits: string[] = [];
    for (const file of files) {
      const text = readCss(file);
      for (const token of banned) {
        if (text.includes(token)) hits.push(`${file}: ${token}`);
      }
    }
    expect(hits).toEqual([]);
  });

  it('theme-tokens.css keeps --system-* as deprecated fallback', () => {
    const text = readCss('theme-tokens.css');
    expect(text).toMatch(/--system-blue\s*:/);
    const deprecatedBlock = text.match(/deprecated[\s\S]{0,240}--system-\*/i);
    expect(deprecatedBlock, 'expected a deprecated comment covering --system-*').toBeTruthy();
  });

  it('apple-theme.css chrome colors are signal tokens, not Apple hex', () => {
    const hits = chromeColorHits(readCss('apple-theme.css'));
    expect(hits).toEqual([]);
    const text = readCss('apple-theme.css');
    expect(text).toMatch(/var\(--act\)/);
    expect(text).toMatch(/var\(--face-/);
    expect(text).not.toMatch(/#007AFF/i);
    expect(text).not.toMatch(/#FF3B30/i);
  });

  it('dispatch-board.css and flowable-modeler.css chrome has no leftover hex or rgba', () => {
    const hits: string[] = [];
    for (const file of ['dispatch-board.css', 'flowable-modeler.css'] as const) {
      for (const hit of chromeColorHits(readCss(file))) {
        hits.push(`${file} ${hit}`);
      }
    }
    expect(hits).toEqual([]);
  });

  it('workflow doc has the mapping table and five-step method', () => {
    const path = join(repoRoot, 'docs/plans/frontend-token-migration-workflow.md');
    expect(existsSync(path)).toBe(true);
    const text = readFileSync(path, 'utf8');
    expect(text).toMatch(/Token 映射速查表/);
    expect(text).toMatch(/五步法/);
    expect(text).toMatch(/--ws-bg/);
    expect(text).toMatch(/--face-page/);
  });

  it('SIGNAL_SURFACE.md includes 5.3.3 and the dispatch-board 122→86 case', () => {
    const path = join(repoRoot, 'docs/architecture/SIGNAL_SURFACE.md');
    expect(existsSync(path)).toBe(true);
    const text = readFileSync(path, 'utf8');
    expect(text).toMatch(/5\.3\.3 整卷迁移五步法/);
    expect(text).toMatch(/dispatch-board\.css/);
    expect(text).toMatch(/122\s*→\s*86/);
  });
});
