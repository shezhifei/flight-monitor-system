<script setup lang="ts">
import FlightList from '../../../components/flight-monitor/FlightList.vue';
import SkeletonCard from '../../../components/ui/SkeletonCard.vue';
import SkeletonTableRow from '../../../components/ui/SkeletonTableRow.vue';
import EmptyState from '../../../components/ui/EmptyState.vue';
import type { FlightViewMode } from '../../../components/flight-monitor/helpers';
import type { Flight } from '../../../composables/useFlightData';
import type { AirportContext } from '../../../composables/useFlightData';
import type { FlightSortDirection } from '../../../composables/useFlightData';
import type { FlightResponse } from '@/types/bindings';
import type { FlightFlashEvent } from '@/composables/useFlightStream';

defineProps<{
  viewMode: FlightViewMode;
  isInitialLoading: boolean;
  showFilteredEmptyState: boolean;
  showDatasetEmptyState: boolean;
  initFailed: boolean;
  showFlightList: boolean;
  isReconnecting: boolean;
  connectionStatusText: string;
  visibleFlights: readonly Flight[];
  airportContext: AirportContext;
  selectedFlightId: string | null;
  alertPoolOpen: boolean;
  hasActiveFilters: boolean;
  sortField: string | null;
  sortDirection: FlightSortDirection;
  visibleColumns: string[];
  canSelectCells?: boolean;
  isCellSelected?: (flightId: string, field: string) => boolean;
  canEditField?: (field: string) => boolean;
  selectionRevision?: number;
  flashEvents?: readonly FlightFlashEvent[];
}>();

const emit = defineEmits<{
  (e: 'select-flight', flightId: string): void;
  (e: 'open-context-menu', event: MouseEvent, flightId: string, field: string, type: string, value: string): void;
  (e: 'sort', field: string): void;
  (e: 'exit-alert-pool'): void;
  (e: 'open-column-config'): void;
  (e: 'edit-field', flightId: string, field: string, type: string, value: string): void;
  (e: 'refresh'): void;
  (e: 'clear-filters'): void;
  (e: 'cell-select-start', flightId: string, field: string, additive: boolean, shiftKey: boolean): void;
  (e: 'cell-select-extend', flightId: string, field: string): void;
  (e: 'cell-select-end'): void;
}>();
</script>

<template>
  <div>
    <template v-if="isInitialLoading">
      <div v-show="viewMode === 'card'" class="card-layout-view" style="padding: 8px 16px 20px;">
        <SkeletonCard :count="6" />
      </div>
      <div
        v-show="viewMode === 'table'"
        class="flight-table-container"
        role="grid"
        aria-label="航班表格加载中"
      >
        <div class="table-scroll-wrapper">
          <table>
            <thead>
              <tr>
                <th v-for="col in ['航班号','航线','状态','起飞时间','落地时间','机位','登机口','COBT','允许登机','开始登机','结束登机','上轮挡','撤轮挡','行李转盘','机型','保障标签','备注']" :key="col">
                  {{ col }}
                </th>
              </tr>
            </thead>
            <tbody>
              <SkeletonTableRow :count="8" :columns="16" />
            </tbody>
          </table>
        </div>
      </div>
    </template>

    <template v-else-if="showFilteredEmptyState">
      <EmptyState
        icon="filter"
        title="没有匹配的航班"
        description="请尝试调整筛选条件或清空搜索条件。"
        action-label="清空筛选"
        @action="emit('clear-filters')"
      />
    </template>

    <template v-else-if="initFailed && showDatasetEmptyState">
      <EmptyState
        icon="alert"
        title="实时航班数据加载失败"
        description="当前无法获取实时航班快照，请重试连接。"
        action-label="重试"
        @action="emit('refresh')"
      />
    </template>

    <template v-else-if="showDatasetEmptyState">
      <EmptyState
        icon="plane"
        title="暂无航班数据"
        description="当前数据源未返回航班记录，请稍后刷新。"
        action-label="刷新"
        @action="emit('refresh')"
      />
    </template>

    <div v-show="showFlightList" class="flight-list-content-shell">
      <FlightList
        :flights="visibleFlights as unknown as readonly FlightResponse[]"
        :airport-context="airportContext"
        :selected-flight-id="selectedFlightId"
        :view-mode="viewMode"
        :show-alert-pool="alertPoolOpen"
        :has-active-filters="hasActiveFilters"
        :sort-field="sortField"
        :sort-direction="sortDirection"
        :visible-columns="visibleColumns"
        :can-select-cells="canSelectCells"
        :is-cell-selected="isCellSelected"
        :can-edit-field="canEditField"
        :selection-revision="selectionRevision"
        :flash-events="flashEvents"
        @select-flight="emit('select-flight', $event)"
        @open-context-menu="(event, flightId, field, type, value) => emit('open-context-menu', event, flightId, field, type, value)"
        @sort="emit('sort', $event)"
        @exit-alert-pool="emit('exit-alert-pool')"
        @open-column-config="emit('open-column-config')"
        @edit-field="(flightId, field, type, value) => emit('edit-field', flightId, field, type, value)"
        @cell-select-start="(flightId, field, additive, shiftKey) => emit('cell-select-start', flightId, field, additive, shiftKey)"
        @cell-select-extend="(flightId, field) => emit('cell-select-extend', flightId, field)"
        @cell-select-end="emit('cell-select-end')"
      />
    </div>

    <div
      v-if="isReconnecting && showFlightList"
      class="reconnect-skeleton-overlay"
      role="status"
      aria-live="polite"
    >
      <svg class="reconnect-spinner" width="16" height="16" viewBox="0 0 16 16" fill="none">
        <circle cx="8" cy="8" r="6" stroke="currentColor" stroke-width="2" stroke-dasharray="20" stroke-dashoffset="8" opacity="0.5" />
      </svg>
      <span>{{ connectionStatusText }}</span>
    </div>
  </div>
</template>
