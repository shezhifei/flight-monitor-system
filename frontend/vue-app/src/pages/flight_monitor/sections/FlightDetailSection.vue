<script setup lang="ts">
import FlightDetail from '../../../components/flight-monitor/FlightDetail.vue';
import SkeletonDetail from '../../../components/ui/SkeletonDetail.vue';
import type { AirportContext } from '../../../composables/useFlightData';
import type { FlightResponse } from '@/types/bindings';

defineProps<{
  isInitialLoading: boolean;
  showFlightList: boolean;
  selectedFlight: FlightResponse | null;
  airportContext: AirportContext;
}>();

const emit = defineEmits<{
  (e: 'close-drawer'): void;
  (e: 'create-business-case'): void;
  (e: 'edit-remark', flightId: string, field: string, value: string): void;
  (e: 'edit-field', flightId: string, field: string, type: string, value: string): void;
}>();
</script>

<template>
  <div id="resizer" class="resizer" title="拖动调整面板大小" />
  <SkeletonDetail v-if="isInitialLoading" :visible="true" />
  <FlightDetail
    v-show="showFlightList"
    :flight="selectedFlight"
    :airport-context="airportContext"
    @close-drawer="emit('close-drawer')"
    @create-business-case="emit('create-business-case')"
    @edit-remark="(flightId, field, value) => emit('edit-remark', flightId, field, value)"
    @edit-field="(flightId, field, type, value) => emit('edit-field', flightId, field, type, value)"
  />
</template>
