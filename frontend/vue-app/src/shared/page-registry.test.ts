import { existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

import { pageRegistry } from './page-registry';

const __dirname = dirname(fileURLToPath(import.meta.url));
const vueAppRoot = resolve(__dirname, '../..');

describe('page registry', () => {
  it('describes production pages, not placeholders', () => {
    const offenders: string[] = [];
    for (const def of Object.values(pageRegistry)) {
      if (/placeholder|pending|future/i.test(def.summary)) {
        offenders.push(`${def.id}: ${def.summary}`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it('every registered url points to an existing root HTML entry', () => {
    const missing: string[] = [];
    for (const def of Object.values(pageRegistry)) {
      if (!def.url) continue;
      const match = def.url.match(/^\/frontend\/([^/]+\.html)$/);
      if (!match) {
        missing.push(`${def.id}: non-canonical url ${def.url}`);
        continue;
      }
      const htmlPath = resolve(vueAppRoot, match[1]);
      if (!existsSync(htmlPath)) {
        missing.push(`${def.id}: missing html ${match[1]}`);
      }
    }
    expect(missing).toEqual([]);
  });
});
