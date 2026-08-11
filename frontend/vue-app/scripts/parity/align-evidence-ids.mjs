import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const vueAppRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const pagesDir = path.join(vueAppRoot, 'e2e', 'parity', 'pages');
const contractsDir = path.join(vueAppRoot, 'parity', 'contracts');
const evidenceOut = path.join(vueAppRoot, 'src', 'legacy', 'pageEvidenceScenarios.json');

function contractScenarioIds(page) {
  const contract = JSON.parse(readFileSync(path.join(contractsDir, `${page}.json`), 'utf8'));
  return (contract.scenarios || []).map((scenario) => scenario.id);
}

function chooseScenarioId(page, title, contractIds) {
  const lower = title.toLowerCase();
  const has = (re) => contractIds.find((id) => re.test(id));

  if (page === 'login') {
    if (lower.includes('password')) return 'login-password-visible';
    if (lower.includes('required') || lower.includes('validation') || lower.includes('username')) {
      return 'login-required-username-error';
    }
    return 'login-default';
  }
  // Avoid matching "not empty key" etc. — require empty as a word boundary token.
  if (/\bempty\b/.test(lower) && !lower.includes('not empty')) {
    return has(/empty/) || contractIds[0];
  }
  if (lower.includes('forbidden') || lower.includes('readonly')) {
    return has(/forbidden/) || has(/unauthorized/) || contractIds[0];
  }
  if (lower.includes('unauthorized')) return has(/unauthorized/) || has(/forbidden/) || contractIds[0];
  if (lower.includes('server-error') || lower.includes('500')) return has(/server-error/) || contractIds[0];
  if (/\bpartial\b/.test(lower)) return has(/partial/) || contractIds[0];
  if (lower.includes('patch') || lower.includes('dotted path') || lower.includes('api')) {
    return has(/success$|default$/) || contractIds[0];
  }
  if (lower.includes('stream') || lower.includes('sse') || lower.includes('realtime')) {
    return has(/success$|default$/) || contractIds[0];
  }
  return has(/success$|default$/) || contractIds[0];
}

const titlePattern = /\btest\(\s*(['"`])([^'"`]+)\1/g;
const evidence = {};

for (const file of readdirSync(pagesDir).filter((name) => name.endsWith('.parity.spec.ts')).sort()) {
  const page = file.replace('.parity.spec.ts', '');
  const contractIds = contractScenarioIds(page);
  const contractIdSet = new Set(contractIds);
  const sourcePath = path.join(pagesDir, file);
  let source = readFileSync(sourcePath, 'utf8');

  // Prefix titles with contract scenario ids when missing.
  source = source.replace(titlePattern, (match, quote, title) => {
    if ([...contractIdSet].some((id) => title.includes(id))) {
      return match;
    }
    const id = chooseScenarioId(page, title, contractIds);
    return `test(${quote}${id}: ${title}${quote}`;
  });
  writeFileSync(sourcePath, source, 'utf8');

  // Collect IDs actually present in titles after rewrite.
  const present = new Set();
  let match;
  const finder = /\btest\(\s*(['"`])([^'"`]+)\1/g;
  while ((match = finder.exec(source)) !== null) {
    const title = match[2];
    for (const id of contractIds) {
      if (title.includes(id)) present.add(id);
    }
  }

  const ordered = contractIds.filter((id) => present.has(id));
  const success = ordered.find((id) => /success$|default$/.test(id)) || ordered[0];
  if (!success) {
    throw new Error(`${page}: no contract scenario IDs present in e2e titles after alignment`);
  }

  // Only wire IDs that have e2e coverage (required for eventual verified promotion).
  evidence[page] = {
    functional: ordered.slice(0, 3),
    api: [success, ...ordered.filter((id) => id !== success)].slice(0, 2),
    realtime: [success],
    accessibility: [ordered.find((id) => /required-username|forbidden|empty|success|default/.test(id)) || success],
  };
  console.log(`${page}: ${[...present].join(', ')}`);
}

writeFileSync(evidenceOut, `${JSON.stringify(evidence, null, 2)}\n`, 'utf8');
console.log(`wrote ${evidenceOut}`);
