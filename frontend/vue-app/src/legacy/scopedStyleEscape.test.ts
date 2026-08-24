import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parse } from '@vue/compiler-sfc';
import { describe, expect, it } from 'vitest';

const __dirname = dirname(fileURLToPath(import.meta.url));
const srcRoot = resolve(__dirname, '..');

/**
 * `<style scoped>` 里只要选择器出现 `:global(X)`，SFC 编译器就只留下 X，
 * 同一条选择器里其余部分连同 scope 属性一起被丢掉：
 * `:global([data-theme='light']) .chip img { … }` 编译成 `[data-theme='light'] { … }`，
 * 规则直接落到 <html> 上。曾因此把浅色主题整页 filter 成纯白（登录后一片空白）。
 *
 * 因此 `:global(…)` 只有「包住整条选择器」这一种安全写法。
 * 要带组件外的祖先前缀，写 `:global(<祖先> <组件内选择器>)`，不要写 `:global(<祖先>) …`。
 */

function vueFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...vueFiles(path));
    else if (entry.name.endsWith('.vue')) out.push(path);
  }
  return out;
}

/** 规则头（`{` 之前那段）——足够拿到选择器，嵌套 at-rule 由 `startsWith('@')` 排除 */
function ruleHeads(css: string): string[] {
  const body = css.replace(/\/\*[\s\S]*?\*\//g, ' ');
  const heads: string[] = [];
  for (const match of body.matchAll(/(?:^|[{}；;])([^{}@;]+)\{/g)) {
    const head = match[1].trim();
    if (head && !head.startsWith('@')) heads.push(head);
  }
  return heads;
}

/** 按顶层逗号切分，`:global(a, b)` / `:is(a, b)` 里的逗号不切 */
function splitSelectors(head: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let current = '';
  for (const ch of head) {
    if (ch === '(') depth += 1;
    if (ch === ')') depth -= 1;
    if (ch === ',' && depth === 0) {
      out.push(current);
      current = '';
      continue;
    }
    current += ch;
  }
  out.push(current);
  return out.map((s) => s.trim()).filter(Boolean);
}

/** `:global(…)` 是否包住了整条选择器 */
function globalWrapsWholeSelector(selector: string): boolean {
  const start = selector.indexOf(':global(');
  if (start !== 0) return false;
  let depth = 0;
  for (let i = ':global'.length; i < selector.length; i += 1) {
    if (selector[i] === '(') depth += 1;
    else if (selector[i] === ')') {
      depth -= 1;
      if (depth === 0) return selector.slice(i + 1).trim() === '';
    }
  }
  return false;
}

describe('SFC scoped styles do not escape to the document root', () => {
  it(':global() in a scoped block always wraps the whole selector', () => {
    const files = vueFiles(srcRoot).sort();
    expect(files.length).toBeGreaterThan(20);

    const offenders: string[] = [];
    for (const abs of files) {
      const rel = relative(srcRoot, abs).split(sep).join('/');
      const { descriptor } = parse(readFileSync(abs, 'utf8'), { filename: rel });
      for (const style of descriptor.styles) {
        if (!style.scoped) continue;
        for (const head of ruleHeads(style.content)) {
          for (const selector of splitSelectors(head)) {
            if (!selector.includes(':global(')) continue;
            if (globalWrapsWholeSelector(selector)) continue;
            offenders.push(`${rel}: ${selector}`);
          }
        }
      }
    }

    expect(offenders).toEqual([]);
  });
});
