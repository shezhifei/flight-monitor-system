<script setup lang="ts">
import { computed } from 'vue';
import type { Flight } from '@/types/bindings';
import { deriveOperationDateLabel, getStatusClassName } from '../helpers';

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
  if (!value) return '--';
  const date = new Date(String(value));
  if (Number.isNaN(date.getTime())) return '--';
  return date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', hour12: false });
}

function onKpiClick(field: string): void {
  emit('edit-field', String(props.flight.flight_id || ''), field, 'datetime-local', String(rawField(field) || ''));
}
</script>

<template>
  <section class="detail-card header-combined-card" style="grid-column: 1 / -1; margin-bottom: 0;">
    <div class="header-left-group" style="flex: 1; min-width: 0;">
      <div class="header-id-section">
        <div v-if="hasInbound" class="header-flight-no">{{ flightNumbers?.inbound }}</div>
        <div v-if="hasOutbound" class="header-flight-no">{{ flightNumbers?.outbound }}</div>
      </div>
      <div class="header-divider" />
      <div class="header-info-section">
        <div class="header-status-row">
          <span class="flight-status" :class="getStatusClassName(flight.status)">{{ flight.status || '计划中' }}</span>
          <span class="header-op-date">{{ operationDate }}</span>
        </div>
        <div class="header-route-line">
          {{ route?.origin }}
          <span class="detail-route-arrow">→</span>
          <template v-if="route?.hasInbound && route?.hasOutbound">
            {{ route?.airport }}
            <span class="detail-route-arrow">→</span>
          </template>
          {{ route?.destination }}
        </div>
      </div>
    </div>
    <div class="header-right-group">
      <div v-if="hasInbound" class="header-kpi-item">
        <span class="header-kpi-label">落地时间</span>
        <span class="header-kpi-value">{{ formatTime(flight.actual_arrival) }}</span>
      </div>
      <div class="header-kpi-item">
        <span class="header-kpi-label">机位</span>
        <span class="header-kpi-value">{{ flight.stand || '--' }}</span>
      </div>
      <div class="header-kpi-item">
        <span class="header-kpi-label">登机口</span>
        <span class="header-kpi-value">{{ flight.gate || '--' }}</span>
      </div>
      <div v-if="hasOutbound" class="header-kpi-item">
        <span class="header-kpi-label">起飞时间</span>
        <span class="header-kpi-value">{{ formatTime(flight.actual_departure) }}</span>
      </div>
      <div class="header-kpi-item">
        <span class="header-kpi-label">COBT</span>
        <span class="header-kpi-value">{{ formatTime(flight.cobt_time) }}</span>
      </div>
    </div>
  </section>

  <section class="detail-card secondary-kpi-strip" style="grid-column: 1 / -1; margin-bottom: 0;">
    <div v-for="item in SECONDARY_KPI_FIELDS" :key="item.field" class="secondary-kpi-item">
      <span class="secondary-kpi-label">{{ item.label }}</span>
      <span
        class="secondary-kpi-value clickable-action"
        @click="onKpiClick(item.field)"
      >{{ formatTime(rawField(item.field)) }}</span>
    </div>
  </section>
</template>

<style scoped>
.header-id-section {
  padding: 0 32px;
  display: flex;
  flex-direction: column;
  justify-content: center;
}

.header-flight-no {
  font-family: var(--mono);
  font-size: 28px;
  font-weight: 600;
  line-height: 1.1;
  letter-spacing: -0.5px;
  white-space: nowrap;
  color: var(--ink);
}

.header-divider {
  width: 1px;
  background-color: var(--line);
  margin: 16px 0;
}

.header-info-section {
  padding: 16px 32px;
  display: flex;
  flex-direction: column;
  justify-content: center;
  overflow: hidden;
}

.header-status-row {
  margin-bottom: 8px;
  display: flex;
  align-items: center;
}

.header-op-date {
  margin-left: 12px;
  font-size: var(--fs-body);
  color: var(--ink-muted);
  white-space: nowrap;
}

.header-route-line {
  padding-top: 8px;
  border-top: 1px solid var(--line);
  color: var(--ink-subtle);
  font-size: var(--fs-body);
  font-weight: var(--fw-medium);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.detail-route-arrow {
  margin: 0 4px;
  color: var(--ink-muted);
}

.clickable-action {
  cursor: pointer;
}

.clickable-action:hover {
  color: var(--act);
}
</style>
