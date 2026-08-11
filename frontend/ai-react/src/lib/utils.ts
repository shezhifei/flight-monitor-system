export function createRequestId(prefix = 'req'): string {
  if (window.crypto && typeof window.crypto.randomUUID === 'function') {
    return `${prefix}_${window.crypto.randomUUID()}`;
  }
  return `${prefix}_${Date.now()}_${Math.random().toString(36).slice(2, 10)}`;
}

export function toPrettyJson(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2);
  } catch (_error) {
    return String(value ?? '');
  }
}

export function normalizeTime(value?: string): string {
  if (!value) return '';
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return value;
  return parsed.toLocaleString();
}

