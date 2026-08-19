// 搬运自 frontend/ai-react/src/lib/http/apiClient.ts + lib/http/legacyEnvelope.ts。
// 适配点：传输层由 authFetch 换成注入的 ApiLike（页面侧传 useApi() 的返回值），
// 信封解包语义（{success, data, message, detail} → ApiError）原样保留。
import type { ApiRequestOptions, ApiResult } from '@/composables/useApi';

/**
 * 传输层接口，与 useApi() 返回值结构兼容（get/post/put/delete 返回
 * {ok, status, data}；raw 返回原始 Response，供 SSE 流式请求使用）。
 */
export interface ApiLike {
  get<T>(url: string, options?: ApiRequestOptions): Promise<ApiResult<T>>;
  post<T>(url: string, body?: unknown, options?: ApiRequestOptions): Promise<ApiResult<T>>;
  put<T>(url: string, body?: unknown, options?: ApiRequestOptions): Promise<ApiResult<T>>;
  delete<T>(url: string, options?: ApiRequestOptions): Promise<ApiResult<T>>;
  raw(input: RequestInfo | URL, options?: ApiRequestOptions): Promise<Response>;
}

export class ApiError extends Error {
  public readonly status: number;
  public readonly payload: unknown;

  constructor(message: string, status: number, payload: unknown) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.payload = payload;
  }
}

export interface LegacyEnvelope<T> {
  success?: boolean;
  message?: string;
  detail?: string | { message?: string; code?: string };
  data?: T;
}

export function normalizeEnvelope<T>(payload: unknown): LegacyEnvelope<T> {
  if (payload && typeof payload === 'object') {
    return payload as LegacyEnvelope<T>;
  }
  return {};
}

export function envelopeErrorMessage(payload: LegacyEnvelope<unknown>, fallback: string): string {
  if (typeof payload.message === 'string' && payload.message.trim()) {
    return payload.message.trim();
  }
  if (typeof payload.detail === 'string' && payload.detail.trim()) {
    return payload.detail.trim();
  }
  if (payload.detail && typeof payload.detail === 'object') {
    const detailObj = payload.detail as { message?: string; code?: string };
    const code = detailObj.code ? `[${detailObj.code}] ` : '';
    const msg = detailObj.message ? detailObj.message : '';
    if (msg) {
      return `${code}${msg}`;
    }
  }
  return fallback;
}

export type EnvelopeMethod = 'GET' | 'POST' | 'PUT' | 'DELETE';

/**
 * 等价于 ai-react 的 requestEnvelope(url, init)：HTTP 非 2xx 抛 ApiError
 * （message 取信封 message/detail），2xx 但 success === false 同样抛
 * ApiError（status 记 200），否则返回 envelope.data ?? 原始 payload。
 */
export async function requestEnvelope<T>(
  api: ApiLike,
  url: string,
  method: EnvelopeMethod = 'GET',
  body?: unknown,
): Promise<T> {
  const result: ApiResult<unknown> =
    method === 'POST'
      ? await api.post<unknown>(url, body)
      : method === 'PUT'
        ? await api.put<unknown>(url, body)
        : method === 'DELETE'
          ? await api.delete<unknown>(url)
          : await api.get<unknown>(url);

  // useApi 在 content-type 非 JSON 时把 body 解析为 string；ai-react 的
  // requestJson 会将其包成 { message: text }，这里保持同样语义。
  const payload = typeof result.data === 'string' ? { message: result.data } : result.data;

  if (!result.ok) {
    const normalized = normalizeEnvelope(payload);
    const message = envelopeErrorMessage(normalized, `HTTP ${result.status}`);
    throw new ApiError(message, result.status, payload);
  }

  const envelope = normalizeEnvelope<T>(payload);
  if (envelope.success === false) {
    throw new ApiError(
      envelopeErrorMessage(envelope as unknown as { message?: string; detail?: string }, '请求失败'),
      200,
      payload,
    );
  }
  return (envelope.data ?? (payload as T)) as T;
}
