import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const __dirname = dirname(fileURLToPath(import.meta.url));
const srcRoot = resolve(__dirname, '..');
const vueAppRoot = resolve(srcRoot, '..');

function walk(dir: string, exts: ReadonlySet<string>, acc: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    if (name === 'node_modules' || name === 'dist' || name.startsWith('.')) continue;
    const full = join(dir, name);
    const st = statSync(full);
    if (st.isDirectory()) {
      walk(full, exts, acc);
    } else {
      const dot = name.lastIndexOf('.');
      if (dot >= 0 && exts.has(name.slice(dot))) acc.push(full);
    }
  }
  return acc;
}

const PLACEHOLDER_PATTERNS: Array<{ pattern: RegExp; label: string }> = [
  { pattern: /功能开发中/g, label: '功能开发中' },
  { pattern: /开发中/g, label: '开发中' },
  { pattern: /待接入/g, label: '待接入' },
  { pattern: /未接入/g, label: '未接入' },
  { pattern: /Frontend Modernization Placeholder/g, label: 'Frontend Modernization Placeholder' },
  { pattern: /placeholder pending/g, label: 'placeholder pending' },
];

const PLACEHOLDER_ATTR_ALLOWLIST: RegExp[] = [
  /\splaceholder="[^"]*"/g,
  /\s:placeholder="[^"]*"/g,
  /\splaceholder='[^']*'/g,
  /\s:placeholder='[^']*'/g,
  /::placeholder\b/g,
  /::-webkit-input-placeholder\b/g,
];

function stripAllowlisted(text: string): string {
  let cleaned = text;
  for (const re of PLACEHOLDER_ATTR_ALLOWLIST) {
    cleaned = cleaned.replace(re, ' ');
  }
  return cleaned;
}

describe('empty affordance audit', () => {
  it('does not ship visible development placeholders in src', () => {
    const files = walk(join(srcRoot), new Set(['.vue', '.ts']));
    const findings: string[] = [];

    for (const file of files) {
      if (file.endsWith('.test.ts') || file.endsWith('.spec.ts')) continue;
      // Allow this audit file itself to contain the literal patterns.
      if (file === resolve(__dirname, 'emptyAffordanceAudit.test.ts')) continue;
      const rel = relative(vueAppRoot, file).replace(/\\/g, '/');
      const text = stripAllowlisted(readFileSync(file, 'utf8'));
      for (const { pattern, label } of PLACEHOLDER_PATTERNS) {
        if (pattern.test(text)) findings.push(`${rel}: ${label}`);
      }
    }

    expect(findings).toEqual([]);
  });

  it('does not ship visible development placeholders in root html entries', () => {
    const findings: string[] = [];
    for (const name of readdirSync(vueAppRoot)) {
      if (!name.endsWith('.html')) continue;
      const full = join(vueAppRoot, name);
      if (!statSync(full).isFile()) continue;
      const text = stripAllowlisted(readFileSync(full, 'utf8'));
      for (const { pattern, label } of PLACEHOLDER_PATTERNS) {
        if (pattern.test(text)) findings.push(`${name}: ${label}`);
      }
    }
    expect(findings).toEqual([]);
  });

  it('does not describe production pages as placeholders in page-registry.ts', () => {
    const registryPath = join(srcRoot, 'shared', 'page-registry.ts');
    const text = readFileSync(registryPath, 'utf8');
    const summaryLines = text.split('\n').filter((line) => /\bsummary:\s*['"]/.test(line));
    const offenders = summaryLines.filter((line) => /placeholder|pending|future/i.test(line));
    expect(offenders).toEqual([]);
  });

  it('every visible <button> in a page has a binding, submit type, or explicit disabled state', () => {
    const pagesRoot = join(srcRoot, 'pages');
    const componentsRoot = join(srcRoot, 'components');
    const files = [
      ...walk(pagesRoot, new Set(['.vue'])),
      ...walk(componentsRoot, new Set(['.vue'])),
    ];
    const offenders: string[] = [];

    for (const file of files) {
      const rel = relative(vueAppRoot, file).replace(/\\/g, '/');
      const text = readFileSync(file, 'utf8');

      // Only inspect the <template> block; ignore <script> and <style>.
      const templateMatch = text.match(/<template[^>]*>([\s\S]*)<\/template>/);
      if (!templateMatch) continue;
      const template = templateMatch[1];
      const templateOffset = text.indexOf(templateMatch[0]) + templateMatch[0].indexOf(templateMatch[1]);

      // Walk for <button literally; read attributes up to the unbraced >.
      const len = template.length;
      let i = 0;
      while (i < len) {
        const openIdx = template.indexOf('<button', i);
        if (openIdx < 0) break;
        const after = template.charCodeAt(openIdx + '<button'.length);
        const isWordBreak =
          (after >= 0x09 && after <= 0x0d) ||
          after === 0x20 ||
          after === 0x2f ||
          after === 0x3e;
        if (!isWordBreak) {
          i = openIdx + '<button'.length;
          continue;
        }

        // Read attributes until the matching > at depth 0, respecting "..." / '...'
        // and JSX-style {...} not present in Vue, but Vue uses balanced quotes;
        // also handle multi-line attributes.
        let cur = openIdx + '<button'.length;
        let inDouble = false;
        let inSingle = false;
        let endIdx = -1;
        while (cur < len) {
          const c = template[cur];
          if (inDouble) {
            if (c === '"' && template[cur - 1] !== '\\') inDouble = false;
          } else if (inSingle) {
            if (c === "'" && template[cur - 1] !== '\\') inSingle = false;
          } else if (c === '"') inDouble = true;
          else if (c === "'") inSingle = true;
          else if (c === '>') {
            endIdx = cur;
            break;
          }
          cur += 1;
        }
        if (endIdx < 0) {
          i = openIdx + '<button'.length;
          continue;
        }

        const attrs = template.slice(openIdx + '<button'.length, endIdx);
        const hasClick = /(^|\s)(@click(\.[a-z]+)*|v-on:click)\s*=/.test(attrs);
        const hasSubmit = /\btype\s*=\s*["']?(submit|reset)["']?/i.test(attrs);
        const hasDisabledTrue = /\bdisabled\b(?!\s*=\s*"false"|\s*=\s*'false'|\s*=\s*\{?\s*false\b)/.test(attrs);
        // Allow buttons that explicitly opt out via v-bind="..." or :="...", though
        // these are rare in this codebase.
        const hasFullBind = /\bv-bind\s*=/.test(attrs);
        // Allow <button> children that are clearly not user-actionable: explicit
        // tabindex="-1" + aria-hidden style icon-only chrome — too narrow to
        // matter here, so do not exempt.

        const ok = hasClick || hasSubmit || hasDisabledTrue || hasFullBind;
        if (!ok) {
          const absoluteIdx = templateOffset + openIdx;
          const line = text.slice(0, absoluteIdx).split('\n').length;
          offenders.push(`${rel}:${line}: <button> without @click / type=submit / disabled / v-bind`);
        }

        i = endIdx + 1;
      }
    }

    expect(offenders).toEqual([]);
  });

  it('does not use bare non-/api/v2 paths in api calls (pages + composables)', () => {
    const pagesRoot = join(srcRoot, 'pages');
    const composablesRoot = join(srcRoot, 'composables');
    const files = [
      ...walk(pagesRoot, new Set(['.ts', '.vue'])),
      ...walk(composablesRoot, new Set(['.ts'])),
    ];
    const offenders: string[] = [];
    const apiCallRe = /api\.(get|post|put|patch|delete)\s*\(\s*['"`](\/[^'"`]*?)['"`]/g;

    for (const file of files) {
      if (file.endsWith('.test.ts') || file.endsWith('.spec.ts')) continue;
      const rel = relative(vueAppRoot, file).replace(/\\/g, '/');
      const text = readFileSync(file, 'utf8');
      for (const m of text.matchAll(apiCallRe)) {
        const path = m[2];
        if (path.startsWith('/api/v2')) continue;
        if (path.startsWith('http://') || path.startsWith('https://')) continue;
        const line = text.slice(0, m.index).split('\n').length;
        offenders.push(`${rel}:${line}: bare path "${path}" should use /api/v2 prefix`);
      }
    }

    expect(offenders).toEqual([]);
  });

  it('does not use Date.now() or Math.random() inside template interpolations (pages + components)', () => {
    const pagesRoot = join(srcRoot, 'pages');
    const componentsRoot = join(srcRoot, 'components');
    const files = [
      ...walk(pagesRoot, new Set(['.vue'])),
      ...walk(componentsRoot, new Set(['.vue'])),
    ];
    const offenders: string[] = [];
    const interpRe = /\{\{([\s\S]*?)\}\}/g;
    const volatileRe = /\b(Date\.now|Math\.random)\s*\(/g;

    for (const file of files) {
      const rel = relative(vueAppRoot, file).replace(/\\/g, '/');
      const text = readFileSync(file, 'utf8');
      const templateMatch = text.match(/<template[^>]*>([\s\S]*)<\/template>/);
      if (!templateMatch) continue;
      const template = templateMatch[1];
      const templateOffset = text.indexOf(templateMatch[0]) + templateMatch[0].indexOf(templateMatch[1]);
      for (const interp of template.matchAll(interpRe)) {
        const expr = interp[1];
        for (const v of expr.matchAll(volatileRe)) {
          const absoluteIdx = templateOffset + interp.index + v.index;
          const line = text.slice(0, absoluteIdx).split('\n').length;
          offenders.push(`${rel}:${line}: ${v[0]} in {{ }} causes reactive noise`);
        }
      }
    }

    expect(offenders).toEqual([]);
  });

  it('does not bind production UI events to noop handlers', () => {
    const files = [
      ...walk(join(srcRoot, 'pages'), new Set(['.vue'])),
      ...walk(join(srcRoot, 'components'), new Set(['.vue'])),
    ];
    const offenders: string[] = [];

    for (const file of files) {
      const rel = relative(vueAppRoot, file).replace(/\\/g, '/');
      const text = readFileSync(file, 'utf8');
      const noopBindingPattern = /(?:@[\w:-]+|v-on:[\w:-]+)\s*=\s*["']noop["']/g;
      for (const match of text.matchAll(noopBindingPattern)) {
        const line = text.slice(0, match.index).split('\n').length;
        offenders.push(`${rel}:${line}: ${match[0]}`);
      }
    }

    expect(offenders).toEqual([]);
  });

  it('list-fetching composables expose ok / error signals (advisory)', () => {
    const composablesRoot = join(srcRoot, 'composables');
    const files = walk(composablesRoot, new Set(['.ts']));
    const warnings: string[] = [];

    for (const file of files) {
      if (file.endsWith('.test.ts') || file.endsWith('.spec.ts')) continue;
      const rel = relative(vueAppRoot, file).replace(/\\/g, '/');
      const text = readFileSync(file, 'utf8');
      const returnsItems = /return\s*\{[^}]*\bitems\b/.test(text);
      if (!returnsItems) continue;
      const hasOk = /\bok\b/.test(text) && /ref\s*[<(]\s*boolean/.test(text);
      const hasError = /\berror\b/.test(text) && /ref\s*[<(]/.test(text);
      if (!hasOk || !hasError) {
        warnings.push(`${rel}: returns items but missing ok=${hasOk} error=${hasError}`);
      }
    }

    if (warnings.length) {
      console.warn('[advisory] composables without ok/error signals:', warnings);
    }
  });
});
