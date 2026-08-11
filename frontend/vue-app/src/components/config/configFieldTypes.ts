export type ConfigValueType =
  | 'boolean'
  | 'integer'
  | 'float'
  | 'string'
  | 'password'
  | 'list'
  | 'readonly';

export interface ConfigFieldItem {
  id: string;
  title: string;
  path?: string;
  description?: string;
  type: ConfigValueType;
  value: unknown;
  masked?: boolean;
  disabled?: boolean;
}

/** path 最后一段做人读标题 */
export function humanizeConfigPath(path: string): string {
  const last = path.split(/[./]/).filter(Boolean).pop() || path;
  return last
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .replace(/[-_]+/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

/** 丢掉无信息的英文模板描述 */
export function cleanConfigDescription(desc?: string | null, path?: string): string | undefined {
  if (!desc?.trim()) return undefined;
  const t = desc.trim();
  if (/^configuration for\s+/i.test(t)) return undefined;
  if (path) {
    const compact = t.replace(/\s/g, '').toLowerCase();
    if (compact === `configurationfor${path}`.toLowerCase()) return undefined;
  }
  return t;
}

export function isSensitiveConfigPath(path: string): boolean {
  return /password|secret|token|encryption\.key|(^|[./_])key($|[./_])/i.test(path);
}
