<script setup lang="ts">
import type { BusinessFilters } from '../../composables/useFlightData';

defineProps<{
  filters: BusinessFilters;
  expanded: boolean;
  anomalyCount: number;
  delayCount: number;
  vipCount: number;
  quickTurnCount: number;
  resetVisible: boolean;
}>();

const emit = defineEmits<{
  (e: 'set-filter', key: keyof BusinessFilters, value: BusinessFilters[keyof BusinessFilters]): void;
  (e: 'reset'): void;
}>();

function handleSelectChange(event: Event, key: keyof BusinessFilters): void {
  emit('set-filter', key, (event.target as HTMLSelectElement).value as BusinessFilters[keyof BusinessFilters]);
}
</script>

<template>
  <div
    id="businessFilterBar"
    class="business-filter-bar"
    :class="{ expanded }"
    role="group"
    aria-label="业务筛选"
  >
    <div class="business-filter-grid">
      <div class="business-filter-item">
        <label for="anomalyFilter">异常 <span id="anomalyFilterCount" class="business-filter-count">{{ anomalyCount }}</span></label>
        <select
          id="anomalyFilter"
          :value="filters.anomalyFilter"
          aria-label="按异常筛选"
          @change="handleSelectChange($event, 'anomalyFilter')"
        >
          <option value="all">
            全部
          </option>
          <option value="only">
            仅异常
          </option>
        </select>
      </div>

      <div class="business-filter-item">
        <label for="delayFilter">延误 <span id="delayFilterCount" class="business-filter-count">{{ delayCount }}</span></label>
        <select
          id="delayFilter"
          :value="filters.delayFilter"
          aria-label="按延误筛选"
          @change="handleSelectChange($event, 'delayFilter')"
        >
          <option value="all">
            全部
          </option>
          <option value="only">
            仅延误
          </option>
        </select>
      </div>

      <div class="business-filter-item">
        <label for="vipFilter">VIP <span id="vipFilterCount" class="business-filter-count">{{ vipCount }}</span></label>
        <select
          id="vipFilter"
          :value="filters.vipFilter"
          aria-label="按VIP筛选"
          @change="handleSelectChange($event, 'vipFilter')"
        >
          <option value="all">
            全部
          </option>
          <option value="only">
            仅VIP
          </option>
        </select>
      </div>

      <div class="business-filter-item">
        <label for="aircraftBodyFilter">机型</label>
        <select
          id="aircraftBodyFilter"
          :value="filters.aircraftBodyFilter"
          aria-label="按机型筛选"
          @change="handleSelectChange($event, 'aircraftBodyFilter')"
        >
          <option value="all">
            全部
          </option>
          <option value="wide">
            宽体机
          </option>
          <option value="narrow">
            窄体机
          </option>
        </select>
      </div>

      <div class="business-filter-item">
        <label for="commercialSignedFilter">签约</label>
        <select
          id="commercialSignedFilter"
          :value="filters.commercialSignedFilter"
          aria-label="按签约状态筛选"
          @change="handleSelectChange($event, 'commercialSignedFilter')"
        >
          <option value="all">
            全部
          </option>
          <option value="yes">
            已签约
          </option>
          <option value="no">
            未签约
          </option>
        </select>
      </div>

      <div class="business-filter-item">
        <label for="quickTurnFilter">快速过站 <span id="quickTurnFilterCount" class="business-filter-count">{{ quickTurnCount }}</span></label>
        <select
          id="quickTurnFilter"
          :value="filters.quickTurnFilter"
          aria-label="按快速过站筛选"
          @change="handleSelectChange($event, 'quickTurnFilter')"
        >
          <option value="all">
            全部
          </option>
          <option value="only">
            仅快速过站
          </option>
        </select>
      </div>
    </div>

    <div class="business-filter-meta">
      <button
        id="resetBusinessFiltersBtn"
        class="business-filter-reset-btn"
        :class="{ 'is-hidden': !resetVisible }"
        type="button"
        aria-label="恢复默认筛选"
        @click="emit('reset')"
      >
        恢复默认
      </button>
    </div>
  </div>
</template>
