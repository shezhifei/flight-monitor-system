import { computed, ref } from 'vue';
import type { ComputedRef, Ref } from 'vue';
import { useToast } from '@/composables/useToast';
import {
  buildResourceFocus,
  completeDispatchOrder,
  fetchOrder,
  fetchOrdersByFlight,
  fetchOrderSafetyChecklist,
  normalizeChecklistLevel,
  normalizeDispatchOrderCompletionGateHint,
  resolveDispatchOrderId,
  submitDispatchOrderSafetyChecklistBatch,
  submitDispatchOrderSafetyChecklistItem,
  type DispatchChecklistResult,
  type DispatchOrder,
  type DispatchOrderCompletionGateHint,
  type DispatchOrderSafetyChecklist,
  type DispatchOrderSafetyChecklistItem,
  type ResourceFocus,
} from '@/composables/useDispatchBoardData';

export type DispatchBoardDetailMode = 'idle' | 'order' | 'flight';
export type DispatchBoardSafetyGateState = 'ready' | 'pending' | 'blocked' | 'unknown';
export type DispatchBoardDetailSource = 'gantt' | 'status_panel' | 'batch' | 'flight_summary' | 'conflict' | 'scenario' | 'ai_assistant' | 'overrun_warning' | 'unknown';

export interface DispatchBoardFlightSummary {
  flight_id?: string | null;
  flight_no?: string | null;
  start_time?: string | null;
  end_time?: string | null;
  status?: string | null;
  related_order_ids?: ReadonlyArray<string>;
  related_orders?: ReadonlyArray<DispatchOrder>;
  [key: string]: unknown;
}

export interface DispatchBoardDetailFocusContext {
  focus: ResourceFocus | null;
  text: string;
}

export interface UseDispatchBoardDetailOptions {
  refreshTimeline: () => Promise<void>;
  refreshSafetyProgress: () => Promise<void>;
  onFocusChange?: (context: DispatchBoardDetailFocusContext) => void;
}

export interface UseDispatchBoardDetailReturn {
  isVisible: Readonly<Ref<boolean>>;
  mode: Readonly<Ref<DispatchBoardDetailMode>>;
  source: Readonly<Ref<DispatchBoardDetailSource>>;
  title: ComputedRef<string>;
  order: Readonly<Ref<DispatchOrder | null>>;
  flightSummary: Readonly<Ref<DispatchBoardFlightSummary | null>>;
  flightOrders: Readonly<Ref<DispatchOrder[]>>;
  checklist: Readonly<Ref<DispatchOrderSafetyChecklist | null>>;
  checklistLoading: Readonly<Ref<boolean>>;
  checklistError: Readonly<Ref<string>>;
  gateHint: Readonly<Ref<DispatchOrderCompletionGateHint | null>>;
  submittingKey: Readonly<Ref<string>>;
  opening: Readonly<Ref<boolean>>;
  completing: Readonly<Ref<boolean>>;
  currentOrderId: ComputedRef<string>;
  safetyGateState: ComputedRef<DispatchBoardSafetyGateState>;
  criticalChecklistItems: ComputedRef<DispatchOrderSafetyChecklistItem[]>;
  routineChecklistItems: ComputedRef<DispatchOrderSafetyChecklistItem[]>;
  completionReady: ComputedRef<boolean>;
  completionButtonText: ComputedRef<string>;
  routinePendingCount: ComputedRef<number>;
  canSubmitChecklist: ComputedRef<boolean>;
  canCompleteOrder: ComputedRef<boolean>;
  openOrderDetail: (orderId: string, source?: DispatchBoardDetailSource) => Promise<void>;
  openFlightSummaryDetail: (
    summary: DispatchBoardFlightSummary,
    source?: DispatchBoardDetailSource,
  ) => Promise<void>;
  openFlightOrderDetail: (orderId: string) => Promise<void>;
  closeDetailDrawer: () => void;
  refreshChecklist: () => Promise<void>;
  submitChecklistItem: (itemCode: string, result: Exclude<DispatchChecklistResult, 'pending'>) => Promise<void>;
  submitRoutineChecklistBatch: () => Promise<void>;
  completeCurrentOrder: () => Promise<void>;
}

function formatLocalDateTimeInput(date: Date): string {
  const year = String(date.getFullYear());
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  const hour = String(date.getHours()).padStart(2, '0');
  const minute = String(date.getMinutes()).padStart(2, '0');
  return `${year}-${month}-${day} ${hour}:${minute}`;
}

function parseLocalDateTimeInput(raw: string): string | null {
  const normalized = String(raw ?? '').trim().replace('T', ' ');
  const match = normalized.match(/^(\d{4})-(\d{2})-(\d{2})\s+(\d{2}):(\d{2})$/);
  if (!match) {
    return null;
  }

  const [, yearText, monthText, dayText, hourText, minuteText] = match;
  const date = new Date(
    Number(yearText),
    Number(monthText) - 1,
    Number(dayText),
    Number(hourText),
    Number(minuteText),
    0,
    0,
  );
  if (Number.isNaN(date.getTime())) {
    return null;
  }

  return date.toISOString();
}

function buildDetailFocusContext(order: DispatchOrder): DispatchBoardDetailFocusContext {
  const orderId = resolveDispatchOrderId(order);
  const focus = buildResourceFocus({
    resource_type: order.team_id ? 'team' : 'employee',
    resource_id: order.team_id || order.focus_user_id || order.individual_user_id,
    resource_label: order.team_name || order.focus_user_name || order.individual_username,
    related_order_ids: orderId ? [orderId] : [],
  });

  return {
    focus,
    text: [order.flight_id, order.task_type_name || order.task_type]
      .map((value) => String(value ?? '').trim())
      .filter(Boolean)
      .join(' / ') || orderId,
  };
}

function getSafetyGateState(checklist: DispatchOrderSafetyChecklist | null): DispatchBoardSafetyGateState {
  if (!checklist || !checklist.enforced) {
    return 'unknown';
  }
  if (checklist.failed_required_items.length > 0) {
    return 'blocked';
  }
  if (checklist.pending_required_items.length > 0) {
    return 'pending';
  }
  return checklist.ready ? 'ready' : 'unknown';
}

export function translateChecklistResult(result: DispatchChecklistResult | null | undefined): string {
  switch (result) {
    case 'pass':
      return '通过';
    case 'fail':
      return '不通过';
    case 'na':
      return '不适用';
    default:
      return '待检查';
  }
}

export function getChecklistBadgeClass(result: DispatchChecklistResult | null | undefined): string {
  switch (result) {
    case 'pass':
      return 'is-pass';
    case 'fail':
      return 'is-danger';
    case 'na':
      return 'is-muted';
    default:
      return 'is-warning';
  }
}

export function formatDetailDateTime(value: unknown): string {
  if (!value) {
    return '-';
  }

  const date = value instanceof Date ? value : new Date(String(value));
  if (Number.isNaN(date.getTime())) {
    return '-';
  }

  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  const hour = String(date.getHours()).padStart(2, '0');
  const minute = String(date.getMinutes()).padStart(2, '0');
  return `${month}-${day} ${hour}:${minute}`;
}

export function useDispatchBoardDetail(options: UseDispatchBoardDetailOptions): UseDispatchBoardDetailReturn {
  const toast = useToast();
  const isVisible = ref(false);
  const mode = ref<DispatchBoardDetailMode>('idle');
  const source = ref<DispatchBoardDetailSource>('unknown');
  const opening = ref(false);
  const completing = ref(false);
  const order = ref<DispatchOrder | null>(null);
  const flightSummary = ref<DispatchBoardFlightSummary | null>(null);
  const flightOrders = ref<DispatchOrder[]>([]);
  const checklist = ref<DispatchOrderSafetyChecklist | null>(null);
  const checklistLoading = ref(false);
  const checklistError = ref('');
  const gateHint = ref<DispatchOrderCompletionGateHint | null>(null);
  const submittingKey = ref('');

  const currentOrderId = computed(() => resolveDispatchOrderId(order.value));
  const title = computed(() => {
    if (mode.value === 'flight' && flightSummary.value) {
      return `${flightSummary.value.flight_no || flightSummary.value.flight_id || '航班'} 全流程`;
    }
    if (mode.value === 'order' && order.value) {
      return `派工单 ${currentOrderId.value || '-'}`;
    }
    return '详情';
  });

  const safetyGateState = computed(() => getSafetyGateState(checklist.value));
  const criticalChecklistItems = computed(() => {
    return (checklist.value?.items || []).filter((item) => normalizeChecklistLevel(item.level) === 'critical');
  });
  const routineChecklistItems = computed(() => {
    return (checklist.value?.items || []).filter((item) => normalizeChecklistLevel(item.level) === 'routine');
  });
  const completionReady = computed(() => {
    if (!checklist.value || !checklist.value.enforced) {
      return true;
    }
    return checklist.value.ready || checklist.value.can_soft_complete;
  });
  const completionButtonText = computed(() => {
    if (
      checklist.value
      && checklist.value.enforced
      && !checklist.value.ready
      && checklist.value.can_soft_complete
    ) {
      return '软闭环完工';
    }
    return '提交完工';
  });
  const routinePendingCount = computed(() => {
    return (checklist.value?.items || []).filter((item) => {
      return normalizeChecklistLevel(item.level) === 'routine' && item.status === 'pending';
    }).length;
  });
  const canSubmitChecklist = computed(() => {
    return Boolean(order.value && ['assigned', 'in_progress'].includes(String(order.value.status || '')));
  });
  const canCompleteOrder = computed(() => {
    return Boolean(order.value && String(order.value.status || '') === 'in_progress');
  });

  async function loadChecklist(orderId: string, silent = false): Promise<void> {
    if (!orderId) {
      checklist.value = null;
      checklistError.value = '';
      return;
    }

    checklistLoading.value = true;
    if (!silent) {
      checklistError.value = '';
    }

    try {
      checklist.value = await fetchOrderSafetyChecklist(orderId);
      checklistError.value = '';
      if (checklist.value?.ready) {
        gateHint.value = null;
      }
    } catch (error) {
      checklist.value = null;
      checklistError.value = error instanceof Error ? error.message : '加载安全清单失败';
    } finally {
      checklistLoading.value = false;
    }
  }

  async function openOrderDetail(orderId: string, nextSource: DispatchBoardDetailSource = 'unknown'): Promise<void> {
    if (!orderId) {
      return;
    }

    opening.value = true;
    source.value = nextSource;
    checklist.value = null;
    checklistError.value = '';
    gateHint.value = null;
    submittingKey.value = '';

    try {
      const nextOrder = await fetchOrder(orderId);
      if (!nextOrder) {
        throw new Error('加载派工详情失败');
      }

      mode.value = 'order';
      order.value = nextOrder;
      flightSummary.value = null;
      flightOrders.value = [];
      isVisible.value = true;
      options.onFocusChange?.(buildDetailFocusContext(nextOrder));
      await loadChecklist(resolveDispatchOrderId(nextOrder), true);
    } catch (error) {
      toast.show('error', error instanceof Error ? error.message : '加载派工详情失败');
    } finally {
      opening.value = false;
    }
  }

  async function openFlightSummaryDetail(
    summary: DispatchBoardFlightSummary,
    nextSource: DispatchBoardDetailSource = 'gantt',
  ): Promise<void> {
    const flightId = String(summary.flight_id ?? '').trim();
    if (!flightId) {
      return;
    }

    opening.value = true;
    source.value = nextSource;
    mode.value = 'flight';
    flightSummary.value = summary;
    order.value = null;
    checklist.value = null;
    checklistError.value = '';
    gateHint.value = null;
    flightOrders.value = [];
    isVisible.value = true;
    options.onFocusChange?.({
      focus: null,
      text: summary.flight_no || summary.flight_id || '航班全流程',
    });

    try {
      flightOrders.value = await fetchOrdersByFlight(flightId);
    } catch (error) {
      toast.show('error', error instanceof Error ? error.message : '加载航班派工明细失败');
    } finally {
      opening.value = false;
    }
  }

  async function openFlightOrderDetail(orderId: string): Promise<void> {
    await openOrderDetail(orderId, 'flight_summary');
  }

  function closeDetailDrawer(): void {
    isVisible.value = false;
  }

  async function refreshChecklist(): Promise<void> {
    if (!currentOrderId.value) {
      return;
    }
    await loadChecklist(currentOrderId.value);
  }

  async function submitChecklistItem(
    itemCode: string,
    result: Exclude<DispatchChecklistResult, 'pending'>,
  ): Promise<void> {
    if (!currentOrderId.value || !itemCode || submittingKey.value) {
      return;
    }

    let note: string | null = null;
    if (result === 'fail') {
      const promptResult = window.prompt('请输入不通过说明（可选）', '');
      if (promptResult === null) {
        return;
      }
      note = promptResult.trim() || null;
    } else if (result === 'na') {
      const promptResult = window.prompt('请输入不适用说明（可选）', '');
      if (promptResult === null) {
        return;
      }
      note = promptResult.trim() || null;
    }

    submittingKey.value = `${itemCode}:${result}`;
    try {
      await submitDispatchOrderSafetyChecklistItem(currentOrderId.value, itemCode, {
        result,
        note,
      });
      gateHint.value = null;
      toast.show('success', `清单项已更新：${translateChecklistResult(result)}`);
      await Promise.all([
        loadChecklist(currentOrderId.value, true),
        options.refreshSafetyProgress(),
      ]);
    } catch (error) {
      toast.show('error', error instanceof Error ? error.message : '提交安全清单失败');
    } finally {
      submittingKey.value = '';
    }
  }

  async function submitRoutineChecklistBatch(): Promise<void> {
    if (!currentOrderId.value || !checklist.value || submittingKey.value) {
      return;
    }

    const items = routineChecklistItems.value
      .filter((item) => item.status === 'pending')
      .map((item) => ({
        item_code: item.item_code,
        result: 'pass' as const,
        note: null,
        handled_on_site: false,
      }));

    if (items.length === 0) {
      toast.show('warning', '常规项已全部确认，无需重复提交');
      return;
    }

    submittingKey.value = 'batch:routine';
    try {
      await submitDispatchOrderSafetyChecklistBatch(currentOrderId.value, items);
      gateHint.value = null;
      toast.show('success', `已批量确认 ${items.length} 项常规检查`);
      await Promise.all([
        loadChecklist(currentOrderId.value, true),
        options.refreshSafetyProgress(),
      ]);
    } catch (error) {
      toast.show('error', error instanceof Error ? error.message : '批量提交常规检查失败');
    } finally {
      submittingKey.value = '';
    }
  }

  async function completeCurrentOrder(): Promise<void> {
    if (!currentOrderId.value || completing.value) {
      return;
    }

    const actualEndInput = window.prompt(
      '请输入真实结束时间（yyyy-MM-dd HH:mm）',
      formatLocalDateTimeInput(new Date()),
    );
    if (actualEndInput === null) {
      return;
    }

    const actualEndTime = parseLocalDateTimeInput(actualEndInput);
    if (!actualEndTime) {
      toast.show('warning', '时间格式错误，请使用 yyyy-MM-dd HH:mm');
      return;
    }

    const noteInput = window.prompt('请输入完工备注（可选）', '');
    if (noteInput === null) {
      return;
    }

    completing.value = true;
    try {
      await completeDispatchOrder(currentOrderId.value, {
        actual_end_time: actualEndTime,
        completion_notes: noteInput.trim() || null,
      });
      gateHint.value = null;
      toast.show('success', '派工单已完成');
      await options.refreshTimeline();
      await openOrderDetail(currentOrderId.value, source.value);
    } catch (error) {
      const normalizedError = error as { detail?: unknown; message?: string };
      const nextGateHint = normalizeDispatchOrderCompletionGateHint(normalizedError?.detail);
      if (nextGateHint) {
        gateHint.value = nextGateHint;
      }
      toast.show('warning', normalizedError?.message || nextGateHint?.message || '完工失败');
    } finally {
      completing.value = false;
    }
  }

  return {
    isVisible,
    mode,
    source,
    title,
    order,
    flightSummary,
    flightOrders,
    checklist,
    checklistLoading,
    checklistError,
    gateHint,
    submittingKey,
    opening,
    completing,
    currentOrderId,
    safetyGateState,
    criticalChecklistItems,
    routineChecklistItems,
    completionReady,
    completionButtonText,
    routinePendingCount,
    canSubmitChecklist,
    canCompleteOrder,
    openOrderDetail,
    openFlightSummaryDetail,
    openFlightOrderDetail,
    closeDetailDrawer,
    refreshChecklist,
    submitChecklistItem,
    submitRoutineChecklistBatch,
    completeCurrentOrder,
  };
}
