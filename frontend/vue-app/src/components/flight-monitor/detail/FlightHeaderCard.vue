<script setup lang="ts">
import { computed } from 'vue';
import type { Flight } from '@/types/bindings';
import { deriveOperationDateLabel, getStatusTone } from '../helpers';
import UiPill from '../../ui/UiPill.vue';
import UiReadout from '../../ui/UiReadout.vue';
import UiReadoutStrip, { type ReadoutItem } from '../../ui/UiReadoutStrip.vue';

const props = defineProps<{
  flight: Flight;
  flightNumbers: { inbound?: string; outbound?: string; combined?: string } | null;
  route: { origin?: string; destination?: string; airport?: string; hasInbound?: boolean; hasOutbound?: boolean } | null;
}>();

const emit = defineEmits<{
  (e: 'edit-field', flightId: string, field: string, type: string, value: string): void;
}>();

const SECONDARY_KPI_FIELDS = [
  { field: 'cabin_door_open_time', label: '开客舱门' },
  { field: 'cleaning_start_time', label: '清洁开始' },
  { field: 'cleaning_end_time', label: '清洁结束' },
  { field: 'boarding_allowed_time', label: '允许登机' },
  { field: 'passenger_ready_time', label: '人齐' },
] as const;

const hasInbound = computed(() => Boolean(props.flightNumbers?.inbound));
const hasOutbound = computed(() => Boolean(props.flightNumbers?.outbound));
const operationDate = computed(() => deriveOperationDateLabel(props.flight));

function rawField(field: string): unknown {
  return (props.flight as unknown as Record<string, unknown>)[field];
}

function formatTime(value: unknown): string {
  if (!value) return '';
  const date = new Date(String(value));
  if (Number.isNaN(date.getTime())) return '';
  return date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', hour12: false });
}

/**
 * 一排读出，不是一排 KPI 卡（§3.2）：缺值一律交给 UiReadout 写「—」，
 * 所以这里空字符串照传，不自己补占位符。
 */
const primaryReadouts = computed<ReadoutItem[]>(() => {
  const items: ReadoutItem[] = [];
  if (hasInbound.value) items.push({ label: '落地时间', value: formatTime(props.flight.actual_arrival) });
  items.push({ label: '机位', value: props.flight.stand });
  items.push({ label: '登机口', value: props.flight.gate });
  if (hasOutbound.value) items.push({ label: '起飞时间', value: formatTime(props.flight.actual_departure) });
  items.push({ label: 'COBT', value: formatTime(props.flight.cobt_time) });
  return items;
});

function onKpiClick(field: string): void {
  emit('edit-field', String(props.flight.flight_id || ''), field, 'datetime-local', String(rawField(field) || ''));
}
</script>

<template>
  <!-- 详情盘第一行通栏：落位写在样式里，不写成模板上的 inline style。
       保障节点那一排是同一张工作面上的第二组读出，靠一根线分组（§3.2）。 -->
  <section class="detail-card flight-header-card">
    <div class="flight-header-card__top">
      <div class="flight-header-card__id">
        <div v-if="hasInbound" class="flight-header-card__no">
          {{ flightNumbers?.inbound }}
        </div>
        <div v-if="hasOutbound" class="flight-header-card__no">
          {{ flightNumbers?.outbound }}
        </div>
      </div>
      <div class="flight-header-card__what">
        <div class="flight-header-card__state">
          <UiPill :tone="getStatusTone(flight.status)">
            {{ flight.status || '计划中' }}
          </UiPill>
          <span class="flight-header-card__date">{{ operationDate }}</span>
        </div>
        <p class="flight-header-card__route">
          {{ route?.origin }}
          <span class="flight-header-card__arrow" aria-hidden="true">→</span>
          <template v-if="route?.hasInbound && route?.hasOutbound">
            {{ route?.airport }}
            <span class="flight-header-card__arrow" aria-hidden="true">→</span>
          </template>
          {{ route?.destination }}
        </p>
      </div>
      <UiReadoutStrip
        class="flight-header-card__now"
        label="当前时刻与位置"
        :items="primaryReadouts"
      />
    </div>

    <UiReadoutStrip class="flight-header-card__punch" label="保障节点">
      <!-- 分隔细线是读数条给直接子节点的，所以谓词包在格里：
           不去和 .ui-readouts > * + * 抢 padding-left / border-left。 -->
      <div v-for="item in SECONDARY_KPI_FIELDS" :key="item.field" class="flight-header-card__punch-cell">
        <button
          type="button"
          class="flight-header-card__punch-btn"
          :aria-label="`修改${item.label}`"
          @click="onKpiClick(item.field)"
        >
          <UiReadout :label="item.label" :value="formatTime(rawField(item.field))" />
        </button>
      </div>
    </UiReadoutStrip>
  </section>
</template>

<style scoped>
.flight-header-card {
  grid-column: 1 / -1;
}

.flight-header-card__top {
  display: flex;
  align-items: stretch;
}

/* 航班号是这一屏看的那个对象的名：页题一档，标识用等宽（§2.4 字阶） */
.flight-header-card__id {
  display: flex;
  flex-direction: column;
  justify-content: center;
  flex: none;
  padding: var(--s3) var(--s5);
  border-right: 1px solid var(--line);
}

.flight-header-card__no {
  font-family: var(--mono);
  font-size: var(--fs-page);
  font-weight: var(--fw-semibold);
  line-height: 1.2;
  white-space: nowrap;
  color: var(--ink);
}

.flight-header-card__what {
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: var(--s2);
  min-width: 0;
  padding: var(--s3) var(--s4);
  overflow: hidden;
}

.flight-header-card__state {
  display: flex;
  align-items: center;
  gap: var(--s3);
}

.flight-header-card__date {
  font-size: var(--fs-body);
  color: var(--ink-muted);
  white-space: nowrap;
}

.flight-header-card__route {
  margin: 0;
  color: var(--ink-subtle);
  font-size: var(--fs-body);
  font-weight: var(--fw-medium);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.flight-header-card__arrow {
  margin: 0 var(--s1);
  color: var(--ink-muted);
}

/* 读出靠右收边；名值对之间的细线由读数条自己给 */
.flight-header-card__now {
  margin-left: auto;
  align-self: center;
}

/* 第二组读出：要分组就加一根线，不再描第二道边、不做成第二张卡（§3.2 / §4.21） */
.flight-header-card__punch {
  border-top: 1px solid var(--line);
}

.flight-header-card__punch-cell {
  min-width: 0;
}

/* 点一下改这个时刻：谓词是静音一档，交感只淡墨一层（§2.6 / §4.2） */
.flight-header-card__punch-btn {
  padding: 0;
  border: none;
  border-radius: var(--r-cell);
  background: none;
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.flight-header-card__punch-btn:hover {
  background: color-mix(in srgb, var(--ink) 10%, transparent);
}

.flight-header-card__punch-btn:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
}
</style>
