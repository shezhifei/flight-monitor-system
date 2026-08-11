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

