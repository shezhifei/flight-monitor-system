import path from 'node:path';

import { describe, expect, it } from 'vitest';

import { extractLegacySourceContract } from '../../scripts/parity/legacy-source-graph.mjs';

const legacyRoot = path.resolve(process.cwd(), '..', 'backup', 'legacy-frontend-archive');

describe('legacy source dependency graph', () => {
  it('recursively hashes login HTML, CSS imports, fonts, images, and JavaScript-owned icons', async () => {
    const graph = await extractLegacySourceContract(legacyRoot, 'html/login.html');
    const stylesheetPaths = graph.stylesheets.map((asset) => asset.archivePath);
    const assetPaths = graph.assets.map((asset) => asset.archivePath);

    expect(stylesheetPaths).toEqual(expect.arrayContaining([
      'css/main.css',
      'css/variables.css',
      'css/base.css',
      'css/layout.css',
      'css/components.css',
      'css/tables.css',
      'css/apple-theme.css',
    ]));
    expect(assetPaths).toEqual(expect.arrayContaining([
      'fonts/MiSansVF.ttf',
      'images/index-pic-01.jpg',
      'icons/plane.svg',
      'icons/password_visible.svg',
      'icons/password_unvisible.svg',
      'icons/forbidden.svg',
      'icons/ok.svg',
    ]));
    expect([...graph.scripts, ...graph.stylesheets, ...graph.assets].filter(
      (asset) => asset.archivePath !== null && !asset.exists,
    )).toEqual([]);
  });

  it('is byte-for-byte deterministic for an unchanged archive', async () => {
    const first = await extractLegacySourceContract(legacyRoot, 'html/login.html');
    const second = await extractLegacySourceContract(legacyRoot, 'html/login.html');

    expect(second).toEqual(first);
  });
});
