import { useApi } from '@/composables/useApi';
import { unwrapApiData, unwrapApiResultOrThrow } from './useDispatchBoardApi';

export type DispatchOrderStatus = 'pending' | 'assigned' | 'in_progress' | 'completed' | 'cancelled';

export interface DispatchOrder {
  id?: string | null;
  order_id: string;
  flight_id?: string | null;
  task_type?: string | null;
  task_type_name?: string | null;
  status?: DispatchOrderStatus | null;
  team_id?: string | null;
  team_name?: string | null;
  terminal?: string | null;
  lane_id?: string | null;
  lane_label?: string | null;
  focus_user_id?: string | null;
  focus_user_name?: string | null;
  individual_user_id?: string | null;
  individual_username?: string | null;
  start_time?: string | null;
  end_time?: string | null;
  planned_start_time?: string | null;
  planned_end_time?: string | null;
  actual_start_time?: string | null;
  actual_end_time?: string | null;
  estimated_completion_time?: string | null;
  effective_end_time?: string | null;
  stand_id?: string | null;
  stand_code?: string | null;
  gate?: string | null;
  source?: string | null;
  origin_label?: string | null;
  dispatch_type?: string | null;
  publication_state?: string | null;
  focus_item_id?: string | null;
  focus_resource_type?: string | null;
  equipment_codes?: ReadonlyArray<string>;
  qualification_gap?: ReadonlyArray<DispatchQualificationGap>;
  related_order_ids?: ReadonlyArray<string>;
  related_orders?: ReadonlyArray<DispatchOrder>;
  flight_no?: string | null;
  notification_receipt_summary?: Record<string, unknown> | null;
  conflict_reason?: string | null;
  availability_reason?: string | null;
  lock_level?: string | null;
  members?: ReadonlyArray<TimelineMember>;
  task_crew?: TaskCrew | null;
  is_flight_summary?: boolean;
  [key: string]: unknown;
}


export interface DispatchQualificationGap {
  slot_code?: string | null;
  qualification_code?: string | null;
  min_level_code?: string | null;
  [key: string]: unknown;
}


export interface TimelineMember {
  user_id?: string | null;
  username?: string | null;
  user_display_name?: string | null;
  name?: string | null;
  slot_code?: string | null;
  qualification_code?: string | null;
  qualification_level_code?: string | null;
  source_team_id?: string | null;
  source_team_name?: string | null;
  [key: string]: unknown;
}

/** 工单只读班组名：名册投影，不是 order.team_id 指派。 */
export function rosterTeamLabel(order: DispatchOrder | null | undefined): string {
  const direct = String(order?.team_name ?? '').trim();
  if (direct) return direct;
  const names = new Set<string>();
  const crewNames = order?.task_crew && Array.isArray(order.task_crew.source_team_names)
    ? order.task_crew.source_team_names
    : [];
  for (const name of crewNames) {
    const trimmed = String(name ?? '').trim();
    if (trimmed) names.add(trimmed);
  }
  for (const member of order?.members ?? []) {
    const trimmed = String(member.source_team_name ?? '').trim();
    if (trimmed) names.add(trimmed);
  }
  return Array.from(names).join(' / ');
}

export function rosterTeamIds(order: DispatchOrder | null | undefined): string[] {
  const ids = new Set<string>();
  const direct = String(order?.team_id ?? '').trim();
  if (direct) ids.add(direct);
  const crewIds = order?.task_crew && Array.isArray(order.task_crew.source_team_ids)
    ? order.task_crew.source_team_ids
    : [];
  for (const id of crewIds) {
    const trimmed = String(id ?? '').trim();
    if (trimmed) ids.add(trimmed);
  }
  for (const member of order?.members ?? []) {
    const trimmed = String(member.source_team_id ?? '').trim();
    if (trimmed) ids.add(trimmed);
  }
  return Array.from(ids);
}


export interface TaskCrew {
  members?: ReadonlyArray<TimelineMember>;
  source_team_ids?: ReadonlyArray<string>;
  source_team_names?: ReadonlyArray<string>;
  [key: string]: unknown;
}


export interface DispatchOrderCompletePayload {
  actual_end_time: string;
  completion_notes?: string | null;
}


export interface DispatchBoardApiError extends Error {
  status?: number;
  detail?: unknown;
}


export const STATUS_LABELS: Record<DispatchOrderStatus, string> = Object.freeze({
  pending: '待派工',
  assigned: '已分配',
  in_progress: '进行中',
  completed: '已完成',
  cancelled: '已取消',
});


export const STATUS_SYMBOLS: Record<DispatchOrderStatus, string> = Object.freeze({
  pending: '○',
  assigned: '●',
  in_progress: '▶',
  completed: '✓',
  cancelled: '×',
});


export const STATUS_ORDER: readonly DispatchOrderStatus[] = Object.freeze([
  'pending',
  'assigned',
  'in_progress',
  'completed',
  'cancelled',
]);


export function resolveDispatchOrderId(order: Pick<DispatchOrder, 'order_id' | 'id'> | null | undefined): string {
  return String(order?.order_id ?? order?.id ?? '').trim();
}


export async function fetchOrder(orderId: string): Promise<DispatchOrder | null> {
  const { get } = useApi();
  const result = await get<unknown>(`/api/v2/dispatch-orders/${encodeURIComponent(orderId)}`);
  if (!result.ok || !result.data) return null;
  return unwrapApiData(result.data) as DispatchOrder | null;
}

/** Fetch all orders for a flight. */

export async function fetchOrdersByFlight(flightId: string): Promise<DispatchOrder[]> {
  const { get } = useApi();
  const result = await get<unknown>(
    `/api/v2/dispatch-orders?flight_id=${encodeURIComponent(flightId)}&page=1&page_size=100`,
  );
  if (!result.ok || !result.data) return [];
  const payload = unwrapApiData(result.data);
  return Array.isArray(payload) ? (payload as DispatchOrder[]) : [];
}

/** Fetch safety checklist for a single order. */

export async function completeDispatchOrder(
  orderId: string,
  payload: DispatchOrderCompletePayload,
): Promise<Record<string, unknown> | null> {
  const { post } = useApi();
  const result = await post<unknown>(
    `/api/v2/dispatch-orders/${encodeURIComponent(orderId)}/complete`,
    payload,
  );
  return unwrapApiResultOrThrow<Record<string, unknown> | null>(result, '完工失败');
}

/** Cancel a dispatch order. */

export async function cancelOrder(orderId: string): Promise<Record<string, unknown> | null> {
  const { post } = useApi();
  const result = await post<unknown>(
    `/api/v2/dispatch-orders/${encodeURIComponent(orderId)}/cancel`,
  );
  return unwrapApiResultOrThrow<Record<string, unknown> | null>(result, '取消工单失败');
}

/** Publish a single dispatch order. */

export async function publishOrder(orderId: string): Promise<Record<string, unknown> | null> {
  const { post } = useApi();
  const result = await post<unknown>(
    `/api/v2/dispatch-orders/${encodeURIComponent(orderId)}/publish`,
  );
  return unwrapApiResultOrThrow<Record<string, unknown> | null>(result, '发布工单失败');
}

/** Report an issue on a dispatch order. */

export async function reportOrderIssue(
  orderId: string,
  payload: Record<string, unknown>,
): Promise<Record<string, unknown> | null> {
  const { post } = useApi();
  const result = await post<unknown>(
    `/api/v2/dispatch-orders/${encodeURIComponent(orderId)}/report-issue`,
    payload,
  );
  return unwrapApiResultOrThrow<Record<string, unknown> | null>(result, '上报问题失败');
}

/** Report estimated completion time for a dispatch order. */

export async function reportOrderEta(
  orderId: string,
  payload: Record<string, unknown>,
): Promise<Record<string, unknown> | null> {
  const { post } = useApi();
  const result = await post<unknown>(
    `/api/v2/dispatch-orders/${encodeURIComponent(orderId)}/eta-report`,
    payload,
  );
  return unwrapApiResultOrThrow<Record<string, unknown> | null>(result, '上报预计完成时间失败');
}

/** Upload a file for mobile attachments. */

export async function uploadMobileFile(
  file: File,
): Promise<Record<string, unknown> | null> {
  const { post } = useApi();
  const formData = new FormData();
  formData.append('file', file);
  const result = await post<unknown>('/api/v2/mobile/uploads', formData);
  return unwrapApiResultOrThrow<Record<string, unknown> | null>(result, '上传文件失败');
}

/** Load configured terminals from stands API, fallback to timeline. */

export interface BatchProcessState {
  isRunning: boolean;
  currentIndex: number;
  totalItems: number;
  successCount: number;
  failCount: number;
  currentOrderId: string | null;
  errors: Array<{ orderId: string; error: string }>;
  isGuided: boolean;
  orderIds: string[];
}

/**
 * Move the current index in a BatchProcessState.
 * Uses 1-based indexing for currentIndex to align with UI and batchCompleteOrders.
 */

export function moveBatchIndex(state: BatchProcessState, step: number): void {
  const next = state.currentIndex + step;
  const orderIds = state.orderIds || [];
  const len = orderIds.length;
  if (len === 0) return;

  if (next >= 1 && next <= len) {
    state.currentIndex = next;
    state.currentOrderId = orderIds[next - 1];
  }
}


export async function batchCompleteOrders(
  orderIds: string[],
  onProgress?: (current: number, total: number, orderId: string) => void,
): Promise<{ success: number; failed: number; errors: Array<{ orderId: string; error: string }> }> {
  const { post } = useApi();
  let successCount = 0;
  let failCount = 0;
  const errors: Array<{ orderId: string; error: string }> = [];

  for (let i = 0; i < orderIds.length; i++) {
    const orderId = orderIds[i];
    if (onProgress) onProgress(i + 1, orderIds.length, orderId);

    try {
      const result = await post(`/api/v2/dispatch-orders/${encodeURIComponent(orderId)}/complete`, {
        actual_end_time: new Date().toISOString(),
      });
      if (result.ok) {
        successCount++;
      } else {
        failCount++;
        errors.push({ orderId, error: `HTTP ${result.status}` });
      }
    } catch (err) {
      failCount++;
      errors.push({ orderId, error: err instanceof Error ? err.message : String(err) });
    }
  }

  return { success: successCount, failed: failCount, errors };
}

