import type { ApiResult } from '@/composables/useApi';
import type { DispatchBoardApiError } from './useDispatchBoardOrders';
import { unwrapApiData } from '@/shared/apiEnvelope';

export { unwrapApiData };

export function toStringArray(value: unknown): string[] {
  return (Array.isArray(value) ? value : [])
    .map((item) => String(item ?? '').trim())
    .filter(Boolean);
}

function extractApiErrorMessage(payload: unknown, fallbackMessage: string): string {
  if (!payload || typeof payload !== 'object') {
    return fallbackMessage;
  }

  const record = payload as Record<string, unknown>;
  if (typeof record.detail === 'string' && record.detail.trim()) {
    return record.detail.trim();
  }
  if (record.detail && typeof record.detail === 'object') {
    const detailRecord = record.detail as Record<string, unknown>;
    if (typeof detailRecord.message === 'string' && detailRecord.message.trim()) {
      return detailRecord.message.trim();
    }
  }
  if (typeof record.message === 'string' && record.message.trim()) {
    return record.message.trim();
  }

  return fallbackMessage;
}

function createDispatchBoardApiError(
  message: string,
  status: number,
  detail?: unknown,
): DispatchBoardApiError {
  const error = new Error(message) as DispatchBoardApiError;
  error.status = status;
  error.detail = detail;
  return error;
}

export function unwrapApiResultOrThrow<T>(result: ApiResult<unknown>, fallbackMessage: string): T {
  if (!result.ok) {
    const payload = result.data as Record<string, unknown> | null;
    const detail = payload && typeof payload === 'object' && 'detail' in payload
      ? payload.detail
      : payload;
    throw createDispatchBoardApiError(
      extractApiErrorMessage(payload, fallbackMessage),
      result.status,
      detail,
    );
  }

  return unwrapApiData(result.data) as T;
}
