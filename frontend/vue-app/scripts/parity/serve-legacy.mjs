import { createReadStream } from 'node:fs';
import { stat } from 'node:fs/promises';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  LEGACY_ASSET_ROOTS,
  LegacyRootValidationError,
  validateLegacyRoot,
} from './legacy-root.mjs';

const DEFAULT_HOST = '127.0.0.1';
const DEFAULT_PORT = 3100;
const allowedRoots = new Set(LEGACY_ASSET_ROOTS);
const allowedRootFiles = new Set(['favicon.ico', 'favicon-full.jpg']);

class LegacyRequestPathError extends Error {
  constructor(message) {
    super(message);
    this.name = 'LegacyRequestPathError';
  }
}

const contentTypes = new Map([
  ['.css', 'text/css; charset=utf-8'],
  ['.gif', 'image/gif'],
  ['.html', 'text/html; charset=utf-8'],
  ['.ico', 'image/x-icon'],
  ['.jpeg', 'image/jpeg'],
  ['.jpg', 'image/jpeg'],
  ['.js', 'text/javascript; charset=utf-8'],
  ['.json', 'application/json; charset=utf-8'],
  ['.map', 'application/json; charset=utf-8'],
  ['.otf', 'font/otf'],
  ['.png', 'image/png'],
  ['.svg', 'image/svg+xml'],
  ['.ttf', 'font/ttf'],
  ['.wasm', 'application/wasm'],
  ['.webp', 'image/webp'],
  ['.woff', 'font/woff'],
  ['.woff2', 'font/woff2'],
]);

function sendText(response, statusCode, message, headers = {}) {
  response.writeHead(statusCode, {
    'Cache-Control': 'no-store',
    'Content-Type': 'text/plain; charset=utf-8',
    'X-Content-Type-Options': 'nosniff',
    ...headers,
  });
  response.end(message);
}

function decodeRequestPath(requestUrl) {
  const rawPath = String(requestUrl ?? '').split('?', 1)[0];
  if (!rawPath.startsWith('/')) {
    throw new LegacyRequestPathError('request target must be an absolute path');
  }
  if (/%2f|%5c/i.test(rawPath)) {
    throw new LegacyRequestPathError('encoded path separators are not allowed');
  }

  let decoded;
  try {
    decoded = decodeURIComponent(rawPath);
  } catch {
    throw new LegacyRequestPathError('request path contains invalid percent encoding');
  }

  if (decoded.includes('\\') || decoded.includes('\0')) {
    throw new LegacyRequestPathError('request path contains forbidden characters');
  }
  if (decoded.split('/').some((segment) => segment === '.' || segment === '..')) {
    throw new LegacyRequestPathError('path traversal is not allowed');
  }
  return decoded;
}

function resolveRequestFile(legacyRoot, requestUrl) {
  const requestPath = decodeRequestPath(requestUrl);

  if (requestPath.startsWith('/frontend/')) {
    const relativeUrl = requestPath.slice('/frontend/'.length);
    const separatorIndex = relativeUrl.indexOf('/');
    if (separatorIndex <= 0) {
      return { kind: 'unknown' };
    }

    const rootName = relativeUrl.slice(0, separatorIndex);
    if (!allowedRoots.has(rootName)) {
      return { kind: 'unknown' };
    }

    const relativeFile = relativeUrl.slice(separatorIndex + 1);
    if (!relativeFile) {
      return { kind: 'unknown' };
    }

    const rootDirectory = path.resolve(legacyRoot, rootName);
    const candidate = path.resolve(rootDirectory, relativeFile);
    const relative = path.relative(rootDirectory, candidate);
    if (relative.startsWith('..') || path.isAbsolute(relative)) {
      throw new LegacyRequestPathError('resolved path escapes the allow-listed archive root');
    }
    return { kind: 'file', candidate };
  }

  const rootFilename = requestPath.slice(1);
  if (allowedRootFiles.has(rootFilename)) {
    return { kind: 'file', candidate: path.join(legacyRoot, rootFilename) };
  }

  return { kind: 'unknown' };
}

async function serveFile(request, response, candidate) {
  let fileStats;
  try {
    fileStats = await stat(candidate);
  } catch (error) {
    if (error?.code === 'ENOENT' || error?.code === 'ENOTDIR') {
      sendText(response, 404, 'Archive asset not found.');
      return;
    }
    throw error;
  }

  if (!fileStats.isFile()) {
    sendText(response, 404, 'Archive asset not found.');
    return;
  }

  const contentType = contentTypes.get(path.extname(candidate).toLowerCase())
    ?? 'application/octet-stream';
  response.writeHead(200, {
    'Cache-Control': 'no-store',
    'Content-Length': fileStats.size,
    'Content-Type': contentType,
    'X-Content-Type-Options': 'nosniff',
  });

  if (request.method === 'HEAD') {
    response.end();
    return;
  }

  const stream = createReadStream(candidate);
  stream.on('error', () => {
    if (!response.headersSent) {
      sendText(response, 500, 'Failed to read archive asset.');
    } else {
      response.destroy();
    }
  });
  stream.pipe(response);
}

export function createLegacyServer(legacyRoot) {
  return http.createServer(async (request, response) => {
    if (request.method !== 'GET' && request.method !== 'HEAD') {
      sendText(response, 405, 'Only GET and HEAD are supported.', { Allow: 'GET, HEAD' });
      return;
    }

    try {
      const resolved = resolveRequestFile(legacyRoot, request.url);
      if (resolved.kind === 'unknown') {
        sendText(response, 404, 'Unknown legacy archive route.');
        return;
      }
      await serveFile(request, response, resolved.candidate);
    } catch (error) {
      if (!(error instanceof LegacyRequestPathError)) {
        console.error('Legacy parity server failed to serve an archive asset.', error);
        sendText(response, 500, 'Failed to serve archive asset.');
        return;
      }
      sendText(response, 400, `Rejected legacy archive request: ${error.message}`);
    }
  });
}

export async function startLegacyServer(options = {}) {
  const validation = await validateLegacyRoot({
    root: options.root,
    environment: options.environment,
  });
  const host = options.host ?? DEFAULT_HOST;
  const port = Number(options.port ?? process.env.FMS_LEGACY_FRONTEND_PORT ?? DEFAULT_PORT);
  if (host !== DEFAULT_HOST) {
    throw new Error(`The legacy archive server must bind to ${DEFAULT_HOST}; received ${host}.`);
  }
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error(`Invalid legacy archive server port: ${port}`);
  }

  const server = createLegacyServer(validation.root);
  await new Promise((resolve, reject) => {
    const handleError = (error) => reject(error);
    server.once('error', handleError);
    server.listen(port, host, () => {
      server.off('error', handleError);
      resolve();
    });
  });

  return { server, host, port, legacyRoot: validation.root };
}

async function main() {
  try {
    const running = await startLegacyServer();
    console.log(`Legacy frontend archive: ${running.legacyRoot}`);
    console.log(`Legacy parity server: http://${running.host}:${running.port}`);
    console.log('Press Ctrl+C to stop.');
  } catch (error) {
    if (error instanceof LegacyRootValidationError) {
      console.error(error.message);
    } else if (error?.code === 'EADDRINUSE') {
      console.error(`Legacy parity server port ${DEFAULT_PORT} is already in use.`);
    } else {
      console.error(error instanceof Error ? error.message : String(error));
    }
    process.exitCode = 1;
  }
}

const isDirectExecution = process.argv[1]
  && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (isDirectExecution) {
  await main();
}
