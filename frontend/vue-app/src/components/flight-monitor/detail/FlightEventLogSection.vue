<script setup lang="ts">
import { computed, inject } from 'vue';
import UiPill from '../../ui/UiPill.vue';
import UiSegment from '../../ui/UiSegment.vue';
import UiTimeline, { type UiTimelineItem } from '../../ui/UiTimeline.vue';
import { flightBusinessCaseKey } from '../../../composables/useFlightBusinessCases';
import {
  formatCaseTimeRange,
  getCaseDisplayName,
  getCaseStatusLabel,
  getCaseStatusTone,
  getCaseVisibilityLabel,
} from './businessCaseHelpers';

/**
 * 事件日志：这架航班上发生过的事，一件一行（信号面 §2.4）。
 *
 * 点的色是事态，右边两枚胶囊报范围与事态，整行是一颗谓词 —— 点开进详情。
 * 表单、回执、描述都在详情弹窗里，这里不再复述一遍（§4.4 不要重复芯片）。
 */
const ctx = inject(flightBusinessCaseKey)!;

interface CaseEntry extends UiTimelineItem {
  scope: string;
  status: string;
}

const entries = computed<CaseEntry[]>(() => ctx.filteredCases.value.map((c) => ({
  key: c.case_id,
  title: getCaseDisplayName(c),
  time: formatCaseTimeRange(c.created_at, c.finished_at),
  tone: getCaseStatusTone(c.status, ctx.caseStatusOptions.value),
  scope: getCaseVisibilityLabel(c),
  status: getCaseStatusLabel(c.status, ctx.caseStatusOptions.value),
})));
</script>

<template>
  <div class="log__bar">
    <span class="log__title">事件日志</span>
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
  <div class="log__scroll">
    <UiTimeline v-if="entries.length > 0" :items="entries">
      <template #item="{ item }">
        <button type="button" class="log__entry" @click="ctx.openCaseDetail(item.key)">
          <span class="log__main">
            <span class="log__name">{{ item.title }}</span>
            <span class="log__time">{{ item.time }}</span>
          </span>
          <span class="log__tags">
            <UiPill>{{ item.scope }}</UiPill>
            <UiPill :tone="item.tone">{{ item.status }}</UiPill>
          </span>
        </button>
      </template>
    </UiTimeline>
    <p v-else class="log__empty">
      无匹配的业务事项
    </p>
  </div>
</template>

<style scoped>
.log__bar {
  padding: 12px 16px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--s3);
  flex-wrap: wrap;
  border-bottom: 1px solid var(--line);
}

.log__title {
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.log__scroll {
  padding: 12px 16px 16px;
  overflow-y: auto;
}

/* 整行是一颗谓词：常态无底，交感洗一层淡墨，不位移 */
.log__entry {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s3);
  padding: 4px 6px;
  border: 0;
  border-radius: var(--r-cell);
  background: transparent;
  color: inherit;
  font-family: inherit;
  text-align: left;
  cursor: pointer;
}

.log__entry:hover {
  background: color-mix(in srgb, var(--ink) 8%, transparent);
}

.log__entry:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}

.log__main {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  flex: 1;
}

.log__name {
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
  line-height: 1.35;
  color: var(--ink);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.log__time {
  font-size: var(--fs-label);
  line-height: 1.3;
  color: var(--ink-muted);
  font-variant-numeric: tabular-nums;
}

.log__tags {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.log__empty {
  margin: 0;
  padding: 20px 0;
  text-align: center;
  font-size: var(--fs-body);
  color: var(--ink-muted);
}
</style>
