import { readFile } from 'node:fs/promises';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  LEGACY_CONTRACT_PAGES,
  assertLegacyContract,
  validateLegacyContract,
  type LegacyContract,
} from './legacyContractSchema';

const contractsRoot = path.resolve(process.cwd(), 'parity', 'contracts');

async function loadContract(page: string): Promise<unknown> {
  const raw = await readFile(path.join(contractsRoot, `${page}.json`), 'utf8');
  return JSON.parse(raw) as unknown;
}

describe('legacy contract schema', () => {
  it('validates all 21 explicit tracked contracts', async () => {
    expect(LEGACY_CONTRACT_PAGES).toHaveLength(21);

    for (const page of LEGACY_CONTRACT_PAGES) {
      const contract = await loadContract(page);
      expect(() => assertLegacyContract(contract), page).not.toThrow();
      expect((contract as LegacyContract).page).toBe(page);
      expect((contract as LegacyContract).source.html).toBe(`html/${page}.html`);
      expect((contract as LegacyContract).scenarios.length).toBeGreaterThan(0);
    }
  });

  it('rejects stale source paths, missing scenarios, and incomplete asset hashes', async () => {
    const dashboard = await loadContract('dashboard') as LegacyContract;
    const invalid = structuredClone(dashboard) as unknown as Record<string, unknown>;
    invalid.source = {
      ...dashboard.source,
      html: 'frontend/html/dashboard.html',
      scripts: [{
        kind: 'script',
        reference: '/frontend/js/auth.js',
        archivePath: 'frontend/js/auth.js',
        exists: true,
        sha256: null,
      }],
    };
    invalid.scenarios = [];

    const result = validateLegacyContract(invalid);
    expect(result.valid).toBe(false);
    expect(result.issues).toEqual(expect.arrayContaining([
      '$.source.html must be archive-relative under html/',
      '$.source.scripts[0].sha256 is required for an existing archive asset',
      '$.source.scripts[0].archivePath must be archive-relative and must not use stale frontend/ paths',
      '$.scenarios must contain at least one captured scenario',
    ]));
  });

  it('rejects contracts outside the fixed legacy page inventory', async () => {
    const dashboard = await loadContract('dashboard') as LegacyContract;
    const invalid = { ...dashboard, page: 'invented_page' };

    expect(validateLegacyContract(invalid).issues).toContain(
      '$.page must be one of the 21 explicit legacy pages',
    );
  });

  it('rejects a rendered scenario that still has fixture gaps', async () => {
    const dashboard = await loadContract('dashboard') as LegacyContract;
    const invalid = structuredClone(dashboard);
    invalid.scenarios[0].captureStatus = 'rendered';
    invalid.scenarios[0].coverageGaps = ['the manifest has no explicit browser scenario list'];
    invalid.scenarios[0].fixtureGaps = [{
      method: 'GET',
      pathname: '/api/v2/uncovered',
      query: {},
      body: null,
      reason: 'no explicit fixture matched',
    }];

    expect(validateLegacyContract(invalid).issues).toContain(
      '$.scenarios[0].captureStatus cannot be rendered while fixture gaps exist',
    );
    expect(validateLegacyContract(invalid).issues).toContain(
      '$.scenarios[0].captureStatus cannot be rendered while coverage gaps exist',
    );
  });

  it('rejects malformed expected-error and named-SSE evidence', async () => {
    const dashboard = await loadContract('dashboard') as LegacyContract;
    const invalid = structuredClone(dashboard);
    invalid.scenarios[0].expectedHttpStatuses = [200];
    invalid.scenarios[0].sseSubscriptions[0].fixtureId = 'dashboard-stream';
    invalid.scenarios[0].sseSubscriptions[0].eventTypes = [];

    const issues = validateLegacyContract(invalid).issues;
    expect(issues).toContain(
      '$.scenarios[0].expectedHttpStatuses[0] must be an HTTP error status from 400 to 599',
    );
    expect(issues).toContain(
      '$.scenarios[0].sseSubscriptions[0].eventTypes must contain named events for a matched SSE fixture',
    );
  });
});
