<script setup lang="ts">
import { inject } from 'vue';
import { flightBusinessCaseKey } from '../../../composables/useFlightBusinessCases';

const emit = defineEmits<{
  (e: 'close-drawer'): void;
}>();

const ctx = inject(flightBusinessCaseKey)!;
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
