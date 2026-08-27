<script lang="ts">
export interface TableColumnDef {
  key: string;
  label: string;
  sortField?: string;
  class?: string;
}

/**
 * Table columns, aligned with the legacy flight_monitor DEFAULT_COLUMNS /
 * DEFAULT_COLUMN_ORDER (36 legacy columns + the Vue-specific 保障标签 column).
 * Legacy key → Vue key differences:
 *   flight_no → flight_number, smart_departure → scheduled_departure,
 *   smart_arrival → scheduled_arrival, aircraft → aircraft_type,
 *   flight_remarks → remarks.
 */
export const BASE_COLUMNS: TableColumnDef[] = [
  { key: 'flight_number', label: '航班号', sortField: 'flight_number' },
  { key: 'status', label: '状态', sortField: 'status' },
  { key: 'route', label: '航线' },
  { key: 'scheduled_departure', label: '起飞时间', sortField: 'scheduled_departure' },
  { key: 'scheduled_arrival', label: '落地时间', sortField: 'scheduled_arrival' },
  { key: 'flight_type', label: '属性' },
  { key: 'time_dep_sch', label: '计划起飞', sortField: 'scheduled_departure' },
  { key: 'time_arr_sch', label: '计划到达', sortField: 'scheduled_arrival' },
  { key: 'time_dep_est', label: '预计起飞', sortField: 'estimated_departure' },
  { key: 'time_arr_est', label: '预计到达', sortField: 'estimated_arrival' },
  { key: 'time_dep_act', label: '实际起飞', sortField: 'actual_departure' },
  { key: 'time_arr_act', label: '实际到达', sortField: 'actual_arrival' },
  { key: 'stand', label: '机位', sortField: 'stand' },
  { key: 'gate', label: '登机口', sortField: 'gate' },
  { key: 'baggage_carousel', label: '行李转盘', sortField: 'baggage_carousel' },
  { key: 'aircraft_type', label: '机型' },
  { key: 'missions', label: '任务' },
  { key: 'cobt_time', label: 'COBT', sortField: 'cobt_time' },
  { key: 'codt', label: 'CODT', sortField: 'codt' },
  { key: 'boarding_allowed_time', label: '允许登机', sortField: 'boarding_allowed_time' },
  { key: 'start_boarding_time', label: '开始登机', sortField: 'start_boarding_time' },
  { key: 'end_boarding_time', label: '结束登机', sortField: 'end_boarding_time' },
  { key: 'passenger_ready_time', label: '人齐', sortField: 'passenger_ready_time' },
  { key: 'on_blocks_time', label: '上轮挡', sortField: 'on_blocks_time' },
  { key: 'off_blocks_time', label: '撤轮挡', sortField: 'off_blocks_time' },
  { key: 'cabin_door_open_time', label: '开舱门', sortField: 'cabin_door_open_time' },
  { key: 'deboarding_complete_time', label: '下客完成', sortField: 'deboarding_complete_time' },
  { key: 'cleaning_start_time', label: '清洁开始', sortField: 'cleaning_start_time' },
  { key: 'cleaning_end_time', label: '清洁结束', sortField: 'cleaning_end_time' },
  { key: 'cabin_door_close_time', label: '关客舱门', sortField: 'cabin_door_close_time' },
  { key: 'cargo_door_close_time', label: '关货舱门', sortField: 'cargo_door_close_time' },
  { key: 'loading_complete_time', label: '装载完成', sortField: 'loading_complete_time' },
  { key: 'remarks', label: '航班备注', class: 'col-remarks' },
  { key: 'load_planning_remarks', label: '配载备注', class: 'col-remarks' },
  { key: 'aircraft_maintenance_remarks', label: '机务备注', class: 'col-remarks' },
  { key: 'aircraft_check_remarks', label: '复核机号', class: 'col-remarks' },
  { key: 'registration', label: '机号', sortField: 'registration' },
  { key: 'tags', label: '保障标签' },
];

/**
 * Default-visible columns, aligned with the legacy DEFAULT_VISIBLE_COLUMNS
 * (flight_no, status, route, smart_departure, smart_arrival, flight_type,
 * stand, gate, aircraft).
 */
export const DEFAULT_VISIBLE_COLUMN_KEYS: readonly string[] = [
  'flight_number',
  'status',
  'route',
  'scheduled_departure',
  'scheduled_arrival',
  'flight_type',
  'stand',
  'gate',
  'aircraft_type',
];
</script>

<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch, toRef } from 'vue';
import type { AirportContext, FlightSortDirection } from '../../composables/useFlightData';
import type { Flight as FlightModel } from '../../composables/useFlightData';
import { getAnomalyCountForFlight, formatTimeValue } from '../../composables/useFlightData';
import { writeDispatchTimelineField } from '../../composables/useFlightSync';
import { useAuth, hasUserPermission } from '../../composables/useAuth';
import { useToast } from '../../composables/useToast';
import type { Flight } from '@/types/bindings';
import FlightItem from './FlightItem.vue';
import EmptyState from '../ui/EmptyState.vue';
import UiButton from '../ui/UiButton.vue';
import UiField from '../ui/UiField.vue';
import UiMenu from '../ui/UiMenu.vue';
import UiMenuItem from '../ui/UiMenuItem.vue';
import UiModal from '../ui/UiModal.vue';
import UiPill from '../ui/UiPill.vue';
import { useVirtualScroll } from '../../composables/useVirtualScroll';
import {
  getAircraftBodyLabel,
  getAnomalySeverity,
  getCommercialSignedLabel,
  getFlightDomId,
  getFlightEndpoints,
  getFlightNumbers,
  getFlightTypeColumnDisplay,
  getMissionDisplay,
  getStatusRowClassName,
  getStatusTone,
  getTimeDisplay,
  getTimeFieldDisplay,
  getTimeFieldRawValue,
  toggleRouteDisplayMode,
  getFlightNumberStyleClass,
} from './helpers';
import type { FlightViewMode } from './helpers';
import {
  BATCH_EDITABLE_FIELD_KEYS,
  PUNCH_TIME_FIELDS,
  getBatchEditableField,
  getPunchTimeFieldLabel,
  isPunchTimeField,
} from '../../pages/flight_monitor/flightBatchEditableFields';
import type { FlightFlashEvent } from '@/composables/useFlightStream';

const props = withDefaults(defineProps<{
  flights: readonly Flight[];
  airportContext: AirportContext;
  selectedFlightId: string | null;
  viewMode: FlightViewMode;
  showAlertPool: boolean;
  hasActiveFilters: boolean;
  sortField: string | null;
  sortDirection: FlightSortDirection;
  visibleColumns?: string[];
  /** When true, editable cells support same-column multi-select. */
  canSelectCells?: boolean;
  /** O(1) membership check from parent selection model. */
  isCellSelected?: (flightId: string, field: string) => boolean;
  /** Permission gate per field (adminOnly, flight.update). */
  canEditField?: (field: string) => boolean;
  /** Bumps when selection Set mutates so computed class re-evaluates. */
  selectionRevision?: number;
  flashEvents?: readonly FlightFlashEvent[];
}>(), {
  canSelectCells: false,
  isCellSelected: () => false,
  canEditField: () => false,
  selectionRevision: 0,
});

// Headers and body cells both follow the configured column order
// (props.visibleColumns comes from the 配置列 drag order). Each body <td> is
// dispatched by column.key inside the same v-for loop, so header and body stay
// aligned after reordering.
const tableColumns = computed(() => {
  if (!props.visibleColumns) {
    return BASE_COLUMNS;
  }
  const byKey = new Map(BASE_COLUMNS.map((c) => [c.key, c]));
  return props.visibleColumns
    .map((key) => byKey.get(key))
    .filter((c): c is TableColumnDef => Boolean(c));
});

/** Split time column key → flight time field rendered in that column. */
const SPLIT_TIME_COLUMN_FIELDS: Record<string, string> = {
  time_dep_sch: 'scheduled_departure',
  time_arr_sch: 'scheduled_arrival',
  time_dep_est: 'estimated_departure',
  time_arr_est: 'estimated_arrival',
  time_dep_act: 'actual_departure',
  time_arr_act: 'actual_arrival',
};

/** Long remark columns rendered after 航班备注. */
const EXTRA_REMARK_FIELD_KEYS = new Set([
  'load_planning_remarks',
  'aircraft_maintenance_remarks',
  'aircraft_check_remarks',
]);

const emit = defineEmits<{
  (e: 'select-flight', flightId: string): void;
  (e: 'edit-field', flightId: string, field: string, type: 'text' | 'datetime-local', value: string): void;
  (e: 'open-context-menu', event: MouseEvent, flightId: string, field: string, type: string, value: string): void;
  (e: 'sort', field: string): void;
  (e: 'exit-alert-pool'): void;
  (e: 'open-column-config'): void;
  (e: 'cell-select-start', flightId: string, field: string, additive: boolean, shiftKey: boolean): void;
  (e: 'cell-select-extend', flightId: string, field: string): void;
  (e: 'cell-select-end'): void;
}>();

// --- 虚拟滚动配置 ---
const cardContainerRef = ref<HTMLElement | null>(null);
const alertContainerRef = ref<HTMLElement | null>(null);
const tableContainerRef = ref<HTMLElement | null>(null);

function isFieldSelectable(field: string): boolean {
  if (!props.canSelectCells) {
    return false;
  }
  if (!BATCH_EDITABLE_FIELD_KEYS.has(field)) {
    return false;
  }
  return props.canEditField(field);
}

function cellSelectedClass(flightId: string, field: string): Record<string, boolean> {
  // Depend on selectionRevision so Vue re-renders when the Set changes.
  void props.selectionRevision;
  return {
    'cell-batch-editable': isFieldSelectable(field),
    'cell-batch-selected': Boolean(flightId && props.isCellSelected(flightId, field)),
  };
}

function fieldInputType(field: string): 'text' | 'datetime-local' {
  const meta = getBatchEditableField(field);
  return meta?.valueType === 'datetime' ? 'datetime-local' : 'text';
}

function readCellRawValue(flight: Flight, field: string): string {
  const record = flight as unknown as Record<string, unknown>;
  const raw = record[field];
  if (raw === null || raw === undefined) {
    return '';
  }
  return String(raw);
}

let cellDragActive = false;
let cellDragField: string | null = null;
let cellDragPointerId: number | null = null;
const EDGE_SCROLL_PX = 40;
const EDGE_SCROLL_STEP = 18;

function autoScrollNearEdge(clientY: number): void {
  const container = tableContainerRef.value;
  if (!container) {
    return;
  }
  const rect = container.getBoundingClientRect();
  if (clientY < rect.top + EDGE_SCROLL_PX) {
    container.scrollTop = Math.max(0, container.scrollTop - EDGE_SCROLL_STEP);
  } else if (clientY > rect.bottom - EDGE_SCROLL_PX) {
    container.scrollTop += EDGE_SCROLL_STEP;
  }
}

/**
 * Resolve the editable cell under the pointer.
 * Required because setPointerCapture routes all events to the capture target,
 * so sibling cells never receive pointerenter during a real drag.
 */
function resolveCellFromPoint(clientX: number, clientY: number): { flightId: string; field: string } | null {
  const el = document.elementFromPoint(clientX, clientY) as HTMLElement | null;
  if (!el) {
    return null;
  }
  const cell = el.closest('td[data-flight-id][data-field]') as HTMLElement | null;
  if (!cell) {
    return null;
  }
  const flightId = String(cell.dataset.flightId || '').trim();
  const field = String(cell.dataset.field || '').trim();
  if (!flightId || !field) {
    return null;
  }
  return { flightId, field };
}

function extendSelectionFromPoint(clientX: number, clientY: number): void {
  if (!cellDragActive || !cellDragField) {
    return;
  }
  autoScrollNearEdge(clientY);
  const hit = resolveCellFromPoint(clientX, clientY);
  if (!hit) {
    return;
  }
  // Stay locked to the drag-start column.
  if (hit.field !== cellDragField) {
    return;
  }
  if (!isFieldSelectable(hit.field)) {
    return;
  }
  emit('cell-select-extend', hit.flightId, hit.field);
}

function onDocumentPointerMove(event: PointerEvent): void {
  if (!cellDragActive) {
    return;
  }
  if (cellDragPointerId != null && event.pointerId !== cellDragPointerId) {
    return;
  }
  extendSelectionFromPoint(event.clientX, event.clientY);
}

function endCellDrag(): void {
  if (!cellDragActive) {
    return;
  }
  cellDragActive = false;
  cellDragField = null;
  cellDragPointerId = null;
  if (typeof document !== 'undefined') {
    document.removeEventListener('pointermove', onDocumentPointerMove, true);
  }
  emit('cell-select-end');
}

// Pointer-down coordinates, used to tell a plain click (punch) apart from a
// drag (batch box-select gesture) on interactive punch cells.
let punchDownActive = false;
let punchDownX = 0;
let punchDownY = 0;

function recordPunchPointerDown(event: PointerEvent): void {
  if (event.button !== 0) {
    return;
  }
  punchDownX = event.clientX;
  punchDownY = event.clientY;
  punchDownActive = true;
}

function onCellPointerDown(event: PointerEvent, flight: Flight, field: string): void {
  recordPunchPointerDown(event);
  if (!isFieldSelectable(field)) {
    return;
  }
  // Only primary button starts drag-select.
  if (event.button !== 0) {
    return;
  }
  const flightId = getFlightDomId(flight);
  if (!flightId) {
    return;
  }
  event.stopPropagation();
  cellDragActive = true;
  cellDragField = field;
  cellDragPointerId = event.pointerId;
  const additive = event.ctrlKey || event.metaKey;
  emit('cell-select-start', flightId, field, additive, event.shiftKey);
  // Capture pointer so drag continues outside the cell; selection extension
  // uses document pointermove + elementFromPoint (not sibling pointerenter).
  (event.currentTarget as HTMLElement | null)?.setPointerCapture?.(event.pointerId);
  if (typeof document !== 'undefined') {
    document.addEventListener('pointermove', onDocumentPointerMove, true);
  }
}

function onCellPointerEnter(event: PointerEvent, flight: Flight, field: string): void {
  // Kept as a lightweight fallback when pointer capture is unavailable.
  if (!cellDragActive || !isFieldSelectable(field)) {
    return;
  }
  if (cellDragField && field !== cellDragField) {
    return;
  }
  const flightId = getFlightDomId(flight);
  if (!flightId) {
    return;
  }
  autoScrollNearEdge(event.clientY);
  emit('cell-select-extend', flightId, field);
}

function onCellPointerUp(): void {
  endCellDrag();
}

function onCellClick(event: MouseEvent, field: string): void {
  if (!isFieldSelectable(field)) {
    return;
  }
  // Editable cells must not trigger row select.
  event.stopPropagation();
}

function onEditableContextMenu(
  event: MouseEvent,
  flight: Flight,
  field: string,
): void {
  const flightId = getFlightDomId(flight);
  if (!flightId) {
    return;
  }
  if (!isFieldSelectable(field)) {
    // Fall through to legacy time context menu for users without manage permission.
    emit('open-context-menu', event, flightId, field, fieldInputType(field), readCellRawValue(flight, field));
    return;
  }
  event.preventDefault();
  event.stopPropagation();
  emit('open-context-menu', event, flightId, field, fieldInputType(field), readCellRawValue(flight, field));
}

// --- 交互式打卡时间单元格（legacy interactive time cells） ---

const auth = useAuth();
const toast = useToast();

/** Timeline edit permission for interactive time-cell events. */
const hasTimelineEditPermission = computed(() => hasUserPermission(auth.getUser(), 'flight.timeline_edit'));

/**
 * Punch permission per field. Batch-registered fields reuse the parent-provided
 * gate (flight.update + registry adminOnly policy); unregistered punch fields
 * use the timeline-event permission required by their write endpoint.
 */
function canPunchField(field: string): boolean {
  if (!isPunchTimeField(field)) {
    return false;
  }
  if (BATCH_EDITABLE_FIELD_KEYS.has(field)) {
    return props.canEditField(field);
  }
  return hasTimelineEditPermission.value;
}

/** Unregistered punch fields rendered between the explicit registered cells. */
const EXTRA_PUNCH_FIELDS = PUNCH_TIME_FIELDS
  .filter((entry) => !entry.batchEditable && entry.field !== 'passenger_ready_time')
  .map((entry) => entry.field);
const EXTRA_PUNCH_FIELD_KEYS = new Set(EXTRA_PUNCH_FIELDS);

const punchingCells = ref<Record<string, boolean>>({});

function punchCellKey(flightId: string, field: string): string {
  return `${flightId}::${field}`;
}

function isPunching(flightId: string, field: string): boolean {
  return Boolean(punchingCells.value[punchCellKey(flightId, field)]);
}

function setPunching(flightId: string, field: string, punching: boolean): void {
  const key = punchCellKey(flightId, field);
  const next = { ...punchingCells.value };
  if (punching) {
    next[key] = true;
  } else {
    delete next[key];
  }
  punchingCells.value = next;
}

function findListedFlight(flightId: string): Flight | null {
  return props.flights.find((flight) => getFlightDomId(flight) === flightId) ?? null;
}

/**
 * Single-cell timeline write (timeline_event strategy) with optimistic update,
 * mirroring flightData.updateFlightField's optimistic pattern. The dispatch
 * timeline API is the same endpoint the app uses for single-cell timeline
 * revoke; SSE reconciles the snapshot afterwards.
 */
async function writePunchValue(
  flightId: string,
  field: string,
  isoValue: string | null,
  successMessage: string,
): Promise<boolean> {
  if (isPunching(flightId, field)) {
    return false;
  }
  const flight = findListedFlight(flightId);
  const record = flight ? (flight as unknown as Record<string, unknown>) : null;
  const model = flight ? (flight as unknown as FlightModel) : null;
  const previous = record ? record[field] : undefined;
  const previousFmt = model?._fmt?.[field];

  if (record) {
    record[field] = isoValue;
  }
  if (model?._fmt) {
    model._fmt[field] = isoValue ? formatTimeValue(isoValue) : null;
  }
  setPunching(flightId, field, true);
  try {
    await writeDispatchTimelineField(flightId, field, {
      apiBase: auth.apiBase.value,
      authFetch: auth.fetch,
      value: isoValue,
    });
    toast.showToast('success', successMessage, { duration: 2500 });
    return true;
  } catch (err) {
    if (record) {
      record[field] = previous ?? null;
    }
    if (model?._fmt) {
      model._fmt[field] = previousFmt ?? null;
    }
    const message = err instanceof Error ? err.message : '操作失败';
    toast.showToast('error', `「${getPunchTimeFieldLabel(field)}」操作失败: ${message}`, { duration: 5000 });
    return false;
  } finally {
    setPunching(flightId, field, false);
  }
}

/** Single click on an empty punch cell writes the current time (legacy quick punch). */
function onPunchCellClick(event: MouseEvent, flight: Flight, field: string): void {
  // Punch cells never trigger row select.
  event.stopPropagation();
  // Ignore drags (batch box-select gestures) — only plain clicks punch.
  if (punchDownActive) {
    punchDownActive = false;
    const moved = Math.abs(event.clientX - punchDownX) + Math.abs(event.clientY - punchDownY);
    if (moved > 6) {
      return;
    }
  }
  if (getTimeFieldRawValue(flight, field)) {
    // Valued cells are edited via the right-click menu, not re-punched.
    return;
  }
  if (!canPunchField(field)) {
    toast.showToast('warning', `当前账号无权打卡「${getPunchTimeFieldLabel(field)}」`, { duration: 4000 });
    return;
  }
  const flightId = getFlightDomId(flight);
  if (!flightId) {
    return;
  }
  void writePunchValue(flightId, field, new Date().toISOString(), `已打卡「${getPunchTimeFieldLabel(field)}」`);
}

const punchMenu = ref({
  isOpen: false,
  x: 0,
  y: 0,
  flightId: '',
  field: '',
  value: '',
});

function closePunchMenu(): void {
  punchMenu.value.isOpen = false;
}

/** 表头右键菜单（legacy #headerContextMenu）：唯一入口「配置列...」 */
const headerMenu = ref({ isOpen: false, x: 0, y: 0 });

function onHeaderContextMenu(event: MouseEvent): void {
  event.preventDefault();
  closePunchMenu();
  const menuWidth = 120;
  const menuHeight = 44;
  const viewportPadding = 8;
  headerMenu.value = {
    isOpen: true,
    x: Math.min(Math.max(event.clientX, viewportPadding), Math.max(viewportPadding, window.innerWidth - menuWidth - viewportPadding)),
    y: Math.min(Math.max(event.clientY, viewportPadding), Math.max(viewportPadding, window.innerHeight - menuHeight - viewportPadding)),
  };
}

function closeHeaderMenu(): void {
  headerMenu.value.isOpen = false;
}

function onHeaderMenuConfigColumns(): void {
  closeHeaderMenu();
  emit('open-column-config');
}

function onHeaderMenuGlobalClick(event: MouseEvent): void {
  const target = event.target as HTMLElement | null;
  if (!target?.closest?.('#headerContextMenu')) {
    closeHeaderMenu();
  }
}

function onHeaderMenuEsc(event: KeyboardEvent): void {
  if (event.key === 'Escape') {
    closeHeaderMenu();
  }
}

document.addEventListener('click', onHeaderMenuGlobalClick);
document.addEventListener('keydown', onHeaderMenuEsc);
onBeforeUnmount(() => {
  document.removeEventListener('click', onHeaderMenuGlobalClick);
  document.removeEventListener('keydown', onHeaderMenuEsc);
});

/**
 * Right-click on a valued punch cell whose field is NOT batch-registered:
 * open the in-component 修改/撤销 menu (legacy time context menu). Registered
 * fields keep using the parent context menu via onEditableContextMenu.
 */
function onPunchContextMenu(event: MouseEvent, flight: Flight, field: string): void {
  event.preventDefault();
  event.stopPropagation();
  const value = getTimeFieldRawValue(flight, field);
  if (!value) {
    // Legacy ignores the context menu on empty cells.
    return;
  }
  const flightId = getFlightDomId(flight);
  if (!flightId) {
    return;
  }
  const menuWidth = 160;
  const menuHeight = 96;
  const viewportPadding = 12;
  punchMenu.value = {
    isOpen: true,
    x: Math.min(Math.max(event.clientX, viewportPadding), Math.max(viewportPadding, window.innerWidth - menuWidth - viewportPadding)),
    y: Math.min(Math.max(event.clientY, viewportPadding), Math.max(viewportPadding, window.innerHeight - menuHeight - viewportPadding)),
    flightId,
    field,
    value,
  };
}

const punchEdit = ref({
  isOpen: false,
  flightId: '',
  field: '',
  label: '',
  value: '',
  saving: false,
});

function toDatetimeLocalValue(raw: string): string {
  if (!raw) {
    return '';
  }
  const date = new Date(raw);
  if (Number.isNaN(date.getTime())) {
    return /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}/.test(raw) ? raw.slice(0, 16) : raw;
  }
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

function onPunchMenuModify(): void {
  const { flightId, field, value } = punchMenu.value;
  closePunchMenu();
  if (!flightId || !field) {
    return;
  }
  if (!canPunchField(field)) {
    toast.showToast('error', '仅允许操作人修改该时间字段', { duration: 4000 });
    return;
  }
  punchEdit.value = {
    isOpen: true,
    flightId,
    field,
    label: getPunchTimeFieldLabel(field),
    value: toDatetimeLocalValue(value),
    saving: false,
  };
}

async function onPunchMenuRevoke(): Promise<void> {
  const { flightId, field } = punchMenu.value;
  closePunchMenu();
  if (!flightId || !field) {
    return;
  }
  if (!canPunchField(field)) {
    toast.showToast('error', '仅允许操作人撤销该时间字段', { duration: 4000 });
    return;
  }
  const label = getPunchTimeFieldLabel(field);
  if (!window.confirm(`确定要撤销「${label}」吗？撤销后将变更为 --。`)) {
    return;
  }
  await writePunchValue(flightId, field, null, `已撤销「${label}」`);
}

async function onPunchEditSave(): Promise<void> {
  const { flightId, field, value } = punchEdit.value;
  if (!flightId || !field || !value.trim()) {
    return;
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    toast.showToast('warning', '时间格式无效', { duration: 4000 });
    return;
  }
  punchEdit.value.saving = true;
  try {
    const ok = await writePunchValue(flightId, field, date.toISOString(), `已修改「${punchEdit.value.label}」`);
    if (ok) {
      punchEdit.value.isOpen = false;
    }
  } finally {
    punchEdit.value.saving = false;
  }
}

function onDocumentClickClosePunchMenu(): void {
  closePunchMenu();
}

function onWindowPointerUp(): void {
  endCellDrag();
}

if (typeof window !== 'undefined') {
  window.addEventListener('pointerup', onWindowPointerUp);
  window.addEventListener('pointercancel', onWindowPointerUp);
}
if (typeof document !== 'undefined') {
  document.addEventListener('click', onDocumentClickClosePunchMenu);
}

onBeforeUnmount(() => {
  if (typeof window !== 'undefined') {
    window.removeEventListener('pointerup', onWindowPointerUp);
    window.removeEventListener('pointercancel', onWindowPointerUp);
  }
  if (typeof document !== 'undefined') {
    document.removeEventListener('pointermove', onDocumentPointerMove, true);
    document.removeEventListener('click', onDocumentClickClosePunchMenu);
  }
});

const anomalyFlights = computed(() => props.flights.filter((flight) => getAnomalyCountForFlight(flight as unknown as FlightModel) > 0));

// 告警池按严重度分档计数（取数方式与卡片上的优先级徽章一致）。
const alertSeverityCounts = computed(() => {
  const counts = { high: 0, medium: 0, low: 0 };
  for (const flight of anomalyFlights.value) {
    counts[getAnomalySeverity(flight)] += 1;
  }
  return counts;
});

const {
  visibleItems: visibleCardFlights,
  topSpacerHeight: cardTopSpacer,
  bottomSpacerHeight: cardBottomSpacer,
} = useVirtualScroll(toRef(props, 'flights'), cardContainerRef, { itemHeight: 180, buffer: 3 });

const {
  visibleItems: visibleAlertFlights,
  topSpacerHeight: alertTopSpacer,
  bottomSpacerHeight: alertBottomSpacer,
} = useVirtualScroll(anomalyFlights, alertContainerRef, { itemHeight: 180, buffer: 3 });

const {
  visibleItems: visibleTableFlights,
  topSpacerHeight: tableTopSpacer,
  bottomSpacerHeight: tableBottomSpacer,
} = useVirtualScroll(toRef(props, 'flights'), tableContainerRef, { itemHeight: 40, buffer: 10 });

function handleSort(field: string): void {
  emit('sort', field);
}

const emptyMessage = computed(() => (props.hasActiveFilters ? '没有匹配的航班，请尝试调整筛选条件。' : '暂无原型航班数据。'));
const alertEmptyMessage = computed(() => (props.hasActiveFilters ? '当前筛选范围内没有异常航班。' : '当前没有异常告警航班。'));
const flashToneByFlightId = ref<Record<string, 'warn' | 'ok'>>({});
const flashTimers = new Map<string, number>();

function clearFlashTimer(flightId: string): void {
  const existing = flashTimers.get(flightId);
  if (typeof existing === 'number') {
    window.clearTimeout(existing);
    flashTimers.delete(flightId);
  }
}

function triggerFlightFlash(flightId: string, tone: 'warn' | 'ok'): void {
  if (!flightId) {
    return;
  }

  flashToneByFlightId.value = { ...flashToneByFlightId.value, [flightId]: tone };
  clearFlashTimer(flightId);

  const timer = window.setTimeout(() => {
    const next = { ...flashToneByFlightId.value };
    delete next[flightId];
    flashToneByFlightId.value = next;
    flashTimers.delete(flightId);
  }, 1000);
  flashTimers.set(flightId, timer);
}

watch(
  () => props.flashEvents,
  (events) => {
    for (const event of events ?? []) {
      triggerFlightFlash(event.flightId, event.tone);
    }
  },
);

function getUpdateTone(flightId: string): 'warn' | 'ok' | null {
  return flashToneByFlightId.value[flightId] ?? null;
}

function getTableRowUpdateClass(flight: Flight): Record<string, boolean> {
  const tone = getUpdateTone(getFlightDomId(flight));
  return {
    'flash-update': tone !== null,
    'flash-update--warn': tone === 'warn',
    'flash-update--ok': tone === 'ok',
  };
}

function isSelected(flight: Flight): boolean {
  return getFlightDomId(flight) !== '' && getFlightDomId(flight) === (props.selectedFlightId ?? '');
}

function selectFlight(flight: Flight): void {
  const flightId = getFlightDomId(flight);
  if (flightId) {
    emit('select-flight', flightId);
  }
}

function handleRowKeydown(event: KeyboardEvent, flight: Flight): void {
  if (event.key === 'Enter' || event.key === ' ') {
    event.preventDefault();
    selectFlight(flight);
    return;
  }
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault();
    const current = event.currentTarget as HTMLElement | null;
    const tbody = current?.parentElement;
    if (!current || !tbody) {
      return;
    }
    const rows = Array.from(tbody.querySelectorAll<HTMLElement>('tr[data-flight-id]'));
    const index = rows.indexOf(current);
    if (index < 0) {
      return;
    }
    const delta = event.key === 'ArrowDown' ? 1 : -1;
    const nextRow = rows[Math.min(rows.length - 1, Math.max(0, index + delta))];
    if (!nextRow) {
      return;
    }
    nextRow.focus();
    const nextFlightId = String(nextRow.dataset.flightId || '').trim();
    if (nextFlightId) {
      emit('select-flight', nextFlightId);
    }
  }
}

onBeforeUnmount(() => {
  for (const timer of flashTimers.values()) {
    window.clearTimeout(timer);
  }
  flashTimers.clear();
  if (typeof window !== 'undefined') {
    window.removeEventListener('pointerup', onWindowPointerUp);
  }
});
</script>

<template>
  <div
    v-show="viewMode === 'card' && !showAlertPool"
    id="flightList"
    ref="cardContainerRef"
    role="list"
    aria-label="实时航班列表"
    aria-busy="false"
    class="card-layout-view"
  >
    <EmptyState
      v-if="!flights.length"
      :icon="hasActiveFilters ? 'search' : 'plane'"
      :title="emptyMessage"
      :description="hasActiveFilters ? '请尝试调整筛选条件或清空搜索' : '暂无航班数据，请稍后刷新或联系管理员'"
    />
    <template v-else>
      <div :style="{ height: `${cardTopSpacer}px` }" aria-hidden="true" />
      <FlightItem
        v-for="flight in visibleCardFlights"
        :key="getFlightDomId(flight)"
        :flight="flight"
        :airport-context="airportContext"
        :selected="isSelected(flight)"
        :update-tone="getUpdateTone(getFlightDomId(flight))"
        @select="emit('select-flight', $event)"
        @edit-field="(id, field, type, val) => emit('edit-field', id, field, type, val)"
      />
      <div :style="{ height: `${cardBottomSpacer}px` }" aria-hidden="true" />
    </template>
  </div>

  <div
    v-show="showAlertPool"
    id="alertPoolContainer"
    class="alert-pool-container"
    role="list"
    aria-label="告警航班池"
  >
    <div class="alert-pool-header">
      <h3>
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="currentColor"
          aria-hidden="true"
        >
          <path d="M12 2L1 21h22M12 6l7.53 13H4.47M11 10v4h2v-4m-2 6v2h2v-4" />
        </svg>
        异常告警池
        <span id="alertCountBadge" class="fm-alert-count">{{ anomalyFlights.length }}</span>
      </h3>
      <p>
        当前共发现 <strong>{{ anomalyFlights.length }}</strong> 个异常航班，已按严重程度排序。
      </p>
      <div class="alert-pool-overview">
        <div class="alert-pool-stats">
          <span class="anomaly-severity-badge badge-high">严重 {{ alertSeverityCounts.high }}</span>
          <span class="anomaly-severity-badge badge-medium">高优 {{ alertSeverityCounts.medium }}</span>
          <span class="anomaly-severity-badge badge-low">中优 {{ alertSeverityCounts.low }}</span>
        </div>
        <UiButton id="alertBackToCardBtn" variant="quiet" @click="emit('exit-alert-pool')">
          返回航班列表
        </UiButton>
      </div>
    </div>

    <div id="alertFlightList" ref="alertContainerRef" class="card-layout-view">
      <EmptyState
        v-if="!anomalyFlights.length"
        icon="alert"
        :title="alertEmptyMessage"
        :description="hasActiveFilters ? '当前筛选条件下没有异常航班' : '当前没有异常告警航班，一切正常'"
      />
      <template v-else>
        <div :style="{ height: `${alertTopSpacer}px` }" aria-hidden="true" />
        <FlightItem
          v-for="flight in visibleAlertFlights"
          :key="`alert-${getFlightDomId(flight)}`"
          :flight="flight"
          :airport-context="airportContext"
          :selected="isSelected(flight)"
          :update-tone="getUpdateTone(getFlightDomId(flight))"
          alert-mode
          @select="emit('select-flight', $event)"
          @edit-field="(id, field, type, val) => emit('edit-field', id, field, type, val)"
        />
        <div :style="{ height: `${alertBottomSpacer}px` }" aria-hidden="true" />
      </template>
    </div>
  </div>

  <div
    v-show="viewMode === 'table' && !showAlertPool"
    id="flightTableContainer"
    class="flight-table-container"
    role="grid"
    aria-label="航班表格"
  >
    <div class="table-header-controls" aria-hidden="true" />
    <div ref="tableContainerRef" class="table-scroll-wrapper">
      <table id="flightTable">
        <caption class="sr-only">
          航班实时监控表格，支持按列查看与导航
        </caption>
        <thead @contextmenu="onHeaderContextMenu">
          <tr>
            <th
              v-for="column in tableColumns"
              :key="column.key"
              :class="[column.class, { 'sortable-col': column.sortField, 'col-active-sort': props.sortField === column.sortField }]"
              :aria-sort="props.sortField === column.sortField ? (props.sortDirection === 'asc' ? 'ascending' : 'descending') : 'none'"
              @click="column.sortField ? handleSort(column.sortField) : undefined"
            >
              <span class="col-label">{{ column.label }}</span>
              <span v-if="column.sortField && props.sortField === column.sortField" class="sort-indicator" :class="props.sortDirection === 'asc' ? 'sort-asc' : 'sort-desc'">
                {{ props.sortDirection === 'asc' ? '↑' : '↓' }}
              </span>
            </th>
          </tr>
        </thead>
        <tbody id="flightTableBody">
          <tr v-if="!flights.length" class="virtual-spacer-row">
            <td :colspan="tableColumns.length">
              <EmptyState
                :icon="hasActiveFilters ? 'search' : 'plane'"
                :title="emptyMessage"
                :description="hasActiveFilters ? '请尝试调整筛选条件或清空搜索' : '暂无航班数据，请稍后刷新或联系管理员'"
              />
            </td>
          </tr>
          <template v-else>
            <tr class="virtual-spacer-row" aria-hidden="true">
              <td :colspan="tableColumns.length" :style="{ height: `${tableTopSpacer}px` }" />
            </tr>
            <tr
              v-for="flight in visibleTableFlights"
              :key="`row-${getFlightDomId(flight)}`"
              :class="[getStatusRowClassName(flight.status), { 'row-selected': isSelected(flight) }, getTableRowUpdateClass(flight)]"
              :aria-selected="isSelected(flight)"
              :data-flight-id="getFlightDomId(flight)"
              tabindex="0"
              @click="selectFlight(flight)"
              @keydown="handleRowKeydown($event, flight)"
            >
              <template v-for="column in tableColumns" :key="column.key">
                <td v-if="column.key === 'flight_number'">
                  <span v-if="getFlightNumbers(flight).inbound" :class="getFlightNumberStyleClass(flight, 'inbound')">{{ getFlightNumbers(flight).inbound }}</span>
                  <span v-if="getFlightNumbers(flight).inbound && getFlightNumbers(flight).outbound && getFlightNumbers(flight).inbound !== getFlightNumbers(flight).outbound"> / </span>
                  <span v-if="getFlightNumbers(flight).outbound && getFlightNumbers(flight).outbound !== getFlightNumbers(flight).inbound" :class="getFlightNumberStyleClass(flight, 'outbound')">{{ getFlightNumbers(flight).outbound }}</span>
                  <span v-else-if="!getFlightNumbers(flight).inbound" :class="getFlightNumberStyleClass(flight, 'outbound')">{{ getFlightNumbers(flight).combined }}</span>
                </td>
                <td v-if="column.key === 'status'">
                  <UiPill :tone="getStatusTone(flight.status)">
                    {{ flight.status || '计划中' }}
                  </UiPill>
                </td>
                <td
                  v-if="column.key === 'route'"
                  class="table-route route-toggle"
                  @dblclick.prevent="toggleRouteDisplayMode"
                >
                  <div class="flight-route centered">
                    <span class="flight-origin">{{ getFlightEndpoints(flight, airportContext).origin }}</span>
                    <span class="flight-arrow">-</span>
                    <span class="flight-destination">{{ getFlightEndpoints(flight, airportContext).destination }}</span>
                  </div>
                </td>
                <td
                  v-if="column.key === 'scheduled_departure'"
                  class="cell-time"
                  :class="[`cell-time--${getTimeDisplay(flight, 'departure').tone}`, cellSelectedClass(getFlightDomId(flight), 'scheduled_departure')]"
                  :data-flight-id="getFlightDomId(flight)"
                  data-field="scheduled_departure"
                  @pointerdown="onCellPointerDown($event, flight, 'scheduled_departure')"
                  @pointerenter="onCellPointerEnter($event, flight, 'scheduled_departure')"
                  @pointerup="onCellPointerUp"
                  @click="onCellClick($event, 'scheduled_departure')"
                  @contextmenu="onEditableContextMenu($event, flight, 'scheduled_departure')"
                >
                  {{ getTimeDisplay(flight, 'departure').value }}
                </td>
                <td
                  v-if="column.key === 'scheduled_arrival'"
                  class="cell-time"
                  :class="[`cell-time--${getTimeDisplay(flight, 'arrival').tone}`, cellSelectedClass(getFlightDomId(flight), 'scheduled_arrival')]"
                  :data-flight-id="getFlightDomId(flight)"
                  data-field="scheduled_arrival"
                  @pointerdown="onCellPointerDown($event, flight, 'scheduled_arrival')"
                  @pointerenter="onCellPointerEnter($event, flight, 'scheduled_arrival')"
                  @pointerup="onCellPointerUp"
                  @click="onCellClick($event, 'scheduled_arrival')"
                  @contextmenu="onEditableContextMenu($event, flight, 'scheduled_arrival')"
                >
                  {{ getTimeDisplay(flight, 'arrival').value }}
                </td>
                <td v-if="column.key === 'flight_type'">
                  {{ getFlightTypeColumnDisplay(flight) }}
                </td>
                <td v-if="SPLIT_TIME_COLUMN_FIELDS[column.key]" class="cell-time">
                  {{ getTimeFieldDisplay(flight, SPLIT_TIME_COLUMN_FIELDS[column.key]) }}
                </td>
                <!-- PR3：stand/baggage_carousel 为只读展示列（真相在占用服务），无编辑/圈选入口 -->
                <td
                  v-if="column.key === 'stand'"
                  :data-flight-id="getFlightDomId(flight)"
                  data-field="stand"
                >
                  {{ flight.stand || '—' }}
                </td>
                <td v-if="column.key === 'gate'">
                  {{ flight.gate || '—' }}
                </td>
                <td
                  v-if="column.key === 'baggage_carousel'"
                  :data-flight-id="getFlightDomId(flight)"
                  data-field="baggage_carousel"
                >
                  {{ flight.baggage_carousel || '—' }}
                </td>

                <td v-if="column.key === 'aircraft_type'">
                  {{ `${flight.aircraft_type_detail || '—'} · ${getAircraftBodyLabel(flight)}` }}
                </td>
                <td v-if="column.key === 'missions'">
                  {{ getMissionDisplay(flight) }}
                </td>
                <td
                  v-if="column.key === 'cobt_time'"
                  class="cell-time"
                  :class="cellSelectedClass(getFlightDomId(flight), 'cobt_time')"
                  :data-flight-id="getFlightDomId(flight)"
                  data-field="cobt_time"
                  @pointerdown="onCellPointerDown($event, flight, 'cobt_time')"
                  @pointerenter="onCellPointerEnter($event, flight, 'cobt_time')"
                  @pointerup="onCellPointerUp"
                  @click="onCellClick($event, 'cobt_time')"
                  @contextmenu="onEditableContextMenu($event, flight, 'cobt_time')"
                >
                  {{ getTimeFieldDisplay(flight, 'cobt_time') }}
                </td>
                <td v-if="column.key === 'codt'" class="cell-time">
                  {{ getTimeFieldDisplay(flight, 'codt') }}
                </td>
                <td
                  v-if="column.key === 'boarding_allowed_time'"
                  class="cell-time"
                  :class="cellSelectedClass(getFlightDomId(flight), 'boarding_allowed_time')"
                  :data-flight-id="getFlightDomId(flight)"
                  data-field="boarding_allowed_time"
                  @pointerdown="onCellPointerDown($event, flight, 'boarding_allowed_time')"
                  @pointerenter="onCellPointerEnter($event, flight, 'boarding_allowed_time')"
                  @pointerup="onCellPointerUp"
                  @click="onPunchCellClick($event, flight, 'boarding_allowed_time')"
                  @contextmenu="onEditableContextMenu($event, flight, 'boarding_allowed_time')"
                >
                  <span v-if="getTimeFieldRawValue(flight, 'boarding_allowed_time')" class="cell-punch-value">{{ getTimeFieldDisplay(flight, 'boarding_allowed_time') }}</span>
                  <span
                    v-else
                    class="cell-punch-placeholder"
                    role="button"
                    :aria-label="`打卡${getPunchTimeFieldLabel('boarding_allowed_time')}`"
                    title="点击打卡当前时间"
                  >+</span>
                </td>
                <td
                  v-if="column.key === 'start_boarding_time'"
                  class="cell-time"
                  :class="cellSelectedClass(getFlightDomId(flight), 'start_boarding_time')"
                  :data-flight-id="getFlightDomId(flight)"
                  data-field="start_boarding_time"
                  @pointerdown="onCellPointerDown($event, flight, 'start_boarding_time')"
                  @pointerenter="onCellPointerEnter($event, flight, 'start_boarding_time')"
                  @pointerup="onCellPointerUp"
                  @click="onCellClick($event, 'start_boarding_time')"
                  @contextmenu="onEditableContextMenu($event, flight, 'start_boarding_time')"
                >
                  {{ getTimeFieldDisplay(flight, 'start_boarding_time') }}
                </td>
                <td
                  v-if="column.key === 'end_boarding_time'"
                  class="cell-time"
                  :class="cellSelectedClass(getFlightDomId(flight), 'end_boarding_time')"
                  :data-flight-id="getFlightDomId(flight)"
                  data-field="end_boarding_time"
                  @pointerdown="onCellPointerDown($event, flight, 'end_boarding_time')"
                  @pointerenter="onCellPointerEnter($event, flight, 'end_boarding_time')"
                  @pointerup="onCellPointerUp"
                  @click="onCellClick($event, 'end_boarding_time')"
                  @contextmenu="onEditableContextMenu($event, flight, 'end_boarding_time')"
                >
                  {{ getTimeFieldDisplay(flight, 'end_boarding_time') }}
                </td>
                <td
                  v-if="column.key === 'passenger_ready_time'"
                  class="cell-time cell-punch"
                  :data-flight-id="getFlightDomId(flight)"
                  data-field="passenger_ready_time"
                  @pointerdown="recordPunchPointerDown"
                  @click="onPunchCellClick($event, flight, 'passenger_ready_time')"
                  @contextmenu="onPunchContextMenu($event, flight, 'passenger_ready_time')"
                >
                  <span v-if="getTimeFieldRawValue(flight, 'passenger_ready_time')" class="cell-punch-value">{{ getTimeFieldDisplay(flight, 'passenger_ready_time') }}</span>
                  <span
                    v-else
                    class="cell-punch-placeholder"
                    role="button"
                    :aria-label="`打卡${getPunchTimeFieldLabel('passenger_ready_time')}`"
                    title="点击打卡当前时间"
                  >+</span>
                </td>
                <td
                  v-if="column.key === 'on_blocks_time'"
                  class="cell-time"
                  :class="cellSelectedClass(getFlightDomId(flight), 'on_blocks_time')"
                  :data-flight-id="getFlightDomId(flight)"
                  data-field="on_blocks_time"
                  @pointerdown="onCellPointerDown($event, flight, 'on_blocks_time')"
                  @pointerenter="onCellPointerEnter($event, flight, 'on_blocks_time')"
                  @pointerup="onCellPointerUp"
                  @click="onPunchCellClick($event, flight, 'on_blocks_time')"
                  @contextmenu="onEditableContextMenu($event, flight, 'on_blocks_time')"
                >
                  <span v-if="getTimeFieldRawValue(flight, 'on_blocks_time')" class="cell-punch-value">{{ getTimeFieldDisplay(flight, 'on_blocks_time') }}</span>
                  <span
                    v-else
                    class="cell-punch-placeholder"
                    role="button"
                    :aria-label="`打卡${getPunchTimeFieldLabel('on_blocks_time')}`"
                    title="点击打卡当前时间"
                  >+</span>
                </td>
                <td
                  v-if="column.key === 'off_blocks_time'"
                  class="cell-time"
                  :class="cellSelectedClass(getFlightDomId(flight), 'off_blocks_time')"
                  :data-flight-id="getFlightDomId(flight)"
                  data-field="off_blocks_time"
                  @pointerdown="onCellPointerDown($event, flight, 'off_blocks_time')"
                  @pointerenter="onCellPointerEnter($event, flight, 'off_blocks_time')"
                  @pointerup="onCellPointerUp"
                  @click="onPunchCellClick($event, flight, 'off_blocks_time')"
                  @contextmenu="onEditableContextMenu($event, flight, 'off_blocks_time')"
                >
                  <span v-if="getTimeFieldRawValue(flight, 'off_blocks_time')" class="cell-punch-value">{{ getTimeFieldDisplay(flight, 'off_blocks_time') }}</span>
                  <span
                    v-else
                    class="cell-punch-placeholder"
                    role="button"
                    :aria-label="`打卡${getPunchTimeFieldLabel('off_blocks_time')}`"
                    title="点击打卡当前时间"
                  >+</span>
                </td>
                <td
                  v-if="EXTRA_PUNCH_FIELD_KEYS.has(column.key)"
                  class="cell-time cell-punch"
                  :data-flight-id="getFlightDomId(flight)"
                  :data-field="column.key"
                  @pointerdown="recordPunchPointerDown"
                  @click="onPunchCellClick($event, flight, column.key)"
                  @contextmenu="onPunchContextMenu($event, flight, column.key)"
                >
                  <span v-if="getTimeFieldRawValue(flight, column.key)" class="cell-punch-value">{{ getTimeFieldDisplay(flight, column.key) }}</span>
                  <span
                    v-else
                    class="cell-punch-placeholder"
                    role="button"
                    :aria-label="`打卡${getPunchTimeFieldLabel(column.key)}`"
                    title="点击打卡当前时间"
                  >+</span>
                </td>
                <td
                  v-if="column.key === 'remarks'"
                  class="cell-remarks"
                  :class="cellSelectedClass(getFlightDomId(flight), 'flight_remarks')"
                  :data-flight-id="getFlightDomId(flight)"
                  data-field="flight_remarks"
                  :title="flight.flight_remarks || ''"
                  @pointerdown="onCellPointerDown($event, flight, 'flight_remarks')"
                  @pointerenter="onCellPointerEnter($event, flight, 'flight_remarks')"
                  @pointerup="onCellPointerUp"
                  @click="onCellClick($event, 'flight_remarks')"
                  @contextmenu="onEditableContextMenu($event, flight, 'flight_remarks')"
                  @dblclick="emit('edit-field', getFlightDomId(flight), 'flight_remarks', 'text', flight.flight_remarks || '')"
                >
                  {{ flight.flight_remarks || '—' }}
                </td>
                <td
                  v-if="EXTRA_REMARK_FIELD_KEYS.has(column.key)"
                  class="cell-remarks"
                  :title="readCellRawValue(flight, column.key)"
                  @dblclick="emit('edit-field', getFlightDomId(flight), column.key, 'text', readCellRawValue(flight, column.key))"
                >
                  {{ readCellRawValue(flight, column.key) || '—' }}
                </td>
                <td v-if="column.key === 'registration'">
                  {{ flight.registration || '—' }}
                </td>
                <td v-if="column.key === 'tags'">
                  {{ `${getMissionDisplay(flight)} · ${getCommercialSignedLabel(flight)}` }}
                </td>
              </template>
            </tr>
            <tr class="virtual-spacer-row" aria-hidden="true">
              <td :colspan="tableColumns.length" :style="{ height: `${tableBottomSpacer}px` }" />
            </tr>
          </template>
        </tbody>
      </table>
    </div>
    <p id="tableScrollHint" class="table-scroll-hint" aria-hidden="true">
      左右滑动可查看更多列
    </p>
  </div>

  <teleport to="body">
    <UiMenu
      v-if="punchMenu.isOpen"
      class="punch-context-menu"
      :x="punchMenu.x"
      :y="punchMenu.y"
      min-width="140px"
      label="打卡时间操作"
    >
      <UiMenuItem
        class="punch-context-menu-item"
        :tone="canPunchField(punchMenu.field) ? 'ink' : 'mute'"
        @click.stop="onPunchMenuModify"
      >
        修改
      </UiMenuItem>
      <UiMenuItem
        class="punch-context-menu-item"
        :tone="canPunchField(punchMenu.field) ? 'danger' : 'mute'"
        @click.stop="onPunchMenuRevoke"
      >
        撤销
      </UiMenuItem>
    </UiMenu>
  </teleport>

  <teleport to="body">
    <UiMenu
      v-if="headerMenu.isOpen"
      id="headerContextMenu"
      class="punch-context-menu"
      :x="headerMenu.x"
      :y="headerMenu.y"
      min-width="140px"
      label="表格列设置"
    >
      <UiMenuItem
        id="ctxConfigColumns"
        class="punch-context-menu-item"
        @click.stop="onHeaderMenuConfigColumns"
      >
        配置列...
      </UiMenuItem>
    </UiMenu>
  </teleport>

  <!-- 幕、帽、脚、Esc、层序都归 UiModal（§3.8 / §3.5）；这里只剩一个器和两颗谓词。
       名已经写在帽上，字段这一格不再重一遍（§4.4）。 -->
  <UiModal
    :open="punchEdit.isOpen"
    :title="`修改${punchEdit.label}`"
    :width="360"
    @close="punchEdit.isOpen = false"
  >
    <UiField>
      <input
        v-model="punchEdit.value"
        type="datetime-local"
        :aria-label="punchEdit.label"
        @keydown.enter="onPunchEditSave"
      >
    </UiField>
    <template #footer>
      <UiButton @click="punchEdit.isOpen = false">
        取消
      </UiButton>
      <UiButton variant="primary" :disabled="punchEdit.saving" @click="onPunchEditSave">
        {{ punchEdit.saving ? '保存中…' : '保存' }}
      </UiButton>
    </template>
  </UiModal>
</template>

<style scoped>
.cell-punch-placeholder {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 20px;
  height: 20px;
  padding: 0 4px;
  border-radius: var(--r-cell);
  border: 1px dashed var(--line-strong);
  color: var(--ink-muted);
  font-weight: var(--fw-semibold);
  line-height: 1;
  cursor: pointer;
  user-select: none;
}

/* 「可以填」由那圈虚线说，交感只淡墨一层（§4.2 悬停不许用行动蓝） */
.cell-punch-placeholder:hover {
  color: var(--ink-subtle);
  border-color: var(--ink-subtle);
  background: color-mix(in srgb, var(--ink) 10%, transparent);
}

.cell-punch-value {
  cursor: context-menu;
}

/* 旧布局占位：永久隐藏 */
.table-header-controls {
  display: none;
}

/* 航线格可双击换显示法，指针说明可点 */
.route-toggle {
  cursor: pointer;
  user-select: none;
}
</style>
