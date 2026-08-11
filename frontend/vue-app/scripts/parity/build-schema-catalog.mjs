import { mkdir, readdir, readFile, stat, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const vueAppRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const pagesRoot = path.join(vueAppRoot, 'parity', 'fixtures', 'pages');
const outRoot = path.join(vueAppRoot, 'parity', 'fixtures', 'schema');

const samples = new Map();

function bodyRichness(body) {
  if (body == null) return 0;
  const text = JSON.stringify(body);
  if (text.includes('"items":[]') || text.includes('"data":[]') || text.includes('"flags":[]')) {
    return 1;
  }
  if (Array.isArray(body) && body.length === 0) return 1;
  return text.length;
}

function rememberRoute(route, source) {
  if (!route?.method || !route?.pathname || !route?.response) return;
  if (Number(route.response.status ?? 200) !== 200 || route.response.body == null) return;
  const key = `${route.method.toUpperCase()} ${route.pathname}`;
  const candidate = {
    method: route.method.toUpperCase(),
    pathname: route.pathname,
    body: route.response.body,
    source,
  };
  const existing = samples.get(key);
  if (!existing || bodyRichness(candidate.body) > bodyRichness(existing.body)) {
    samples.set(key, candidate);
  }
}

async function main() {
  await mkdir(outRoot, { recursive: true });
  for (const page of await readdir(pagesRoot)) {
    const dir = path.join(pagesRoot, page);
    if (!(await stat(dir)).isDirectory()) continue;
    for (const file of await readdir(dir)) {
      if (!file.endsWith('.json') || file === 'manifest.json') continue;
      const definition = JSON.parse(await readFile(path.join(dir, file), 'utf8'));
      for (const route of definition.routes ?? []) {
        rememberRoute(route, `${page}/${file}`);
      }
    }
  }

  const catalog = [...samples.values()].sort((left, right) => (
    `${left.pathname} ${left.method}`.localeCompare(`${right.pathname} ${right.method}`)
  ));

  await writeFile(
    path.join(outRoot, 'README.md'),
    [
      '# Rust-shaped fixture catalog',
      '',
      'Representative HTTP 200 bodies collected from tracked page fixtures.',
      'Field names must match backend DTOs (`services/api-server/crates/application`).',
      'Do not invent Vue-only aliases in this directory.',
      '',
    ].join('\n'),
    'utf8',
  );

  await writeFile(
    path.join(outRoot, 'catalog.json'),
    `${JSON.stringify({ generated_from: 'parity/fixtures/pages', count: catalog.length, routes: catalog }, null, 2)}\n`,
    'utf8',
  );

  const focused = [
    ['GET', '/api/v2/anomalies'],
    ['GET', '/api/v2/anomalies/stats'],
    ['GET', '/api/v2/system/flags'],
    ['GET', '/api/v2/auth/users'],
    ['GET', '/api/v2/auth/roles'],
    ['GET', '/api/v2/auth/permissions'],
    ['GET', '/api/v2/auth/admin/permission-templates'],
    ['GET', '/api/v2/dispatch/teams'],
    ['GET', '/api/v2/dispatch/equipment'],
    ['GET', '/api/v2/dispatch/team-types'],
    ['GET', '/api/v2/dispatch/equipment-types'],
  ];

  for (const [method, pathname] of focused) {
    const hit = catalog.find((route) => route.method === method && route.pathname === pathname);
    if (!hit) {
      console.warn(`missing ${method} ${pathname}`);
      continue;
    }
    const filename = `${method.toLowerCase()}${pathname.replaceAll('/', '__')}.json`;
    await writeFile(path.join(outRoot, filename), `${JSON.stringify(hit, null, 2)}\n`, 'utf8');
    console.log(`wrote ${filename}`);
  }

  console.log(`catalog routes: ${catalog.length}`);
}

await main();
