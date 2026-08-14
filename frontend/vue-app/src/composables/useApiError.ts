/**
 * Extract a human-readable error message from an API error body.
 *
 * Backend error responses come in two live shapes:
 * - flat: `{ detail | message | error }` used by several route families
 *   (e.g. flowable drafts, business case types/workflows, workflow dispatch)
 * - unified envelope: `{ success: false, error: { message, kind, ... } }`
 *   produced by the Rust `ApiError`.
 *
 * Returns `fallback` when no usable text is present.
 */
export function describeApiError(data: unknown, fallback: string): string {
  if (!data || typeof data !== 'object') return fallback;
  const record = data as Record<string, unknown>;

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