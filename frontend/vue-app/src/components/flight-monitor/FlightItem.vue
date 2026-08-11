<script setup lang="ts">
import { computed } from 'vue';
import type { AirportContext } from '../../composables/useFlightData';
import type { Flight } from '@/types/bindings';
import { getAnomalyCountForFlight, hasVipMarker, type Flight as FlightModel } from '../../composables/useFlightData';
import {
  getAnomalyBadgeClass,
  getAnomalySeverity,
  getCommercialSignedLabel,
  getFlightDomId,
  getFlightEndpoints,
  getFlightNumbers,
  getFlightNumberStyleClass,
  getFlightTypeDisplay,
  getMissionDisplay,
  getStandGateDisplay,
  getStatusClassName,
  getTimeDisplay,
  getTimeToneClass,
  toggleRouteDisplayMode,
} from './helpers';

const props = defineProps<{
  flight: Flight;
  airportContext: AirportContext;
  selected: boolean;
  alertMode?: boolean;
  updateTone?: 'warn' | 'ok' | null;
}>();

const emit = defineEmits<{
  (e: 'select', flightId: string): void;
  (e: 'edit-field', flightId: string, field: string, type: 'text' | 'datetime-local', value: string): void;
}>();

const flightId = computed(() => getFlightDomId(props.flight));
const flightNumbers = computed(() => getFlightNumbers(props.flight));
const route = computed(() => getFlightEndpoints(props.flight, props.airportContext));
const arrivalTime = computed(() => getTimeDisplay(props.flight, 'arrival'));
const departureTime = computed(() => getTimeDisplay(props.flight, 'departure'));
const statusClass = computed(() => getStatusClassName(props.flight.status));
const anomalyCount = computed(() => getAnomalyCountForFlight(props.flight as unknown as FlightModel));
const anomalyBadgeClass = computed(() => getAnomalyBadgeClass(props.flight));
const anomalySeverityLabel = computed(() => {
  const severity = getAnomalySeverity(props.flight);
  return severity === 'high' ? '高优先级' : severity === 'medium' ? '中优先级' : '低优先级';
});

function selectFlight(): void {
  if (flightId.value) {
    emit('select', flightId.value);
  }
}

function handleKeydown(event: KeyboardEvent): void {
  if (event.key === 'Enter' || event.key === ' ') {
    event.preventDefault();
    selectFlight();
    return;
  }
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault();
    const current = event.currentTarget as HTMLElement | null;
    const container = current?.parentElement;
    if (!current || !container) {
      return;
    }
    const items = Array.from(container.querySelectorAll<HTMLElement>('.flight-item'));
    const index = items.indexOf(current);
    if (index < 0) {
      return;
    }
    const delta = event.key === 'ArrowDown' ? 1 : -1;
    const nextItem = items[Math.min(items.length - 1, Math.max(0, index + delta))];
    if (!nextItem) {
      return;
    }
    nextItem.focus();
    const nextFlightId = String(nextItem.dataset.flightId || '').trim();
    if (nextFlightId) {
      emit('select', nextFlightId);
    }
  }
}
</script>

<template>
  <article
    class="flight-item"
    :class="[
      {
        selected,
        'quick-turnaround': Boolean(flight.is_quick_turnaround),
        'flight-item--quick-turn': Boolean(flight.is_quick_turnaround),
        'alert-pool-card': alertMode,
        'flash-update': Boolean(updateTone),
        'flash-update--warn': updateTone === 'warn',
        'flash-update--ok': updateTone === 'ok',
      },
    ]"
    :data-flight-id="flightId"
    tabindex="0"
    role="button"
    :aria-pressed="selected"
    :aria-label="`查看航班 ${flightNumbers.combined} 详情`"
    @click="selectFlight"
    @keydown="handleKeydown"
  >
    <div class="flight-header" :class="{ 'has-both': Boolean(flightNumbers.inbound && flightNumbers.outbound) }">
      <span v-if="flightNumbers.inbound" class="flight-number inbound" :class="[getFlightNumberStyleClass(flight, 'inbound'), { vip: Boolean(flight.inbound_leg?.is_vip) }]">{{ flightNumbers.inbound }}</span>
      <span class="flight-status" :class="statusClass">{{ flight.status || '计划中' }}</span>
      <span v-if="flightNumbers.outbound && flightNumbers.outbound !== flightNumbers.inbound" class="flight-number outbound" :class="[getFlightNumberStyleClass(flight, 'outbound'), { vip: Boolean(flight.outbound_leg?.is_vip) }]">{{ flightNumbers.outbound }}</span>
      <span v-else-if="!flightNumbers.inbound" class="flight-number outbound" :class="[getFlightNumberStyleClass(flight, 'outbound'), { vip: Boolean(flight.outbound_leg?.is_vip) }]">{{ flightNumbers.combined }}</span>
    </div>

    <div class="flight-route" style="cursor: pointer; user-select: none;" @dblclick.prevent="toggleRouteDisplayMode">
      <span class="flight-origin">{{ route.origin }}</span>
      <span class="flight-arrow">→</span>
      <template v-if="route.hasInbound && route.hasOutbound">
        <span class="flight-airport">{{ route.airport }}</span>
        <span class="flight-arrow">→</span>
      </template>
      <span class="flight-destination">{{ route.destination }}</span>
    </div>

    <div class="flight-info">
      <span class="flight-type">{{ getFlightTypeDisplay(flight) }}</span>
      <span class="mission-type">{{ getMissionDisplay(flight) }}</span>
      <span v-if="anomalyCount > 0" class="anomaly-severity-badge" :class="anomalyBadgeClass">{{ anomalySeverityLabel }}</span>
    </div>

    <div class="flight-times">
      <div class="time-left" @contextmenu.prevent="emit('edit-field', flightId, 'scheduled_arrival', 'datetime-local', flight.scheduled_arrival || '')">
        <span class="text-tertiary">进港</span>
        <div :class="[getTimeToneClass(arrivalTime.tone), `cell-time--${arrivalTime.tone}`]">
          {{ arrivalTime.value }}
        </div>
      </div>
      <div class="time-right" @contextmenu.prevent="emit('edit-field', flightId, 'scheduled_departure', 'datetime-local', flight.scheduled_departure || '')">
        <span class="text-tertiary">离港</span>
        <div :class="[getTimeToneClass(departureTime.tone), `cell-time--${departureTime.tone}`]">
          {{ departureTime.value }}
        </div>
      </div>
    </div>

    <div class="flight-info" style="margin-top: 8px; margin-bottom: 0;">
      <span class="flight-type" @dblclick="emit('edit-field', flightId, 'stand', 'text', flight.stand || '')">
        {{ getStandGateDisplay(flight) }}
      </span>
      <span class="mission-type">{{ getCommercialSignedLabel(flight) }} · {{ hasVipMarker(flight as unknown as FlightModel) ? 'VIP' : '常规' }}</span>
    </div>
  </article>
</template>


