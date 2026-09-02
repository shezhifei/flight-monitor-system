import { computed, ref, type ComputedRef, type Ref } from 'vue';
import { useToast } from '../../../composables/useToast';
import { useAuth, hasUserPermission, type JwtUser } from '../../../composables/useAuth';
import {
  patchFlightBatchCells,
  type FlightBatchCellsResponse,
} from '../../../composables/useFlightCrud';
import {
  normalizeFlightId,
  resolveDirectionalFlightId,
  type Flight,
  type UseFlightDataReturn,
} from '../../../composables/useFlightData';
import {
  getBatchEditableField,
  getBatchFieldLabel,
  getBatchFieldValueType,
  isBatchEditableField,
  type BatchEditableField,
  type BatchValueType,
} from '../flightBatchEditableFields';
import {
  MAX_BATCH_CELL_EDIT,
  type UseFlightCellSelectionReturn,
} from './useFlightCellSelection';

export interface BatchEditModalState {
  isOpen: boolean;
  field: string;
  label: string;
  valueType: BatchValueType;
  value: string;
  flightIds: string[];
  saving: boolean;
  error: string | null;
  lastResult: FlightBatchCellsResponse | null;
}

export interface CellContextMenuState {
  isOpen: boolean;
  x: number;
  y: number;
  flightId: string;
  field: string;
  type: string;
  value: string;
  /** True when the context menu targets a multi-cell selection. */
  multi: boolean;
  selectedCount: number;
}

export interface UseFlightBatchEditOptions {
  flightData: UseFlightDataReturn;
  cellSelection: UseFlightCellSelectionReturn;
  announce: (message: string) => void;
  refreshFlights: () => Promise<void>;
  /** Optional single-cell fallback (existing field edit modal). */
  openSingleFieldEdit?: (flightId: string, field: string, type: string, value: string) => void;
  /** Single-cell time revoke (not batch). Prefer existing modal revoke path. */
  revokeSingleTimeField?: (flightId: string, field: string) => Promise<void>;
}

export interface UseFlightBatchEditReturn {
  modalState: Ref<BatchEditModalState>;
  contextMenuState: Ref<CellContextMenuState>;
  canManageFlights: ComputedRef<boolean>;
  isAdminUser: ComputedRef<boolean>;
  canEditField: (field: string) => boolean;
  /** Single datetime cell with a non-empty value and not multi-select. */
  canRevokeCurrentContext: ComputedRef<boolean>;
  canSubmitCurrent: ComputedRef<boolean>;
  selectedFieldMeta: ComputedRef<BatchEditableField | undefined>;
  openBatchEditFromSelection: () => void;
  openBatchEditForField: (field: string, flightIds: string[]) => void;
  closeBatchEdit: () => void;
  setBatchValue: (value: string) => void;
  submitBatchEdit: () => Promise<void>;
  handleCellContextMenu: (
    event: MouseEvent,
    flightId: string,
    field: string,
    type: string,
    value: string,
  ) => void;
  closeCellContextMenu: () => void;
  handleContextBatchEdit: () => void;
  handleContextSingleEdit: () => void;
  handleContextRevoke: () => Promise<void>;
  handleContextClearSelection: () => void;
  /** Escape / external clear. */
  clearAll: () => void;
}

function readFlightVersion(flight: Flight | null | undefined): number | null {
  if (!flight || typeof flight !== 'object') {
    return null;
  }
  const raw = (flight as Record<string, unknown>).version;
  if (typeof raw === 'number' && Number.isFinite(raw)) {
    return raw;
  }
  if (typeof raw === 'string' && raw.trim() !== '' && Number.isFinite(Number(raw))) {
    return Number(raw);
  }
  return null;
}

function readFlightFieldValue(flight: Flight | null | undefined, field: string): unknown {
  if (!flight || typeof flight !== 'object' || !field) {
    return null;
  }
  const raw = (flight as Record<string, unknown>)[field];
  return raw === undefined ? null : raw;
}

function createClientActionId(): string {
  try {
    if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
      return crypto.randomUUID().replace(/-/g, '').toUpperCase();
    }
  } catch {
    // ignore
  }
  return `BATCH${Date.now().toString(36).toUpperCase()}${Math.random().toString(36).slice(2, 10).toUpperCase()}`;
}

function toDatetimeLocalValue(raw: string): string {
  if (!raw) {
    return '';
  }
  const date = new Date(raw);
  if (Number.isNaN(date.getTime())) {
    // Already looks like datetime-local (YYYY-MM-DDTHH:mm)
    if (/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}/.test(raw)) {
      return raw.slice(0, 16);
    }
    return raw;
  }
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

function normalizeSubmitValue(value: string, valueType: BatchValueType): unknown {
  const trimmed = value.trim();
  if (valueType === 'datetime') {
    if (!trimmed) {
      return null;
    }
    const date = new Date(trimmed);
    if (Number.isNaN(date.getTime())) {
      throw new Error('时间格式无效');
    }
    return date.toISOString();
  }
  return trimmed === '' ? null : trimmed;
}

export function useFlightBatchEdit(options: UseFlightBatchEditOptions): UseFlightBatchEditReturn {
  const {
    flightData,
    cellSelection,
    announce,
    refreshFlights,
    openSingleFieldEdit,
    revokeSingleTimeField,
  } = options;
  const auth = useAuth();
  const toast = useToast();

  const modalState = ref<BatchEditModalState>({
    isOpen: false,
    field: '',
    label: '',
    valueType: 'text',
    value: '',
    flightIds: [],
    saving: false,
    error: null,
    lastResult: null,
  });

  const contextMenuState = ref<CellContextMenuState>({
    isOpen: false,
    x: 0,
    y: 0,
    flightId: '',
    field: '',
    type: 'text',
    value: '',
    multi: false,
    selectedCount: 0,
  });

  const canManageFlights = computed(() => hasUserPermission(auth.getUser(), 'flight.update'));
  const isAdminUser = computed(() => {
    const user = auth.getUser() as JwtUser | null;
    if (!user) {
      return false;
    }
    if (user.is_admin === true || user.role === 'admin') {
      return true;
    }
    const permissions = Array.isArray(user.permissions) ? user.permissions : [];
    return permissions.includes('*');
  });

  function canEditField(field: string): boolean {
    if (!canManageFlights.value) {
      return false;
    }
    if (!isBatchEditableField(field)) {
      return false;
    }
    const meta = getBatchEditableField(field);
    if (meta?.adminOnly && !isAdminUser.value) {
      return false;
    }
    return true;
  }

  const selectedFieldMeta = computed(() => {
    const field = cellSelection.selectedField.value;
    return field ? getBatchEditableField(field) : undefined;
  });

  const canRevokeCurrentContext = computed(() => {
    const state = contextMenuState.value;
    if (!state.isOpen || state.multi) {
      return false;
    }
    if (!canEditField(state.field)) {
      return false;
    }
    const meta = getBatchEditableField(state.field);
    if (!meta || meta.valueType !== 'datetime') {
      return false;
    }
    // Only offer revoke when the cell currently has a value.
    return Boolean(String(state.value || '').trim());
  });

  const canSubmitCurrent = computed(() => {
    if (!modalState.value.isOpen || modalState.value.saving) {
      return false;
    }
    if (!modalState.value.field || modalState.value.flightIds.length === 0) {
      return false;
    }
    if (modalState.value.flightIds.length > MAX_BATCH_CELL_EDIT) {
      return false;
    }
    const meta = getBatchEditableField(modalState.value.field);
    if (!meta) {
      return false;
    }
    if (meta.maxLength != null && modalState.value.value.length > meta.maxLength) {
      return false;
    }
    if (meta.valueType === 'datetime' && !modalState.value.value.trim()) {
      // Allow clear? design allows revoke via null; for batch we require a value unless explicitly empty clear.
      // Require non-empty for datetime batch fill.
      return false;
    }
    return canEditField(modalState.value.field);
  });

  function openBatchEditForField(field: string, flightIds: string[]): void {
    if (!canEditField(field)) {
      toast.showToast('warning', '当前账号无权编辑该字段', { duration: 4000 });
      return;
    }
    const uniqueIds = Array.from(new Set(flightIds.map((id) => normalizeFlightId(id)).filter(Boolean)));
    if (!uniqueIds.length) {
      toast.showToast('warning', '未选中任何单元格', { duration: 3000 });
      return;
    }
    if (uniqueIds.length > MAX_BATCH_CELL_EDIT) {
      toast.showToast('warning', `批量编辑最多 ${MAX_BATCH_CELL_EDIT} 个单元格`, { duration: 4000 });
      return;
    }

    const valueType = getBatchFieldValueType(field);
    // Seed value from the first selected flight's current field when homogeneous.
    const firstFlight = flightData.findFlightById(uniqueIds[0]);
    const rawFirst = firstFlight
      ? String((firstFlight as Record<string, unknown>)[field] ?? '')
      : '';
    const seedValue = valueType === 'datetime' ? toDatetimeLocalValue(rawFirst) : rawFirst;

    modalState.value = {
      isOpen: true,
      field,
      label: getBatchFieldLabel(field),
      valueType,
      value: seedValue,
      flightIds: uniqueIds,
      saving: false,
      error: null,
      lastResult: null,
    };
  }

  function openBatchEditFromSelection(): void {
    const field = cellSelection.selectedField.value;
    if (!field || !cellSelection.canSubmitBatch.value) {
      toast.showToast('warning', '请先在同一列选中至少一个单元格', { duration: 3000 });
      return;
    }
    openBatchEditForField(field, cellSelection.selectedFlightIds.value);
  }

  function closeBatchEdit(): void {
    modalState.value = {
      ...modalState.value,
      isOpen: false,
      saving: false,
      error: null,
    };
  }

  function setBatchValue(value: string): void {
    modalState.value.value = value;
    modalState.value.error = null;
  }

  async function submitBatchEdit(): Promise<void> {
    if (!canSubmitCurrent.value) {
      return;
    }
    const { field, flightIds, valueType } = modalState.value;
    let submitValue: unknown;
    try {
      submitValue = normalizeSubmitValue(modalState.value.value, valueType);
    } catch (err) {
      const message = err instanceof Error ? err.message : '输入无效';
      modalState.value.error = message;
      toast.showToast('warning', message, { duration: 4000 });
      return;
    }

    const meta = getBatchEditableField(field);
    if (meta?.maxLength != null && typeof submitValue === 'string' && submitValue.length > meta.maxLength) {
      const message = `${meta.label}最多 ${meta.maxLength} 个字符`;
      modalState.value.error = message;
      toast.showToast('warning', message, { duration: 4000 });
      return;
    }

    // batch-cells 的写入目标是方向航班，不是监控行 row_id（拆表后过站行的
    // row_id = 链 id = 已软删聚合行，后端拒绝写）。选中集合里携带的是 row_id，
    // 这里按字段方向解析成 inbound_flight_id / outbound_flight_id。
    const targets: Array<{ flight_id: string; expected_version: number | null; expected_value: unknown }> = [];
    const unresolvedRows: string[] = [];
    for (const flightId of flightIds) {
      const flight = flightData.findFlightById(flightId);
      const targetFlightId = flight ? resolveDirectionalFlightId(flight, field) : null;
      if (!targetFlightId) {
        unresolvedRows.push(flightId);
        continue;
      }
      targets.push({
        flight_id: targetFlightId,
        expected_version: readFlightVersion(flight),
        expected_value: readFlightFieldValue(flight, field),
      });
    }
    if (unresolvedRows.length) {
      const message = `有 ${unresolvedRows.length} 行无法解析该字段所属方向的航班，已取消本次批量提交`;
      modalState.value.error = message;
      toast.showToast('warning', message, { duration: 5000 });
      announce(`批量更新失败：${message}`);
      return;
    }

    modalState.value.saving = true;
    modalState.value.error = null;
    try {
      // Prefer batch API even for N=1 so single-cell and multi-cell share one path.
      const result = await patchFlightBatchCells(
        {
          field,
          value: submitValue,
          client_action_id: createClientActionId(),
          targets,
        },
        {
          apiBase: auth.apiBase.value,
          authFetch: auth.fetch,
        },
      );
      modalState.value.lastResult = result;
      // Atomic API: success means full batch applied (no partial success).
      const succeeded = typeof result.updated_count === 'number'
        ? result.updated_count
        : (result.results?.length ?? flightIds.length);

      const message = `已批量更新 ${succeeded} 个「${getBatchFieldLabel(field)}」`;
      toast.showToast('success', message, { duration: 4000 });
      announce(message);

      // No optimistic multi-cell update: refresh snapshot from server.
      closeBatchEdit();
      cellSelection.clearSelection();
      await refreshFlights();
    } catch (err) {
      const message = err instanceof Error ? err.message : '批量更新失败';
      modalState.value.error = message;
      toast.showToast('error', message, { duration: 6000 });
      announce(`批量更新失败：${message}`);
    } finally {
      modalState.value.saving = false;
    }
  }

  function handleCellContextMenu(
    event: MouseEvent,
    flightId: string,
    field: string,
    type: string,
    value: string,
  ): void {
    if (!canEditField(field)) {
      return;
    }

    const id = normalizeFlightId(flightId);
    const isSelected = cellSelection.isCellSelected(id, field);
    const multi = isSelected && cellSelection.selectedCount.value > 1
      && cellSelection.selectedField.value === field;

    // If right-click target is not part of selection, select only that cell.
    if (!isSelected || cellSelection.selectedField.value !== field) {
      cellSelection.beginSelection(id, field);
    }

    const menuWidth = 220;
    const menuHeight = multi ? 140 : 96;
    const viewportPadding = 12;
    const maxX = Math.max(viewportPadding, window.innerWidth - menuWidth - viewportPadding);
    const maxY = Math.max(viewportPadding, window.innerHeight - menuHeight - viewportPadding);

    contextMenuState.value = {
      isOpen: true,
      x: Math.min(Math.max(event.clientX, viewportPadding), maxX),
      y: Math.min(Math.max(event.clientY, viewportPadding), maxY),
      flightId: id,
      field,
      type,
      value,
      multi,
      selectedCount: multi ? cellSelection.selectedCount.value : 1,
    };
  }

  function closeCellContextMenu(): void {
    contextMenuState.value.isOpen = false;
  }

  function handleContextBatchEdit(): void {
    const { field } = contextMenuState.value;
    closeCellContextMenu();
    openBatchEditFromSelection();
    if (!modalState.value.isOpen && field) {
      // Fallback when selection was empty somehow
      openBatchEditForField(field, cellSelection.selectedFlightIds.value);
    }
  }

  function handleContextSingleEdit(): void {
    const { flightId, field } = contextMenuState.value;
    closeCellContextMenu();
    // Always N=1 through the batch-cells path for registered fields so
    // snapshot vs timeline write strategies stay consistent.
    if (isBatchEditableField(field)) {
      openBatchEditForField(field, [flightId]);
      return;
    }
    // Non-batch registry fields (if any) may still use the legacy editor.
    if (openSingleFieldEdit) {
      const { type, value } = contextMenuState.value;
      openSingleFieldEdit(flightId, field, type, value);
    }
  }

  async function handleContextRevoke(): Promise<void> {
    const { flightId, field, multi } = contextMenuState.value;
    closeCellContextMenu();
    if (multi || !flightId || !field) {
      return;
    }
    if (!canEditField(field)) {
      toast.showToast('warning', '当前无权限撤销该字段', { duration: 4000 });
      return;
    }
    const meta = getBatchEditableField(field);
    if (!meta || meta.valueType !== 'datetime') {
      return;
    }
    const confirmed = window.confirm(`确定要撤销「${meta.label}」吗？撤销后将变更为 --。`);
    if (!confirmed) {
      return;
    }
    try {
      if (meta.writeStrategy === 'timeline_event') {
        // Timeline milestones must be cleared via the timeline command path,
        // not a generic FlightUpdate PATCH.
        if (revokeSingleTimeField) {
          await revokeSingleTimeField(flightId, field);
        } else {
          throw new Error('时间线撤销未配置');
        }
      } else {
        // Snapshot datetime: clear via batch API N=1. Target the directional
        // flight, not the monitor row_id.
        const revokeFlight = flightData.findFlightById(flightId);
        const revokeTargetId = revokeFlight
          ? resolveDirectionalFlightId(revokeFlight, field) ?? flightId
          : flightId;
        await patchFlightBatchCells(
          {
            field,
            value: null,
            client_action_id: createClientActionId(),
            targets: [{
              flight_id: revokeTargetId,
              expected_version: readFlightVersion(revokeFlight),
              expected_value: readFlightFieldValue(revokeFlight, field),
            }],
          },
          {
            apiBase: auth.apiBase.value,
            authFetch: auth.fetch,
          },
        );
        await refreshFlights();
      }
      announce(`已撤销「${meta.label}」`);
      toast.showToast('success', `${meta.label} 已撤销`);
      cellSelection.clearSelection();
    } catch (err) {
      const message = err instanceof Error ? err.message : '撤销失败';
      toast.showToast('error', `撤销失败: ${message}`, { duration: 5000 });
      announce(`撤销失败：${message}`);
    }
  }

  function handleContextClearSelection(): void {
    closeCellContextMenu();
    cellSelection.clearSelection();
  }

  function clearAll(): void {
    closeCellContextMenu();
    closeBatchEdit();
    cellSelection.clearSelection();
  }

  return {
    modalState,
    contextMenuState,
    canManageFlights,
    isAdminUser,
    canEditField,
    canRevokeCurrentContext,
    canSubmitCurrent,
    selectedFieldMeta,
    openBatchEditFromSelection,
    openBatchEditForField,
    closeBatchEdit,
    setBatchValue,
    submitBatchEdit,
    handleCellContextMenu,
    closeCellContextMenu,
    handleContextBatchEdit,
    handleContextSingleEdit,
    handleContextRevoke,
    handleContextClearSelection,
    clearAll,
  };
}
