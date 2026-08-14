import { describeApiError } from '@/composables/useApiError';

/**
 * Unwrap a backend response envelope into its payload.
 *
 * Backend success responses come in two live shapes:
 * - envelope: `{ success, data, ... }` produced by most v2 routes
 * - raw payload: arrays/objects returned directly by some routes
 *
 * Semantics (kept identical to the historical `unwrapEnvelope` copies):
 * - non-object payload (including `null`/`undefined`) resolves to `null`
 * - envelope without a `data` key (or `data: null`) resolves to `null`
 * - otherwise the payload itself is returned
 *
 * Returns `null` instead of throwing so callers can fall back to a default.
 */
export function unwrapApiData<T>(payload: unknown): T | null {
  if (!payload || typeof payload !== 'object') return null;
  const record = payload as Record<string, unknown>;
  return ('data' in record ? (record.data ?? null) : payload) as T | null;
}

/**
 * Like `unwrapApiData`, but throws instead of returning `null`.
 *
 * - missing / non-object payload throws `Error(fallback)`
 * - `success === false` throws the extracted server message, or `fallback`
 * - envelope `data` (when defined) is returned
 * - otherwise the payload itself is returned
 */
export function unwrapApiDataOrThrow<T>(payload: unknown, fallback: string): T {
  if (!payload || typeof payload !== 'object') {
    throw new Error(fallback);
  }
  const record = payload as Record<string, unknown>;
  if (record.success === false) {
    throw new Error(describeApiError(record, fallback));
  }
  if ('data' in record && record.data !== undefined) {
    return record.data as T;
  }
  return payload as T;
}

/**
 * Extract a readable error message from an API failure result.
 *
 * Looks inside `result.data` for `error` (string or object with `message`)
 * or a top-level `message`; falls back to `fallback (HTTP <status>)`.
 */
export function readApiErrorMessage(
  result: { data?: unknown; status: number },
  fallback: string,
): string {
  const extracted = describeApiError(result.data, '');
  if (extracted) return extracted;
  return `${fallback} (HTTP ${result.status})`;
}