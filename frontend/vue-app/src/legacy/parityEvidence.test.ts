import { createHash } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';
import { describe, expect, it } from 'vitest';
import {
  PAGE_PARITY_MATRIX,
  REQUIRED_PARITY_VIEWPORTS,
  type EvidenceGatedSurfaceParityRow,
  isEvidenceGatedSurface,
} from './pageParityMatrix';
import { assertLegacyContract, type LegacyContract } from './legacyContractSchema';

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, '../../../..');
const auditPath = join(repoRoot, 'docs/operations/frontend-parity-audit.md');

function repoPath(relativePath: string): string {
  return join(repoRoot, relativePath);
}

function readTrackedText(relativePath: string): string | undefined {
  const absolutePath = repoPath(relativePath);
  return existsSync(absolutePath) ? readFileSync(absolutePath, 'utf8') : undefined;
}

function validateTrackedContract(contractText: string): string | undefined {
  try {
    const contract: unknown = JSON.parse(contractText);
    assertLegacyContract(contract);
    return undefined;
  } catch (error) {
    return error instanceof Error ? error.message : String(error);
  }
}

function parseTrackedContract(contractText: string): { contract?: LegacyContract; error?: string } {
  try {
    const contract: unknown = JSON.parse(contractText);
    assertLegacyContract(contract);
    return { contract };
  } catch (error) {
    return { error: error instanceof Error ? error.message : String(error) };
  }
}

function ownerPageId(row: EvidenceGatedSurfaceParityRow): string {
  return row.legacyHtml ? basename(row.legacyHtml, '.html') : basename(row.evidence.contract, '.json');
}

function enabledTestTitles(source: string, filename: string): string[] {
  const sourceFile = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const titles: string[] = [];
  const visit = (node: ts.Node): void => {
    if (ts.isCallExpression(node) && ts.isIdentifier(node.expression)
      && ['test', 'it'].includes(node.expression.text)) {
      const title = node.arguments[0];
      if (title && (ts.isStringLiteral(title) || ts.isNoSubstitutionTemplateLiteral(title))) {
        titles.push(title.text);
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(sourceFile);
  return titles;
}

function sha256(bytes: Buffer): string {
  return createHash('sha256').update(bytes).digest('hex');
}

function validatePng(bytes: Buffer): boolean {
  return bytes.length >= 24
    && bytes.subarray(0, 8).toString('hex') === '89504e470d0a1a0a'
    && bytes.readUInt32BE(16) > 0
    && bytes.readUInt32BE(20) > 0;
}

function isDeclaredScenarioError(error: string, scenario: LegacyContract['scenarios'][number]): boolean {
  if (scenario.expectedLegacyErrors.some((expected) => error.includes(expected))) return true;
  if (scenario.expectedHttpStatuses.some((status) => [
    `status of ${status}`,
    `HTTP ${status}`,
    `failed: ${status}`,
    `failure: ${status}`,
  ].some((fragment) => error.includes(fragment)))) return true;
  return scenario.expectedHttpStatuses.includes(401)
    && /Execution context was destroyed|navigation|Failed to fetch/i.test(error);
}

function collectContractCapturedFailures(row: EvidenceGatedSurfaceParityRow): string[] {
  if (row.status !== 'contract-captured') return [];

  const contractText = readTrackedText(row.evidence.contract);
  if (contractText === undefined) {
    return [`${row.id}: contract-captured but contract is missing — ${row.evidence.contract}`];
  }

  const contractError = validateTrackedContract(contractText);
  return contractError === undefined
    ? []
    : [
        `${row.id}: contract-captured but contract fails legacy schema — ${row.evidence.contract}: ${contractError}`,
      ];
}

function collectVerifiedFailures(row: EvidenceGatedSurfaceParityRow): string[] {
  if (row.status !== 'verified') return [];

  const failures: string[] = [];
  const { evidence } = row;
  const contractText = readTrackedText(evidence.contract);
  const browserSpecText = readTrackedText(evidence.browserSpec);
  let contract: LegacyContract | undefined;

  if (contractText === undefined) {
    failures.push(`${row.id}: verified contract is missing — ${evidence.contract}`);
  } else {
    const parsed = parseTrackedContract(contractText);
    contract = parsed.contract;
    if (parsed.error !== undefined) {
      failures.push(
        `${row.id}: verified contract fails legacy schema — ${evidence.contract}: ${parsed.error}`,
      );
    } else if (contract) {
      const expectedPage = ownerPageId(row);
      if (contract.page !== expectedPage) {
        failures.push(`${row.id}: verified contract page ${contract.page} != owner page ${expectedPage}`);
      }
      const missingAssets = [
        ...contract.source.scripts,
        ...contract.source.stylesheets,
        ...contract.source.assets,
      ].filter((asset) => !['external', 'dynamic'].includes(asset.kind) && !asset.exists);
      for (const asset of missingAssets) {
        failures.push(`${row.id}: verified contract references a missing legacy asset — ${asset.reference}`);
      }
      for (const scenario of contract.scenarios) {
        const hasDeclaredErrorEvidence = scenario.expectedHttpStatuses.length > 0
          || scenario.expectedLegacyErrors.length > 0;
        const undeclaredConsoleErrors = scenario.consoleErrors.filter(
          (error) => !isDeclaredScenarioError(error, scenario),
        );
        if ((scenario.captureStatus !== 'rendered'
            && !(scenario.captureStatus === 'rendered-with-errors' && hasDeclaredErrorEvidence))
          || scenario.fixtureGaps.length > 0
          || scenario.coverageGaps.length > 0
          || undeclaredConsoleErrors.length > 0) {
          failures.push(`${row.id}: verified contract scenario is not cleanly rendered — ${scenario.id}`);
        }
      }
      if (JSON.stringify(contract.approvedExceptions) !== JSON.stringify(evidence.exceptions)) {
        failures.push(`${row.id}: verified exception IDs differ between contract and matrix evidence`);
      }
    }
  }

  if (browserSpecText === undefined) {
    failures.push(`${row.id}: verified browser spec is missing — ${evidence.browserSpec}`);
  } else {
    if (/\b(?:test|it|describe)\.(?:skip|fixme)\b|\btest\.describe\.(?:skip|fixme)\b/.test(browserSpecText)) {
      failures.push(`${row.id}: verified browser spec contains skip/fixme scenarios`);
    }

    const testTitles = enabledTestTitles(browserSpecText, evidence.browserSpec);
    if (testTitles.length === 0) failures.push(`${row.id}: verified browser spec has no enabled test cases`);
    const contractScenarioIds = new Set(contract?.scenarios.map((scenario) => scenario.id) ?? []);
    for (const scenarioId of [
      ...evidence.functionalScenarios,
      ...evidence.apiScenarios,
      ...evidence.realtimeScenarios,
      ...evidence.accessibilityScenarios,
    ]) {
      if (!scenarioId.trim()) {
        failures.push(`${row.id}: verified scenario ID is empty`);
      } else if (scenarioId.startsWith('planned:')) {
        failures.push(`${row.id}: verified scenario is still planned — ${scenarioId}`);
      } else {
        if (!contractScenarioIds.has(scenarioId)) {
          failures.push(`${row.id}: scenario is absent from the owner contract — ${scenarioId}`);
        }
        if (!testTitles.some((title) => title.includes(scenarioId))) {
          failures.push(`${row.id}: browser spec has no enabled test title for scenario — ${scenarioId}`);
        }
      }
    }
  }

  if (evidence.functionalScenarios.length === 0) {
    failures.push(`${row.id}: verified row has no functional scenarios`);
  }
  if (evidence.apiScenarios.length === 0) {
    failures.push(`${row.id}: verified row has no API scenarios`);
  }
  if (evidence.realtimeScenarios.length === 0) {
    failures.push(`${row.id}: verified row has no realtime scenarios`);
  }
  if (evidence.accessibilityScenarios.length === 0) {
    failures.push(`${row.id}: verified row has no accessibility scenarios`);
  }

  const expectedViewports = REQUIRED_PARITY_VIEWPORTS.join('|');
  if (evidence.viewports.join('|') !== expectedViewports) {
    failures.push(`${row.id}: verified row does not cover every required viewport`);
  }

  if (evidence.legacyScreenshots.length < REQUIRED_PARITY_VIEWPORTS.length) {
    failures.push(`${row.id}: verified row has fewer screenshots than required viewports`);
  }

  const uniqueScreenshots = new Set(evidence.legacyScreenshots);
  if (uniqueScreenshots.size !== evidence.legacyScreenshots.length) {
    failures.push(`${row.id}: verified row contains duplicate screenshot paths`);
  }

  for (const viewport of REQUIRED_PARITY_VIEWPORTS) {
    const viewportSuffixes = [`--${viewport}.png`, `/${viewport}.png`];
    if (
      !evidence.legacyScreenshots.some((screenshot) =>
        viewportSuffixes.some((suffix) => screenshot.endsWith(suffix)),
      )
    ) {
      failures.push(`${row.id}: verified row has no screenshot for ${viewport}`);
    }
  }

  const metadataByDirectory = new Map<string, Record<string, unknown>>();
  const screenshotViewports = new Set<string>();
  for (const screenshot of evidence.legacyScreenshots) {
    const screenshotPath = repoPath(screenshot);
    if (!existsSync(screenshotPath)) {
      failures.push(`${row.id}: verified screenshot is missing — ${screenshot}`);
      continue;
    }
    const bytes = readFileSync(screenshotPath);
    if (!validatePng(bytes)) failures.push(`${row.id}: verified screenshot is not a valid PNG — ${screenshot}`);

    const snapshotDirectory = dirname(screenshotPath);
    let metadata = metadataByDirectory.get(snapshotDirectory);
    if (!metadata) {
      const metadataPath = join(snapshotDirectory, 'capture.metadata.json');
      if (!existsSync(metadataPath)) {
        failures.push(`${row.id}: screenshot metadata is missing — ${metadataPath}`);
        continue;
      }
      try {
        metadata = JSON.parse(readFileSync(metadataPath, 'utf8')) as Record<string, unknown>;
        metadataByDirectory.set(snapshotDirectory, metadata);
      } catch {
        failures.push(`${row.id}: screenshot metadata is invalid JSON — ${metadataPath}`);
        continue;
      }
    }
    if (metadata.page !== ownerPageId(row)) {
      failures.push(`${row.id}: screenshot metadata page does not match its owner contract`);
    }
    if (contract && metadata.source_sha256 !== contract.source.sourceHash) {
      failures.push(`${row.id}: screenshot source hash does not match the owner contract`);
    }
    const snapshotMarker = 'e2e/parity/snapshots/legacy/';
    const metadataCapturePath = screenshot.includes(snapshotMarker)
      ? screenshot.slice(screenshot.indexOf(snapshotMarker) + snapshotMarker.length)
      : screenshot;
    const capture = Array.isArray(metadata.captures)
      ? metadata.captures.find((candidate: unknown) => (
          typeof candidate === 'object' && candidate !== null
          && (candidate as Record<string, unknown>).file === metadataCapturePath
        )) as Record<string, unknown> | undefined
      : undefined;
    if (!capture) {
      failures.push(`${row.id}: screenshot is not registered in capture metadata — ${screenshot}`);
      continue;
    }
    if (capture.sha256 !== sha256(bytes)) failures.push(`${row.id}: screenshot hash does not match metadata — ${screenshot}`);
    const ownsWholePage = row.id === ownerPageId(row);
    if (ownsWholePage && capture.kind !== 'full-page') {
      failures.push(`${row.id}: page viewport evidence must use a full-page capture — ${screenshot}`);
    }
    if (!ownsWholePage && (capture.kind !== 'region' || capture.region !== row.id)) {
      failures.push(`${row.id}: embedded surface evidence must use its named region capture — ${screenshot}`);
    }
    if (typeof capture.viewport === 'object' && capture.viewport !== null) {
      const viewportId = (capture.viewport as Record<string, unknown>).id;
      if (typeof viewportId === 'string') screenshotViewports.add(viewportId);
    }
  }
  for (const viewport of REQUIRED_PARITY_VIEWPORTS) {
    if (!screenshotViewports.has(viewport)) failures.push(`${row.id}: screenshot metadata has no ${viewport} capture`);
  }

  return failures;
}

function parseAuditRows(markdown: string): Map<string, { kind: string; status: string }> {
  const section = markdown.match(
    /<!-- parity-matrix:start -->([\s\S]*?)<!-- parity-matrix:end -->/,
  )?.[1];
  if (!section) return new Map();

  const rows = new Map<string, { kind: string; status: string }>();
  for (const line of section.split(/\r?\n/)) {
    if (!line.trim().startsWith('|')) continue;
    const cells = line
      .split('|')
      .slice(1, -1)
      .map((cell) => cell.trim().replace(/^`|`$/g, ''));
    if (cells.length !== 5 || cells[0] === 'Surface' || /^-+$/.test(cells[0])) continue;
    rows.set(cells[0], { kind: cells[1], status: cells[4] });
  }
  return rows;
}

describe('strict parity evidence promotion', () => {
  it('allows contract-captured only when the tracked contract exists and is valid JSON', () => {
    const failures = PAGE_PARITY_MATRIX.filter(isEvidenceGatedSurface).flatMap(
      collectContractCapturedFailures,
    );
    expect(failures).toEqual([]);
  });

  it('rejects a contract-captured row whose target is not schema-valid evidence', () => {
    const baseRow = PAGE_PARITY_MATRIX.find(isEvidenceGatedSurface);
    expect(baseRow).toBeDefined();
    if (!baseRow) return;

    const invalidRow: EvidenceGatedSurfaceParityRow = {
      ...baseRow,
      status: 'contract-captured',
      evidence: {
        ...baseRow.evidence,
        contract: 'frontend/vue-app/parity/contracts/__missing_contract__.json',
      },
    };

    expect(collectContractCapturedFailures(invalidRow)).toEqual([
      `${invalidRow.id}: contract-captured but contract is missing — ${invalidRow.evidence.contract}`,
    ]);
  });

  it('allows verified only with complete, enabled, on-disk evidence', () => {
    const failures = PAGE_PARITY_MATRIX.filter(isEvidenceGatedSurface).flatMap(
      collectVerifiedFailures,
    );
    expect(failures).toEqual([]);
  });

  it('rejects a verified row with registered targets but incomplete evidence', () => {
    const baseRow = PAGE_PARITY_MATRIX.find(isEvidenceGatedSurface);
    expect(baseRow).toBeDefined();
    if (!baseRow) return;

    const invalidRow: EvidenceGatedSurfaceParityRow = {
      ...baseRow,
      status: 'verified',
      evidence: {
        ...baseRow.evidence,
        contract: 'frontend/vue-app/parity/contracts/__missing_contract__.json',
        browserSpec: 'frontend/vue-app/e2e/parity/pages/__missing_spec__.parity.spec.ts',
        legacyScreenshots: [],
        functionalScenarios: [],
        apiScenarios: [],
        realtimeScenarios: [],
        accessibilityScenarios: [],
        viewports: [],
      },
    };
    const failures = collectVerifiedFailures(invalidRow).join('\n');

    expect(failures).toContain('verified contract is missing');
    expect(failures).toContain('verified browser spec is missing');
    expect(failures).toContain('verified row has no functional scenarios');
    expect(failures).toContain('verified row has no API scenarios');
    expect(failures).toContain('verified row has no realtime scenarios');
    expect(failures).toContain('verified row has no accessibility scenarios');
    expect(failures).toContain('does not cover every required viewport');
    expect(failures).toContain('fewer screenshots than required viewports');
  });

  it('rejects valid on-disk evidence that belongs to a different owner page', () => {
    const login = PAGE_PARITY_MATRIX.find(
      (row): row is EvidenceGatedSurfaceParityRow => isEvidenceGatedSurface(row) && row.id === 'login',
    );
    expect(login).toBeDefined();
    if (!login) return;

    const invalidRow: EvidenceGatedSurfaceParityRow = {
      ...login,
      status: 'verified',
      evidence: {
        ...login.evidence,
        contract: 'frontend/vue-app/parity/contracts/dashboard.json',
        browserSpec: 'frontend/vue-app/e2e/parity/legacy-capture.spec.ts',
        functionalScenarios: ['dashboard-success'],
        apiScenarios: ['dashboard-success'],
        realtimeScenarios: ['dashboard-success'],
        accessibilityScenarios: ['dashboard-success'],
      },
    };

    const failures = collectVerifiedFailures(invalidRow).join('\n');
    expect(failures).toContain('verified contract page dashboard != owner page login');
    expect(failures).toContain('screenshot source hash does not match the owner contract');
    expect(failures).toContain('browser spec has no enabled test title for scenario');
  });

  it('keeps the audit status for every surface exactly aligned with the code matrix', () => {
    expect(existsSync(auditPath)).toBe(true);
    const markdown = readFileSync(auditPath, 'utf8');
    const auditRows = parseAuditRows(markdown);
    const failures: string[] = [];

    for (const row of PAGE_PARITY_MATRIX) {
      const documented = auditRows.get(row.id);
      if (!documented) {
        failures.push(`${row.id}: missing from audit matrix`);
        continue;
      }
      if (documented.kind !== row.kind) {
        failures.push(`${row.id}: audit kind ${documented.kind} != code kind ${row.kind}`);
      }
      if (documented.status !== row.status) {
        failures.push(`${row.id}: audit status ${documented.status} != code status ${row.status}`);
      }
    }

    for (const documentedId of auditRows.keys()) {
      if (!PAGE_PARITY_MATRIX.some((row) => row.id === documentedId)) {
        failures.push(`${documentedId}: audit row has no code matrix row`);
      }
    }

    expect(failures).toEqual([]);
  });

  it('rejects broad prose claims that bypass per-surface evidence status', () => {
    const markdown = readFileSync(auditPath, 'utf8');
    const unsupportedClaims = [
      /\b(?:all|every)\b[^\n.]{0,80}\b(?:complete|verified|migrated)\b/i,
      /\b(?:strict\s+)?parity\s+(?:is|remains)\s+(?:complete|verified)\b/i,
      /\b\d+\s*\/\s*\d+\s+(?:pages?|surfaces?)\s+(?:complete|verified|migrated)\b/i,
      /status:\s*(?:complete|verified)\b/i,
      /✅/,
      /(?:全部|所有)[^。\n]{0,40}(?:完成|已验证)/,
    ].filter((pattern) => pattern.test(markdown));

    expect(unsupportedClaims.map((pattern) => pattern.source)).toEqual([]);
  });
});
