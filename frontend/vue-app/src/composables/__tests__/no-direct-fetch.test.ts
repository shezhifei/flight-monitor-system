import { describe, it, expect } from 'vitest';
import { readFileSync, readdirSync } from 'fs';
import { join, extname, basename } from 'path';

const SRC_DIR = join(__dirname, '..', '..');

// Files allowed to call fetch() directly because they run outside the
// Vue composable context (web workers, legacy bootstrap, auth foundation,
// static public asset loaders, or the useApi wrapper itself).
const ALLOWED_FILES = new Set([
  'useApi.ts',
  'useAuth.ts',
  'aiEntryLoader.ts',
  'dispatchReplanWorker.ts',
  // SvgIcon loads static public icons (must also work pre-auth, e.g. login page)
  'SvgIcon.vue',
]);

function getVueAndTsFiles(dir: string): string[] {
  const results: string[] = [];
  const entries = readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    const fullPath = join(dir, entry.name);
    if (entry.isDirectory() && entry.name !== 'node_modules' && entry.name !== '__tests__' && entry.name !== 'dist') {
      results.push(...getVueAndTsFiles(fullPath));
    } else if (entry.isFile()) {
      const ext = extname(entry.name);
      if (ext === '.vue' || ext === '.ts') {
        // Skip test files and type definition files
        if (!entry.name.endsWith('.test.ts') && !entry.name.endsWith('.d.ts')) {
          results.push(fullPath);
        }
      }
    }
  }
  return results;
}

describe('no direct fetch in components', () => {
  it('Vue SFCs and TS modules should not call fetch() directly', () => {
    const files = getVueAndTsFiles(SRC_DIR);
    const violations: string[] = [];

    for (const file of files) {
      const fileName = basename(file);
      if (ALLOWED_FILES.has(fileName)) continue;

      const content = readFileSync(file, 'utf-8');
      const lines = content.split('\n');
      for (let i = 0; i < lines.length; i++) {
        const line = lines[i];
        const trimmed = line.trim();
        // Skip comments
        if (trimmed.startsWith('//') || trimmed.startsWith('*') || trimmed.startsWith('/*')) continue;
        // Flag bare fetch() calls; allow auth.fetch, response.fetch, and any .fetch() method call
        if (/\bfetch\s*\(/.test(line) && !line.includes('auth.fetch') && !line.includes('response.fetch') && !line.includes('.fetch(')) {
          violations.push(`${file}:${i + 1}: ${trimmed}`);
        }
      }
    }

    expect(violations).toEqual([]);
  });
});
