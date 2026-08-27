/**
 * Registry of flight-monitor table cells that support same-column multi-select
 * and batch edit. Keys match the FlightList cell field identifiers used in
 * context-menu / edit-field handlers (not necessarily BASE_COLUMNS keys).
 *
 * PR3（本体两层改造）：stand / gate / terminal / baggage_carousel 为只读展示列，
 * 真相在占用服务（StandOccupation / GateAssignment / CarouselAssignment），
 * 监控不再提供这四列的批量/单格编辑入口，后端 batch/PATCH 同步拒绝。
 */

export type BatchWriteStrategy = 'flight_patch' | 'timeline_event';
export type BatchValueType = 'datetime' | 'text';

export interface BatchEditableField {
  field: string;
  label: string;
  valueType: BatchValueType;
  writeStrategy: BatchWriteStrategy;
  /** Requires admin (is_admin / role admin / permission *). */
  adminOnly?: boolean;
  maxLength?: number;
  /** Column key in FlightList BASE_COLUMNS when it differs from `field`. */
  columnKey?: string;
}

export const BATCH_EDITABLE_FIELDS: readonly BatchEditableField[] = [
  {
    field: 'scheduled_departure',
    label: '计划起飞',
    valueType: 'datetime',
    writeStrategy: 'flight_patch',
    // External sync-controlled: admin / * only (matches backend sync_locked).
    adminOnly: true,
  },
  {
    field: 'scheduled_arrival',
    label: '计划到达',
    valueType: 'datetime',
    writeStrategy: 'flight_patch',
    adminOnly: true,
  },
  {
    field: 'cobt_time',
    label: 'COBT',
    valueType: 'datetime',
    writeStrategy: 'flight_patch',
    adminOnly: true,
  },
  {
    field: 'boarding_allowed_time',
    label: '允许登机',
    valueType: 'datetime',
    writeStrategy: 'timeline_event',
  },
  {
    field: 'start_boarding_time',
    label: '开始登机',
    valueType: 'datetime',
    writeStrategy: 'timeline_event',
  },
  {
    field: 'end_boarding_time',
    label: '结束登机',
    valueType: 'datetime',
    writeStrategy: 'timeline_event',
  },
  {
    field: 'on_blocks_time',
    label: '上轮挡',
    valueType: 'datetime',
    writeStrategy: 'timeline_event',
  },
  {
    field: 'off_blocks_time',
    label: '撤轮挡',
    valueType: 'datetime',
    writeStrategy: 'timeline_event',
  },
  {
    field: 'flight_remarks',
    label: '航班备注',
    valueType: 'text',
    writeStrategy: 'flight_patch',
    maxLength: 500,
    columnKey: 'remarks',
  },
] as const;

export const BATCH_EDITABLE_FIELD_MAP: ReadonlyMap<string, BatchEditableField> = new Map(
  BATCH_EDITABLE_FIELDS.map((entry) => [entry.field, entry]),
);

/**
 * Interactive punch (打卡) time columns, aligned with the legacy flight_monitor
 * interactive time cells: an empty cell renders a clickable 「+」 placeholder
 * that writes the current time on click; a valued cell offers 修改/撤销 via a
 * context menu.
 *
 * Single-cell writes for these fields always use the dispatch-timeline API
 * (the `timeline_event` write path — the same endpoint the app already uses
 * for single-cell timeline revoke). Only the fields also registered in
 * BATCH_EDITABLE_FIELDS above are accepted by the backend batch-cells
 * endpoint (`FlightBatchEditableField` enum), so the rest are intentionally
 * NOT multi-select batch editable.
 */
export interface PunchTimeField {
  field: string;
  label: string;
  /** True when the field is also registered for batch edit above. */
  batchEditable: boolean;
}

export const PUNCH_TIME_FIELDS: readonly PunchTimeField[] = [
  { field: 'on_blocks_time', label: '上轮挡', batchEditable: true },
  { field: 'cabin_door_open_time', label: '开舱门', batchEditable: false },
  { field: 'deboarding_complete_time', label: '下客完成', batchEditable: false },
  { field: 'cleaning_start_time', label: '清洁开始', batchEditable: false },
  { field: 'cleaning_end_time', label: '清洁结束', batchEditable: false },
  { field: 'cabin_door_close_time', label: '关客舱门', batchEditable: false },
  { field: 'cargo_door_close_time', label: '关货舱门', batchEditable: false },
  { field: 'loading_complete_time', label: '装载完成', batchEditable: false },
  { field: 'off_blocks_time', label: '撤轮挡', batchEditable: true },
  { field: 'passenger_ready_time', label: '人齐', batchEditable: false },
  { field: 'boarding_allowed_time', label: '允许登机', batchEditable: true },
] as const;

export const PUNCH_TIME_FIELD_MAP: ReadonlyMap<string, PunchTimeField> = new Map(
  PUNCH_TIME_FIELDS.map((entry) => [entry.field, entry]),
);

/** Interactive punch (打卡) time column field identifiers. */
export const PUNCH_TIME_FIELD_KEYS = new Set(PUNCH_TIME_FIELDS.map((entry) => entry.field));

export function isPunchTimeField(field: string): boolean {
  return PUNCH_TIME_FIELD_KEYS.has(field);
}

export function getPunchTimeFieldLabel(field: string): string {
  return PUNCH_TIME_FIELD_MAP.get(field)?.label ?? field;
}

/** Fields that can be multi-selected / batch-edited. */
export const BATCH_EDITABLE_FIELD_KEYS = new Set(BATCH_EDITABLE_FIELDS.map((entry) => entry.field));

export function isBatchEditableField(field: string): boolean {
  return BATCH_EDITABLE_FIELD_KEYS.has(field);
}

export function getBatchEditableField(field: string): BatchEditableField | undefined {
  return BATCH_EDITABLE_FIELD_MAP.get(field);
}

export function getBatchFieldLabel(field: string): string {
  return BATCH_EDITABLE_FIELD_MAP.get(field)?.label ?? field;
}

export function getBatchFieldValueType(field: string): BatchValueType {
  return BATCH_EDITABLE_FIELD_MAP.get(field)?.valueType ?? 'text';
}

export function getBatchFieldWriteStrategy(field: string): BatchWriteStrategy {
  return BATCH_EDITABLE_FIELD_MAP.get(field)?.writeStrategy ?? 'flight_patch';
}
