import { unwrapApiData } from '@/shared/apiEnvelope';
import { useApi } from '@/composables/useApi';
import { useToast } from '@/composables/useToast';
import { batchCompleteOrders, type BatchProcessState, type DispatchOrder } from '@/composables/useDispatchBoardData';
import type { Ref } from 'vue';

export interface UseDispatchBoardPageBatchOptions {
  selectedOrderIds: Ref<string[]>;
  visibleTimelineItems: { readonly value: readonly DispatchOrder[] };
  impactedOrderIds: Ref<string[]>;
  batchProcess: Ref<BatchProcessState>;
  refreshTimeline: () => Promise<void>;
}

export interface UseDispatchBoardPageBatchReturn {
  handleBatchComplete: () => Promise<void>;
  handleBatchPublish: () => Promise<void>;
  handleBatchClear: () => void;
}

export function useDispatchBoardPageBatch(options: UseDispatchBoardPageBatchOptions): UseDispatchBoardPageBatchReturn {
  const { selectedOrderIds, visibleTimelineItems, impactedOrderIds: _impactedOrderIds, batchProcess, refreshTimeline } = options;
  const api = useApi();
  const toast = useToast();

  async function handleBatchComplete() {
    if (selectedOrderIds.value.length === 0) {
      toast.show('warning', '请先选择要处理的工单');
      return;
    }
    const eligible = selectedOrderIds.value.filter((id) => {
      const item = visibleTimelineItems.value.find((it: DispatchOrder) => String(it.order_id ?? it.id ?? '') === id);
      return item && String(item.status ?? '').trim() === 'in_progress';
    });
    if (eligible.length === 0) {
      toast.show('warning', '选中的工单中没有进行中的工单可完工');
      return;
    }

    batchProcess.value = {
      ...batchProcess.value,
      isRunning: true,
      currentIndex: 0,
      totalItems: eligible.length,
      successCount: 0,
      failCount: 0,
      currentOrderId: null,
      errors: [],
      orderIds: [...eligible],
    };
    try {
      const result = await batchCompleteOrders(eligible, (current: number, _total: number, orderId: string) => {
        batchProcess.value.currentIndex = current;
        batchProcess.value.currentOrderId = orderId;
      });
      batchProcess.value.successCount = result.success;
      batchProcess.value.failCount = result.failed;
      batchProcess.value.errors = result.errors;
      if (result.success > 0) {
        toast.show('success', `批量完成：成功 ${result.success} 项`);
        await refreshTimeline();
      }
      if (result.failed > 0) toast.show('error', `批量完成：失败 ${result.failed} 项`);
    } catch (error) {
      batchProcess.value.errors = [...batchProcess.value.errors, { orderId: batchProcess.value.currentOrderId || 'batch', error: String(error) }];
      toast.show('error', `批量完成失败: ${String(error)}`);
    } finally {
      batchProcess.value.isRunning = false;
    }
  }

  async function handleBatchPublish() {
    if (selectedOrderIds.value.length === 0) {
      toast.show('warning', '请先选择要发布的工单');
      return;
    }
    try {
      const res = await api.post('/api/v2/dispatch-orders/batch-publish-drafts', { order_ids: selectedOrderIds.value });
      const payload = unwrapApiData<{ published?: number; failed?: number }>(res.data);
      if (res.ok && payload) {
        if (payload.published) toast.show('success', `已发布 ${payload.published} 条工单`);
        if (payload.failed) toast.show('error', `发布失败 ${payload.failed} 条`);
        selectedOrderIds.value = [];
        await refreshTimeline();
      }
    } catch (e) {
      console.warn('Failed to batch publish:', e);
    }
  }

  function handleBatchClear() {
    selectedOrderIds.value = [];
  }

  return {
    handleBatchComplete,
    handleBatchPublish,
    handleBatchClear,
  };
}
