<script setup lang="ts">
import type { DispatchOrder, BatchProcessState } from '@/composables/useDispatchBoardData';
import { STATUS_LABELS } from '@/composables/useDispatchBoardData';
import type { DispatchOrderSafetyChecklist, DispatchOrderCompletionGateHint, DispatchOrderSafetyChecklistItem } from '@/composables/useDispatchBoardData';

defineProps<{
  isDetailDrawerVisible: boolean;
  detailTitle: string;
  detailMode: string;
  detailOpening: boolean;
  detailOrder: DispatchOrder | null;
  detailFlightSummary: Record<string, unknown> | null;
  detailFlightOrders: DispatchOrder[];
  detailChecklist: DispatchOrderSafetyChecklist | null;
  detailChecklistLoading: boolean;
  detailChecklistError: string;
  detailGateHint: DispatchOrderCompletionGateHint | null;
  detailSubmittingKey: string | null;
  detailCompleting: boolean;
  detailCurrentOrderId: string | null;
  detailSafetyGateState: string;
  criticalChecklistItems: DispatchOrderSafetyChecklistItem[];
  routineChecklistItems: DispatchOrderSafetyChecklistItem[];
  detailCompletionReady: boolean;
  detailCompletionButtonText: string;
  detailRoutinePendingCount: number;
  detailCanSubmitChecklist: boolean;
  detailCanCompleteOrder: boolean;
  detailCrewMembers: string[];
  detailQualificationGaps: string[];
  detailEquipmentCodes: string[];
  detailTaskInfoRows: Array<{ label: string; value: string; className?: string }>;
  detailTimeInfoRows: Array<{ label: string; value: string }>;
  detailResourceInfoRows: Array<{ label: string; value: string }>;
  detailFlightStatusSummary: Array<{ label: string; value: number }>;
  batchProcess: BatchProcessState;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'openFlightOrderDetail', orderId: string): void;
  (e: 'refreshChecklist'): void;
  (e: 'submitChecklistItem', itemCode: string, result: string): void;
  (e: 'submitRoutineChecklistBatch'): void;
  (e: 'completeCurrentOrder'): void;
  (e: 'moveBatchIndex', step: number): void;
}>();
</script>

<template>
  <aside id="detailDrawer" class="drawer" :class="{ open: isDetailDrawerVisible }" :aria-hidden="!isDetailDrawerVisible">
    <div class="drawer-header">
      <h3 id="detailTitle" class="drawer-title">{{ detailTitle }}</h3>
      <button id="detailCloseBtn" class="panel-close-btn" aria-label="关闭详情" @click="emit('close')">×</button>
    </div>
    <div id="detailContent" class="drawer-body">
      <div v-if="detailOpening" class="empty-state">详情加载中...</div>

      <template v-else-if="detailMode === 'flight' && detailFlightSummary">
        <div class="section-title">航班总览</div>
        <div class="kv-row"><span class="kv-key">航班号</span><span class="kv-value">{{ detailFlightSummary.flight_no || detailFlightSummary.flight_id }}</span></div>
        <div class="kv-row"><span class="kv-key">航班 ID</span><span class="kv-value">{{ detailFlightSummary.flight_id }}</span></div>
        <div class="kv-row"><span class="kv-key">时间范围</span><span class="kv-value">{{ detailFlightSummary.start_time }} - {{ detailFlightSummary.end_time }}</span></div>
        <div class="kv-row"><span class="kv-key">当前主状态</span><span class="kv-value">{{ STATUS_LABELS[(detailFlightSummary as Record<string, unknown>).status as keyof typeof STATUS_LABELS] || detailFlightSummary.status }}</span></div>
        <div class="kv-row"><span class="kv-key">覆盖派工</span><span class="kv-value">{{ detailFlightOrders.length }}</span></div>
        <div class="section-title">状态分布</div>
        <div v-if="detailFlightStatusSummary.length > 0" class="safety-summary">
          <span v-for="item in detailFlightStatusSummary" :key="item.label" class="safety-pill is-muted">{{ item.label }} {{ item.value }}</span>
        </div>
        <div v-else class="empty-list-tip">暂无状态分布</div>
        <div class="section-title">派工明细</div>
        <div v-if="detailFlightOrders.length > 0" class="detail-order-list">
          <button v-for="flightOrder in detailFlightOrders" :key="String(flightOrder.order_id || flightOrder.id || '')" type="button" class="detail-order-item" @click="emit('openFlightOrderDetail', String(flightOrder.order_id || flightOrder.id || ''))">
            <div style="font-weight:600;">{{ flightOrder.task_type_name || flightOrder.task_type }}</div>
            <div class="detail-meta-row" style="margin:6px 0 0;"><span class="status-badge" :class="String(flightOrder.status || 'pending')">{{ STATUS_LABELS[flightOrder.status || 'pending'] }}</span></div>
            <div style="margin-top:4px;color:var(--dispatch-detail-subtext, #5f7082);">{{ flightOrder.planned_start_time || flightOrder.start_time }}</div>
          </button>
        </div>
        <div v-else class="empty-state">暂无航班派工明细</div>
      </template>

      <template v-else-if="detailMode === 'order' && detailOrder">
        <div class="section-title">任务信息</div>
        <div v-for="row in detailTaskInfoRows" :key="row.label" class="kv-row"><span class="kv-key">{{ row.label }}</span><span class="kv-value" :class="row.className || ''">{{ row.value }}</span></div>
        <div class="section-title">时间信息</div>
        <div v-for="row in detailTimeInfoRows" :key="row.label" class="kv-row"><span class="kv-key">{{ row.label }}</span><span class="kv-value">{{ row.value }}</span></div>
        <div class="section-title">资源信息</div>
        <div v-for="row in detailResourceInfoRows" :key="row.label" class="kv-row"><span class="kv-key">{{ row.label }}</span><span class="kv-value">{{ row.value }}</span></div>
        <div class="section-title">安全清单</div>
        <div class="safety-box">
          <div v-if="detailChecklistError" class="safety-alert is-error">{{ detailChecklistError }}</div>
          <template v-else-if="detailChecklist">
            <div class="safety-summary">
              <span class="safety-pill" :class="detailChecklist.enforced ? 'is-active' : 'is-muted'">{{ detailChecklist.enforced ? '门禁启用' : '门禁未启用' }}</span>
              <span v-if="detailChecklist.enforced" class="safety-pill">必填 {{ detailChecklist.completed_required }}/{{ detailChecklist.required_total }}</span>
              <span v-if="detailChecklist.template_version" class="safety-pill is-muted">版本 {{ detailChecklist.template_version }}</span>
              <span v-if="detailChecklist.routine_total > 0" class="safety-pill is-muted">常规项 {{ detailChecklist.completed_routine }}/{{ detailChecklist.routine_total }}</span>
              <span class="safety-pill">{{ detailSafetyGateState }}</span>
            </div>
            <p v-if="detailChecklist.pending_required_items.length > 0" class="safety-summary-text">待完成：{{ detailChecklist.pending_required_items.join(' / ') }}</p>
            <p v-if="detailChecklist.failed_required_items.length > 0" class="safety-summary-text is-danger">不通过：{{ detailChecklist.failed_required_items.join(' / ') }}</p>
            <p v-if="detailChecklist.soft_missing_count > 0 && detailChecklist.can_soft_complete" class="safety-summary-text">常规项仍有 {{ detailChecklist.soft_missing_count }} 项待补齐，可软闭环完工。</p>
            <div v-if="detailGateHint" class="safety-alert is-warning">
              <div>{{ detailGateHint.message }}</div>
              <div class="safety-alert-sub">{{ [...(detailGateHint.pending_required_items || []).map((item) => `待完成:${item}`), ...(detailGateHint.failed_required_items || []).map((item) => `不通过:${item}`)].join(' / ') || '请先补齐关键项' }}</div>
            </div>
            <div v-if="detailChecklistLoading" class="safety-empty">安全清单加载中...</div>
            <template v-if="criticalChecklistItems.length > 0">
              <div class="section-title" style="margin-top: 18px;">关键安全项</div>
              <div class="safety-item-list">
                <div v-for="item in criticalChecklistItems" :key="item.item_code" class="safety-item-card">
                  <div class="safety-item-head">
                    <div class="safety-item-title-wrap"><p class="safety-item-title">{{ item.title || item.item_code }}</p><p class="safety-item-code">{{ item.item_code }}</p></div>
                    <div class="safety-item-tags">
                      <span class="safety-pill is-active">关键</span>
                      <span class="safety-pill" :class="item.required ? 'is-warning' : 'is-muted'">{{ item.required ? '必填' : '可选' }}</span>
                    </div>
                  </div>
                  <div v-if="detailCanSubmitChecklist" class="safety-item-actions">
                    <button type="button" class="safety-item-action" :disabled="Boolean(detailSubmittingKey)" @click="emit('submitChecklistItem', item.item_code, 'pass')">通过</button>
                    <button type="button" class="safety-item-action is-danger" :disabled="Boolean(detailSubmittingKey)" @click="emit('submitChecklistItem', item.item_code, 'fail')">不通过</button>
                    <button v-if="item.allow_na" type="button" class="safety-item-action" :disabled="Boolean(detailSubmittingKey)" @click="emit('submitChecklistItem', item.item_code, 'na')">不适用</button>
                  </div>
                </div>
              </div>
            </template>
            <template v-if="routineChecklistItems.length > 0">
              <div class="section-title" style="margin-top: 18px;">常规安全项</div>
              <div v-if="detailCanSubmitChecklist" class="safety-alert is-warning">
                <div>常规项确认后可一次性批量提交。</div>
                <div class="safety-alert-sub">待批量确认 {{ detailRoutinePendingCount }} 项；仅关键项会阻断完工。</div>
                <div style="margin-top:10px;">
                  <button type="button" class="suggestion-chip" :disabled="Boolean(detailSubmittingKey) || detailRoutinePendingCount <= 0" @click="emit('submitRoutineChecklistBatch')">{{ detailRoutinePendingCount > 0 ? `常规项已检查（${detailRoutinePendingCount}）` : '常规项已全部确认' }}</button>
                </div>
              </div>
            </template>
            <div v-if="criticalChecklistItems.length === 0 && routineChecklistItems.length === 0" class="safety-empty">当前模板未配置检查项</div>
          </template>
          <div v-else-if="detailChecklistLoading" class="safety-empty">安全清单加载中...</div>
          <div v-else class="safety-empty">当前暂无安全清单数据</div>
        </div>
      </template>

      <div v-else class="empty-state">暂无详情数据</div>
    </div>
    <div id="detailActions" class="drawer-footer">
      <button class="action-btn" type="button" @click="emit('close')">关闭</button>
      <button v-if="detailMode === 'order' && detailCurrentOrderId" class="action-btn" type="button" @click="emit('refreshChecklist')">刷新清单</button>
      <button v-if="detailMode === 'order' && detailCanCompleteOrder" class="action-btn primary" type="button" :disabled="!detailCompletionReady || detailCompleting" @click="emit('completeCurrentOrder')">{{ detailCompleting ? '提交中...' : detailCompletionButtonText }}</button>
      <template v-if="batchProcess.isRunning || batchProcess.isGuided">
        <button class="action-btn" type="button" :disabled="batchProcess.currentIndex <= 1" @click="emit('moveBatchIndex', -1)">上一条</button>
        <button class="action-btn" type="button" :disabled="batchProcess.currentIndex >= batchProcess.totalItems" @click="emit('moveBatchIndex', 1)">下一条</button>
      </template>
    </div>
  </aside>
</template>
