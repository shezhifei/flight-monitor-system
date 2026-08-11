/**
 * Ontology V1 API client helpers.
 * Endpoints under `/api/v2/ontology/*`.
 */

import type { ApiResult } from '@/composables/useApi';

export type ApiClient = {
  get: <T>(url: string, options?: RequestInit) => Promise<ApiResult<T>>;
  post: <T>(url: string, body?: unknown, options?: RequestInit) => Promise<ApiResult<T>>;
  patch: <T>(url: string, body?: unknown, options?: RequestInit) => Promise<ApiResult<T>>;
};

/** Unwrap `{ success, data }` or bare payload. */
export function unwrapData<T>(payload: unknown): T | null {
  if (payload && typeof payload === 'object' && 'data' in (payload as Record<string, unknown>)) {
    return ((payload as Record<string, unknown>).data ?? null) as T | null;
  }
  return (payload ?? null) as T | null;
}

export async function extractApiError(response: Response, fallback: string): Promise<string> {
  try {
    const body = await response.clone().json();
    if (body && typeof body === 'object') {
      const err = (body as { error?: { message?: string }; message?: string }).error?.message
        ?? (body as { message?: string }).message;
      if (typeof err === 'string' && err.trim()) return err;
    }
  } catch {
    // ignore
  }
  return `${fallback} (${response.status})`;
}

export async function ontologyGet<T>(
  api: ApiClient,
  path: string,
): Promise<{ ok: true; data: T } | { ok: false; error: string }> {
  const res = await api.get<unknown>(path);
  if (!res.ok) {
    return { ok: false, error: await extractApiError(res.response, '请求失败') };
  }
  const data = unwrapData<T>(res.data);
  if (data === null || data === undefined) {
    return { ok: false, error: '响应为空' };
  }
  return { ok: true, data };
}

export async function ontologyPost<T>(
  api: ApiClient,
  path: string,
  body?: unknown,
): Promise<{ ok: true; data: T } | { ok: false; error: string }> {
  const res = await api.post<unknown>(path, body, {
    headers: { 'Content-Type': 'application/json' },
  });
  if (!res.ok) {
    return { ok: false, error: await extractApiError(res.response, '操作失败') };
  }
  const data = unwrapData<T>(res.data);
  return { ok: true, data: data as T };
}

export async function ontologyPatch<T>(
  api: ApiClient,
  path: string,
  body?: unknown,
): Promise<{ ok: true; data: T } | { ok: false; error: string }> {
  const res = await api.patch<unknown>(path, body, {
    headers: { 'Content-Type': 'application/json' },
  });
  if (!res.ok) {
    return { ok: false, error: await extractApiError(res.response, '更新失败') };
  }
  const data = unwrapData<T>(res.data);
  return { ok: true, data: data as T };
}

export const ONTOLOGY_BASE = '/api/v2/ontology';
