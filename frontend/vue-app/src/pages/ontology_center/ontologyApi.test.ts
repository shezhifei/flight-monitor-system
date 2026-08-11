import { describe, expect, it } from 'vitest';
import { unwrapData } from './ontologyApi';
import { idField, suggestionStatusTone, linkStatusTone } from './types';

describe('ontologyApi unwrapData', () => {
  it('unwraps success envelope', () => {
    expect(unwrapData<{ a: number }>({ success: true, data: { a: 1 } })).toEqual({ a: 1 });
  });

  it('returns bare payload', () => {
    expect(unwrapData<number[]>([1, 2])).toEqual([1, 2]);
  });

  it('returns null for empty envelope data', () => {
    expect(unwrapData({ success: true, data: null })).toBeNull();
  });
});

describe('ontology display helpers', () => {
  it('normalizes flight id wrappers', () => {
    expect(idField('FL1')).toBe('FL1');
    expect(idField({ 0: 'FL2' })).toBe('FL2');
  });

  it('maps suggestion and link status tones', () => {
    expect(suggestionStatusTone('pending')).toBe('warn');
    expect(suggestionStatusTone('accepted_executed')).toBe('ok');
    expect(linkStatusTone('active')).toBe('ok');
    expect(linkStatusTone('broken')).toBe('danger');
  });
});
