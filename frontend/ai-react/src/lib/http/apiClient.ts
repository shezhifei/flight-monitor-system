import { authFetch } from '@/lib/auth/authBridge';
import { envelopeErrorMessage, normalizeEnvelope } from '@/lib/http/legacyEnvelope';

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

async function parseResponseBody(response: Response): Promise<unknown> {
  const contentType = response.headers.get('content-type') || '';
  if (contentType.includes('application/json')) {
    return response.json().catch(() => ({}));
  }
  const text = await response.text().catch(() => '');
  return text ? { message: text } : {};
}

export async function requestJson<T>(url: string, init?: RequestInit): Promise<T> {
  const response = await authFetch(url, init);
  const payload = await parseResponseBody(response);
  if (!response.ok) {
    const normalized = normalizeEnvelope(payload);
    const message = envelopeErrorMessage(normalized, `HTTP ${response.status}`);
    throw new ApiError(message, response.status, payload);
  }
  return payload as T;
}

export async function requestEnvelope<T>(url: string, init?: RequestInit): Promise<T> {
  const payload = await requestJson<unknown>(url, init);
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

