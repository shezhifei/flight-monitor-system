<script setup lang="ts">
import { STATUS_ORDER, STATUS_LABELS } from '@/composables/useDispatchBoardData';
import type { DispatchOrderStatus, BatchProcessState } from '@/composables/useDispatchBoardData';

defineProps<{
  isStatusPanelVisible: boolean;
  selectedStatus: DispatchOrderStatus;
  statusCounts: Record<DispatchOrderStatus, number>;
  statusOrderList: Array<{ id: string; title: string; status: string; start_time: string }>;
  statusTotalCount: number;
  selectedOrderIds: string[];
  batchProcess: BatchProcessState;
  searchQuery: string;
}>();

const emit = defineEmits<{
  (e: 'closeStatusPanel'): void;
  (e: 'selectStatus', status: DispatchOrderStatus): void;
  (e: 'filterBlocked'): void;
  (e: 'showAll'): void;
  (e: 'selectAll'): void;
  (e: 'startBatchComplete'): void;
  (e: 'moveBatchIndex', step: number): void;
  (e: 'toggleOrderSelection', orderId: string): void;
  (e: 'openOrderDetail', orderId: string): void;
}>();
</script>

<template>
  <aside id="statusPanel" class="status-panel" :class="{ open: isStatusPanelVisible }" :aria-hidden="!isStatusPanelVisible">
    <div class="status-panel-header">
      <span>状态定位器</span>
      <button id="statusPanelClose" class="panel-close-btn" aria-label="关闭状态定位器" @click="emit('closeStatusPanel')">×</button>
    </div>
    <div id="statusCounts" class="status-count-grid">
      <div v-for="status in STATUS_ORDER" :key="status" class="status-count-item" :class="{ active: selectedStatus === status }" @click="emit('selectStatus', status)">
        <span class="status-label">{{ STATUS_LABELS[status] }}</span>
        <span class="status-value">{{ statusCounts[status] || 0 }}</span>
      </div>
    </div>
    <div id="statusToolbar" class="status-toolbar">
      <button id="statusFilterBlockedBtn" class="action-btn status-mini-btn" @click="emit('filterBlocked')">一键筛阻断</button>
      <button id="statusShowAllBtn" class="action-btn status-mini-btn" @click="emit('showAll')">显示全部</button>
      <button id="statusSelectAllBtn" class="action-btn status-mini-btn" @click="emit('selectAll')">全选本列</button>
      <button id="statusBatchOpenBtn" class="action-btn primary status-mini-btn" :disabled="selectedOrderIds.length === 0 || batchProcess.isRunning" @click="emit('startBatchComplete')">{{ batchProcess.isRunning ? `处理中 ${batchProcess.currentIndex}/${batchProcess.totalItems}` : `批量完成 (${selectedOrderIds.length})` }}</button>
      <div v-if="batchProcess.isRunning || batchProcess.isGuided" class="status-batch-nav">
        <button :disabled="batchProcess.currentIndex <= 1" @click="emit('moveBatchIndex', -1)">上一条</button>
        <span>{{ batchProcess.currentIndex }} / {{ batchProcess.totalItems }}</span>
        <button :disabled="batchProcess.currentIndex >= batchProcess.totalItems" @click="emit('moveBatchIndex', 1)">下一条</button>
      </div>
      <div id="statusSelectionSummary" class="status-selection-summary">已勾选 {{ selectedOrderIds.length }} 条 | 当前列 {{ statusTotalCount }} 条</div>
    </div>
    <div id="statusOrderList" class="status-order-list">
      <div v-for="order in statusOrderList" :key="order.id" class="status-order-item" @click="emit('openOrderDetail', order.id)">
        <input type="checkbox" class="status-order-checkbox" :checked="selectedOrderIds.includes(order.id)" :aria-label="`选择工单 ${order.title}`" @click.stop @change.stop="emit('toggleOrderSelection', order.id)">
        <strong>{{ order.title }}</strong>
        <span class="status-badge" :class="order.status">{{ STATUS_LABELS[order.status as keyof typeof STATUS_LABELS] || order.status }}</span>
      </div>
      <div v-if="statusOrderList.length === 0" class="empty-list-tip">当前时间窗内无待处理工单</div>
    </div>
    <div id="statusListTip" class="status-list-tip">-</div>
  </aside>
</template>
