import type { ApiResult } from './useApi';

/**
 * Extract a human-readable error message from an API error body.
 *
 * Handles both the flat shape (`{ detail | message | error }`) used by several
 * legacy endpoints and the unified error envelope
 * (`{ success: false, error: { message, kind, ... } }`) produced by the Rust
 * `ApiError`. Returns `fallback` when no usable text is present.
 *
 * Consolidates the previously duplicated `describeApiFailure` /
 * `describeError` helpers across composables.
 */
export function describeApiError(data: unknown, fallback: string): string {
  if (!data || typeof data !== 'object') return fallback;
  const record = data as Record<string, unknown>;

  // Unified envelope: prefer the nested error.message.
  const envelope = record.error;
  if (envelope && typeof envelope === 'object') {
    const nested = envelope as Record<string, unknown>;
    const message = nested.message;
    if (typeof message === 'string' && message.trim()) return message.trim();
  }

  for (const key of ['detail', 'message', 'error'] as const) {
    const value = record[key];
    if (typeof value === 'string' && value.trim()) return value.trim();
  }

  return fallback;
}

/**
 * The machine-discriminable error category from the unified envelope, if any.
 * Clients should branch on this rather than parsing `message` text.
 */
export function apiErrorKind(data: unknown): string | null {
  if (!data || typeof data !== 'object') return null;
  const envelope = (data as Record<string, unknown>).error;
  if (envelope && typeof envelope === 'object') {
    const kind = (envelope as Record<string, unknown>).kind;
    if (typeof kind === 'string' && kind.trim()) return kind.trim();
  }
  return null;
}

export interface RetryOn5xxOptions {
  /** Number of retries after the initial attempt. */
  retries?: number;
  /** Base backoff delay in ms; grows exponentially per attempt. */
  baseDelayMs?: number;
}

const delay = (ms: number) => (ms > 0 ? new Promise<void>(resolve => setTimeout(resolve, ms)) : Promise.resolve());

/**
 * Run an API call and retry it with exponential backoff *only* on 5xx
 * responses. 4xx (including 401, which `useAuth` refreshes/retries on its own)
 * is returned immediately to avoid double-handling. Returns the last result.
 */
export async function retryOn5xx<T>(
  call: () => Promise<ApiResult<T>>,
  options: RetryOn5xxOptions = {},
): Promise<ApiResult<T>> {
  const retries = Math.max(0, options.retries ?? 2);
  const baseDelayMs = options.baseDelayMs ?? 300;

  let result = await call();
  for (let attempt = 0; attempt < retries && result.status >= 500; attempt += 1) {
    await delay(baseDelayMs * 2 ** attempt);
    result = await call();
  }
  return result;
}

export function useApiError() {
  return { describeApiError, apiErrorKind, retryOn5xx };
}
