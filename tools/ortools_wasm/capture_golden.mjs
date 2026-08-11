import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

function readArgument(flag, fallback = null) {
  const index = process.argv.indexOf(flag);
  if (index === -1 || index + 1 >= process.argv.length) {
    if (fallback !== null) {
      return fallback;
    }
    throw new Error(`missing argument ${flag}`);
  }
  return process.argv[index + 1];
}

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(SCRIPT_DIR, '..', '..');

// Volatile fields depend on host speed / search nondeterminism and must never
// participate in a golden comparison.
const VOLATILE_KEYS = new Set([
  'wall_time_ms',
  'conflicts',
  'branches',
  'best_bound',
  'timeout_ms',
]);

function stripVolatile(value) {
  if (Array.isArray(value)) {
    return value.map(stripVolatile);
  }
  if (value && typeof value === 'object') {
    const result = {};
    for (const key of Object.keys(value).sort()) {
      if (VOLATILE_KEYS.has(key)) {
        continue;
      }
      result[key] = stripVolatile(value[key]);
    }
    return result;
  }
  return value;
}

// Compare a fresh capture against a committed baseline, on `result` only.
// Returns true when they agree; prints the offending fixtures when they do not.
async function compareAgainstBaseline(baselinePath, captures) {
  const baseline = JSON.parse(await fs.readFile(baselinePath, 'utf8'));
  const baselineCaptures = baseline?.captures ?? {};
  const baselineKeys = Object.keys(baselineCaptures).sort();
  const currentKeys = Object.keys(captures).sort();

  const missing = baselineKeys.filter((key) => !currentKeys.includes(key));
  const added = currentKeys.filter((key) => !baselineKeys.includes(key));
  const changed = baselineKeys
    .filter((key) => currentKeys.includes(key))
    .filter(
      (key) =>
        JSON.stringify(baselineCaptures[key]?.result) !== JSON.stringify(captures[key]?.result),
    );

  for (const key of missing) {
    console.error(`missing from capture: ${key}`);
  }
  for (const key of added) {
    console.error(`absent from baseline: ${key}`);
  }
  for (const key of changed) {
    console.error(`result differs: ${key}`);
  }

  if (missing.length || added.length || changed.length) {
    console.error(
      `\ngolden check FAILED against ${baselinePath} ` +
        `(${changed.length} changed, ${missing.length} missing, ${added.length} added)`,
    );
    return false;
  }
  console.log(`\ngolden check OK against ${baselinePath} (${currentKeys.length} fixtures)`);
  return true;
}

async function resolveArtifacts() {
  const explicitJs = process.argv.indexOf('--js');
  if (explicitJs !== -1) {
    return {
      jsPath: path.resolve(readArgument('--js')),
      wasmPath: path.resolve(readArgument('--wasm')),
      source: 'explicit',
    };
  }
  // Default to whatever the app actually loads, rather than a hardcoded
  // artifact version that silently goes stale.
  const runtimeManifestPath = path.join(
    REPO_ROOT,
    'frontend/vendor/ortools/runtime-manifest.json',
  );
  const runtimeManifest = JSON.parse(await fs.readFile(runtimeManifestPath, 'utf8'));
  const toRepoPath = (url) =>
    path.join(REPO_ROOT, String(url || '').replace(/^\/frontend\//, 'frontend/'));
  return {
    jsPath: toRepoPath(runtimeManifest.js_url),
    wasmPath: toRepoPath(runtimeManifest.wasm_url),
    source: `runtime-manifest (${runtimeManifest.artifact_version})`,
  };
}

async function main() {
  const outputPath = path.resolve(
    readArgument('--output', path.join(SCRIPT_DIR, 'golden', 'baseline.json')),
  );
  const repeat = Math.max(1, Math.trunc(Number(readArgument('--repeat', '1')) || 1));
  const { jsPath, wasmPath, source } = await resolveArtifacts();

  const tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'ortools-wasm-golden-'));
  const tempModulePath = path.join(tempDir, 'dispatch_replan_solver.mjs');
  await fs.copyFile(jsPath, tempModulePath);

  try {
    const moduleNamespace = await import(pathToFileURL(tempModulePath).href);
    const initModule = moduleNamespace.default;
    if (typeof initModule !== 'function') {
      throw new Error('solver module missing default initializer');
    }
    const solverModule = await initModule({
      locateFile(fileName) {
        return String(fileName || '').endsWith('.wasm') ? wasmPath : fileName;
      },
    });
    const solveCluster =
      typeof solverModule.solve_cluster === 'function'
        ? solverModule.solve_cluster.bind(solverModule)
        : null;
    if (!solveCluster) {
      throw new Error('solver export solve_cluster not found');
    }

    const fixtureDir = path.resolve(
      readArgument('--fixtures-dir', path.join(SCRIPT_DIR, 'fixtures')),
    );
    const fixtureFiles = (await fs.readdir(fixtureDir))
      .filter((entry) => entry.endsWith('.json'))
      .sort();

    const captures = {};
    for (const fixtureFile of fixtureFiles) {
      const fixture = JSON.parse(
        await fs.readFile(path.join(fixtureDir, fixtureFile), 'utf8'),
      );
      const requestJson = JSON.stringify(fixture.request);

      const timings = [];
      let response = null;
      let stable = null;
      let stableJson = null;
      for (let attempt = 0; attempt < repeat; attempt += 1) {
        const startedAt = process.hrtime.bigint();
        response = JSON.parse(String(solveCluster(requestJson) || '{}'));
        timings.push(Number(process.hrtime.bigint() - startedAt) / 1e6);
        const currentStable = stripVolatile(response);
        const currentStableJson = JSON.stringify(currentStable);
        if (stableJson !== null && currentStableJson !== stableJson) {
          throw new Error(
            `${fixtureFile} is nondeterministic: normalized result differs on repeat ${attempt + 1}`,
          );
        }
        stable = currentStable;
        stableJson = currentStableJson;
      }

      // Determinism guard: a fixture that varies run to run cannot serve as a
      // golden, and we want to know that now rather than during the rewrite.
      // `result` is the golden payload and the ONLY thing compared. Timing is
      // informational and inherently host-dependent, so it lives under a
      // separate key that `--check` never reads -- keeping it as a sibling of
      // `result` would make every comparison fail on machine speed alone.
      captures[fixtureFile] = {
        result: stable,
        volatile_info: {
          timing_ms: {
            min: Math.min(...timings),
            max: Math.max(...timings),
            median: timings.slice().sort((a, b) => a - b)[Math.floor(timings.length / 2)],
          },
          stage_wall_time_ms: (response?.solver_run_metadata?.objective_stage_results ?? []).map(
            (stage) => ({ stage: stage?.stage, wall_time_ms: stage?.wall_time_ms }),
          ),
        },
      };
      console.log(
        `${fixtureFile.padEnd(64)} ${captures[fixtureFile].volatile_info.timing_ms.median.toFixed(1)}ms  ${
          response?.solver_run_metadata?.solve_status ?? 'NO_STATUS'
        }`,
      );
    }

    const checkPath = readArgument('--check', '');
    if (checkPath) {
      return compareAgainstBaseline(path.resolve(checkPath), captures);
    }

    await fs.mkdir(path.dirname(outputPath), { recursive: true });
    await fs.writeFile(
      outputPath,
      `${JSON.stringify({ artifact_source: source, js_path: jsPath, captures }, null, 2)}\n`,
      'utf8',
    );
    console.log(`\ngolden written to ${outputPath}`);
    return true;
  } finally {
    await fs.rm(tempDir, { recursive: true, force: true });
  }
}

main()
  .then((ok) => {
    if (!ok) {
      process.exit(1);
    }
  })
  .catch((error) => {
    console.error(error?.stack || error?.message || String(error));
    process.exit(1);
  });
