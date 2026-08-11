<script setup lang="ts">
import type { Flight } from '@/types/bindings';

defineProps<{
  flight: Flight | null;
  detailItems: Array<{ label: string; value: string; field?: string; type?: string; readonly?: boolean }>;
}>();

const emit = defineEmits<{
  (e: 'edit-remark', flightId: string, field: string, value: string): void;
  (e: 'edit-field', flightId: string, field: string, type: string, value: string): void;
}>();
</script>

<template>
  <section class="detail-card info-grid-card">
    <div class="info-grid-compact">
      <div v-for="item in detailItems" :key="item.label" class="info-field">
        <label>{{ item.label }}</label>
        <span v-if="item.field === 'aircraft_check_remarks'" class="editable-remark" @click="emit('edit-remark', String(flight!.flight_id || ''), item.field, String(flight!.aircraft_check_remarks || ''))">{{ item.value }}</span>
        <span v-else-if="item.type === 'datetime-local' && !item.readonly" class="editable-remark" @click="emit('edit-field', String(flight!.flight_id || ''), item.field!, 'datetime-local', String((flight as unknown as Record<string, unknown>)[item.field!] || ''))">{{ item.value }}</span>
        <span v-else-if="item.type === 'datetime-local'">{{ item.value }}</span>
        <span v-else>{{ item.value }}</span>
      </div>
    </div>
  </section>
</template>
