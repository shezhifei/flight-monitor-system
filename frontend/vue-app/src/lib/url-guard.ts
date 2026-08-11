/**
 * URL security validation utilities.
 *
 * Centralizes same-origin and path-prefix checks used across
 * authenticated fetch, SSE, static asset loading, and worker script loading.
 */

export function assertSameOriginHttpUrl(
  url: string | URL,
  context = 'URL',
): URL {
  if (typeof window === 'undefined') {
    throw new Error(`${context} validation is only available in the browser`);
  }

  const resolved = new URL(url, window.location.origin);
  if (!['http:', 'https:'].includes(resolved.protocol) || resolved.origin !== window.location.origin) {
    throw new Error(`${context} only supports same-origin HTTP(S) URLs`);
  }

  return resolved;
}


export function assertStaticAssetUrl(
  url: string,
  allowedPrefix: string,
  context = 'static asset',
): string {
  const resolved = assertSameOriginHttpUrl(url, context);
  if (!resolved.pathname.startsWith(allowedPrefix)) {
    throw new Error(`${context} must stay under ${allowedPrefix}`);
  }
  return `${resolved.pathname}${resolved.search}${resolved.hash}`;
}


export function assertWorkerScriptUrl(url: string): string {
  const resolved = assertSameOriginHttpUrl(url, 'Worker script');
  return resolved.toString();
}
