// 搬运自 frontend/ai-react/src/lib/utils.ts（仅 createRequestId / normalizeTime；
// 同文件的 toPrettyJson 在源仓库已无引用，属死代码，未搬运）。
export function createRequestId(prefix = 'req'): string {
  if (window.crypto && typeof window.crypto.randomUUID === 'function') {
    return `${prefix}_${window.crypto.randomUUID()}`;
  }
  return `${prefix}_${Date.now()}_${Math.random().toString(36).slice(2, 10)}`;
}

export function normalizeTime(value?: string): string {
  if (!value) return '';
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return value;
  return parsed.toLocaleString();
}
