import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import ts from 'typescript';

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const appRoot = path.resolve(scriptDirectory, '..', '..');
const repoRoot = path.resolve(appRoot, '..', '..');
const matrixPath = path.join(appRoot, 'src', 'legacy', 'pageParityMatrix.ts');
const reportPath = path.join(repoRoot, 'docs', 'operations', 'frontend-parity-audit.md');

function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
}

function escapeCell(value) {
  return String(value ?? '—').replaceAll('|', '\\|').replaceAll('\n', '<br>');
}

function code(value) {
  return value ? `\`${escapeCell(value)}\`` : '—';
}

async function loadMatrix() {
  const source = await readFile(matrixPath, 'utf8');
  const evidenceJsonPath = path.join(appRoot, 'src', 'legacy', 'pageEvidenceScenarios.json');
  const evidenceJson = await readFile(evidenceJsonPath, 'utf8');
  const transpiled = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
    fileName: matrixPath,
    reportDiagnostics: true,
  });
  const errors = (transpiled.diagnostics ?? []).filter(
    (diagnostic) => diagnostic.category === ts.DiagnosticCategory.Error,
  );
  if (errors.length > 0) {
    throw new Error(errors.map((diagnostic) => (
      ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n')
    )).join('\n'));
  }
  // data: URL modules cannot resolve relative JSON imports; inline the evidence map.
  const rewritten = transpiled.outputText.replace(
    /import\s+pageEvidenceScenarios\s+from\s+['"]\.\/pageEvidenceScenarios\.json['"];?/,
    `const pageEvidenceScenarios = ${evidenceJson};`,
  );
  if (rewritten === transpiled.outputText) {
    throw new Error('Failed to inline pageEvidenceScenarios.json into the parity matrix module.');
  }
  const encoded = Buffer.from(rewritten, 'utf8').toString('base64');
  return import(`data:text/javascript;base64,${encoded}`);
}

async function readContractEvidence(row) {
  if (!row.evidence?.contract) return { hash: '—', contract: null };
  try {
    const raw = await readFile(path.join(repoRoot, row.evidence.contract));
    return { hash: sha256(raw), contract: JSON.parse(raw.toString('utf8')) };
  } catch {
    return { hash: 'missing', contract: null };
  }
}

function owner(row) {
  return row.vueComponent ?? row.reactEntry ?? row.vueEntry ?? row.canonicalUrl ?? row.notes ?? 'inventory-only';
}

function passCount(row, field) {
  if (row.status !== 'verified') return 0;
  const ids = row.evidence?.[field] ?? [];
  const planned = ids.filter((id) => id.startsWith('planned:'));
  if (planned.length > 0) throw new Error(`${row.id}: verified evidence still contains planned IDs`);
  return new Set(ids).size;
}

function isDeclaredScenarioError(error, scenario) {
  if ((scenario.expectedLegacyErrors ?? []).some((expected) => error.includes(expected))) return true;
  if ((scenario.expectedHttpStatuses ?? []).some((status) => [
    `status of ${status}`,
    `HTTP ${status}`,
    `failed: ${status}`,
    `failure: ${status}`,
  ].some((fragment) => error.includes(fragment)))) return true;
  return (scenario.expectedHttpStatuses ?? []).includes(401)
    && /Execution context was destroyed|navigation|Failed to fetch/i.test(error);
}

function contractBlockers(contract) {
  if (!contract) return [];
  const blockers = [];
  for (const scenario of contract.scenarios ?? []) {
    if ((scenario.fixtureGaps ?? []).length > 0) blockers.push(`fixture-gap:${scenario.id}`);
    if ((scenario.coverageGaps ?? []).length > 0) blockers.push(`coverage-gap:${scenario.id}`);
    if ((scenario.consoleErrors ?? []).some((error) => !isDeclaredScenarioError(error, scenario))) {
      blockers.push(`undeclared-console-error:${scenario.id}`);
    }
  }
  for (const asset of [
    ...(contract.source?.scripts ?? []),
    ...(contract.source?.stylesheets ?? []),
    ...(contract.source?.assets ?? []),
  ]) {
    if (!['external', 'dynamic'].includes(asset.kind) && !asset.exists) {
      blockers.push(`missing-asset:${asset.reference}`);
    }
  }
  return blockers;
}

function statusTable(rows) {
  return [
    '<!-- parity-matrix:start -->',
    '| Surface | Kind | Current owner | Legacy reference | Status |',
    '|---|---|---|---|---|',
    ...rows.map((row) => [
      code(row.id),
      code(row.kind),
      code(owner(row)),
      code(row.legacyHtml),
      code(row.status),
    ].join(' | ').replace(/^/, '| ').replace(/$/, ' |')),
    '<!-- parity-matrix:end -->',
  ].join('\n');
}

async function detailedTable(rows) {
  const lines = [
    '| Page | Kind | Current owner | Legacy contract hash | Functional pass | API contract pass | Realtime pass | Visual pass | Accessibility pass | Open exceptions | Final status |',
    '|---|---|---|---|---:|---:|---:|---:|---:|---|---|',
  ];
  for (const row of rows) {
    const { hash, contract } = await readContractEvidence(row);
    const exceptions = [...new Set([
      ...(row.evidence?.exceptions ?? []),
      ...(contract?.approvedExceptions ?? []),
      ...contractBlockers(contract),
    ])];
    const visualPass = row.status === 'verified'
      ? new Set(row.evidence?.viewports ?? []).size
      : 0;
    lines.push([
      code(row.id),
      code(row.kind),
      code(owner(row)),
      code(hash),
      passCount(row, 'functionalScenarios'),
      passCount(row, 'apiScenarios'),
      passCount(row, 'realtimeScenarios'),
      visualPass,
      passCount(row, 'accessibilityScenarios'),
      exceptions.length > 0 ? exceptions.map(code).join('<br>') : '—',
      code(row.status),
    ].join(' | ').replace(/^/, '| ').replace(/$/, ' |'));
  }
  return lines.join('\n');
}

async function main() {
  const { PAGE_PARITY_MATRIX } = await loadMatrix();
  const clock = JSON.parse(await readFile(path.join(appRoot, 'parity', 'fixtures', 'common', 'clock.json'), 'utf8'));
  const statusCounts = PAGE_PARITY_MATRIX.reduce((counts, row) => {
    counts[row.status] = (counts[row.status] ?? 0) + 1;
    return counts;
  }, {});
  const activeCount = PAGE_PARITY_MATRIX.filter((row) => row.evidence).length;
  const verifiedCount = statusCounts.verified ?? 0;

  const markdown = `# Frontend Strict-Parity Audit\n\n`
    + `> Generated by \`npm run parity:report\` from the TypeScript ownership matrix and tracked evidence. Do not edit the tables manually.\n\n`
    + `Evidence clock: \`${escapeCell(clock.instant)}\`. Active evidence-gated surfaces: **${activeCount}**. Verified: **${verifiedCount}**. Unverified: **${statusCounts.unverified ?? 0}**.\n\n`
    + `A source file or production owner does not imply parity. Pass counts remain zero until a row is \`verified\`; promotion is enforced by \`src/legacy/parityEvidence.test.ts\`. Contract hashes are SHA-256 digests of the tracked JSON files. Visual pass count is the number of required viewports admitted by verified evidence.\n\n`
    + `## Ownership And Status Matrix\n\n${statusTable(PAGE_PARITY_MATRIX)}\n\n`
    + `## Evidence Report\n\n${await detailedTable(PAGE_PARITY_MATRIX)}\n\n`
    + `## Open Controls\n\n`
    + `- Legacy capture is fail-closed: unknown API requests, missing named SSE fixtures, stale source hashes, malformed PNG files, or missing scenarios prevent evidence promotion.\n`
    + `- \`contract-captured\` means only that a schema-valid legacy contract exists. \`verified\` additionally requires clean scenarios, matching page ownership, executable browser test titles, complete screenshot metadata, all five viewports, accessibility evidence, and aligned exception IDs.\n`
    + `- Retired, redirect, and debug-excluded rows are inventory decisions and are never counted as strict-parity verification.\n`;

  await mkdir(path.dirname(reportPath), { recursive: true });
  await writeFile(reportPath, markdown, 'utf8');
  console.log(`Wrote parity report for ${PAGE_PARITY_MATRIX.length} surface(s) to ${reportPath}.`);
}

await main().catch((error) => {
  console.error(error instanceof Error ? error.stack ?? error.message : String(error));
  process.exitCode = 1;
});
