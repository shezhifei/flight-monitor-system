<script setup lang="ts">
import { computed, provide } from 'vue';
import type { Flight } from '@/types/bindings';
import {
  hasVipMarker,
  type AirportContext,
} from '../../composables/useFlightData';
import type { Flight as FlightModel } from '../../composables/useFlightDataTypes';
import {
  flightBusinessCaseKey,
  useFlightBusinessCases,
} from '../../composables/useFlightBusinessCases';
import {
  deriveOperationDateLabel,
  getFlightEndpoints,
  getFlightNumbers,
  getMissionDisplay,
} from './helpers';
import { getLegFlightTypeLabel } from '../../composables/useFlightField';
import FlightHeaderCard from './detail/FlightHeaderCard.vue';
import FlightLegInfo from './detail/FlightLegInfo.vue';
import FlightMilestoneSection from './detail/FlightMilestoneSection.vue';
import FlightEditableFields from './detail/FlightEditableFields.vue';
import FlightEventLogSection from './detail/FlightEventLogSection.vue';
import FlightCaseDetailSection from './detail/FlightCaseDetailSection.vue';
import FlightAiResultCards from './detail/FlightAiResultCards.vue';
import FlightDetailHeader from './detail/FlightDetailHeader.vue';

const props = defineProps<{
  flight: Flight | null;
  airportContext: AirportContext;
}>();

const emit = defineEmits<{
  (e: 'close-drawer'): void;
  (e: 'create-business-case'): void;
  (e: 'edit-remark', flightId: string, field: string, value: string): void;
  (e: 'edit-field', flightId: string, field: string, type: string, value: string): void;
}>();

const flightRef = computed(() => props.flight as unknown as FlightModel);
const ctx = useFlightBusinessCases(flightRef);
provide(flightBusinessCaseKey, ctx);

const flightNumbers = computed(() => (props.flight ? getFlightNumbers(props.flight) : null));
const route = computed(() => (props.flight ? getFlightEndpoints(props.flight, props.airportContext, 'name') : null));
const operationDate = computed(() => (props.flight ? deriveOperationDateLabel(props.flight) : '—'));
const inboundLabels = computed(() => ((props.flight?.inbound_leg as unknown as { labels?: unknown[] })?.labels || []) as string[]);
const outboundLabels = computed(() => ((props.flight?.outbound_leg as unknown as { labels?: unknown[] })?.labels || []) as string[]);

function formatTimeValue(value: unknown): string {
  if (!value) return '—';
  const date = new Date(String(value));
  if (Number.isNaN(date.getTime())) return '—';
  return date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', hour12: false });
}

const detailItems = computed(() => {
  const flight = props.flight;
  if (!flight) {
    return [];
  }

  const model = flight as unknown as FlightModel;
  const raw = flight as unknown as Record<string, unknown>;
  const hasInbound = Boolean(flightNumbers.value?.inbound);
  const hasOutbound = Boolean(flightNumbers.value?.outbound);

  const items: Array<{ label: string; value: string; field?: string; type?: string; readonly?: boolean }> = [
    { label: '机型', value: flight.aircraft_type_detail || '—' },
    { label: '任务类型', value: getMissionDisplay(flight) },
    { label: '执行日期', value: operationDate.value },
    { label: '重要旅客', value: hasInbound || hasOutbound ? (hasVipMarker(model) ? '是' : '否') : '—' },
    { label: '快速过站', value: flight.is_quick_turnaround ? '是' : '否' },
  ];

  if (hasInbound) {
    items.push(
      { label: '进港航班号', value: flightNumbers.value?.inbound || '—' },
      { label: '进港类别', value: getLegFlightTypeLabel(model, 'inbound') || '—' },
    );
  }
  if (hasOutbound) {
    items.push(
      { label: '出港航班号', value: flightNumbers.value?.outbound || '—' },
      { label: '出港类别', value: getLegFlightTypeLabel(model, 'outbound') || '—' },
      { label: '结束登机', value: formatTimeValue(raw.end_boarding_time) },
      { label: '登机限制', value: flight.has_boarding_restriction ? '是' : '否' },
      { label: '撤轮挡', value: formatTimeValue(raw.off_blocks_time), field: 'off_blocks_time', type: 'datetime-local' },
    );
  }

  items.push(
    { label: '行李转盘', value: String(raw.baggage_carousel || '—') },
    { label: '复核机号', value: flight.aircraft_check_remarks || '点击编辑', field: 'aircraft_check_remarks' },
    { label: '登机机号', value: String(raw.boarding_id || '—') },
  );

  return items;
});
</script>

<template>
  <div
    class="flight-detail-panel"
    role="region"
    aria-label="航班详情"
    tabindex="0"
  >
    <FlightDetailHeader
      :flight-id="flight?.flight_id ?? null"
      :registration="flight?.registration ?? null"
      @close-drawer="emit('close-drawer')"
    />

    <div id="flightDetail" aria-live="polite">
      <div v-if="!flight" class="no-selection">
        请选择一个航班查看详细信息
      </div>

      <div v-else class="detail-dashboard">
        <FlightHeaderCard
          :flight="flight"
          :flight-numbers="flightNumbers"
          :route="route"
          @edit-field="(flightId, field, type, value) => emit('edit-field', flightId, field, type, value)"
        />

        <div class="detail-col-left">
          <FlightLegInfo
            :flight="flight"
            :inbound-labels="inboundLabels"
            :outbound-labels="outboundLabels"
          />
          <FlightEditableFields
            :flight="flight"
            :detail-items="detailItems"
            @edit-remark="(flightId, field, value) => emit('edit-remark', flightId, field, value)"
            @edit-field="(flightId, field, type, value) => emit('edit-field', flightId, field, type, value)"
          />
        </div>

        <div class="detail-col-right">
          <FlightMilestoneSection
            :flight="flight as unknown as Record<string, unknown>"
            @create-business-case="emit('create-business-case')"
          >
            <template #event-log>
              <FlightEventLogSection />
            </template>
          </FlightMilestoneSection>

          <FlightAiResultCards />
        </div>
      </div>
    </div>

    <!-- Body-level modal (Teleport inside component); must stay under provide() tree -->
    <FlightCaseDetailSection />
  </div>
</template>
