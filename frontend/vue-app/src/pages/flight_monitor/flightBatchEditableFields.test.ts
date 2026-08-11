import { describe, expect, it } from 'vitest';
import {
  BATCH_EDITABLE_FIELDS,
  getBatchEditableField,
  getBatchFieldWriteStrategy,
  isBatchEditableField,
} from './flightBatchEditableFields';

describe('flightBatchEditableFields registry', () => {
  it('registers phase-1 fields with unique keys', () => {
    const keys = BATCH_EDITABLE_FIELDS.map((f) => f.field);
    expect(new Set(keys).size).toBe(keys.length);
    expect(keys).toEqual(expect.arrayContaining([
      'scheduled_departure',
      'scheduled_arrival',
      'stand',
      'cobt_time',
      'boarding_allowed_time',
      'start_boarding_time',
      'end_boarding_time',
      'on_blocks_time',
      'off_blocks_time',
      'baggage_carousel',
      'flight_remarks',
    ]));
  });

  it('marks external sync-controlled snapshot fields adminOnly', () => {
    for (const field of [
      'scheduled_departure',
      'scheduled_arrival',
      'stand',
      'cobt_time',
      'baggage_carousel',
    ]) {
      expect(getBatchEditableField(field)?.adminOnly).toBe(true);
    }
    expect(getBatchEditableField('flight_remarks')?.adminOnly).toBeFalsy();
    expect(getBatchEditableField('start_boarding_time')?.adminOnly).toBeFalsy();
  });

  it('classifies write strategies', () => {
    expect(getBatchFieldWriteStrategy('stand')).toBe('flight_patch');
    expect(getBatchFieldWriteStrategy('cobt_time')).toBe('flight_patch');
    expect(getBatchFieldWriteStrategy('boarding_allowed_time')).toBe('timeline_event');
    expect(getBatchFieldWriteStrategy('off_blocks_time')).toBe('timeline_event');
  });

  it('rejects unknown fields', () => {
    expect(isBatchEditableField('status')).toBe(false);
    expect(isBatchEditableField('gate')).toBe(false);
    expect(getBatchEditableField('not_a_field')).toBeUndefined();
  });
});
