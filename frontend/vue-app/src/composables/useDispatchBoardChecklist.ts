import { useApi } from '@/composables/useApi';
import { unwrapApiData, unwrapApiResultOrThrow, toStringArray } from './useDispatchBoardApi';

export interface SafetyProgressEntry {
  dispatch_order_id: string;
  task_type: string;
  enforced: boolean;
  ready: boolean;
  required_total: number;
  completed_required: number;
  pending_required_count: number;
  failed_required_count: number;
  template_version: string | null;
  blocking_issues: string[];
  soft_missing_count: number;
  can_soft_complete: boolean;
}


export type SafetyProgressMap = Record<string, SafetyProgressEntry>;

export type DispatchChecklistLevel = 'critical' | 'routine';
export type DispatchChecklistResult = 'pass' | 'fail' | 'na' | 'pending';


export interface DispatchOrderSafetyChecklistItem {
  item_code: string;
  title: string;
  required: boolean;
  allow_na: boolean;
  order: number;
  level: DispatchChecklistLevel;
  result: DispatchChecklistResult | null;
  checked_by: string | null;
  checked_by_username: string | null;
  checked_at: string | null;
  note: string | null;
  status: DispatchChecklistResult;
}


export interface DispatchOrderSafetyChecklist {
  dispatch_order_id: string;
  task_type: string;
  template_id: string | null;
  template_version: string | null;
  enforced: boolean;
  ready: boolean;
  required_total: number;
  completed_required: number;
  pending_required_items: string[];
  failed_required_items: string[];
  blocking_issues: string[];
  soft_missing_count: number;
  can_soft_complete: boolean;
  routine_total: number;
  completed_routine: number;
  pending_routine_items: string[];
  failed_routine_items: string[];
  items: DispatchOrderSafetyChecklistItem[];
}


export interface DispatchOrderCompletionGateHint {
  message: string;
  pending_required_items: string[];
  failed_required_items: string[];
  blocking_issues: string[];
  soft_missing_count: number;
  can_soft_complete: boolean;
  required_total: number;
  completed_required: number;
  template_version: string | null;
}


export interface DispatchChecklistItemSubmitPayload {
  result: Exclude<DispatchChecklistResult, 'pending'>;
  note?: string | null;
  handled_on_site?: boolean;
}


export interface DispatchChecklistBatchSubmitItem {
  item_code: string;
  result: Exclude<DispatchChecklistResult, 'pending'>;
  note?: string | null;
  handled_on_site?: boolean;
}


export function normalizeChecklistLevel(value: unknown): DispatchChecklistLevel {
  return String(value ?? '').trim().toLowerCase() === 'routine' ? 'routine' : 'critical';
}


export function normalizeChecklistResult(value: unknown): DispatchChecklistResult {
  const normalized = String(value ?? '').trim().toLowerCase();
  if (normalized === 'pass' || normalized === 'fail' || normalized === 'na') {
    return normalized;
  }
  return 'pending';
}


export function normalizeDispatchOrderCompletionGateHint(
  raw: unknown,
  fallbackMessage = '关键安全检查未完成，无法完工',
): DispatchOrderCompletionGateHint | null {
  if (!raw || typeof raw !== 'object') return null;
  const record = raw as Record<string, unknown>;
  return {
    message: String(record.message ?? fallbackMessage).trim() || fallbackMessage,
    pending_required_items: toStringArray(record.pending_required_items),
    failed_required_items: toStringArray(record.failed_required_items),
    blocking_issues: toStringArray(record.blocking_issues),
    soft_missing_count: Number(record.soft_missing_count ?? 0),
    can_soft_complete: Boolean(record.can_soft_complete ?? true),
    required_total: Number(record.required_total ?? 0),
    completed_required: Number(record.completed_required ?? 0),
    template_version: String(record.template_version ?? '').trim() || null,
  };
}


export function normalizeDispatchOrderSafetyChecklistItem(raw: unknown): DispatchOrderSafetyChecklistItem | null {
  if (!raw || typeof raw !== 'object') return null;
  const record = raw as Record<string, unknown>;
  const rawResult = record.result === null || record.result === undefined
    ? null
    : normalizeChecklistResult(record.result);
  return {
    item_code: String(record.item_code ?? '').trim(),
    title: String(record.title ?? '').trim(),
    required: Boolean(record.required),
    allow_na: Boolean(record.allow_na),
    order: Number(record.order ?? 0),
    level: normalizeChecklistLevel(record.level),
    result: rawResult,
    checked_by: String(record.checked_by ?? '').trim() || null,
    checked_by_username: String(record.checked_by_username ?? '').trim() || null,
    checked_at: String(record.checked_at ?? '').trim() || null,
    note: String(record.note ?? '').trim() || null,
    status: normalizeChecklistResult(record.status ?? record.result),
  };
}


export function normalizeDispatchOrderSafetyChecklist(raw: unknown): DispatchOrderSafetyChecklist | null {
  if (!raw || typeof raw !== 'object') return null;
  const record = raw as Record<string, unknown>;
  return {
    dispatch_order_id: String(record.dispatch_order_id ?? '').trim(),
    task_type: String(record.task_type ?? '').trim(),
    template_id: String(record.template_id ?? '').trim() || null,
    template_version: String(record.template_version ?? '').trim() || null,
    enforced: Boolean(record.enforced),
    ready: Boolean(record.ready),
    required_total: Number(record.required_total ?? 0),
    completed_required: Number(record.completed_required ?? 0),
    pending_required_items: toStringArray(record.pending_required_items),
    failed_required_items: toStringArray(record.failed_required_items),
    blocking_issues: toStringArray(record.blocking_issues),
    soft_missing_count: Number(record.soft_missing_count ?? 0),
    can_soft_complete: Boolean(record.can_soft_complete ?? true),
    routine_total: Number(record.routine_total ?? 0),
    completed_routine: Number(record.completed_routine ?? 0),
    pending_routine_items: toStringArray(record.pending_routine_items),
    failed_routine_items: toStringArray(record.failed_routine_items),
    items: (Array.isArray(record.items) ? record.items : [])
      .map(normalizeDispatchOrderSafetyChecklistItem)
      .filter((item): item is DispatchOrderSafetyChecklistItem => item !== null)
      .sort((left, right) => left.order - right.order),
  };
}


export async function fetchOrderSafetyChecklist(orderId: string): Promise<DispatchOrderSafetyChecklist | null> {
  const { get } = useApi();
  const result = await get<unknown>(
    `/api/v2/dispatch-orders/${encodeURIComponent(orderId)}/safety-checklist`,
  );
  if (!result.ok || !result.data) return null;
  return normalizeDispatchOrderSafetyChecklist(unwrapApiData(result.data));
}


export async function submitDispatchOrderSafetyChecklistItem(
  orderId: string,
  itemCode: string,
  payload: DispatchChecklistItemSubmitPayload,
): Promise<DispatchOrderSafetyChecklistItem | null> {
  const { post } = useApi();
  const result = await post<unknown>(
    `/api/v2/dispatch-orders/${encodeURIComponent(orderId)}/safety-checklist/items/${encodeURIComponent(itemCode)}`,
    payload,
  );
  const record = unwrapApiResultOrThrow<Record<string, unknown> | null>(result, '提交安全清单失败');
  return normalizeDispatchOrderSafetyChecklistItem(record);
}


export async function submitDispatchOrderSafetyChecklistBatch(
  orderId: string,
  items: DispatchChecklistBatchSubmitItem[],
): Promise<DispatchOrderSafetyChecklist | null> {
  const { post } = useApi();
  const result = await post<unknown>(
    `/api/v2/dispatch-orders/${encodeURIComponent(orderId)}/safety-checklist/batch-submit`,
    { items },
  );
  const record = unwrapApiResultOrThrow<Record<string, unknown> | null>(result, '批量提交安全清单失败');
  return normalizeDispatchOrderSafetyChecklist(record);
}

