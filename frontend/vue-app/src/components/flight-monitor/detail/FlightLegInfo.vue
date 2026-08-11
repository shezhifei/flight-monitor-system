<script setup lang="ts">
import type { Flight } from '@/types/bindings';
import LabelPill from '../LabelPill.vue';

defineProps<{
  flight: Flight | null;
  inboundLabels: string[];
  outboundLabels: string[];
}>();
</script>

<template>
  <section v-if="flight?.labels?.length || inboundLabels?.length || outboundLabels?.length" class="detail-card labels-card">
    <div class="labels-section">
      <div v-if="flight?.labels?.length" class="labels-group">
        <span class="labels-group-title">航班标签</span>
        <LabelPill :labels="flight.labels" />
      </div>
      <div v-if="inboundLabels?.length" class="labels-group">
        <span class="labels-group-title">进港</span>
        <LabelPill :labels="inboundLabels" />
      </div>
      <div v-if="outboundLabels?.length" class="labels-group">
        <span class="labels-group-title">出港</span>
        <LabelPill :labels="outboundLabels" />
      </div>
    </div>
  </section>
</template>
