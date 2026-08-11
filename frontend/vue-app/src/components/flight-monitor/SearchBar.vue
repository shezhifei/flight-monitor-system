<script setup lang="ts">
import { computed } from 'vue';
import type { SearchFields } from '../../composables/useFlightData';
import type { FlightViewMode } from './helpers';
import { SEARCH_FIELD_OPTIONS } from './helpers';

const props = defineProps<{
  viewMode: FlightViewMode;
  searchQuery: string;
  searchFields: SearchFields;
  searchOptionsExpanded: boolean;
  businessFilterExpanded: boolean;
  visibleCount: number;
  totalCount: number;
  hasSelectedFlight: boolean;
  canClearFilters: boolean;
}>();

const emit = defineEmits<{
  (e: 'update:viewMode', value: FlightViewMode): void;
  (e: 'update:searchQuery', value: string): void;
  (e: 'toggle-search-options'): void;
  (e: 'toggle-business-filters'): void;
  (e: 'set-search-field', key: keyof SearchFields, checked: boolean): void;
  (e: 'submit-search'): void;
  (e: 'clear-search'): void;
  (e: 'focus-selected-flight'): void;
  (e: 'clear-all-filters'): void;
}>();

const hasQuery = computed(() => props.searchQuery.trim().length > 0);
const resultPillText = computed(() => `当前显示 ${props.visibleCount}/${props.totalCount}`);

function handleSearchInput(event: Event): void {
  emit('update:searchQuery', (event.target as HTMLInputElement).value);
}

function handleSearchFieldChange(event: Event, key: keyof SearchFields): void {
  emit('set-search-field', key, (event.target as HTMLInputElement).checked);
}
</script>

<template>
  <div>
    <div class="search-container">
      <div class="search-controls">
        <div
          class="view-switcher"
          :data-active="viewMode"
          role="group"
          aria-label="视图切换"
        >
          <div id="viewGlider" class="view-glider" />
          <button
            id="viewCardBtn"
            class="view-btn"
            :class="{ active: viewMode === 'card' }"
            type="button"
            aria-label="卡片视图"
            title="卡片视图"
            @click="emit('update:viewMode', 'card')"
          >
            <svg
              width="16"
              height="16"
              viewBox="0 0 16 16"
              fill="currentColor"
            >
              <path d="M2 3.5C2 2.67157 2.67157 2 3.5 2H12.5C13.3284 2 14 2.67157 14 3.5V5H2V3.5Z" opacity="0.5" />
              <rect
                x="2"
                y="2"
                width="12"
                height="12"
                rx="1.5"
                stroke="currentColor"
                stroke-width="1.5"
                fill="none"
              />
              <line
                x1="2"
                y1="6"
                x2="14"
                y2="6"
                stroke="currentColor"
                stroke-width="1.5"
              />
              <line
                x1="6"
                y1="6"
                x2="6"
                y2="14"
                stroke="currentColor"
                stroke-width="1.5"
              />
            </svg>
          </button>
          <button
            id="viewTableBtn"
            class="view-btn"
            :class="{ active: viewMode === 'table' }"
            type="button"
            aria-label="表格视图"
            title="表格视图"
            @click="emit('update:viewMode', 'table')"
          >
            <svg
              width="16"
              height="16"
              viewBox="0 0 16 16"
              fill="currentColor"
            >
              <path
                d="M2 3.5C2 2.67157 2.67157 2 3.5 2H12.5C13.3284 2 14 2.67157 14 3.5V5.5H2V3.5ZM2 6.5H14V12.5C14 13.3284 13.3284 14 12.5 14H3.5C2.67157 14 2 13.3284 2 12.5V6.5Z"
                opacity="0.5"
              />
              <path d="M2 3.5C2 2.67157 2.67157 2 3.5 2H12.5C13.3284 2 14 2.67157 14 3.5V5H2V3.5Z" />
              <rect
                x="2"
                y="6"
                width="12"
                height="8"
                rx="1"
              />
            </svg>
          </button>
        </div>

        <div class="search-input-wrapper">
          <span class="search-leading-icon" aria-hidden="true">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none">
              <circle
                cx="11"
                cy="11"
                r="8"
                stroke="currentColor"
                stroke-width="2.5"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
              <line
                x1="21"
                y1="21"
                x2="16.65"
                y2="16.65"
                stroke="currentColor"
                stroke-width="2.5"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
          </span>
          <input
            id="searchInput"
            :value="searchQuery"
            type="text"
            placeholder="搜索航班号、目的地、状态等..."
            aria-label="搜索航班"
            aria-describedby="searchOptions"
            @input="handleSearchInput"
            @keydown.enter="emit('submit-search')"
          >
          <button
            id="clearSearchBtn"
            class="search-clear-btn"
            type="button"
            aria-label="清除搜索"
            :style="{ display: hasQuery ? 'flex' : 'none' }"
            @click="emit('clear-search')"
          >
            <svg width="14" height="14" viewBox="0 0 14 14">
              <circle
                cx="7"
                cy="7"
                r="7"
                class="search-clear-disc"
              />
              <path
                d="M4.5 4.5L9.5 9.5M9.5 4.5L4.5 9.5"
                class="search-clear-x"
                stroke-width="1.5"
                stroke-linecap="round"
              />
            </svg>
          </button>
          <button
            id="searchOptionsToggle"
            class="search-options-toggle"
            :class="{ expanded: searchOptionsExpanded }"
            type="button"
            aria-label="展开搜索选项"
            :aria-expanded="searchOptionsExpanded"
            aria-controls="searchOptionsPanel"
            @click="emit('toggle-search-options')"
          >
            <svg
              width="12"
              height="12"
              viewBox="0 0 12 12"
              fill="none"
            >
              <path
                d="M3 4.5L6 7.5L9 4.5"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
          </button>
        </div>

        <button
          id="searchBtn"
          type="button"
          aria-label="执行搜索"
          @click="emit('submit-search')"
        >
          搜索
        </button>
        <button
          id="businessFilterToggle"
          class="search-filter-toggle"
          :class="{ active: businessFilterExpanded }"
          type="button"
          aria-label="展开筛选"
          :aria-expanded="businessFilterExpanded"
          aria-controls="businessFilterBar"
          title="筛选"
          @click="emit('toggle-business-filters')"
        >
          <svg
            width="14"
            height="14"
            viewBox="0 0 14 14"
            fill="none"
            aria-hidden="true"
          >
            <path
              d="M2 3C2 2.72386 2.22386 2.5 2.5 2.5H11.5C11.7761 2.5 12 2.72386 12 3C12 3.12224 11.9553 3.24027 11.8745 3.33195L8.5 7.16667V10.7C8.5 10.8894 8.39299 11.0626 8.22361 11.1472L6.22361 12.1472C5.89112 12.3134 5.5 12.0716 5.5 11.7V7.16667L2.12553 3.33195C2.04473 3.24027 2 3.12224 2 3Z"
              fill="currentColor"
            />
          </svg>
        </button>
      </div>

      <div class="flight-toolbar-actions">
        <span id="flightResultPill" class="flight-filter-status" aria-live="polite">{{ resultPillText }}</span>
        <button
          id="focusSelectedFlightBtn"
          type="button"
          class="flight-text-btn"
          :disabled="!hasSelectedFlight"
          @click="emit('focus-selected-flight')"
        >
          定位选中航班
        </button>
        <button
          id="clearAllFiltersBtn"
          type="button"
          class="flight-text-btn"
          :disabled="!canClearFilters"
          @click="emit('clear-all-filters')"
        >
          清空筛选
        </button>
      </div>
    </div>

    <div id="searchOptionsPanel" class="search-options-panel" :class="{ expanded: searchOptionsExpanded }">
      <div
        id="searchOptions"
        class="search-options"
        role="group"
        aria-label="搜索选项"
      >
        <div v-for="option in SEARCH_FIELD_OPTIONS" :key="option.id" class="search-option">
          <input
            :id="option.id"
            :checked="searchFields[option.key]"
            type="checkbox"
            :aria-label="option.ariaLabel"
            @change="handleSearchFieldChange($event, option.key)"
          >
          <label :for="option.id">{{ option.label }}</label>
        </div>
      </div>
    </div>
  </div>
</template>


