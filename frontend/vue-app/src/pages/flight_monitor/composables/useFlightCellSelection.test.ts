// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import {
  MAX_BATCH_CELL_EDIT,
  cellKey,
  parseCellKey,
  useFlightCellSelection,
} from './useFlightCellSelection';

const ORDERED = ['f1', 'f2', 'f3', 'f4', 'f5'] as const;

describe('cellKey / parseCellKey', () => {
  it('builds and parses flightId::field keys', () => {
    expect(cellKey('ABC', 'stand')).toBe('ABC::stand');
    expect(parseCellKey('ABC::stand')).toEqual({ flightId: 'ABC', field: 'stand' });
    expect(parseCellKey('bad')).toBeNull();
    expect(parseCellKey('::field')).toBeNull();
  });
});

describe('useFlightCellSelection', () => {
  it('selects a single cell on beginSelection', () => {
    const sel = useFlightCellSelection();
    const result = sel.beginSelection('f1', 'stand');
    expect(result.ok).toBe(true);
    expect(sel.selectedField.value).toBe('stand');
    expect(sel.selectedCount.value).toBe(1);
    expect(sel.isCellSelected('f1', 'stand')).toBe(true);
    expect(sel.canSubmitBatch.value).toBe(true);
  });

  it('rejects cross-column additive toggle with reason', () => {
    const sel = useFlightCellSelection();
    sel.beginSelection('f1', 'stand');
    const result = sel.toggleCell('f2', 'gate');
    expect(result.ok).toBe(false);
    expect(result.reason).toBe('cross_column');
    expect(sel.isCellSelected('f1', 'stand')).toBe(true);
    expect(sel.selectedCount.value).toBe(1);
  });

  it('replaces selection when clicking a different column without additive', () => {
    const sel = useFlightCellSelection();
    sel.beginSelection('f1', 'stand');
    const result = sel.beginSelection('f2', 'scheduled_departure');
    expect(result.ok).toBe(true);
    expect(sel.selectedField.value).toBe('scheduled_departure');
    expect(sel.isCellSelected('f1', 'stand')).toBe(false);
    expect(sel.isCellSelected('f2', 'scheduled_departure')).toBe(true);
  });

  it('toggles same-column cells with Ctrl-style additive mode', () => {
    const sel = useFlightCellSelection();
    sel.beginSelection('f1', 'stand');
    sel.toggleCell('f3', 'stand');
    expect(sel.selectedCount.value).toBe(2);
    expect(sel.isCellSelected('f1', 'stand')).toBe(true);
    expect(sel.isCellSelected('f3', 'stand')).toBe(true);

    sel.toggleCell('f1', 'stand');
    expect(sel.selectedCount.value).toBe(1);
    expect(sel.isCellSelected('f1', 'stand')).toBe(false);
    expect(sel.isCellSelected('f3', 'stand')).toBe(true);
  });

  it('selects a contiguous range via orderedFlightIds', () => {
    const sel = useFlightCellSelection();
    sel.beginSelection('f2', 'stand');
    const result = sel.extendTo('f4', 'stand', { orderedFlightIds: ORDERED });
    expect(result.ok).toBe(true);
    expect(sel.selectedCount.value).toBe(3);
    expect(sel.isCellSelected('f2', 'stand')).toBe(true);
    expect(sel.isCellSelected('f3', 'stand')).toBe(true);
    expect(sel.isCellSelected('f4', 'stand')).toBe(true);
    expect(sel.isCellSelected('f1', 'stand')).toBe(false);
    expect(sel.isCellSelected('f5', 'stand')).toBe(false);
  });

  it('selectRange works both directions and supports additive base', () => {
    const sel = useFlightCellSelection();
    const result = sel.selectRange('f4', 'f2', 'cobt_time', ORDERED);
    expect(result.ok).toBe(true);
    expect(sel.selectedField.value).toBe('cobt_time');
    expect(sel.selectedCount.value).toBe(3);
    expect(sel.selectedFlightIds.value).toEqual(expect.arrayContaining(['f2', 'f3', 'f4']));
  });

  it('drag start/update/end selects a range', () => {
    const sel = useFlightCellSelection();
    expect(sel.startDrag('f1', 'stand').ok).toBe(true);
    expect(sel.dragging.value).toBe(true);
    expect(sel.updateDrag('f3', 'stand', ORDERED).ok).toBe(true);
    expect(sel.selectedCount.value).toBe(3);
    sel.endDrag();
    expect(sel.dragging.value).toBe(false);
    expect(sel.isCellSelected('f2', 'stand')).toBe(true);
  });

  it('clearSelection resets all state (Esc-friendly)', () => {
    const sel = useFlightCellSelection();
    sel.beginSelection('f1', 'stand');
    sel.extendTo('f3', 'stand', { orderedFlightIds: ORDERED });
    sel.clearSelection();
    expect(sel.selectedField.value).toBeNull();
    expect(sel.selectedCount.value).toBe(0);
    expect(sel.anchorFlightId.value).toBeNull();
    expect(sel.activeFlightId.value).toBeNull();
    expect(sel.dragging.value).toBe(false);
    expect(sel.canSubmitBatch.value).toBe(false);
  });

  it('isCellSelected is O(1) Set membership and ignores empty ids', () => {
    const sel = useFlightCellSelection();
    sel.beginSelection('f1', 'stand');
    expect(sel.isCellSelected('', 'stand')).toBe(false);
    expect(sel.isCellSelected('f1', '')).toBe(false);
    expect(sel.isCellSelected('f1', 'stand')).toBe(true);
  });

  it('canSubmitBatch is false when empty or over MAX_BATCH_CELL_EDIT', () => {
    const sel = useFlightCellSelection();
    expect(sel.canSubmitBatch.value).toBe(false);

    sel.beginSelection('f0', 'stand');
    expect(sel.canSubmitBatch.value).toBe(true);

    // Build an over-limit selection via selectRange with a long ordered list.
    const many = Array.from({ length: MAX_BATCH_CELL_EDIT + 5 }, (_, i) => `fx${i}`);
    const over = sel.selectRange(many[0], many[many.length - 1], 'stand', many);
    expect(over.ok).toBe(false);
    expect(over.reason).toBe('over_limit');
    // Previous valid selection remains.
    expect(sel.selectedCount.value).toBe(1);
  });

  it('rejects empty flight id / field', () => {
    const sel = useFlightCellSelection();
    expect(sel.beginSelection('', 'stand').reason).toBe('empty_id');
    expect(sel.beginSelection('f1', '').reason).toBe('empty_field');
  });
});
