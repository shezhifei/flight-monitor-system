import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

function readArgument(flag) {
  const index = process.argv.indexOf(flag);
  if (index === -1 || index + 1 >= process.argv.length) {
    throw new Error(`missing argument ${flag}`);
  }
  return process.argv[index + 1];
}

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(SCRIPT_DIR, '..', '..');

// Resolve whatever the app actually loads, rather than a hardcoded artifact
// version that silently goes stale across releases.
async function resolveArtifacts() {
  if (process.argv.indexOf('--js') !== -1) {
    return {
      jsPath: path.resolve(readArgument('--js')),
      wasmPath: path.resolve(readArgument('--wasm')),
    };
  }
  const runtimeManifest = JSON.parse(
    await fs.readFile(
      path.join(REPO_ROOT, 'frontend/vendor/ortools/runtime-manifest.json'),
      'utf8',
    ),
  );
  const toRepoPath = (url) =>
    path.join(REPO_ROOT, String(url || '').replace(/^\/frontend\//, 'frontend/'));
  return {
    jsPath: toRepoPath(runtimeManifest.js_url),
    wasmPath: toRepoPath(runtimeManifest.wasm_url),
  };
}

async function main() {
  const snapshotPath = path.resolve(readArgument('--snapshot'));
  const outputPath = path.resolve(readArgument('--output'));
  const { jsPath, wasmPath } = await resolveArtifacts();

  const tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'ortools-wasm-snapshot-'));
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
        if (String(fileName || '').endsWith('.wasm')) {
          return wasmPath;
        }
        return fileName;
      }
    });
    const solveCluster = typeof solverModule.solve_cluster === 'function'
      ? solverModule.solve_cluster.bind(solverModule)
      : null;
    if (!solveCluster) {
      throw new Error('solver export solve_cluster not found');
    }

    const snapshot = JSON.parse(await fs.readFile(snapshotPath, 'utf8'));
    const response = JSON.parse(String(solveCluster(JSON.stringify(snapshot)) || '{}'));
    const payload = {
      snapshot_path: snapshotPath,
      payload: response,
    };
    await fs.mkdir(path.dirname(outputPath), { recursive: true });
    await fs.writeFile(outputPath, JSON.stringify(payload, null, 2), 'utf8');
    const solverMetadata = response?.solver_run_metadata && typeof response.solver_run_metadata === 'object'
      ? response.solver_run_metadata
      : (response?.solver_metadata && typeof response.solver_metadata === 'object'
        ? response.solver_metadata
        : {});
    const objectiveBreakdown = response?.objective_breakdown && typeof response.objective_breakdown === 'object'
      ? response.objective_breakdown
      : {};
    console.log(JSON.stringify({
      order_result_count: Array.isArray(response?.order_results) ? response.order_results.length : 0,
      personnel_slot_assignment_count: Array.isArray(response?.personnel_slot_assignments) ? response.personnel_slot_assignments.length : 0,
      equipment_slot_assignment_count: Array.isArray(response?.equipment_slot_assignments) ? response.equipment_slot_assignments.length : 0,
      continuity_decision_count: Array.isArray(response?.continuity_decisions) ? response.continuity_decisions.length : 0,
      solve_status: solverMetadata?.solve_status ?? null,
      total_lateness_minutes: solverMetadata?.total_lateness_minutes ?? objectiveBreakdown?.total_lateness_minutes ?? null,
      slot_gap: objectiveBreakdown?.slot_gap ?? null,
      continuity_penalty: objectiveBreakdown?.continuity_penalty ?? null,
      timed_out: solverMetadata?.timed_out ?? null,
      wall_time_ms: solverMetadata?.wall_time_ms ?? null,
    }));
  } finally {
    await fs.rm(tempDir, { recursive: true, force: true });
  }
}

main().catch((error) => {
  console.error(error?.stack || error?.message || String(error));
  process.exit(1);
});
