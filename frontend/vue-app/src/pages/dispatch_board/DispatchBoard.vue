<script setup lang="ts">
import { computed, onBeforeUnmount } from 'vue';
import { useDispatchBoardPage } from './composables/useDispatchBoardPage';
import { useDispatchBoardPageActions } from './composables/useDispatchBoardPageActions';
import { filterTimelineBySafetyGate, countOrdersByStatus } from '@/composables/useDispatchBoardData';
import GuideAndLegendPanel from '@/components/dispatch-board/GuideAndLegendPanel.vue';
import OverrunWarningBar from '@/components/dispatch-board/OverrunWarningBar.vue';
import AiReactEntryShell from '@/components/ai/AiReactEntryShell.vue';
import ToolbarSection from './sections/ToolbarSection.vue';
import GanttPanel from './sections/GanttPanel.vue';
import OrderDetailPanel from './sections/OrderDetailPanel.vue';
import ResourceSidebar from './sections/ResourceSidebar.vue';
import AiDrawerSection from './sections/AiDrawerSection.vue';
import ChatDrawerSection from './sections/ChatDrawerSection.vue';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import { disposeTrendChart } from './composables/useTrendChart';
onBeforeUnmount(disposeTrendChart);

const p = useDispatchBoardPage();
const messagesIconHref = '/frontend/icons/messages.svg';

const visibleTimelineItems = computed(() => {
  const items = p.timelineData.value?.items || [];
  return p.guideSettings.showCompleted ? items : items.filter((item) => String(item.status || '').trim().toLowerCase() !== 'completed');
});
const displayedTimelineData = computed(() => {
  const data = p.timelineData.value;
  return data ? { ...data, items: visibleTimelineItems.value } : data;
});
const statusCounts = computed(() => countOrdersByStatus(visibleTimelineItems.value, p.safetyProgress.value, p.safetyGateFilter.value));
const statusOrderList = computed(() => {
  const filtered = filterTimelineBySafetyGate(visibleTimelineItems.value, p.safetyProgress.value, p.safetyGateFilter.value);
  return filtered.filter(item => !item.is_flight_summary && item.status === p.selectedStatus.value).map(item => ({ id: String(item.order_id || ''), title: String(item.task_type || item.flight_id || '-'), status: String(item.status || 'pending'), start_time: String(item.start_time || '') })).sort((a, b) => new Date(a.start_time || 0).getTime() - new Date(b.start_time || 0).getTime());
});
const terminalSelectorData = computed(() => p.terminals.value.filter(t => t.terminal !== 'all').map(t => ({ id: t.terminal, name: t.label, count: 0 })));
const currentTerminalId = computed(() => p.activeTerminal.value);

const actions = useDispatchBoardPageActions({ p, visibleTimelineItems, statusOrderList });
const {
  detailCrewMembers, detailQualificationGaps, detailEquipmentCodes,
  detailTaskInfoRows, detailTimeInfoRows, detailResourceInfoRows, detailFlightStatusSummary,
} = actions;
</script>

<template>
  <div class="dispatch-board-page">
    <div id="header-host" />
    <div id="breadcrumb-host" />
    <section class="gantt-shell">
      <ToolbarSection
        :search-query="p.searchQuery.value" :search-results="p.searchResults.value" :search-meta-label="p.searchMetaLabel.value"
        :active-view-mode="p.activeViewMode.value" :terminals="p.terminals.value" :active-terminal="p.activeTerminal.value"
        :is-ops-menu-visible="p.isOpsMenuVisible.value" :guide-settings="p.guideSettings"
        :setting-refresh-interval="p.settingRefreshInterval.value" :setting-safety-gate-filter="p.settingSafetyGateFilter.value"
        :is-batch-toolbar-visible="p.isBatchToolbarVisible.value" :selected-order-ids="p.selectedOrderIds.value"
        :resource-focus-text="p.resourceFocusText.value" :chat-unread-total="p.chatUnreadTotal.value"
        :terminal-selector-data="terminalSelectorData" :current-terminal-id="currentTerminalId"
        @update:search-query="p.searchQuery.value = $event" @search="actions.handleSearch" @search-next="actions.handleSearchNext"
        @toggle-ai-drawer="actions.toggleAiDrawer" @toggle-status-panel="actions.toggleStatusPanel" @toggle-chat-drawer="actions.toggleChatDrawer"
        @reset-window-to-now="actions.resetWindowToNow" @toggle-guide-and-legend-panel="actions.toggleGuideAndLegendPanel"
        @toggle-ops-menu="actions.toggleOpsMenu" @handle-view-tab-change="actions.handleViewTabChange" @switch-terminal="actions.switchTerminal"
        @refresh-timeline="p.refreshTimeline" @close-ops-menu="actions.closeOpsMenu" @handle-settings-apply="actions.handleSettingsApply"
        @toggle-batch-toolbar="actions.toggleBatchToolbar"
        @clear-resource-focus="() => { p.resourceFocusText.value = ''; p.resourceFocus.value = null; p.impactedOrderIds.value = []; }"
        @handle-terminal-change="actions.switchTerminal"
        @update:setting-refresh-interval="p.settingRefreshInterval.value = $event"
        @update:setting-safety-gate-filter="p.settingSafetyGateFilter.value = $event"
        @update:guide-settings="Object.assign(p.guideSettings, $event)"
      />
      <OverrunWarningBar
        :warnings="actions.overrunWarnings.value"
        :busy-ids="actions.overrunWarningBusyIds.value"
        @acknowledge="actions.handleOverrunAcknowledge"
        @resolve="actions.handleOverrunResolve"
        @jump-order="actions.handleOverrunJumpOrder"
        @jump-orders="actions.handleOverrunJumpOrders"
      />
      <GanttPanel
        :displayed-timeline-data="displayedTimelineData" :window-start-ms="p.windowStartMs.value" :window-end-ms="p.windowEndMs.value"
        :safety-progress="p.safetyProgress.value" :resource-focus="p.resourceFocus.value" :safety-gate-filter="p.safetyGateFilter.value"
        :detail-current-order-id="p.detailCurrentOrderId.value" :selected-order-ids="p.selectedOrderIds.value"
        :impacted-order-ids="p.impactedOrderIds.value" :is-batch-toolbar-visible="p.isBatchToolbarVisible.value"
        :is-gantt-legend-popover-visible="p.isGanttLegendPopoverVisible.value" :guide-settings="p.guideSettings"
        @toggle-batch-toolbar="actions.toggleBatchToolbar" @handle-batch-complete="actions.handleBatchComplete"
        @handle-batch-publish="actions.handleBatchPublish" @handle-batch-clear="actions.handleBatchClear"
        @toggle-gantt-legend="actions.toggleGanttLegend" @item-dbl-click="actions.handleGanttChartDoubleClick"
        @item-click="actions.handleGanttChartClick"
      />
    </section>

    <button id="openChatCornerBadgeBtn" class="dispatch-chat-corner-badge" type="button" aria-label="打开航班保障群聊" @click="actions.toggleChatDrawer">
      <img :src="messagesIconHref" alt="" aria-hidden="true">
      <span class="dispatch-chat-corner-text">航班群聊</span>
      <span v-show="p.chatUnreadTotal.value > 0" id="chatCornerUnreadBadge" class="chat-unread-badge chat-unread-corner-badge">{{ p.chatUnreadTotal.value }}</span>
    </button>

    <ResourceSidebar
      :is-status-panel-visible="p.isStatusPanelVisible.value" :selected-status="p.selectedStatus.value" :status-counts="statusCounts"
      :status-order-list="statusOrderList" :status-total-count="statusOrderList.length" :selected-order-ids="p.selectedOrderIds.value"
      :batch-process="p.batchProcess.value" :search-query="p.searchQuery.value"
      @close-status-panel="actions.closeStatusPanel" @select-status="p.selectedStatus.value = $event"
      @filter-blocked="actions.handleStatusFilterBlocked" @show-all="actions.handleStatusShowAll" @select-all="actions.handleStatusSelectAll"
      @start-batch-complete="actions.handleBatchComplete"
      @move-batch-index="(s: number) => { const n = p.batchProcess.value.currentIndex + s; const len = p.batchProcess.value.orderIds?.length || 0; if (n >= 1 && n <= len) p.batchProcess.value.currentIndex = n; }"
      @toggle-order-selection="actions.toggleOrderSelection" @open-order-detail="actions.handleStatusOrderOpen"
    />

    <OrderDetailPanel
      :is-detail-drawer-visible="p.isDetailDrawerVisible.value" :detail-title="p.detailTitle.value" :detail-mode="p.detailMode.value"
      :detail-opening="p.detailOpening.value" :detail-order="p.detailOrder.value" :detail-flight-summary="p.detailFlightSummary.value"
      :detail-flight-orders="p.detailFlightOrders.value" :detail-checklist="p.detailChecklist.value" :detail-checklist-loading="p.detailChecklistLoading.value"
      :detail-checklist-error="p.detailChecklistError.value" :detail-gate-hint="p.detailGateHint.value" :detail-submitting-key="p.detailSubmittingKey.value"
      :detail-completing="p.detailCompleting.value" :detail-current-order-id="p.detailCurrentOrderId.value" :detail-safety-gate-state="p.detailSafetyGateState.value"
      :critical-checklist-items="p.criticalChecklistItems.value" :routine-checklist-items="p.routineChecklistItems.value"
      :detail-completion-ready="p.detailCompletionReady.value" :detail-completion-button-text="p.detailCompletionButtonText.value"
      :detail-routine-pending-count="p.detailRoutinePendingCount.value" :detail-can-submit-checklist="p.detailCanSubmitChecklist.value"
      :detail-can-complete-order="p.detailCanCompleteOrder.value" :detail-crew-members="detailCrewMembers"
      :detail-qualification-gaps="detailQualificationGaps" :detail-equipment-codes="detailEquipmentCodes"
      :detail-task-info-rows="detailTaskInfoRows" :detail-time-info-rows="detailTimeInfoRows"
      :detail-resource-info-rows="detailResourceInfoRows" :detail-flight-status-summary="detailFlightStatusSummary"
      :batch-process="p.batchProcess.value"
      @close="p.closeDetailDrawer" @open-flight-order-detail="(id: string) => p.openFlightOrderDetail(id)"
      @refresh-checklist="p.refreshDetailChecklist" @submit-checklist-item="(code: string, result: string) => p.submitDetailChecklistItem(code, result as 'pass' | 'fail' | 'na')"
      @submit-routine-checklist-batch="p.submitDetailRoutineChecklistBatch" @complete-current-order="p.completeCurrentDetailOrder"
      @move-batch-index="(s: number) => { const n = p.batchProcess.value.currentIndex + s; const len = p.batchProcess.value.orderIds?.length || 0; if (n >= 1 && n <= len) p.batchProcess.value.currentIndex = n; }"
    />

    <AiDrawerSection
      :is-ai-drawer-visible="p.isAiDrawerVisible.value" :active-ai-tab="p.activeAiTab.value" :ai-stream-enabled="p.aiStreamEnabled.value"
      :ai-objective="p.aiObjective.value" :ai-metrics="p.aiMetrics.value" :ai-suggestion-list="p.aiSuggestionList.value"
      :analytics-data="p.analyticsData.value" :analytics-mode="p.analyticsMode.value" :analytics-metrics="p.analyticsMetrics.value"
      :analytics-breakdown-list="p.analyticsBreakdownList.value" :conflict-list="p.conflictList.value" :conflict-metrics="p.conflictMetrics.value"
      :conflict-severity-filter="p.conflictSeverityFilter.value" :conflict-type-filter="p.conflictTypeFilter.value"
      :conflict-query-input="p.conflictQueryInput.value" :available-conflict-types="p.availableConflictTypes.value"
      :scenario-equipment="p.scenarioEquipment.value" :scenario-stand="p.scenarioStand.value" :scenario-delay="p.scenarioDelay.value"
      :scenario-frozen="p.scenarioFrozen.value" :scenario-impacted-orders="p.scenarioImpactedOrders.value"
      :scenario-projected-conflicts="p.scenarioProjectedConflicts.value" :scenario-recommendations="p.scenarioRecommendations.value"
      :replan-strategy="p.replanStrategy.value" :replan-max-suggestions="p.replanMaxSuggestions.value" :replan-mode="p.replanMode.value"
      :replan-suggestion-list="p.replanSuggestionList.value" :replan-can-apply="p.replanCanApply.value" :replan-status-label="p.replanStatusLabel.value"
      :categorized-replan-suggestions="p.categorizedReplanSuggestions.value"
      :replan-direct-apply-enabled="actions.replanDirectApplyEnabled"
      @close="actions.closeAiDrawer" @update:active-ai-tab="p.activeAiTab.value = $event" @update:ai-stream-enabled="p.aiStreamEnabled.value = $event"
      @update:ai-objective="p.aiObjective.value = $event" @update:analytics-mode="p.analyticsMode.value = $event"
      @update:conflict-severity-filter="p.conflictSeverityFilter.value = $event" @update:conflict-type-filter="p.conflictTypeFilter.value = $event"
      @update:conflict-query-input="p.conflictQueryInput.value = $event" @update:scenario-equipment="p.scenarioEquipment.value = $event"
      @update:scenario-stand="p.scenarioStand.value = $event" @update:scenario-delay="p.scenarioDelay.value = $event"
      @update:scenario-frozen="p.scenarioFrozen.value = $event" @update:replan-strategy="p.replanStrategy.value = $event"
      @update:replan-max-suggestions="p.replanMaxSuggestions.value = $event"
      @fetch-conflicts="actions.fetchConflicts" @fetch-analytics="actions.fetchAnalytics" @preview-scenario="actions.previewScenario"
      @clear-scenario="actions.clearScenario" @handle-ai-generate="actions.handleAiGenerate" @open-assistant-shell="actions.openAssistantShell"
      @handle-replan-preview="actions.handleReplanPreview" @handle-replan-apply="actions.handleReplanApply" @handle-replan-clear="actions.handleReplanClear"
      @preview-ai-suggestion="actions.previewAiSuggestion" @apply-ai-suggestion="actions.applyAiSuggestion"
      @set-trend-chart-ref="(el) => p.trendChartRef.value = el"
    />

    <ChatDrawerSection
      :is-chat-drawer-visible="p.isChatDrawerVisible.value" :chat-group-list="p.chatGroupList.value" :chat-active-group="p.chatActiveGroup.value"
      :chat-message-list="p.chatMessageList.value" :chat-messages-error="p.chatMessagesError.value" :chat-input="p.chatInput.value"
      @close="actions.closeChatDrawer" @select-chat-group="p.selectChatGroup"
      @send-chat-message="() => { p.sendChatMessage(p.chatInput.value, p.chatAtAll.value); p.chatInput.value = ''; }"
      @update:chat-input="p.chatInput.value = $event"
    />

    <div v-show="p.isAiDrawerVisible.value || p.isStatusPanelVisible.value || p.isDetailDrawerVisible.value || p.isChatDrawerVisible.value || p.isOpsMenuVisible.value || p.isGuideAndLegendPanelVisible.value" id="backdrop" class="backdrop" @click="actions.handleBackdropClick" />

    <GuideAndLegendPanel :class="{ open: p.isGuideAndLegendPanelVisible.value }" :settings="p.guideSettings" @close="actions.closeGuideAndLegendPanel" @settings-change="actions.handleGuideSettingsChange" />
    <AiReactEntryShell :entry-name="'dispatch_board_ai'" surface="drawer" />
    <ThemeToggle />
  </div>
</template>
