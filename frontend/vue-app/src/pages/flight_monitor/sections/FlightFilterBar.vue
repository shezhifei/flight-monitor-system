<script setup lang="ts">
import SearchBar from '../../../components/flight-monitor/SearchBar.vue';
import BusinessFilter from '../../../components/flight-monitor/BusinessFilter.vue';
import SvgIcon from '../../../components/ui/SvgIcon.vue';
import type { FlightViewMode } from '../../../components/flight-monitor/helpers';
import type { BusinessFilters } from '../../../composables/useFlightData';
import { DEFAULT_SEARCH_FIELDS } from '../../../composables/useFlightData';

defineProps<{
  homeIconHref: string;
  pageUrl: (name: string) => string;
  connectionStatusClass: string;
  connectionStatusText: string;
  lastUpdatedLabel: string;
  isRefreshing: boolean;
  viewMode: FlightViewMode;
  searchQuery: string;
  searchFields: typeof DEFAULT_SEARCH_FIELDS;
  searchOptionsExpanded: boolean;
  businessFilterExpanded: boolean;
  visibleCount: number;
  totalCount: number;
  hasSelectedFlight: boolean;
  hasActiveFilters: boolean;
  filters: BusinessFilters;
  anomalyCount: number;
  delayCount: number;
  vipCount: number;
  quickTurnCount: number;
  statusBanner: { tone: 'info' | 'warning' | 'danger'; title: string; description: string } | null;
}>();

const emit = defineEmits<{
  (e: 'refresh'): void;
  (e: 'update:viewMode', mode: FlightViewMode): void;
  (e: 'update:searchQuery', query: string): void;
  (e: 'toggle-search-options'): void;
  (e: 'toggle-business-filters'): void;
  (e: 'set-search-field', key: keyof typeof DEFAULT_SEARCH_FIELDS, checked: boolean): void;
  (e: 'submit-search'): void;
  (e: 'clear-search'): void;
  (e: 'focus-selected-flight'): void;
  (e: 'clear-all-filters'): void;
  (e: 'set-business-filter', key: keyof BusinessFilters, value: BusinessFilters[keyof BusinessFilters]): void;
  (e: 'reset-business-filters'): void;
}>();
</script>

<template>
  <div class="flight-workbar" data-role="flight-workbar">
    <div class="panel-title flight-panel-title">
      <div class="flight-panel-title-group">
        <a class="flight-home-link" :href="pageUrl('dashboard')" title="返回工作台" aria-label="返回工作台">
          <SvgIcon :src="homeIconHref" :size="18" />
        </a>
        <div class="flight-panel-heading">
          <div class="flight-panel-eyebrow">航班监控</div>
          <div class="flight-panel-title-text">实时航班状态</div>
        </div>
      </div>
      <div class="flight-panel-meta">
        <div id="connectionStatusPill" :class="['flight-connection-pill', 'connection-status', connectionStatusClass]">
          {{ connectionStatusText }}
        </div>
        <div id="lastUpdated" class="last-updated">{{ lastUpdatedLabel }}</div>
        <div class="action-buttons-group" style="display: flex; gap: 8px; flex-wrap: nowrap; align-items: center;">
          <button
            id="refreshBtn"
            class="export-btn"
            type="button"
            aria-label="刷新航班数据"
            title="刷新数据"
            :class="{ 'btn-loading btn-loading--dark': isRefreshing }"
            :disabled="isRefreshing"
            @click="emit('refresh')"
          >
            <svg
              v-if="!isRefreshing"
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              class="svg-icon svg-icon-sm"
              aria-hidden="true"
              style="margin-right: -2px;"
            >
              <polyline points="23 4 23 10 17 10" />
              <polyline points="1 20 1 14 7 14" />
              <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" />
            </svg>
            <span>{{ isRefreshing ? '刷新中' : '刷新' }}</span>
          </button>
        </div>
      </div>
    </div>

    <SearchBar
      :view-mode="viewMode"
      :search-query="searchQuery"
      :search-fields="searchFields"
      :search-options-expanded="searchOptionsExpanded"
      :business-filter-expanded="businessFilterExpanded"
      :visible-count="visibleCount"
      :total-count="totalCount"
      :has-selected-flight="hasSelectedFlight"
      :can-clear-filters="hasActiveFilters"
      @update:view-mode="emit('update:viewMode', $event)"
      @update:search-query="emit('update:searchQuery', $event)"
      @toggle-search-options="emit('toggle-search-options')"
      @toggle-business-filters="emit('toggle-business-filters')"
      @set-search-field="(key, checked) => emit('set-search-field', key, checked)"
      @submit-search="emit('submit-search')"
      @clear-search="emit('clear-search')"
      @focus-selected-flight="emit('focus-selected-flight')"
      @clear-all-filters="emit('clear-all-filters')"
    />

    <BusinessFilter
      :filters="filters"
      :expanded="businessFilterExpanded"
      :anomaly-count="anomalyCount"
      :delay-count="delayCount"
      :vip-count="vipCount"
      :quick-turn-count="quickTurnCount"
      :reset-visible="hasActiveFilters"
      @set-filter="(key, value) => emit('set-business-filter', key, value)"
      @reset="emit('reset-business-filters')"
    />

    <div
      v-if="statusBanner"
      class="flight-monitor-status-banner"
      :class="`flight-monitor-status-banner--${statusBanner.tone}`"
      role="status"
      aria-live="polite"
    >
      <span class="flight-monitor-status-banner__title">{{ statusBanner.title }}</span>
      <span class="flight-monitor-status-banner__description">{{ statusBanner.description }}</span>
    </div>
  </div>
</template>
