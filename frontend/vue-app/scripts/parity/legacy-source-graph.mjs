import { createHash } from 'node:crypto';
import { readFile, stat } from 'node:fs/promises';
import path from 'node:path';

function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
}

function compareCodePoints(left, right) {
  return left === right ? 0 : left < right ? -1 : 1;
}

async function fileExists(candidate) {
  try {
    return (await stat(candidate)).isFile();
  } catch {
    return false;
  }
}

function parseAttributes(tag) {
  const attributes = {};
  const pattern = /([:\w-]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+)))?/g;
  for (const match of tag.matchAll(pattern)) {
    attributes[match[1].toLowerCase()] = match[2] ?? match[3] ?? match[4] ?? '';
  }
  return attributes;
}

function toArchivePath(reference, sourceArchivePath) {
  const trimmed = reference.trim();
  if (!trimmed || /^(?:data:|javascript:|mailto:|tel:|#)/i.test(trimmed)) return null;
  if (/^https?:\/\//i.test(trimmed) || trimmed.startsWith('//')) return null;

  const withoutQuery = trimmed.split(/[?#]/, 1)[0];
  let decoded;
  try {
    decoded = decodeURIComponent(withoutQuery).replaceAll('\\', '/');
  } catch {
    return null;
  }

  let archivePath;
  if (decoded.startsWith('/frontend/')) {
    archivePath = decoded.slice('/frontend/'.length);
  } else if (decoded === '/favicon.ico' || decoded === '/favicon-full.jpg') {
    archivePath = decoded.slice(1);
  } else if (decoded.startsWith('/')) {
    return null;
  } else {
    archivePath = path.posix.join(path.posix.dirname(sourceArchivePath), decoded);
  }

  const normalized = path.posix.normalize(archivePath);
  if (normalized === '..' || normalized.startsWith('../') || path.posix.isAbsolute(normalized)) return null;
  return normalized;
}

async function buildAssetReference(legacyRoot, sourceArchivePath, reference, expectedKind) {
  if (reference.includes('${')) {
    return {
      kind: 'dynamic',
      reference,
      archivePath: null,
      exists: false,
      sha256: null,
    };
  }
  const archivePath = toArchivePath(reference, sourceArchivePath);
  if (!archivePath) {
    return {
      kind: /^https?:\/\//i.test(reference) || reference.startsWith('//') ? 'external' : expectedKind,
      reference,
      archivePath: null,
      exists: false,
      sha256: null,
    };
  }

  const candidate = path.join(legacyRoot, ...archivePath.split('/'));
  const exists = await fileExists(candidate);
  return {
    kind: expectedKind,
    reference,
    archivePath,
    exists,
    sha256: exists ? sha256(await readFile(candidate)) : null,
  };
}

function isIgnorableReference(reference) {
  return !reference || /^(?:data:|javascript:|mailto:|tel:|#)/i.test(reference.trim());
}

function cssReferenceTokens(css) {
  const imports = [];
  const assets = [];
  const importPattern = /@import\s+(?:url\(\s*)?(?:["']([^"']+)["']|([^\s;)]+))\s*\)?[^;]*;/gi;
  for (const match of css.matchAll(importPattern)) {
    const reference = match[1] ?? match[2];
    if (!isIgnorableReference(reference)) imports.push(reference.trim());
  }

  const urlPattern = /url\(\s*(?:["']([^"']+)["']|([^\s)]+))\s*\)/gi;
  for (const match of css.matchAll(urlPattern)) {
    const reference = match[1] ?? match[2];
    if (!isIgnorableReference(reference) && !imports.includes(reference.trim())) {
      assets.push(reference.trim());
    }
  }
  return { imports, assets };
}

function javascriptAssetReferences(source) {
  const references = [];
  const pattern = /["'`](\/frontend\/(?:static|vendor|icons|images|fonts)\/[^"'`?#]+|(?:\.\.?\/)*(?:static|vendor|icons|images|fonts)\/[^"'`?#]+)[?#]?[^"'`]*/gi;
  for (const match of source.matchAll(pattern)) {
    if (!isIgnorableReference(match[1])) references.push(match[1]);
  }
  return references;
}

function deduplicateAssetReferences(references) {
  const unique = new Map();
  for (const reference of references) {
    const key = `${reference.kind}|${reference.archivePath ?? reference.reference}`;
    if (!unique.has(key)) unique.set(key, reference);
  }
  return [...unique.values()].sort((left, right) => compareCodePoints(
    `${left.archivePath ?? left.reference}|${left.kind}`,
    `${right.archivePath ?? right.reference}|${right.kind}`,
  ));
}

export async function extractLegacySourceContract(legacyRoot, htmlArchivePath) {
  const htmlPath = path.join(legacyRoot, ...htmlArchivePath.split('/'));
  const html = await readFile(htmlPath, 'utf8');
  const scripts = [];
  const stylesheets = [];
  const assets = [];
  const stylesheetQueue = [];
  const tagPattern = /<([a-z][\w:-]*)\b[^>]*>/gi;

  for (const match of html.matchAll(tagPattern)) {
    const tagName = match[1].toLowerCase();
    const attributes = parseAttributes(match[0]);
    if (tagName === 'script' && !isIgnorableReference(attributes.src)) {
      scripts.push(await buildAssetReference(legacyRoot, htmlArchivePath, attributes.src, 'script'));
    }

    if (tagName === 'link' && !isIgnorableReference(attributes.href)) {
      const relTokens = String(attributes.rel ?? '').toLowerCase().split(/\s+/).filter(Boolean);
      const kind = relTokens.includes('stylesheet') ? 'stylesheet' : 'asset';
      const built = await buildAssetReference(legacyRoot, htmlArchivePath, attributes.href, kind);
      if (kind === 'stylesheet') {
        stylesheets.push(built);
        if (built.exists && built.archivePath) stylesheetQueue.push(built);
      } else {
        assets.push(built);
      }
    }

    for (const attributeName of ['src', 'poster']) {
      const reference = attributes[attributeName];
      if (tagName !== 'script' && !isIgnorableReference(reference)) {
        assets.push(await buildAssetReference(legacyRoot, htmlArchivePath, reference, 'asset'));
      }
    }

    if (!isIgnorableReference(attributes.srcset)) {
      for (const candidate of attributes.srcset.split(',')) {
        const reference = candidate.trim().split(/\s+/, 1)[0];
        if (!isIgnorableReference(reference)) {
          assets.push(await buildAssetReference(legacyRoot, htmlArchivePath, reference, 'asset'));
        }
      }
    }

    if (attributes.style) {
      const inlineReferences = cssReferenceTokens(attributes.style);
      for (const reference of [...inlineReferences.imports, ...inlineReferences.assets]) {
        assets.push(await buildAssetReference(legacyRoot, htmlArchivePath, reference, 'asset'));
      }
    }
  }

  for (const styleMatch of html.matchAll(/<style\b[^>]*>([\s\S]*?)<\/style>/gi)) {
    const inlineReferences = cssReferenceTokens(styleMatch[1]);
    for (const reference of inlineReferences.imports) {
      const built = await buildAssetReference(legacyRoot, htmlArchivePath, reference, 'stylesheet');
      stylesheets.push(built);
      if (built.exists && built.archivePath) stylesheetQueue.push(built);
    }
    for (const reference of inlineReferences.assets) {
      assets.push(await buildAssetReference(legacyRoot, htmlArchivePath, reference, 'asset'));
    }
  }

  const visitedStylesheets = new Set();
  while (stylesheetQueue.length > 0) {
    const stylesheet = stylesheetQueue.shift();
    if (!stylesheet?.archivePath || visitedStylesheets.has(stylesheet.archivePath)) continue;
    visitedStylesheets.add(stylesheet.archivePath);
    const stylesheetPath = path.join(legacyRoot, ...stylesheet.archivePath.split('/'));
    const css = await readFile(stylesheetPath, 'utf8');
    const references = cssReferenceTokens(css);
    for (const reference of references.imports) {
      const imported = await buildAssetReference(legacyRoot, stylesheet.archivePath, reference, 'stylesheet');
      stylesheets.push(imported);
      if (imported.exists && imported.archivePath) stylesheetQueue.push(imported);
    }
    for (const reference of references.assets) {
      assets.push(await buildAssetReference(legacyRoot, stylesheet.archivePath, reference, 'asset'));
    }
  }

  const scriptSources = [{ archivePath: htmlArchivePath, source: html }];
  for (const script of scripts) {
    if (!script.exists || !script.archivePath) continue;
    const scriptPath = path.join(legacyRoot, ...script.archivePath.split('/'));
    scriptSources.push({ archivePath: script.archivePath, source: await readFile(scriptPath, 'utf8') });
  }
  for (const scriptSource of scriptSources) {
    for (const reference of javascriptAssetReferences(scriptSource.source)) {
      assets.push(await buildAssetReference(legacyRoot, scriptSource.archivePath, reference, 'asset'));
    }
  }

  const uniqueScripts = deduplicateAssetReferences(scripts);
  const uniqueStylesheets = deduplicateAssetReferences(stylesheets);
  const uniqueAssets = deduplicateAssetReferences(assets);
  const hashMaterial = [
    `${htmlArchivePath}:${sha256(html)}`,
    ...[...uniqueScripts, ...uniqueStylesheets, ...uniqueAssets].map((asset) => (
      `${asset.archivePath ?? asset.reference}:${asset.sha256 ?? 'missing'}`
    )),
  ].join('\n');
  const localFiles = new Map([[htmlArchivePath, sha256(html)]]);
  for (const asset of [...uniqueScripts, ...uniqueStylesheets, ...uniqueAssets]) {
    if (asset.exists && asset.archivePath && asset.sha256) localFiles.set(asset.archivePath, asset.sha256);
  }

  return {
    html: htmlArchivePath,
    htmlSha256: sha256(html),
    scripts: uniqueScripts,
    stylesheets: uniqueStylesheets,
    assets: uniqueAssets,
    sourceHash: sha256(hashMaterial),
    sourceFiles: [...localFiles.entries()]
      .sort(([left], [right]) => compareCodePoints(left, right))
      .map(([sourcePath, digest]) => ({ path: sourcePath, sha256: digest })),
  };
}
