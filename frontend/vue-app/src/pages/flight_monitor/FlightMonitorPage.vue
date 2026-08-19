<script setup lang="ts">
import { computed } from 'vue';
import { useFlightMonitorPage } from './composables/useFlightMonitorPage';
import FlightMonitorHeader from './sections/FlightMonitorHeader.vue';
import FlightFilterBar from './sections/FlightFilterBar.vue';
import FlightListSection from './sections/FlightListSection.vue';
import FlightDetailSection from './sections/FlightDetailSection.vue';
import FlightEventModal from './sections/FlightEventModal.vue';
import FlightEditModals from './sections/FlightEditModals.vue';
import FlightFloatingPanel from './sections/FlightFloatingPanel.vue';
import FlightCellContextMenu from './sections/FlightCellContextMenu.vue';
import FlightBatchEditModal from './sections/FlightBatchEditModal.vue';
import { getBatchEditableField } from './flightBatchEditableFields';
import MilestonePulse from '@/components/flight-monitor/MilestonePulse.vue';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';

const p = useFlightMonitorPage();

// 对齐 legacy syncFlightMonitorLayout：表格视图下列表面板占满整行，
// 隐藏详情面板与拖拽条；切回卡片视图时恢复原有宽度（含拖拽持久化宽度）。
const isTableFullView = computed(() => p.viewMode.value === 'table' && !p.alertPoolOpen.value);
</script>

<template>
  <FlightMonitorHeader :ariaAnnouncement="p.ariaAnnouncement.value" />

  <div class="container flight-monitor-container" :class="{ 'flight-monitor-container--table': isTableFullView }">
    <div
      id="flight-list-main"
      class="flight-list-panel"
      role="region"
      aria-label="实时航班列表"
      tabindex="0"
    >
      <FlightFilterBar
        :page-url="p.pageUrl"
        :connection-status-class="p.flightStream.connectionStatusClass.value"
        :connection-status-text="p.flightStream.connectionStatusText.value"
        :last-updated-label="p.lastUpdatedLabel.value"
        :is-refreshing="p.isRefreshing.value"
        :view-mode="p.viewMode.value"
        :search-query="p.flightData.searchQuery.value"
        :search-fields="p.flightData.searchFields.value"
        :search-options-expanded="p.searchOptionsExpanded.value"
        :visible-count="p.list.visibleCount.value"
        :total-count="p.list.totalFlights.value"
        :has-selected-flight="Boolean(p.selectedFlight.value)"
        :has-active-filters="p.list.hasActiveFilters.value"
        :filters="p.flightData.businessFilters.value"
        :anomaly-count="p.list.filterCounts.value.anomaly"
        :delay-count="p.list.filterCounts.value.delay"
        :vip-count="p.list.filterCounts.value.vip"
        :quick-turn-count="p.list.filterCounts.value.quickTurn"
        :status-banner="p.statusBanner.value"
        @refresh="p.refreshFlights"
        @update:view-mode="p.list.setViewMode"
        @update:search-query="p.flightData.setSearchQuery"
        @toggle-search-options="p.searchOptionsExpanded.value = !p.searchOptionsExpanded.value"
        @set-search-field="p.list.handleSearchFieldChange"
        @submit-search="p.list.submitSearch"
        @clear-search="p.flightData.setSearchQuery('')"
        @focus-selected-flight="p.list.focusSelectedFlight"
        @clear-all-filters="p.list.clearAllFilters"
        @set-business-filter="p.list.handleBusinessFilterChange"
      />

      <FlightListSection
        :view-mode="p.viewMode.value"
        :is-initial-loading="p.isInitialLoading.value"
        :show-filtered-empty-state="p.list.showFilteredEmptyState.value"
        :show-dataset-empty-state="p.list.showDatasetEmptyState.value"
        :init-failed="p.initFailed.value"
        :show-flight-list="p.list.showFlightList.value"
        :is-reconnecting="p.flightStream.connectionStatusKey.value === 'reconnecting'"
        :connection-status-text="p.flightStream.connectionStatusText.value"
        :visible-flights="p.list.visibleFlights.value"
        :airport-context="p.flightData.airportContext.value"
        :selected-flight-id="p.selectedFlightId.value"
        :alert-pool-open="p.alertPoolOpen.value"
        :has-active-filters="p.list.hasActiveFilters.value"
        :sort-field="p.flightData.sortConfig.value.field"
        :sort-direction="p.flightData.sortConfig.value.direction"
        :visible-columns="p.list.visibleColumns.value"
        :can-select-cells="p.batchEdit.canManageFlights.value"
        :is-cell-selected="p.cellSelection.isCellSelected"
        :can-edit-field="p.batchEdit.canEditField"
        :selection-revision="p.selectionRevision.value"
        :flash-events="p.flightStream.flightFlashEvents.value"
        @select-flight="p.list.selectFlight"
        @open-context-menu="(event, flightId, field, type, value) => {
          if (p.batchEdit.canEditField(field)) {
            p.batchEdit.handleCellContextMenu(event, flightId, field, type, value);
          } else {
            p.modals.handleContextMenu(event, flightId, field, type, value);
          }
        }"
        @sort="p.list.handleSort"
        @exit-alert-pool="p.list.closeAlertPool"
        @open-column-config="p.list.columnConfigState.value.isOpen = true"
        @edit-field="p.modals.handleEditField"
        @refresh="p.refreshFlights"
        @clear-filters="p.list.clearAllFilters"
        @cell-select-start="p.handleCellSelectStart"
        @cell-select-extend="p.handleCellSelectExtend"
        @cell-select-end="p.handleCellSelectEnd"
      />
    </div>

    <FlightDetailSection
      v-show="!isTableFullView"
      :is-initial-loading="p.isInitialLoading.value"
      :show-flight-list="p.list.showFlightList.value"
      :selected-flight="p.selectedFlight.value as unknown as import('@/types/bindings').FlightResponse | null"
      :airport-context="p.flightData.airportContext.value"
      @close-drawer="p.selectedFlightId.value = null"
      @create-business-case="p.modals.openEventModal"
      @edit-remark="p.modals.openRemarkEdit"
      @edit-field="p.modals.handleEditField"
    />
  </div>

  <FlightEventModal
    :is-open="p.modals.eventCreationState.value.isOpen"
    :event-type="p.modals.eventCreationState.value.form.eventType"
    :event-status="p.modals.eventCreationState.value.form.eventStatus"
    :description="p.modals.eventCreationState.value.form.description"
    :gate="p.modals.eventCreationState.value.form.gate"
    :trigger-reason="p.modals.eventCreationState.value.form.triggerReason"
    :bound-flight-value="p.modals.eventCreationState.value.form.boundFlightValue"
    :submitting="p.modals.eventCreationState.value.submitting"
    :can-submit="p.modals.canSubmitEventCreation.value"
    :business-case-types="p.modals.eventCreationState.value.types"
    :bound-flight-binding-options="p.modals.boundFlightBindingOptions.value"
    @close="p.modals.closeEventModal"
    @submit="p.modals.handleEventCreationSubmit"
    @update:event-type="p.modals.eventCreationState.value.form.eventType = $event"
    @update:event-status="p.modals.eventCreationState.value.form.eventStatus = $event"
    @update:description="p.modals.eventCreationState.value.form.description = $event"
    @update:gate="p.modals.eventCreationState.value.form.gate = $event"
    @update:trigger-reason="p.modals.eventCreationState.value.form.triggerReason = $event"
    @update:bound-flight-value="p.modals.eventCreationState.value.form.boundFlightValue = $event"
  />

  <FlightEditModals
    :column-config-is-open="p.list.columnConfigState.value.isOpen"
    :column-config-items="p.list.columnConfigState.value.items"
    :column-config-visible-columns="p.list.columnConfigState.value.visibleColumns"
    :field-edit-is-open="p.modals.fieldEditState.value.isOpen"
    :field-edit-label="p.modals.fieldEditState.value.label"
    :field-edit-type="p.modals.fieldEditState.value.type"
    :field-edit-value="p.modals.fieldEditState.value.value"
    :field-edit-saving="p.modals.fieldEditState.value.saving"
    :remark-edit-is-open="p.modals.remarkEditState.value.isOpen"
    :remark-edit-label="p.modals.remarkEditState.value.label"
    :remark-edit-value="p.modals.remarkEditState.value.value"
    :remark-edit-saving="p.modals.remarkEditState.value.saving"
    @update:column-config-visible-columns="p.list.columnConfigState.value.visibleColumns = $event"
    @reorder-column-config="(from, to) => p.list.reorderColumnItems(from, to)"
    @save-column-config="p.list.handleColumnSave"
    @close-column-config="p.list.closeColumnModal"
    @reset-column-config="p.list.resetColumnConfig"
    @update:field-edit-value="p.modals.fieldEditState.value.value = $event"
    @save-field-edit="p.modals.saveFieldEdit"
    @close-field-edit="p.modals.fieldEditState.value.isOpen = false"
    @update:remark-edit-value="p.modals.remarkEditState.value.value = $event"
    @save-remark-edit="p.modals.saveRemarkEdit"
    @close-remark-edit="p.modals.remarkEditState.value.isOpen = false"
  />

  <FlightFloatingPanel
    :anomaly-count="p.list.visibleAnomalyFlights.value.length"
    :anomaly-severity="p.list.anomalySeverity.value"
    :update-messages="p.realtimeUpdateMessages.value"
    :update-panel-open="p.updatePanelOpen.value"
    :notification-count="p.notificationData.unreadCount.value"
    :dispatch-notify-open="p.dispatchNotifyOpen.value"
    :dispatch-chat-open="p.dispatchChatOpen.value"
    :flight-insight-open="p.flightInsightOpen.value"
    :selected-flight-id="p.selectedFlightId.value"
    :selected-flight-no="p.selectedFlight.value?.flight_number ?? undefined"
    :flight-no-resolver="(flightId: string) => p.flightData.findFlightById(flightId)?.flight_number ?? null"
    :business-case-types="p.modals.eventCreationState.value.types"
    :critical-notification-queue="(p.flightStream.criticalNotificationQueue.value as import('@/types/bindings').UserNotification[])"
    :sent-receipt-reminder-queue="p.notificationData.sentReceiptReminderQueue.value"
    @toggle-anomaly-pool="p.list.toggleAlertPool"
    @toggle-update-panel="p.updatePanelOpen.value = !p.updatePanelOpen.value"
    @close-update-panel="p.updatePanelOpen.value = false"
    @open-dispatch="p.dispatchNotifyOpen.value = true"
    @open-chat="p.dispatchChatOpen.value = true"
    @open-insight="p.flightInsightOpen.value = true"
    @close-dispatch="p.dispatchNotifyOpen.value = false"
    @close-chat="p.dispatchChatOpen.value = false"
    @close-insight="p.flightInsightOpen.value = false"
    @shift-critical-notification="p.flightStream.shiftCriticalNotification"
    @shift-reminder="p.notificationData.sentReceiptReminderQueue.value.shift()"
    @view-reminder-history="p.dispatchNotifyOpen.value = true"
    @auto-copilot-created="p.modals.handleAutoCopilotCreated"
    @toast="(msg: string) => p.toast.showToast('info', msg, { duration: 4000 })"
    @error="(msg: string) => p.toast.showToast('error', msg, { duration: 5000 })"
  />

  <teleport to="body">
    <div
      v-if="p.modals.contextMenuState.value.isOpen"
      id="timeContextMenu"
      class="context-menu"
      :style="{ top: `${p.modals.contextMenuState.value.y}px`, left: `${p.modals.contextMenuState.value.x}px` }"
    >
      <button id="ctxModify" class="context-menu-item" @click.stop="p.modals.handleContextModify">
        修改预期时间 (P)
      </button>
      <button id="ctxRevoke" class="context-menu-item danger-action" @click.stop="p.modals.handleContextRevoke">
        撤销该节点关联 (Revoke)
      </button>
    </div>
  </teleport>

  <FlightCellContextMenu
    :is-open="p.batchEdit.contextMenuState.value.isOpen"
    :x="p.batchEdit.contextMenuState.value.x"
    :y="p.batchEdit.contextMenuState.value.y"
    :multi="p.batchEdit.contextMenuState.value.multi"
    :selected-count="p.batchEdit.contextMenuState.value.selectedCount"
    :field-label="p.batchContextFieldLabel.value"
    :can-revoke="p.batchEdit.canRevokeCurrentContext.value"
    :over-limit="p.batchEdit.contextMenuState.value.selectedCount > 200"
    @batch-edit="p.batchEdit.handleContextBatchEdit"
    @single-edit="p.batchEdit.handleContextSingleEdit"
    @revoke="p.batchEdit.handleContextRevoke"
    @clear-selection="() => { p.batchEdit.handleContextClearSelection(); p.selectionRevision.value += 1; }"
    @close="p.batchEdit.closeCellContextMenu"
  />

  <FlightBatchEditModal
    :is-open="p.batchEdit.modalState.value.isOpen"
    :label="p.batchEdit.modalState.value.label"
    :value-type="p.batchEdit.modalState.value.valueType"
    :value="p.batchEdit.modalState.value.value"
    :flight-count="p.batchEdit.modalState.value.flightIds.length"
    :max-length="getBatchEditableField(p.batchEdit.modalState.value.field)?.maxLength ?? null"
    :saving="p.batchEdit.modalState.value.saving"
    :can-submit="p.batchEdit.canSubmitCurrent.value"
    :error="p.batchEdit.modalState.value.error"
    @update:value="p.batchEdit.setBatchValue"
    @submit="p.batchEdit.submitBatchEdit"
    @close="p.batchEdit.closeBatchEdit"
  />

  <div
    v-if="p.cellSelection.selectedCount.value > 0 && p.batchEdit.canManageFlights.value"
    class="batch-cell-selection-bar"
    role="status"
    aria-live="polite"
  >
    <span>
      已选 {{ p.cellSelection.selectedCount.value }} 个「{{ p.batchContextFieldLabel.value || getBatchEditableField(p.cellSelection.selectedField.value || '')?.label || '单元格' }}」
    </span>
    <button
      type="button"
      class="flight-text-btn"
      :disabled="!p.cellSelection.canSubmitBatch.value"
      @click="p.batchEdit.openBatchEditFromSelection"
    >
      批量修改
    </button>
    <button
      type="button"
      class="flight-text-btn"
      @click="() => { p.cellSelection.clearSelection(); p.selectionRevision.value += 1; }"
    >
      清除 (Esc)
    </button>
  </div>
  <ThemeToggle />
  <MilestonePulse
    :flight-no="p.milestonePulse.activePulse.value?.flightNo ?? ''"
    :label="p.milestonePulse.activePulse.value?.label ?? ''"
    :visible="Boolean(p.milestonePulse.activePulse.value)"
  />
</template>

<style>
.flight-monitor-container {
  display: flex;
  flex-direction: row;
  width: 100%;
  min-width: 0;
  min-height: 0;
  height: 100vh;
  overflow: hidden;
  background-color: var(--face-page);
}

.flight-list-content-shell {
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}

.flight-list-content-shell > * {
  min-height: 0;
}

.context-menu {
  position: fixed;
  background-color: var(--face-raised);
  border: 1px solid var(--line);
  box-shadow: var(--shadow-md);
  z-index: 10000;
  min-width: 180px;
  display: flex;
  flex-direction: column;
  border-radius: var(--r-control);
}

.context-menu-item {
  background: none;
  border: none;
  padding: 10px 16px;
  text-align: left;
  color: var(--ink);
  cursor: pointer;
  font-size: var(--fs-body);
}

.context-menu-item:hover {
  background-color: var(--face-work);
}

.context-menu-item:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: -2px;
}

.context-menu-item.danger-action {
  color: var(--danger);
}

.batch-cell-selection-bar {
  position: fixed;
  left: 50%;
  bottom: 24px;
  transform: translateX(-50%);
  z-index: 9000;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 16px;
  border-radius: var(--r-panel);
  background: var(--face-raised);
  border: 1px solid var(--line);
  box-shadow: var(--shadow-md);
  font-size: var(--fs-body);
  color: var(--ink);
}

.reconnect-skeleton-overlay {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 16px;
  background: var(--scrim);
  color: var(--ink);
  font-size: var(--fs-body);
  border-radius: var(--r-control);
  z-index: 50;
  pointer-events: none;
}

.reconnect-skeleton-overlay .reconnect-spinner {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}
</style>
