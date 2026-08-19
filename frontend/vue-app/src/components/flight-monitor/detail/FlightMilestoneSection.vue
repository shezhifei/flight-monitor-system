<script setup lang="ts">
import { inject } from 'vue';
import FlightMilestoneTimeline from '../FlightMilestoneTimeline.vue';
import SvgIcon from '@/components/ui/SvgIcon.vue';
import UiButton from '../../ui/UiButton.vue';
import { flightBusinessCaseKey } from '../../../composables/useFlightBusinessCases';

defineProps<{
  flight: Record<string, unknown> | null;
}>();

const emit = defineEmits<{
  (e: 'create-business-case'): void;
}>();

const ctx = inject(flightBusinessCaseKey)!;
</script>

<template>
  <section class="detail-card ops-card">
    <div class="ops-card-header">
      <span class="ops-card-title">
        <SvgIcon src="/frontend/icons/bar_chart.svg" :size="16" style="vertical-align: -2px; margin-right: 6px;" />
        业务全景监控
      </span>
      <div class="business-insight-actions">
        <UiButton
          id="generateHistoryReportBtn"
          variant="ghost"
          :disabled="ctx.reportLoading.value || ctx.diagnosisLoading.value || ctx.journeyLoading.value"
          @click="ctx.runHistoryReport"
        >
          {{ ctx.reportLoading.value ? '生成中...' : '生成动态报表' }}
        </UiButton>
        <UiButton
          id="generateEventJourneyBtn"
          variant="ghost"
          :disabled="ctx.journeyLoading.value || ctx.diagnosisLoading.value || ctx.reportLoading.value"
          @click="ctx.runAiEventJourney"
        >
          {{ ctx.journeyLoading.value ? '生成中...' : '生成事件经过' }}
        </UiButton>
        <UiButton
          id="createEventBtn"
          variant="primary"
          @click="emit('create-business-case')"
        >
          + 新建事项
        </UiButton>
      </div>
    </div>
    <div class="ops-gantt-area">
      <FlightMilestoneTimeline v-if="flight" :flight="flight" />
    </div>
    <div class="ops-log-area">
      <slot name="event-log" />
    </div>
  </section>
</template>

<style scoped>
.ops-card-header {
  position: relative;
  z-index: 5;
  padding: 12px 16px;
  border-bottom: 1px solid var(--line);
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
  /* 避免被下方甘特 canvas 层叠挡住点击 */
  pointer-events: auto;
  background: var(--face-work);
}

.ops-card-title {
  font-weight: var(--fw-semibold);
  font-size: var(--fs-body);
  color: var(--ink);
  display: inline-flex;
  align-items: center;
}

.business-insight-actions {
  position: relative;
  z-index: 6;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  pointer-events: auto;
}

.business-insight-actions :deep(button) {
  position: relative;
  z-index: 1;
  pointer-events: auto;
  cursor: pointer;
}

.ops-gantt-area {
  position: relative;
  z-index: 0;
  overflow: hidden;
  /* 限制 echarts 内部 absolute 层不溢出盖住标题栏 */
  isolation: isolate;
}
</style>
