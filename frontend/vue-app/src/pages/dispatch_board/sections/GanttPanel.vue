<script setup lang="ts">
import GanttChart from '@/components/dispatch-board/GanttChart.vue';
import BatchActionsToolbar from '@/components/dispatch-board/BatchActionsToolbar.vue';
import type { TimelineData, SafetyProgressMap, ResourceFocus, SafetyGateFilter } from '@/composables/useDispatchBoardData';

defineProps<{
  displayedTimelineData: TimelineData | null;
  windowStartMs: number;
  windowEndMs: number;
  safetyProgress: SafetyProgressMap;
  resourceFocus: ResourceFocus | null;
  safetyGateFilter: SafetyGateFilter;
  detailCurrentOrderId: string | null;
  selectedOrderIds: string[];
  impactedOrderIds: string[];
  isBatchToolbarVisible: boolean;
  isGanttLegendPopoverVisible: boolean;
  guideSettings: { cornerFade: boolean };
}>();

const emit = defineEmits<{
  (e: 'toggleBatchToolbar'): void;
  (e: 'handleBatchComplete'): void;
  (e: 'handleBatchPublish'): void;
  (e: 'handleBatchClear'): void;
  (e: 'toggleGanttLegend'): void;
  (e: 'itemDblClick', params: Record<string, unknown>): void;
  (e: 'itemClick', params: Record<string, unknown>): void;
}>();
</script>

<template>
  <div class="gantt-stage-stub">
    <BatchActionsToolbar
      v-if="isBatchToolbarVisible"
      :selected-count="selectedOrderIds.length"
      @complete="emit('handleBatchComplete')"
      @publish="emit('handleBatchPublish')"
      @clear="emit('handleBatchClear')"
    />
    <div id="ganttStage" class="gantt-stage">
      <GanttChart
        :timeline-data="displayedTimelineData"
        :window-start-ms="windowStartMs"
        :window-end-ms="windowEndMs"
        :safety-progress="safetyProgress"
        :resource-focus="resourceFocus"
        :safety-gate-filter="safetyGateFilter"
        :highlighted-item-id="detailCurrentOrderId"
        :selected-order-ids="selectedOrderIds"
        :impacted-order-ids="impactedOrderIds"
        @item-dblclick="emit('itemDblClick', $event)"
        @item-click="emit('itemClick', $event)"
      />
      <div id="ganttLegendOverlay" class="gantt-legend-overlay" data-density="full">
        <div class="gantt-legend-strip" aria-hidden="true">
          <span class="legend-item" data-group="primary"><span class="status-symbol pending" aria-hidden="true">○</span><span class="legend-dot pending" />待派</span>
          <span class="legend-item" data-group="primary"><span class="status-symbol assigned" aria-hidden="true">●</span><span class="legend-dot assigned" />已派</span>
          <span class="legend-item" data-group="primary"><span class="status-symbol in-progress" aria-hidden="true">▶</span><span class="legend-dot in-progress" />进行</span>
          <span class="legend-item" data-group="primary"><span class="status-symbol completed" aria-hidden="true">✓</span><span class="legend-dot completed" />完成</span>
          <span class="legend-item" data-group="primary"><span class="status-symbol cancelled" aria-hidden="true">×</span><span class="legend-dot cancelled" />取消</span>
          <span class="legend-item is-secondary" data-group="secondary"><span class="legend-swatch alert" />冲突</span>
          <span class="legend-item is-secondary" data-group="secondary"><span class="legend-swatch lock" />优化约束</span>
          <span class="legend-item" data-group="secondary"><span class="legend-dot safety-blocked" />阻断</span>
        </div>
        <button
          id="ganttLegendMoreBtn"
          class="gantt-legend-more"
          type="button"
          :aria-expanded="isGanttLegendPopoverVisible ? 'true' : 'false'"
          aria-controls="ganttLegendPopover"
          @click="emit('toggleGanttLegend')"
        >
          说明
        </button>
        <div id="ganttLegendPopover" class="gantt-legend-popover" :hidden="!isGanttLegendPopoverVisible">
          <p class="gantt-legend-popover-title">
            甘特图图例说明
          </p>
          <div class="gantt-legend-popover-body">
            <div class="legend-cluster">
              <div class="legend-cluster-title">
                主状态
              </div>
              <div class="legend-cluster-items">
                <span class="legend-item"><span class="status-symbol pending" aria-hidden="true">○</span><span class="legend-dot pending" />待派工</span>
                <span class="legend-item"><span class="status-symbol assigned" aria-hidden="true">●</span><span class="legend-dot assigned" />已分配</span>
                <span class="legend-item"><span class="status-symbol in-progress" aria-hidden="true">▶</span><span class="legend-dot in-progress" />进行中</span>
                <span class="legend-item"><span class="status-symbol completed" aria-hidden="true">✓</span><span class="legend-dot completed" />已完成</span>
                <span class="legend-item"><span class="status-symbol cancelled" aria-hidden="true">×</span><span class="legend-dot cancelled" />已取消</span>
              </div>
            </div>
            <div class="legend-cluster">
              <div class="legend-cluster-title">
                次级语义
              </div>
              <div class="legend-cluster-items">
                <span class="legend-item is-secondary"><span class="legend-swatch draft" />预发布草稿</span>
                <span class="legend-item is-secondary"><span class="legend-swatch alert" />冲突 / 阻塞 / 缺口</span>
                <span class="legend-item is-secondary"><span class="legend-swatch lock" />后端规则标记为不可自动优化</span>
                <span class="legend-item is-secondary"><span class="legend-swatch summary" />聚合条含状态分布</span>
                <span class="legend-item"><span class="legend-dot safety-ready" />清单就绪</span>
                <span class="legend-item"><span class="legend-dot safety-pending" />清单待补齐</span>
                <span class="legend-item"><span class="legend-dot safety-blocked" />清单阻断</span>
              </div>
            </div>
          </div>
          <p class="gantt-legend-popover-note">
            悬浮条默认只保留关键识别项，完整语义在这里查看。
          </p>
        </div>
      </div>
      <div id="cornerInfo" class="corner-info" :class="{ 'is-faded': guideSettings.cornerFade }">
        <h1>实时派工时间线</h1>
        <p id="windowLabel" class="window-label">
          -
        </p>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* 角信息：淁隐态走修饰类，过渡归梯 */
.corner-info {
  transition: opacity var(--t-slow) var(--ease);
}

.corner-info.is-faded {
  opacity: 0.3;
}
</style>
