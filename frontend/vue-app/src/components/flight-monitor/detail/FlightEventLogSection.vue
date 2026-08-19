<script setup lang="ts">
import { inject } from 'vue';
import UiSegment from '../../ui/UiSegment.vue';
import { flightBusinessCaseKey } from '../../../composables/useFlightBusinessCases';
import {
  formatCaseTimeRange,
  getCaseDisplayName,
  getCaseReceiptStatusLabel,
  getCaseReceiptSummaryText,
  getCaseStatusClass,
  getCaseStatusLabel,
  getCaseVisibilityLabel,
  getCaseWorkflowSummary,
  isCommonCase,
} from './businessCaseHelpers';

const ctx = inject(flightBusinessCaseKey)!;

function workflowSummaryLabel(c: Parameters<typeof getCaseWorkflowSummary>[0]) {
  return getCaseWorkflowSummary(c, ctx.getCachedCaseWorkflowForms(c?.case_id), ctx.hasLoadedCaseWorkflowForms(c?.case_id));
}
</script>

<template>
  <div class="log-toolbar">
    <span class="log-toolbar-title">事件日志</span>
    <UiSegment label="事件日志过滤">
      <button
        type="button"
        :aria-checked="ctx.caseFilter.value === 'all'"
        @click="ctx.caseFilter.value = 'all'"
      >
        全部
      </button>
      <button
        v-for="option in ctx.caseStatusOptions.value"
        :key="option.value"
        type="button"
        :aria-checked="ctx.caseFilter.value === option.value"
        @click="ctx.caseFilter.value = option.value"
      >
        {{ option.label }}
      </button>
    </UiSegment>
  </div>
  <div class="cases-scroll-area">
    <div v-if="ctx.filteredCases.value.length > 0" class="timeline event-case-timeline">
      <div
        v-for="c in ctx.filteredCases.value"
        :key="c.case_id"
        class="timeline-item"
        :class="getCaseStatusClass(c.status, ctx.caseStatusOptions.value)"
        @click="ctx.openCaseDetail(c.case_id)"
      >
        <div class="timeline-row">
          <div class="timeline-main">
            <span class="timeline-type">{{ getCaseDisplayName(c) }}</span>
            <div class="timeline-time">
              {{ formatCaseTimeRange(c.created_at, c.finished_at) }}
            </div>
          </div>
          <div class="timeline-tags">
            <span class="timeline-visibility-pill" :class="{ common: isCommonCase(c), department: !isCommonCase(c) }">
              {{ getCaseVisibilityLabel(c) }}
            </span>
            <span class="timeline-status" :class="getCaseStatusClass(c.status, ctx.caseStatusOptions.value)">
              {{ getCaseStatusLabel(c.status, ctx.caseStatusOptions.value) }}
            </span>
          </div>
        </div>
        <div class="timeline-details">
          <div class="timeline-workflow-summary">
            <span
              class="timeline-workflow-pill"
              :class="{
                pending: workflowSummaryLabel(c).label === '待填写',
                submitted: workflowSummaryLabel(c).label === '已提交',
                passive: ['未配置', '无待处理', '最近提交'].includes(workflowSummaryLabel(c).label),
              }"
            >
              {{ workflowSummaryLabel(c).label }}
            </span>
            <span v-if="workflowSummaryLabel(c).detail" class="timeline-workflow-text">
              {{ workflowSummaryLabel(c).detail }}
            </span>
          </div>
          <div v-if="c.description">
            <strong>描述:</strong> {{ c.description }}
          </div>
          <div v-if="c.created_by">
            <strong>创建者:</strong> {{ c.created_by }}
          </div>
          <div v-if="c.append_count && c.append_count > 0" class="append-badge">
            追加 {{ c.append_count }} 次
          </div>
          <div v-if="getCaseReceiptStatusLabel(c)" class="timeline-receipt-summary">
            <span class="timeline-receipt-pill">{{ getCaseReceiptStatusLabel(c) }}</span>
            <span class="timeline-receipt-text">{{ getCaseReceiptSummaryText(c) }}</span>
          </div>
        </div>
      </div>
    </div>
    <div v-else class="gantt-empty">
      无匹配的业务事项
    </div>
  </div>
</template>

<style scoped>
.log-toolbar {
  padding: 12px 16px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
  border-bottom: 1px solid var(--line);
}

.log-toolbar-title {
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.cases-scroll-area {
  padding: 8px 16px 16px;
  overflow-y: auto;
}

/* 事件日志专用：左侧主信息 + 右侧胶囊一排对齐 */
.event-case-timeline {
  padding-left: 22px;
}

.event-case-timeline :deep(.timeline-item) {
  margin-bottom: 0;
  padding: 12px 0 12px 2px;
}

.event-case-timeline :deep(.timeline-item::before) {
  left: -18px;
  top: 16px;
  width: 10px;
  height: 10px;
  border-width: 2px;
  border-color: var(--face-work);
  box-shadow: 0 0 0 1px var(--line);
}

.event-case-timeline :deep(.timeline-row) {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}

.event-case-timeline :deep(.timeline-main) {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
  flex: 1;
}

.event-case-timeline :deep(.timeline-type) {
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
  line-height: 1.35;
  color: var(--ink);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.event-case-timeline :deep(.timeline-time) {
  margin: 0;
  font-size: 11px;
  line-height: 1.3;
  color: var(--ink-muted);
  font-variant-numeric: tabular-nums;
  letter-spacing: 0.01em;
}

.event-case-timeline :deep(.timeline-tags) {
  padding-top: 1px;
  align-self: center;
}

.event-case-timeline :deep(.timeline-item:hover) {
  transform: none;
  background: color-mix(in srgb, var(--ink) 5%, transparent);
  border-radius: var(--r-panel);
}
</style>
