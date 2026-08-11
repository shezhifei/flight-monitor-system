import { describe, it, expect } from 'vitest';
import { deriveInputAccept } from './aiInputAccept';

describe('deriveInputAccept', () => {
  it('disables upload for text-only models', () => {
    const r = deriveInputAccept(['text'], ['text/plain', 'image/png']);
    expect(r.uploadEnabled).toBe(false);
    expect(r.accept).toBe('');
    expect(r.mimeTypes).toEqual([]);
  });

  it('disables upload when no modalities provided', () => {
    const r = deriveInputAccept(undefined, ['image/png']);
    expect(r.uploadEnabled).toBe(false);
  });

  it('keeps only image MIME types when only image modality enabled', () => {
    const r = deriveInputAccept(['text', 'image'], ['text/plain', 'image/png', 'image/jpeg', 'audio/wav']);
    expect(r.uploadEnabled).toBe(true);
    expect(r.mimeTypes).toEqual(['image/png', 'image/jpeg']);
    expect(r.accept).toBe('image/png,image/jpeg');
  });

  it('keeps audio MIME types when audio modality enabled', () => {
    const r = deriveInputAccept(['audio'], ['audio/wav', 'image/png']);
    expect(r.mimeTypes).toEqual(['audio/wav']);
  });

  it('file modality acts as a fallback allowing any non-text declared MIME', () => {
    const r = deriveInputAccept(['file'], ['application/pdf', 'text/plain']);
    expect(r.uploadEnabled).toBe(true);
    expect(r.mimeTypes).toEqual(['application/pdf']);
  });

  it('excludes text/* from accept even when modalities allow upload', () => {
    const r = deriveInputAccept(['image'], ['text/plain', 'text/csv']);
    // no concrete image MIME -> wildcard fallback
    expect(r.mimeTypes).toEqual(['image/*']);
  });

  it('falls back to wildcards when modality declared but no concrete MIME matches', () => {
    const r = deriveInputAccept(['image', 'audio'], []);
    expect(r.mimeTypes).toEqual(['image/*', 'audio/*']);
    expect(r.accept).toBe('image/*,audio/*');
  });

  it('dedupes repeated MIME entries', () => {
    const r = deriveInputAccept(['image'], ['image/png', 'image/png']);
    expect(r.mimeTypes).toEqual(['image/png']);
  });
});
