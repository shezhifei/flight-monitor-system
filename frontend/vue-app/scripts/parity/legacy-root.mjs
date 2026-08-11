import { access, readdir, realpath, stat } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const LEGACY_ASSET_ROOTS = Object.freeze([
  'html',
  'js',
  'css',
  'static',
  'vendor',
  'icons',
  'images',
  'fonts',
]);

export const LEGACY_HTML_FILES = Object.freeze([
  'ai_config_center.html',
  'ai_monitor.html',
  'anomaly_monitor.html',
  'command_center.html',
  'dashboard.html',
  'dispatch_board.html',
  'dispatch_rule_center.html',
  'flight_imports.html',
  'flight_monitor.html',
  'flowable_modeler.html',
  'kpi_dashboard.html',
  'label_manager.html',
  'llm_eval_lab.html',
  'login.html',
  'nl_query.html',
  'operations_review_report.html',
  'resource_manager.html',
  'resource_utilization.html',
  'system_flags.html',
  'system_status.html',
  'user_manager.html',
]);

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const defaultLegacyRoot = path.resolve(
  scriptDirectory,
  '..',
  '..',
  '..',
  'backup',
  'legacy-frontend-archive',
);

export class LegacyRootValidationError extends Error {
  constructor(root, problems) {
    const details = problems.map((problem) => `  - ${problem}`).join('\n');
    super(`Legacy frontend archive validation failed at:\n  ${root}\n${details}`);
    this.name = 'LegacyRootValidationError';
    this.root = root;
    this.problems = problems;
  }
}

export function resolveLegacyRoot(environment = process.env) {
  const configuredRoot = environment.FMS_LEGACY_FRONTEND_ROOT?.trim();
  return configuredRoot ? path.resolve(configuredRoot) : defaultLegacyRoot;
}

async function isDirectory(candidate) {
  try {
    return (await stat(candidate)).isDirectory();
  } catch {
    return false;
  }
}

async function isFile(candidate) {
  try {
    await access(candidate);
    return (await stat(candidate)).isFile();
  } catch {
    return false;
  }
}

export async function validateLegacyRoot(options = {}) {
  const resolvedRoot = path.resolve(options.root ?? resolveLegacyRoot(options.environment));
  const problems = [];

  if (!(await isDirectory(resolvedRoot))) {
    problems.push(`missing archive directory: ${resolvedRoot}`);
    throw new LegacyRootValidationError(resolvedRoot, problems);
  }

  for (const directory of LEGACY_ASSET_ROOTS) {
    const candidate = path.join(resolvedRoot, directory);
    if (!(await isDirectory(candidate))) {
      problems.push(`missing required directory: ${candidate}`);
    }
  }

  const htmlRoot = path.join(resolvedRoot, 'html');
  if (await isDirectory(htmlRoot)) {
    for (const filename of LEGACY_HTML_FILES) {
      const candidate = path.join(htmlRoot, filename);
      if (!(await isFile(candidate))) {
        problems.push(`missing required legacy page: ${candidate}`);
      }
    }

    const actualHtmlFiles = (await readdir(htmlRoot, { withFileTypes: true }))
      .filter((entry) => entry.isFile() && entry.name.toLowerCase().endsWith('.html'))
      .map((entry) => entry.name)
      .sort();
    const expected = new Set(LEGACY_HTML_FILES);
    for (const filename of actualHtmlFiles) {
      if (!expected.has(filename)) {
        problems.push(`unexpected legacy HTML page (update the explicit contract first): ${path.join(htmlRoot, filename)}`);
      }
    }
  }

  if (problems.length > 0) {
    throw new LegacyRootValidationError(resolvedRoot, problems);
  }

  return {
    root: await realpath(resolvedRoot),
    htmlFiles: [...LEGACY_HTML_FILES],
    assetRoots: [...LEGACY_ASSET_ROOTS],
  };
}

async function main() {
  try {
    const result = await validateLegacyRoot();
    console.log(`Legacy frontend root: ${result.root}`);
    console.log(`Validated ${result.assetRoots.length} required asset directories.`);
    console.log(`Validated ${result.htmlFiles.length} legacy HTML page files.`);
  } catch (error) {
    if (error instanceof LegacyRootValidationError) {
      console.error(error.message);
      process.exitCode = 1;
      return;
    }
    throw error;
  }
}

const isDirectExecution = process.argv[1]
  && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (isDirectExecution) {
  await main();
}
