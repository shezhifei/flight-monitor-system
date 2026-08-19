<script setup lang="ts">
import { ref, watch } from 'vue';
import { useApi } from '@/composables/useApi';
import { useToast } from '@/composables/useToast';
import { createAiApi } from '@/lib/ai/api';
import {
  latestRecoverableCheckpoint,
  normalizeCheckpoint,
  type RunCheckpointItem,
} from '@/lib/ai/runResume';

const props = defineProps<{
  runId: string;
  jobId?: string;
}>();

const emit = defineEmits<{
  resumed: [];
  cancelled: [];
}>();

const aiApi = createAiApi(useApi());
const toast = useToast();

const checkpoints = ref<RunCheckpointItem[]>([]);
const busy = ref<'resume' | 'cancel' | null>(null);
const latest = ref<RunCheckpointItem | null>(null);

watch(
  () => [props.jobId, props.runId],
  async ([jobId, runId]) => {
    if (!jobId || !runId) {
      checkpoints.value = [];
      latest.value = null;
      return;
    }
    try {
      const rows = await aiApi.listAiRunCheckpoints(jobId, runId);
      checkpoints.value = rows.map(normalizeCheckpoint);
      latest.value = latestRecoverableCheckpoint(checkpoints.value);
    } catch {
      checkpoints.value = [];
      latest.value = null;
    }
  },
  { immediate: true },
);

async function handleResume() {
  busy.value = 'resume';
  try {
    await aiApi.resumeAiRun(props.runId, latest.value?.checkpointId || undefined);
    toast.showToast('success', `已请求从 checkpoint 恢复运行 ${props.runId}`);
    emit('resumed');
  } catch (error) {
    toast.showToast('error', error instanceof Error ? error.message : '恢复运行失败');
  } finally {
    busy.value = null;
  }
}

async function handleCancel() {
  if (!props.jobId) return;
  busy.value = 'cancel';
  try {
    await aiApi.cancelAiJob(props.jobId);
    toast.showToast('warning', `已请求取消任务 ${props.jobId}`);
    emit('cancelled');
  } catch (error) {
    toast.showToast('error', error instanceof Error ? error.message : '取消失败');
  } finally {
    busy.value = null;
  }
}
</script>

<template>
  <div class="ai-run-resume" data-testid="run-resume-bar">
    <div class="ai-run-resume-alert">
      <div class="ai-run-resume-title">运行已中断，可从最近的 checkpoint 恢复</div>
      <div class="ai-run-resume-meta">
        <span class="is-mono">run: {{ runId }}</span>
        <span v-if="latest" class="ai-run-resume-tag">
          最近 checkpoint: {{ latest.checkpointType }} #{{ latest.sequenceNo }}
        </span>
        <span v-else class="ai-run-resume-tag is-muted">未发现可恢复 checkpoint</span>
      </div>
    </div>
    <div class="ai-run-resume-actions">
      <button
        type="button"
        class="ai-run-resume-btn is-resume"
        :disabled="busy !== null"
        @click="handleResume"
      >
        {{ busy === 'resume' ? '恢复中…' : '恢复运行' }}
      </button>
      <button
        v-if="jobId"
        type="button"
        class="ai-run-resume-btn is-cancel"
        :disabled="busy !== null"
        @click="handleCancel"
      >
        {{ busy === 'cancel' ? '取消中…' : '取消运行' }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.ai-run-resume {
  border: 1px solid var(--warn);
  border-radius: var(--r-panel);
  background: var(--face-raised);
  padding: 12px 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.ai-run-resume-alert {
  background: var(--warn-soft);
  border-radius: var(--r-control);
  padding: 8px 10px;
}

.ai-run-resume-title {
  font-size: var(--fs-body);
  font-weight: var(--fw-medium);
  color: var(--warn);
}

.ai-run-resume-meta {
  margin-top: 4px;
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  align-items: center;
  font-size: var(--fs-label);
  color: var(--ink-subtle);
}

.ai-run-resume-meta .is-mono {
  font-family: var(--mono);
}

.ai-run-resume-tag {
  font-size: 11px;
  padding: 0 6px;
  border-radius: var(--r-cell);
  color: var(--act);
  background: var(--act-soft);
}

.ai-run-resume-tag.is-muted {
  color: var(--ink-muted);
  background: color-mix(in srgb, var(--ink) 8%, transparent);
}

.ai-run-resume-actions {
  display: flex;
  gap: 8px;
}

.ai-run-resume-btn {
  min-height: var(--h-sm);
  padding: 0 16px;
  border-radius: var(--r-control);
  border: none;
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  cursor: pointer;
}

.ai-run-resume-btn.is-resume {
  background: var(--act);
  color: var(--act-on);
}

.ai-run-resume-btn.is-cancel {
  background: var(--danger-soft);
  color: var(--danger);
}

.ai-run-resume-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
