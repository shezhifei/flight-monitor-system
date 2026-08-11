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

function readPath(payload, dottedPath) {
  return dottedPath.split('.').reduce((current, segment) => {
    if (segment === 'length' && Array.isArray(current)) {
      return current.length;
    }
    if (current === null || current === undefined) {
      return undefined;
    }
    if (/^\d+$/.test(segment)) {
      return current[Number(segment)];
    }
    return current[segment];
  }, payload);
}

async function main() {
  const jsPath = path.resolve(readArgument('--js'));
  const wasmPath = path.resolve(readArgument('--wasm'));
  const tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'ortools-wasm-fixture-'));
  const tempModulePath = path.join(tempDir, 'dispatch_replan_solver.mjs');
  await fs.copyFile(jsPath, tempModulePath);
  const moduleNamespace = await import(pathToFileURL(tempModulePath).href);
  const initModule = moduleNamespace.default;
  if (typeof initModule !== 'function') {
    throw new Error('solver module missing default initializer');
  }
  try {
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

    const fixtureDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), 'fixtures');
    const fixtureEntries = await fs.readdir(fixtureDir);
    const fixtureFiles = fixtureEntries.filter((entry) => entry.endsWith('.json')).sort();
    for (const fixtureFile of fixtureFiles) {
      const rawFixture = await fs.readFile(path.join(fixtureDir, fixtureFile), 'utf8');
      const fixture = JSON.parse(rawFixture);
      const response = JSON.parse(String(solveCluster(JSON.stringify(fixture.request)) || '{}'));
      for (const [pathKey, expectedValue] of Object.entries(fixture.equals || {})) {
        const actualValue = readPath(response, pathKey);
        const actualJson = JSON.stringify(actualValue);
        const expectedJson = JSON.stringify(expectedValue);
        if (actualJson !== expectedJson) {
          throw new Error(`${fixtureFile}: ${pathKey} expected ${expectedJson}, got ${actualJson}`);
        }
      }
    }
  } finally {
    await fs.rm(tempDir, { recursive: true, force: true });
  }
}

main().catch((error) => {
  console.error(error?.stack || error?.message || String(error));
  process.exit(1);
});
