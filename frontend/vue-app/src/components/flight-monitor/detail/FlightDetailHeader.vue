<script setup lang="ts">
import { computed, inject } from 'vue';
import { flightBusinessCaseKey } from '../../../composables/useFlightBusinessCases';
import { ontologyCenterUrl } from '../../../shared/page-routes';

const props = defineProps<{
  flightId?: string | null;
  registration?: string | null;
}>();

const emit = defineEmits<{
  (e: 'close-drawer'): void;
}>();

const ctx = inject(flightBusinessCaseKey)!;

const ontologyHref = computed(() =>
  ontologyCenterUrl({
    flightId: props.flightId,
    registration: props.registration,
  }),
);

const canOpenOntology = computed(
  () => Boolean(String(props.flightId || '').trim() || String(props.registration || '').trim()),
);
</script>

<template>
  <div class="detail-panel-shell">
    <div class="panel-title detail-panel-title">
      <div class="detail-panel-heading">
        <span>航班详情</span>
        <button
          id="closeDetailDrawerBtn"
          type="button"
          class="detail-drawer-close"
          aria-label="关闭详情面板"
          @click="emit('close-drawer')"
        >
          收起
        </button>
      </div>
      <div
        id="editControls"
        class="edit-controls"
        role="group"
        aria-label="编辑控件"
      >
        <a
          v-if="canOpenOntology"
          id="openOntologyCenterBtn"
          class="btn btn-secondary"
          :href="ontologyHref"
          target="_blank"
          rel="noopener noreferrer"
          title="在本体资源台打开本航班/机号"
          data-testid="open-ontology-center"
        >
          本体资源
        </a>
        <button
          id="aiDiagnoseBtn"
          type="button"
          class="btn btn-ai"
          aria-label="AI诊断"
          :disabled="ctx.diagnosisLoading.value || ctx.journeyLoading.value || ctx.reportLoading.value"
          @click="ctx.runAiDiagnosis"
        >
          {{ ctx.diagnosisLoading.value ? '诊断中...' : 'AI诊断' }}
        </button>
      </div>
    </div>
  </div>
</template>
