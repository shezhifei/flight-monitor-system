import { computed, ref, type ComputedRef, type Ref } from 'vue';

/** Maximum cells accepted by a single batch-cells submit. */
export const MAX_BATCH_CELL_EDIT = 200;

export type CellSelectRejectReason =
  | 'cross_column'
  | 'empty_id'
  | 'empty_field'
  | 'over_limit';

export interface CellSelectResult {
  ok: boolean;
  reason?: CellSelectRejectReason;
  message?: string;
}

export interface UseFlightCellSelectionReturn {
  selectedField: Ref<string | null>;
  selectedCellKeys: Ref<Set<string>>;
  anchorFlightId: Ref<string | null>;
  activeFlightId: Ref<string | null>;
  dragging: Ref<boolean>;
  additive: Ref<boolean>;
  selectedCount: ComputedRef<number>;
  canSubmitBatch: ComputedRef<boolean>;
  selectedFlightIds: ComputedRef<string[]>;
  cellKey: (flightId: string, field: string) => string;
  parseCellKey: (key: string) => { flightId: string; field: string } | null;
  isCellSelected: (flightId: string, field: string) => boolean;
  clearSelection: () => void;
  beginSelection: (
    flightId: string,
    field: string,
    options?: { additive?: boolean; orderedFlightIds?: readonly string[] },
  ) => CellSelectResult;
  toggleCell: (
    flightId: string,
    field: string,
    options?: { orderedFlightIds?: readonly string[] },
  ) => CellSelectResult;
  extendTo: (
    flightId: string,
    field: string,
    options?: { orderedFlightIds?: readonly string[] },
  ) => CellSelectResult;
  selectRange: (
    fromFlightId: string,
    toFlightId: string,
    field: string,
    orderedFlightIds: readonly string[],
    options?: { additive?: boolean },
  ) => CellSelectResult;
  startDrag: (flightId: string, field: string, options?: { additive?: boolean }) => CellSelectResult;
  updateDrag: (flightId: string, field: string, orderedFlightIds: readonly string[]) => CellSelectResult;
  endDrag: () => void;
}

export function cellKey(flightId: string, field: string): string {
  return `${String(flightId)}::${String(field)}`;
}

export function parseCellKey(key: string): { flightId: string; field: string } | null {
  const separator = key.indexOf('::');
  if (separator <= 0 || separator === key.length - 2) {
    return null;
  }
  const flightId = key.slice(0, separator);
  const field = key.slice(separator + 2);
  if (!flightId || !field) {
    return null;
  }
  return { flightId, field };
}

function normalizeId(value: string | null | undefined): string {
  return String(value ?? '').trim();
}

export function useFlightCellSelection(): UseFlightCellSelectionReturn {
  const selectedField = ref<string | null>(null);
  const selectedCellKeys = ref<Set<string>>(new Set());
  const anchorFlightId = ref<string | null>(null);
  const activeFlightId = ref<string | null>(null);
  const dragging = ref(false);
  const additive = ref(false);

  /** Snapshot of keys present when an additive drag starts (for range replace within additive mode). */
  const additiveBaseKeys = ref<Set<string>>(new Set());

  const selectedCount = computed(() => selectedCellKeys.value.size);
  const canSubmitBatch = computed(() => {
    const count = selectedCellKeys.value.size;
    return count >= 1 && count <= MAX_BATCH_CELL_EDIT && selectedField.value !== null;
  });

  const selectedFlightIds = computed(() => {
    const ids: string[] = [];
    const seen = new Set<string>();
    for (const key of selectedCellKeys.value) {
      const parsed = parseCellKey(key);
      if (!parsed || seen.has(parsed.flightId)) {
        continue;
      }
      seen.add(parsed.flightId);
      ids.push(parsed.flightId);
    }
    return ids;
  });

  function isCellSelected(flightId: string, field: string): boolean {
    const id = normalizeId(flightId);
    const f = normalizeId(field);
    if (!id || !f) {
      return false;
    }
    return selectedCellKeys.value.has(cellKey(id, f));
  }

  function clearSelection(): void {
    selectedField.value = null;
    selectedCellKeys.value = new Set();
    anchorFlightId.value = null;
    activeFlightId.value = null;
    dragging.value = false;
    additive.value = false;
    additiveBaseKeys.value = new Set();
  }

  function rejectCrossColumn(field: string): CellSelectResult {
    return {
      ok: false,
      reason: 'cross_column',
      message: `只能在同一列内多选（当前列：${selectedField.value}，尝试：${field}）`,
    };
  }

  function setKeys(next: Set<string>): void {
    // Always assign a new Set so Vue tracks the change.
    selectedCellKeys.value = next;
  }

  function applyRangeKeys(
    fromFlightId: string,
    toFlightId: string,
    field: string,
    orderedFlightIds: readonly string[],
    base: Set<string>,
  ): CellSelectResult {
    const fromId = normalizeId(fromFlightId);
    const toId = normalizeId(toFlightId);
    const f = normalizeId(field);
    if (!fromId || !toId || !f) {
      return { ok: false, reason: 'empty_id', message: '航班标识或字段缺失' };
    }

    const fromIndex = orderedFlightIds.findIndex((id) => normalizeId(id) === fromId);
    const toIndex = orderedFlightIds.findIndex((id) => normalizeId(id) === toId);
    if (fromIndex < 0 || toIndex < 0) {
      // Fall back to single-cell when either side is not in the ordered list.
      const next = new Set(base);
      next.add(cellKey(toId, f));
      if (next.size > MAX_BATCH_CELL_EDIT) {
        return {
          ok: false,
          reason: 'over_limit',
          message: `批量编辑最多 ${MAX_BATCH_CELL_EDIT} 个单元格`,
        };
      }
      setKeys(next);
      activeFlightId.value = toId;
      return { ok: true };
    }

    const start = Math.min(fromIndex, toIndex);
    const end = Math.max(fromIndex, toIndex);
    const next = new Set(base);
    for (let i = start; i <= end; i += 1) {
      const id = normalizeId(orderedFlightIds[i]);
      if (id) {
        next.add(cellKey(id, f));
      }
    }
    if (next.size > MAX_BATCH_CELL_EDIT) {
      return {
        ok: false,
        reason: 'over_limit',
        message: `批量编辑最多 ${MAX_BATCH_CELL_EDIT} 个单元格`,
      };
    }
    setKeys(next);
    activeFlightId.value = toId;
    return { ok: true };
  }

  function beginSelection(
    flightId: string,
    field: string,
    options: { additive?: boolean; orderedFlightIds?: readonly string[] } = {},
  ): CellSelectResult {
    const id = normalizeId(flightId);
    const f = normalizeId(field);
    if (!id) {
      return { ok: false, reason: 'empty_id', message: '航班标识缺失' };
    }
    if (!f) {
      return { ok: false, reason: 'empty_field', message: '字段缺失' };
    }

    const isAdditive = Boolean(options.additive);

    if (selectedField.value && selectedField.value !== f) {
      if (isAdditive) {
        return rejectCrossColumn(f);
      }
      // Non-additive click on another column replaces the selection.
      clearSelection();
    }

    if (isAdditive && selectedField.value === f) {
      additive.value = true;
      additiveBaseKeys.value = new Set(selectedCellKeys.value);
      const next = new Set(selectedCellKeys.value);
      const key = cellKey(id, f);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      if (next.size > MAX_BATCH_CELL_EDIT) {
        return {
          ok: false,
          reason: 'over_limit',
          message: `批量编辑最多 ${MAX_BATCH_CELL_EDIT} 个单元格`,
        };
      }
      selectedField.value = f;
      setKeys(next);
      anchorFlightId.value = id;
      activeFlightId.value = id;
      return { ok: true };
    }

    selectedField.value = f;
    setKeys(new Set([cellKey(id, f)]));
    anchorFlightId.value = id;
    activeFlightId.value = id;
    additive.value = false;
    additiveBaseKeys.value = new Set();
    return { ok: true };
  }

  function toggleCell(
    flightId: string,
    field: string,
  ): CellSelectResult {
    return beginSelection(flightId, field, { additive: true });
  }

  function selectRange(
    fromFlightId: string,
    toFlightId: string,
    field: string,
    orderedFlightIds: readonly string[],
    options: { additive?: boolean } = {},
  ): CellSelectResult {
    const f = normalizeId(field);
    if (!f) {
      return { ok: false, reason: 'empty_field', message: '字段缺失' };
    }
    if (selectedField.value && selectedField.value !== f) {
      return rejectCrossColumn(f);
    }
    selectedField.value = f;
    const base = options.additive
      ? new Set(additiveBaseKeys.value.size ? additiveBaseKeys.value : selectedCellKeys.value)
      : new Set<string>();
    if (!options.additive) {
      anchorFlightId.value = normalizeId(fromFlightId) || anchorFlightId.value;
    }
    return applyRangeKeys(fromFlightId, toFlightId, f, orderedFlightIds, base);
  }

  function extendTo(
    flightId: string,
    field: string,
    options: { orderedFlightIds?: readonly string[] } = {},
  ): CellSelectResult {
    const id = normalizeId(flightId);
    const f = normalizeId(field);
    if (!id || !f) {
      return { ok: false, reason: 'empty_id', message: '航班标识或字段缺失' };
    }
    if (!selectedField.value) {
      return beginSelection(id, f);
    }
    if (selectedField.value !== f) {
      return rejectCrossColumn(f);
    }
    const ordered = options.orderedFlightIds ?? [];
    const anchor = anchorFlightId.value || id;
    const base = additive.value
      ? new Set(additiveBaseKeys.value)
      : new Set<string>();
    return applyRangeKeys(anchor, id, f, ordered, base);
  }

  function startDrag(
    flightId: string,
    field: string,
    options: { additive?: boolean } = {},
  ): CellSelectResult {
    const result = beginSelection(flightId, field, { additive: options.additive });
    if (result.ok) {
      dragging.value = true;
      if (options.additive) {
        additive.value = true;
        // Base is keys after toggle of the anchor cell for additive drag.
        additiveBaseKeys.value = new Set(selectedCellKeys.value);
        // Re-include the anchor so range extension does not drop it if toggle removed it.
        // For additive drag UX: keep the post-begin state as the range base.
      }
    }
    return result;
  }

  function updateDrag(
    flightId: string,
    field: string,
    orderedFlightIds: readonly string[],
  ): CellSelectResult {
    if (!dragging.value) {
      return { ok: false, reason: 'empty_id', message: '未开始拖拽选择' };
    }
    return extendTo(flightId, field, { orderedFlightIds });
  }

  function endDrag(): void {
    dragging.value = false;
  }

  return {
    selectedField,
    selectedCellKeys,
    anchorFlightId,
    activeFlightId,
    dragging,
    additive,
    selectedCount,
    canSubmitBatch,
    selectedFlightIds,
    cellKey,
    parseCellKey,
    isCellSelected,
    clearSelection,
    beginSelection,
    toggleCell,
    extendTo,
    selectRange,
    startDrag,
    updateDrag,
    endDrag,
  };
}
