<script setup lang="ts">
import { computed } from 'vue';
import UiBanner from '../../../components/ui/UiBanner.vue';
import UiButton from '../../../components/ui/UiButton.vue';
import UiPill from '../../../components/ui/UiPill.vue';
import UiPlaceBar from '../../../components/ui/UiPlaceBar.vue';
import UiSegment from '../../../components/ui/UiSegment.vue';
import UiSelect from '../../../components/ui/UiSelect.vue';
import UiToolbar from '../../../components/ui/UiToolbar.vue';
import type { FlightViewMode } from '../../../components/flight-monitor/helpers';
import { SEARCH_FIELD_OPTIONS } from '../../../components/flight-monitor/helpers';
import type { BusinessFilters, SearchFields } from '../../../composables/useFlightData';
import { DEFAULT_SEARCH_FIELDS } from '../../../composables/useFlightData';

const props = defineProps<{
  pageUrl: (name: string) => string;
  connectionStatusClass: string;
  connectionStatusText: string;
  lastUpdatedLabel: string;
  isRefreshing: boolean;
  viewMode: FlightViewMode;
  searchQuery: string;
  searchFields: SearchFields;
  searchOptionsExpanded: boolean;
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
  (e: 'set-search-field', key: keyof typeof DEFAULT_SEARCH_FIELDS, checked: boolean): void;
  (e: 'submit-search'): void;
  (e: 'clear-search'): void;
  (e: 'focus-selected-flight'): void;
  (e: 'clear-all-filters'): void;
  (e: 'set-business-filter', key: keyof BusinessFilters, value: BusinessFilters[keyof BusinessFilters]): void;
}>();

const crumbs = computed(() => [
  { label: '工作台', href: props.pageUrl('dashboard') },
  { label: '航班监控' },
]);

const placeCountLabel = computed(() => `${props.visibleCount}/${props.totalCount}`);

const connectionTone = computed(() => {
  if (props.connectionStatusClass === 'online') return 'ok' as const;
  if (props.connectionStatusClass === 'offline') return 'danger' as const;
  return 'warn' as const;
});

const bannerTone = computed(() => {
  if (props.statusBanner?.tone === 'danger') return 'danger' as const;
  if (props.statusBanner?.tone === 'warning') return 'warn' as const;
  return 'act' as const;
});

const hasQuery = computed(() => props.searchQuery.trim().length > 0);

/** 可叠加的布尔谓词：开关式按钮（aria-pressed），开 = 仅该类航班。 */
const binaryFilters = computed(() => [
  { key: 'anomalyFilter' as const, id: 'anomalyFilter', countId: 'anomalyFilterCount', label: '异常', count: props.anomalyCount },
  { key: 'delayFilter' as const, id: 'delayFilter', countId: 'delayFilterCount', label: '延误', count: props.delayCount },
  { key: 'vipFilter' as const, id: 'vipFilter', countId: 'vipFilterCount', label: 'VIP', count: props.vipCount },
  { key: 'quickTurnFilter' as const, id: 'quickTurnFilter', countId: 'quickTurnFilterCount', label: '快速过站', count: props.quickTurnCount },
]);

const aircraftBodyOptions = [
  { value: 'all', label: '全部机型' },
  { value: 'wide', label: '宽体机' },
  { value: 'narrow', label: '窄体机' },
];

const commercialSignedOptions = [
  { value: 'all', label: '全部签约' },
  { value: 'yes', label: '已签约' },
  { value: 'no', label: '未签约' },
];

function toggleBinaryFilter(key: keyof BusinessFilters): void {
  emit('set-business-filter', key, props.filters[key] === 'only' ? 'all' : 'only');
}

function setAircraftBodyFilter(value: string): void {
  emit('set-business-filter', 'aircraftBodyFilter', value as BusinessFilters['aircraftBodyFilter']);
}

function setCommercialSignedFilter(value: string): void {
  emit('set-business-filter', 'commercialSignedFilter', value as BusinessFilters['commercialSignedFilter']);
}

function handleSearchInput(event: Event): void {
  emit('update:searchQuery', (event.target as HTMLInputElement).value);
}

function handleSearchFieldChange(event: Event, key: keyof SearchFields): void {
  emit('set-search-field', key, (event.target as HTMLInputElement).checked);
}
</script>

<template>
  <div class="flight-workbar" data-role="flight-workbar">
    <UiPlaceBar :crumbs="crumbs" :count-label="placeCountLabel">
      <template #meta>
        <UiPill id="connectionStatusPill" :tone="connectionTone">{{ connectionStatusText }}</UiPill>
        <div id="lastUpdated" class="fm-last-updated">{{ lastUpdatedLabel }}</div>
        <UiButton
          id="refreshBtn"
          variant="quiet"
          aria-label="刷新航班数据"
          :disabled="isRefreshing"
          @click="emit('refresh')"
        >
          {{ isRefreshing ? '刷新中' : '刷新' }}
        </UiButton>
      </template>
    </UiPlaceBar>

    <UiToolbar seek-label="筛选航班" solve-label="列表操作">
      <template #seek>
        <div class="fm-search" :class="{ 'has-value': hasQuery }">
          <span class="fm-search__icon" aria-hidden="true">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none">
              <circle cx="11" cy="11" r="8" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
          </span>
          <input
            id="searchInput"
            :value="searchQuery"
            type="text"
            placeholder="搜索航班号、目的地、状态等..."
            aria-label="搜索航班"
            aria-describedby="searchOptionsPanel"
            @input="handleSearchInput"
            @keydown.enter="emit('submit-search')"
          >
          <button
            id="clearSearchBtn"
            class="fm-search__clear"
            type="button"
            aria-label="清除搜索"
            :style="{ display: hasQuery ? 'inline-flex' : 'none' }"
            @click="emit('clear-search')"
          >
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none" aria-hidden="true">
              <path d="M3 3L9 9M9 3L3 9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
            </svg>
          </button>
        </div>

        <UiSegment label="视图切换">
          <button
            id="viewCardBtn"
            type="button"
            role="radio"
            aria-label="卡片视图"
            :aria-checked="viewMode === 'card'"
            @click="emit('update:viewMode', 'card')"
          >
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <rect x="2" y="2" width="12" height="12" rx="1.5" stroke="currentColor" stroke-width="1.5" />
              <line x1="2" y1="6" x2="14" y2="6" stroke="currentColor" stroke-width="1.5" />
              <line x1="6" y1="6" x2="6" y2="14" stroke="currentColor" stroke-width="1.5" />
            </svg>
          </button>
          <button
            id="viewTableBtn"
            type="button"
            role="radio"
            aria-label="表格视图"
            :aria-checked="viewMode === 'table'"
            @click="emit('update:viewMode', 'table')"
          >
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <rect x="2" y="2" width="12" height="12" rx="1.5" stroke="currentColor" stroke-width="1.5" />
              <line x1="2" y1="6" x2="14" y2="6" stroke="currentColor" stroke-width="1.5" />
              <line x1="2" y1="10" x2="14" y2="10" stroke="currentColor" stroke-width="1.5" />
            </svg>
          </button>
        </UiSegment>

        <UiButton
          v-for="filter in binaryFilters"
          :key="filter.key"
          :id="filter.id"
          :pressed="filters[filter.key] === 'only'"
          :aria-label="`仅显示${filter.label}航班`"
          @click="toggleBinaryFilter(filter.key)"
        >
          {{ filter.label }}<span :id="filter.countId" class="fm-filter-count">{{ filter.count }}</span>
        </UiButton>

        <UiSelect
          id="aircraftBodyFilter"
          label="按机型筛选"
          :model-value="filters.aircraftBodyFilter"
          :options="aircraftBodyOptions"
          @update:model-value="setAircraftBodyFilter"
        />

        <UiSelect
          id="commercialSignedFilter"
          label="按签约状态筛选"
          :model-value="filters.commercialSignedFilter"
          :options="commercialSignedOptions"
          @update:model-value="setCommercialSignedFilter"
        />

        <UiButton
          id="searchOptionsToggle"
          variant="quiet"
          :pressed="searchOptionsExpanded"
          aria-label="展开搜索字段选项"
          :aria-expanded="searchOptionsExpanded"
          aria-controls="searchOptionsPanel"
          @click="emit('toggle-search-options')"
        >
          字段
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none" aria-hidden="true" :style="{ transform: searchOptionsExpanded ? 'rotate(180deg)' : 'none' }">
            <path d="M3 4.5L6 7.5L9 4.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </UiButton>
      </template>

      <template #solve>
        <UiButton
          id="focusSelectedFlightBtn"
          variant="quiet"
          :disabled="!hasSelectedFlight"
          @click="emit('focus-selected-flight')"
        >
          定位选中航班
        </UiButton>
        <UiButton
          id="clearAllFiltersBtn"
          variant="quiet"
          :disabled="!hasActiveFilters"
          @click="emit('clear-all-filters')"
        >
          清空筛选
        </UiButton>
      </template>
    </UiToolbar>

    <div id="searchOptionsPanel" class="fm-search-options" :class="{ expanded: searchOptionsExpanded }">
      <div
        id="searchOptions"
        class="fm-search-options__group"
        role="group"
        aria-label="搜索字段"
      >
        <label v-for="option in SEARCH_FIELD_OPTIONS" :key="option.id" class="fm-search-field" :for="option.id">
          <input
            :id="option.id"
            :checked="searchFields[option.key]"
            type="checkbox"
            :aria-label="option.ariaLabel"
            @change="handleSearchFieldChange($event, option.key)"
          >
          <span>{{ option.label }}</span>
        </label>
      </div>
    </div>

    <UiBanner v-if="statusBanner" :tone="bannerTone" class="fm-status-banner">
      <span class="fm-status-banner__title">{{ statusBanner.title }}</span>
      <span class="fm-status-banner__description">{{ statusBanner.description }}</span>
    </UiBanner>
  </div>
</template>

<style scoped>
.fm-last-updated {
  font-size: var(--fs-label);
  color: var(--ink-subtle);
  margin: 0;
  font-variant-numeric: tabular-nums;
}

.fm-filter-count {
  margin-left: 2px;
  font-variant-numeric: tabular-nums;
  color: var(--ink-subtle);
  font-weight: var(--fw-regular);
}

.fm-status-banner__title {
  font-weight: var(--fw-semibold);
}

.fm-status-banner__description {
  font-weight: var(--fw-regular);
  color: inherit;
}

.flight-workbar :deep(.fm-status-banner) {
  margin: 0 var(--s3) var(--s2);
}
</style>
