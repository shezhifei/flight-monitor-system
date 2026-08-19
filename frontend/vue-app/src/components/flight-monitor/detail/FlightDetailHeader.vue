<script setup lang="ts">
import { computed, inject } from 'vue';
import UiButton from '../../ui/UiButton.vue';
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
          class="detail-link-btn"
          :href="ontologyHref"
          target="_blank"
          rel="noopener noreferrer"
          title="在本体资源台打开本航班/机号"
          data-testid="open-ontology-center"
        >
          本体资源
        </a>
        <UiButton
          id="aiDiagnoseBtn"
          variant="ghost"
          aria-label="AI诊断"
          :disabled="ctx.diagnosisLoading.value || ctx.journeyLoading.value || ctx.reportLoading.value"
          @click="ctx.runAiDiagnosis"
        >
          {{ ctx.diagnosisLoading.value ? '诊断中...' : 'AI诊断' }}
        </UiButton>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* 链接型按钮：与 UiButton ghost 同壳，仅壳同源、标签仍为 a */
.detail-link-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  height: var(--h-sm);
  padding: 0 12px;
  border-radius: var(--r-control);
  border: 1px solid var(--line-strong);
  background: transparent;
  color: var(--ink);
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  text-decoration: none;
}

.detail-link-btn:hover {
  border-color: var(--ink-muted);
}

.detail-link-btn:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
}
</style>
