import { describe, it, expect } from 'vitest';
import * as fs from 'fs';
import * as path from 'path';

const COMPONENT_DIRS = [
  'src/components',
  'src/pages',
];

const ALLOWED_FILES = [
  'useApi.ts',
  'useAuth.ts',
  // SvgIcon loads static public icons (must also work pre-auth, e.g. login page)
  'SvgIcon.vue',
];

const ALLOWED_PATTERNS = [
  'auth.fetch',
  '// ',
  'window.fetch',
];

describe('no direct fetch in components', () => {
  it('components should not call fetch() directly (use useApi instead)', () => {
    const violations: string[] = [];
    for (const dir of COMPONENT_DIRS) {
      const fullDir = path.resolve(__dirname, '../../', dir);
      if (!fs.existsSync(fullDir)) continue;
      walkDir(fullDir, (filePath) => {
        if (!filePath.endsWith('.vue') && !filePath.endsWith('.ts')) return;
        const fileName = path.basename(filePath);
        if (ALLOWED_FILES.includes(fileName)) return;
        const content = fs.readFileSync(filePath, 'utf-8');
        const lines = content.split('\n');
        lines.forEach((line, i) => {
          const hasBareFetch = /[^.]fetch\s*\(/.test(line);
          if (!hasBareFetch) return;
          const isAllowed = ALLOWED_PATTERNS.some(pattern => line.includes(pattern));
          if (!isAllowed) {
            violations.push(`${filePath}:${i + 1}: ${line.trim()}`);
          }
        });
      });
    }
    expect(violations).toEqual([]);
  });
});

function walkDir(dir: string, cb: (filePath: string) => void) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walkDir(full, cb);
    else cb(full);
  }
}
