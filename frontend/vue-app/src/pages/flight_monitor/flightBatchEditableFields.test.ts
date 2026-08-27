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
      'cobt_time',
      'boarding_allowed_time',
      'start_boarding_time',
      'end_boarding_time',
      'on_blocks_time',
      'off_blocks_time',
      'flight_remarks',
    ]));
  });

  it('keeps occupancy-owned display columns out of the registry (PR3)', () => {
    // stand/gate/terminal/baggage_carousel 真相在占用服务，监控只读展示。
    for (const field of ['stand', 'gate', 'terminal', 'baggage_carousel']) {
      expect(isBatchEditableField(field)).toBe(false);
      expect(getBatchEditableField(field)).toBeUndefined();
    }
  });

  it('marks external sync-controlled snapshot fields adminOnly', () => {
    for (const field of [
      'scheduled_departure',
      'scheduled_arrival',
      'cobt_time',
    ]) {
      expect(getBatchEditableField(field)?.adminOnly).toBe(true);
    }
    expect(getBatchEditableField('flight_remarks')?.adminOnly).toBeFalsy();
    expect(getBatchEditableField('start_boarding_time')?.adminOnly).toBeFalsy();
  });

  it('classifies write strategies', () => {
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
