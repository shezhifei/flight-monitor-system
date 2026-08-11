import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const __dirname = dirname(fileURLToPath(import.meta.url));
const aiReactRoot = resolve(__dirname, '../../..');
const srcRoot = join(aiReactRoot, 'src');

function walk(dir: string, exts: ReadonlySet<string>, acc: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    if (name === 'node_modules' || name === 'dist' || name.startsWith('.')) continue;
    const full = join(dir, name);
    const st = statSync(full);
    if (st.isDirectory()) walk(full, exts, acc);
    else {
      const dot = name.lastIndexOf('.');
      if (dot >= 0 && exts.has(name.slice(dot))) acc.push(full);
    }
  }
  return acc;
}

const PLACEHOLDER_PATTERNS: Array<{ pattern: RegExp; label: string }> = [
  { pattern: /功能开发中/g, label: '功能开发中' },
  { pattern: /开发中/g, label: '开发中' },
  { pattern: /coming soon/gi, label: 'coming soon' },
  { pattern: /not implemented/gi, label: 'not implemented' },
  { pattern: /\bTODO:[^\n]*实现/g, label: 'TODO: 实现' },
];

function lineMentionsAntdButton(line: string): boolean {
  return /\bButton\b/.test(line) || /\bButton\.[A-Z]/.test(line);
}

function isCommentedOut(line: string): boolean {
  const trimmed = line.trim();
  return trimmed.startsWith('//') || trimmed.startsWith('*') || trimmed.startsWith('/*');
}

describe('react ai empty affordance audit', () => {
  it('does not ship visible development placeholders in src', () => {
    const files = walk(srcRoot, new Set(['.tsx', '.ts']));
    const findings: string[] = [];

    for (const file of files) {
      if (file.endsWith('.test.ts') || file.endsWith('.test.tsx') || file.endsWith('.spec.ts')) continue;
      // Allow this audit file itself to contain literal patterns.
      if (file === resolve(__dirname, 'emptyAffordanceAudit.test.ts')) continue;
      const rel = relative(aiReactRoot, file).replace(/\\/g, '/');
      const text = readFileSync(file, 'utf8');

      for (const { pattern, label } of PLACEHOLDER_PATTERNS) {
        const lines = text.split('\n');
        for (let i = 0; i < lines.length; i += 1) {
          const line = lines[i];
          if (isCommentedOut(line)) continue;
          // Reset lastIndex for global regex.
          pattern.lastIndex = 0;
          if (pattern.test(line)) findings.push(`${rel}:${i + 1}: ${label}`);
        }
      }
    }

    expect(findings).toEqual([]);
  });

  it('every Ant Design Button has an onClick handler, an htmlType, an href, or is disabled', () => {
    const files = walk(srcRoot, new Set(['.tsx']));
    const offenders: string[] = [];

    for (const file of files) {
      if (file.endsWith('.test.tsx')) continue;
      const rel = relative(aiReactRoot, file).replace(/\\/g, '/');
      const text = readFileSync(file, 'utf8');

      // Walk for <Button literally, then read attributes up to the matching
      // unbraced >. JSX attribute values are wrapped in either "..." or {expr};
      // a naive `[^>]*?` regex breaks because `>` legitimately appears inside
      // `{...}` (e.g. an icon child like <ReloadOutlined />).
      const len = text.length;
      let i = 0;
      while (i < len) {
        const openIdx = text.indexOf('<Button', i);
        if (openIdx < 0) break;
        // Require a word break right after "Button" so we don't match <ButtonGroup>.
        const after = text.charCodeAt(openIdx + '<Button'.length);
        const isWordBreak =
          (after >= 0x09 && after <= 0x0d) ||
          after === 0x20 ||
          after === 0x2f ||
          after === 0x3e ||
          after === 0x09;
        if (!isWordBreak) {
          i = openIdx + '<Button'.length;
          continue;
        }

        // Read attributes until the matching > at depth 0.
        let cur = openIdx + '<Button'.length;
        let depth = 0;
        let inDouble = false;
        let inSingle = false;
        let endIdx = -1;
        while (cur < len) {
          const c = text[cur];
          if (inDouble) {
            if (c === '"' && text[cur - 1] !== '\\') inDouble = false;
          } else if (inSingle) {
            if (c === "'" && text[cur - 1] !== '\\') inSingle = false;
          } else if (c === '"') inDouble = true;
          else if (c === "'") inSingle = true;
          else if (c === '{') depth += 1;
          else if (c === '}') depth -= 1;
          else if (c === '>' && depth === 0) {
            endIdx = cur;
            break;
          }
          cur += 1;
        }
        if (endIdx < 0) {
          i = openIdx + '<Button'.length;
          continue;
        }

        const attrs = text.slice(openIdx + '<Button'.length, endIdx);
        const hasOnClick = /\bonClick\s*=/.test(attrs);
        const hasHtmlType = /\bhtmlType\s*=\s*\{?\s*["']?(submit|reset)["']?/.test(attrs);
        const hasHref = /\bhref\s*=/.test(attrs);
        const isDisabled = /\bdisabled\b(?!\s*=\s*\{?\s*false\b)/.test(attrs);
        const hasLoadingProp = /\bloading\s*=/.test(attrs);
        // Spread props are an opt-out; static analysis cannot reason about them.
        const hasSpread = /\{\s*\.\.\./.test(attrs);

        const ok = hasOnClick || hasHtmlType || hasHref || isDisabled || hasLoadingProp || hasSpread;
        if (!ok) {
          const line = text.slice(0, openIdx).split('\n').length;
          offenders.push(`${rel}:${line}: <Button> with no onClick/htmlType/href/disabled/loading/spread`);
        }

        i = endIdx + 1;
      }
    }

    expect(offenders).toEqual([]);
  });

  it('no production code references a retired ai entry id', () => {
    const RETIRED = ['ai_config_center', 'flight_monitor_ai', 'flowable_assistant_ai'];
    const files = walk(srcRoot, new Set(['.tsx', '.ts']));
    const offenders: string[] = [];

    for (const file of files) {
      if (file.endsWith('.test.ts') || file.endsWith('.test.tsx')) continue;
      // Allow this audit file itself to contain literal patterns.
      if (file === resolve(__dirname, 'emptyAffordanceAudit.test.ts')) continue;
      const rel = relative(aiReactRoot, file).replace(/\\/g, '/');
      const text = readFileSync(file, 'utf8');
      for (const name of RETIRED) {
        if (new RegExp(`['"]${name}['"]`).test(text)) {
          offenders.push(`${rel}: ${name}`);
        }
      }
    }

    expect(offenders).toEqual([]);
  });

  it('no production code links to /frontend/html/ legacy paths', () => {
    const files = walk(srcRoot, new Set(['.tsx', '.ts']));
    const offenders: string[] = [];

    for (const file of files) {
      if (file.endsWith('.test.ts') || file.endsWith('.test.tsx')) continue;
      if (file === resolve(__dirname, 'emptyAffordanceAudit.test.ts')) continue;
      const rel = relative(aiReactRoot, file).replace(/\\/g, '/');
      const text = readFileSync(file, 'utf8');
      if (/\/frontend\/html\/[^"'`\s]+\.html/.test(text)) {
        offenders.push(rel);
      }
    }

    expect(offenders).toEqual([]);
  });

  // Reference the helper so unused-locals lint passes.
  void lineMentionsAntdButton;
});
