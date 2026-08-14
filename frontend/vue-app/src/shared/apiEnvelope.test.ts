import { describe, expect, it } from 'vitest';
import { readApiErrorMessage, unwrapApiData, unwrapApiDataOrThrow } from './apiEnvelope';

describe('unwrapApiData', () => {
  it('passes through non-envelope payloads', () => {
    expect(unwrapApiData({ items: [1, 2] })).toEqual({ items: [1, 2] });
    expect(unwrapApiData([1, 2])).toEqual([1, 2]);
  });

  it('unwraps envelope data', () => {
    expect(unwrapApiData({ success: true, data: { id: 1 } })).toEqual({ id: 1 });
    expect(unwrapApiData({ success: true, data: { answer: 'x' } })).toEqual({ answer: 'x' });
  });

  it('returns null for missing payload or missing data', () => {
    expect(unwrapApiData(null)).toBeNull();
    expect(unwrapApiData(undefined)).toBeNull();
    expect(unwrapApiData(null)).toBeNull();
    expect(unwrapApiData('text')).toBeNull();
    expect(unwrapApiData({ data: null })).toBeNull();
    expect(unwrapApiData({ success: true, data: undefined })).toBeNull();
  });

  it('passes through payloads without a data key', () => {
    expect(unwrapApiData({ success: true })).toEqual({ success: true });
  });

  it('keeps falsy data values when present', () => {
    expect(unwrapApiData({ data: 0 })).toBe(0);
    expect(unwrapApiData({ data: false })).toBe(false);
    expect(unwrapApiData({ data: '' })).toBe('');
  });
});

describe('unwrapApiDataOrThrow', () => {
  it('throws fallback on missing or non-object payload', () => {
    expect(() => unwrapApiDataOrThrow(null, 'fb')).toThrow('fb');
    expect(() => unwrapApiDataOrThrow(undefined, 'fb')).toThrow('fb');
    expect(() => unwrapApiDataOrThrow('text', 'fb')).toThrow('fb');
  });

  it('throws extracted message when success is false', () => {
    expect(() => unwrapApiDataOrThrow({ success: false, error: { message: 'boom' } }, 'fb')).toThrow('boom');
    expect(() => unwrapApiDataOrThrow({ success: false, error: 'boom' }, 'fb')).toThrow('boom');
    expect(() => unwrapApiDataOrThrow({ success: false, message: 'boom' }, 'fb')).toThrow('boom');
    expect(() => unwrapApiDataOrThrow({ success: false }, 'fb')).toThrow('fb');
  });

  it('unwraps envelope data or passes the payload through', () => {
    expect(unwrapApiDataOrThrow({ success: true, data: { a: 1 } }, 'fb')).toEqual({ a: 1 });
    expect(unwrapApiDataOrThrow({ success: true }, 'fb')).toEqual({ success: true });
    expect(unwrapApiDataOrThrow({ items: [1] }, 'fb')).toEqual({ items: [1] });
  });
});

describe('readApiErrorMessage', () => {
  it('extracts error text from the body', () => {
    expect(readApiErrorMessage({ data: { error: 'boom' }, status: 500 }, 'fb')).toBe('boom');
    expect(readApiErrorMessage({ data: { error: { message: 'boom' } }, status: 500 }, 'fb')).toBe('boom');
    expect(readApiErrorMessage({ data: { message: 'boom' }, status: 404 }, 'fb')).toBe('boom');
  });

  it('falls back to fallback with HTTP status', () => {
    expect(readApiErrorMessage({ data: { ok: true }, status: 404 }, 'fb')).toBe('fb (HTTP 404)');
    expect(readApiErrorMessage({ data: null, status: 502 }, 'fb')).toBe('fb (HTTP 502)');
  });
});