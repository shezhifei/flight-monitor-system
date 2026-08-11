import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import {
  ACTIVE_REACT_ENTRY_IDS,
  PAGE_PARITY_MATRIX,
  REQUIRED_PARITY_VIEWPORTS,
  RETIRED_REACT_ENTRY_IDS,
  isEvidenceGatedSurface,
} from './pageParityMatrix';
import { PRODUCTION_HTML_ENTRIES } from '../shared/production-html-entries';
import { FRONTEND_ENTRY_REGISTRY } from '../../../ai-react/src/lib/shell/entryRegistry';

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, '../../../..');
const legacyHtmlPrefix = 'frontend/backup/legacy-frontend-archive/html/';

function repoPath(relativePath: string): string {
  return join(repoRoot, relativePath);
}

describe('page parity matrix', () => {
  it('has unique row ids', () => {
    const ids = PAGE_PARITY_MATRIX.map((row) => row.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('keeps source ownership checks separate from parity status', () => {
    const missing: string[] = [];

    for (const row of PAGE_PARITY_MATRIX) {
      if (row.kind === 'vue-page') {
        for (const key of ['vueHtml', 'vueEntry', 'vueComponent'] as const) {
          const value = row[key];
          if (!value) {
            missing.push(`${row.id}: missing ${key}`);
          } else if (!existsSync(repoPath(value))) {
            missing.push(`${row.id}: file does not exist — ${value}`);
          }
        }
      }

      if (row.kind.startsWith('react-')) {
        if (!row.reactEntry) {
          missing.push(`${row.id}: missing reactEntry`);
        } else if (!existsSync(repoPath(row.reactEntry))) {
          missing.push(`${row.id}: react entry missing — ${row.reactEntry}`);
        }
      }
    }

    expect(missing).toEqual([]);
  });

  it('references all 21 legacy pages through backup archive metadata', () => {
    const vuePages = PAGE_PARITY_MATRIX.filter((row) => row.kind === 'vue-page');
    const offenders = vuePages
      .filter(
        (row) =>
          row.legacyHtml !== `${legacyHtmlPrefix}${row.id}.html` ||
          row.legacyHtml.includes('frontend/html/'),
      )
      .map((row) => `${row.id}: ${row.legacyHtml ?? 'missing legacyHtml'}`);

    expect(vuePages).toHaveLength(21);
    expect(offenders).toEqual([]);
  });

  it('gives every active surface explicit evidence targets', () => {
    const offenders: string[] = [];

    for (const row of PAGE_PARITY_MATRIX) {
      if (!isEvidenceGatedSurface(row)) continue;

      const { evidence } = row;
      if (!evidence.contract.startsWith('frontend/vue-app/parity/contracts/')) {
        offenders.push(`${row.id}: invalid contract target — ${evidence.contract}`);
      }
      if (!evidence.browserSpec.startsWith('frontend/vue-app/e2e/parity/')) {
        offenders.push(`${row.id}: invalid browser spec target — ${evidence.browserSpec}`);
      }
      if (evidence.legacyScreenshots.length === 0) {
        offenders.push(`${row.id}: no legacy screenshot targets`);
      }
      for (const screenshot of evidence.legacyScreenshots) {
        if (
          !screenshot.startsWith('frontend/vue-app/e2e/parity/snapshots/legacy/') ||
          !screenshot.endsWith('.png')
        ) {
          offenders.push(`${row.id}: invalid legacy screenshot target — ${screenshot}`);
        }
      }
      if (evidence.apiScenarios.length === 0) {
        offenders.push(`${row.id}: no API scenario targets`);
      }
      if (evidence.realtimeScenarios.length === 0) {
        offenders.push(`${row.id}: no realtime scenario targets`);
      }
      if (evidence.viewports.join('|') !== REQUIRED_PARITY_VIEWPORTS.join('|')) {
        offenders.push(`${row.id}: required viewport set is incomplete`);
      }
      if (new Set(evidence.exceptions).size !== evidence.exceptions.length) {
        offenders.push(`${row.id}: duplicate exception IDs`);
      }
    }

    expect(offenders).toEqual([]);
  });

  it('keeps retired, redirect, and debug semantics outside parity promotion', () => {
    const offenders = PAGE_PARITY_MATRIX.filter((row) => {
      if (row.kind === 'retired') return row.status !== 'retired' || 'evidence' in row;
      if (row.kind === 'redirect') return row.status !== 'redirect' || 'evidence' in row;
      if (row.kind === 'debug-removed') {
        return row.status !== 'debug-excluded' || 'evidence' in row;
      }
      return false;
    }).map((row) => `${row.id}: ${row.kind}/${row.status}`);

    expect(offenders).toEqual([]);
  });

  it('every retired react entry is absent from disk and from FRONTEND_ENTRY_REGISTRY', () => {
    const offenders: string[] = [];
    for (const row of PAGE_PARITY_MATRIX) {
      if (row.kind !== 'retired') continue;
      if (row.reactEntry && existsSync(repoPath(row.reactEntry))) {
        offenders.push(`${row.id}: file still exists at ${row.reactEntry}`);
      }
    }
    for (const retired of RETIRED_REACT_ENTRY_IDS) {
      if (FRONTEND_ENTRY_REGISTRY[retired]) {
        offenders.push(`${retired}: still present in FRONTEND_ENTRY_REGISTRY`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it('all canonical urls use /frontend/<page>.html, not /frontend/html/', () => {
    const offenders: string[] = [];
    for (const row of PAGE_PARITY_MATRIX) {
      if (!row.canonicalUrl) continue;
      if (row.canonicalUrl.includes('/frontend/html/')) {
        offenders.push(`${row.id}: ${row.canonicalUrl}`);
      }
      if (!row.canonicalUrl.startsWith('/frontend/')) {
        offenders.push(`${row.id}: not under /frontend/ — ${row.canonicalUrl}`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it('every PRODUCTION_HTML_ENTRIES entry is described in the matrix', () => {
    const matrixIds = new Set(
      PAGE_PARITY_MATRIX.filter(
        (row) => row.kind === 'vue-page' || row.kind === 'redirect',
      ).map((row) => row.id),
    );
    const missing = PRODUCTION_HTML_ENTRIES.filter((id) => !matrixIds.has(id));
    expect(missing).toEqual([]);
  });

  it('every active React entry id matches FRONTEND_ENTRY_REGISTRY', () => {
    const registryIds = new Set(Object.keys(FRONTEND_ENTRY_REGISTRY));
    const matrixIds = new Set(ACTIVE_REACT_ENTRY_IDS);
    expect([...matrixIds].sort()).toEqual([...registryIds].sort());
  });

  it('debug-removed entries are absent from PRODUCTION_HTML_ENTRIES', () => {
    const offenders = PAGE_PARITY_MATRIX.filter(
      (row) => row.kind === 'debug-removed' && PRODUCTION_HTML_ENTRIES.includes(row.id as never),
    ).map((row) => row.id);
    expect(offenders).toEqual([]);
  });

  it('redirect entries reference an html file that exists and contains a meta refresh', () => {
    const offenders: string[] = [];
    for (const row of PAGE_PARITY_MATRIX) {
      if (row.kind !== 'redirect') continue;
      if (!row.vueHtml) {
        offenders.push(`${row.id}: missing vueHtml`);
        continue;
      }
      const fullPath = repoPath(row.vueHtml);
      if (!existsSync(fullPath)) {
        offenders.push(`${row.id}: file does not exist — ${row.vueHtml}`);
        continue;
      }
      const html = readFileSync(fullPath, 'utf8');
      if (!/http-equiv=["']refresh["']/i.test(html)) {
        offenders.push(`${row.id}: missing meta refresh in ${row.vueHtml}`);
      }
    }
    expect(offenders).toEqual([]);
  });
});
