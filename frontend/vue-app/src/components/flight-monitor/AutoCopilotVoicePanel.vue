<template>
  <section class="auto-copilot" :class="{ 'auto-copilot--open': isOpen }" aria-label="Auto Copilot 语音业务事项">
    <button
      v-if="!isOpen"
      type="button"
      class="auto-copilot__launcher"
      @click="isOpen = true"
    >
      <span class="auto-copilot__pulse" aria-hidden="true" />
      <span>Auto Copilot</span>
    </button>

    <div v-else class="auto-copilot__panel">
      <header class="auto-copilot__header">
        <div>
          <h2>Auto Copilot</h2>
          <p>{{ statusLabel }}</p>
        </div>
        <button
          type="button"
          class="auto-copilot__icon-btn"
          aria-label="关闭"
          @click="closePanel"
        >
          x
        </button>
      </header>

      <div class="auto-copilot__controls">
        <button
          type="button"
          class="auto-copilot__primary"
          :disabled="isBusy"
          @click="listening ? stopListening() : startListening()"
        >
          {{ listening ? '停止监听' : '启用麦克风' }}
        </button>
        <button
          type="button"
          class="auto-copilot__secondary"
          :disabled="Boolean(draftButtonDisabledReason)"
          :title="draftHint || '生成当前语音片段草稿'"
          @click="generateDraft"
        >
          {{ draftLoading ? '生成中...' : '生成草稿' }}
        </button>
        <button
          type="button"
          class="auto-copilot__diagnose"
          :disabled="!canDraft || draftLoading || commitLoading"
          @click="() => diagnoseCurrentTranscript()"
        >
          诊断模型输出
        </button>
        <button
          type="button"
          class="auto-copilot__diagnose"
          :disabled="draftLoading || commitLoading || failedBatchesLoading || metricsLoading"
          @click="loadOperationalMetrics"
        >
          运行指标
        </button>
      </div>

      <div v-if="errorMessage" class="auto-copilot__alert">
        {{ errorMessage }}
      </div>
      <div v-if="diagnostic" class="auto-copilot__diagnostic">
        <div class="auto-copilot__diagnostic-head">
          <span>模型诊断</span>
          <button type="button" @click="diagnostic = null">
            关闭
          </button>
        </div>
        <div class="auto-copilot__diagnostic-grid">
          <span>状态</span>
          <strong>{{ diagnostic.ok ? '通过' : '失败' }}</strong>
          <span>阶段</span>
          <strong>{{ diagnostic.error_stage || '-' }}</strong>
          <span>候选事项</span>
          <strong>{{ diagnostic.candidate_case_types.map((item) => item.name || item.code).join('、') || '-' }}</strong>
        </div>
        <pre v-if="diagnostic.error_message">{{ diagnostic.error_message }}</pre>
        <details v-if="diagnostic.llm_raw_preview">
          <summary>模型原始输出</summary>
          <pre>{{ diagnostic.llm_raw_preview }}</pre>
        </details>
      </div>
      <div v-if="resultMessage" class="auto-copilot__success">
        {{ resultMessage }}
      </div>

      <div v-if="operationalMetrics" class="auto-copilot__metrics">
        <div class="auto-copilot__diagnostic-head">
          <span>运行指标</span>
          <button type="button" @click="operationalMetrics = null">
            关闭
          </button>
        </div>
        <div class="auto-copilot__metric-grid">
          <div>
            <span>草稿批次</span>
            <strong>{{ operationalMetrics.batch_status.draft }}</strong>
          </div>
          <div>
            <span>已创建</span>
            <strong>{{ operationalMetrics.batch_status.committed }}</strong>
          </div>
          <div>
            <span>创建失败</span>
            <strong>{{ operationalMetrics.batch_status.failed }}</strong>
          </div>
          <div>
            <span>派发失败</span>
            <strong>{{ operationalMetrics.workflow_dispatch.failed }}</strong>
          </div>
          <div>
            <span>待自动重试</span>
            <strong>{{ operationalMetrics.workflow_dispatch.retry_due }}</strong>
          </div>
          <div>
            <span>重试耗尽</span>
            <strong>{{ operationalMetrics.workflow_dispatch.retry_exhausted }}</strong>
          </div>
        </div>
        <div v-if="operationalMetrics.recent_errors.length" class="auto-copilot__recent-errors">
          <div
            v-for="item in operationalMetrics.recent_errors"
            :key="item.batch_id"
            class="auto-copilot__recent-error"
          >
            <strong>{{ item.stage || item.workflow_dispatch_status || item.status }}</strong>
            <small>{{ item.batch_id }} · {{ item.updated_at }}</small>
            <span>{{ item.message || '无错误详情' }}</span>
          </div>
        </div>
      </div>

      <div v-if="failedBatches.length" class="auto-copilot__failed-batches">
        <div class="auto-copilot__diagnostic-head">
          <span>失败批次</span>
          <button type="button" @click="failedBatches = []">
            关闭
          </button>
        </div>
        <div
          v-for="batch in failedBatches"
          :key="batch.batch_id"
          class="auto-copilot__failed-item"
        >
          <div>
            <strong>{{ batch.transcript_summary || batch.batch_id }}</strong>
            <small>{{ batch.batch_id }} · {{ batch.updated_at }}</small>
            <small v-if="batch.committed_case_ids.length">已创建事项 {{ batch.committed_case_ids.join(', ') }}</small>
            <small v-if="formatCommitError(batch.commit_error)">{{ formatCommitError(batch.commit_error) }}</small>
            <small v-if="batch.workflow_dispatch_status === 'failed'">
              流程派发失败 {{ formatCommitError(batch.workflow_dispatch_error) }}
            </small>
            <small v-if="batch.workflow_dispatch_status === 'failed'">
              已重试 {{ batch.workflow_dispatch_attempts }} 次<span v-if="batch.workflow_dispatch_next_retry_at"> · 下次自动重试 {{ batch.workflow_dispatch_next_retry_at }}</span>
            </small>
          </div>
          <div class="auto-copilot__failed-actions">
            <button
              v-if="batch.workflow_dispatch_status === 'failed'"
              type="button"
              :disabled="failedBatchesLoading"
              @click="retryWorkflowDispatch(batch.batch_id)"
            >
              重试派发
            </button>
            <button
              v-if="batch.status === 'failed'"
              type="button"
              :disabled="failedBatchesLoading || batch.committed_case_ids.length > 0"
              @click="resolveBatch(batch.batch_id, 'reset_to_draft')"
            >
              重置草稿
            </button>
            <button
              v-if="batch.status === 'failed'"
              type="button"
              :disabled="failedBatchesLoading"
              @click="resolveBatch(batch.batch_id, 'mark_resolved')"
            >
              已处理
            </button>
          </div>
        </div>
      </div>

      <div class="auto-copilot__transcript">
        <div class="auto-copilot__section-title">
          <span>识别文本</span>
          <button type="button" :disabled="listening || draftLoading || commitLoading" @click="clearSession">
            清空
          </button>
        </div>
        <div class="auto-copilot__session-meta">
          <span class="auto-copilot__session-state" :class="`is-${utterance.status.value}`">
            {{ utteranceStatusLabel }}
          </span>
          <small v-if="draftHint">{{ draftHint }}</small>
        </div>
        <textarea
          v-model="manualTranscript"
          rows="4"
          placeholder="开启麦克风后自动填充，也可以粘贴 ASR 文本后生成草稿。"
          :disabled="listening || draftLoading || commitLoading"
        />
        <p v-if="transcriptNeedsConfirmation" class="auto-copilot__partial auto-copilot__partial--warning">
          文本包含低置信收尾片段，请人工确认航班号、座位号等关键信息。
        </p>
        <p v-if="partialTranscript" class="auto-copilot__partial">
          {{ partialTranscript }}
        </p>
      </div>

      <div v-if="draft" class="auto-copilot__draft">
        <div class="auto-copilot__summary">
          <span>摘要</span>
          <strong>{{ draft.summary }}</strong>
        </div>

        <div class="auto-copilot__table-wrap">
          <table class="auto-copilot__table">
            <thead>
              <tr>
                <th>事项</th>
                <th>航班</th>
                <th>航段</th>
                <th>额外信息</th>
                <th>置信度</th>
                <th />
              </tr>
            </thead>
            <tbody>
              <tr v-for="action in editableActions" :key="action.action_id" :class="{ 'needs-review': action.needs_review }">
                <td>
                  <select v-model="action.case_type">
                    <option
                      v-for="type in normalizedCaseTypes"
                      :key="type.code"
                      :value="type.code"
                    >
                      {{ type.name }}
                    </option>
                    <option v-if="!hasCaseType(action.case_type)" :value="action.case_type">
                      {{ action.case_type_name || action.case_type }}
                    </option>
                  </select>
                </td>
                <td>
                  <select v-model="action.selected_flight_key" @change="applySelectedFlight(action)">
                    <option value="">
                      请选择
                    </option>
                    <option
                      v-for="candidate in action.candidates"
                      :key="`${candidate.flight_id}:${candidate.leg_type}`"
                      :value="`${candidate.flight_id}:${candidate.flight_no}:${candidate.leg_type}`"
                    >
                      {{ candidate.flight_no }} · {{ scoreLabel(candidate.score) }}
                    </option>
                  </select>
                  <small v-if="action.review_reason">{{ action.review_reason }}</small>
                </td>
                <td>{{ action.bound_leg_type || action.leg_type_hint || 'outbound' }}</td>
                <td>
                  <input v-model="action.remarks" placeholder="额外信息" style="margin-bottom: 6px;">

                  <!-- 动态渲染 extra_info fields -->
                  <div v-if="Object.keys(getExtraInfoFields(action.case_type)).length > 0" class="dynamic-fields-container">
                    <div
                      v-for="(fieldCfg, fieldName) in getExtraInfoFields(action.case_type)"
                      :key="fieldName"
                      class="dynamic-field-item"
                    >
                      <span class="field-label">
                        {{ fieldCfg.label || fieldName }}
                        <span v-if="fieldCfg.required" class="required-star">*</span>
                      </span>

                      <select
                        v-if="fieldCfg.enum_values && fieldCfg.enum_values.length > 0"
                        v-model="action.fields[fieldName]"
                      >
                        <option value="">
                          请选择
                        </option>
                        <option v-for="val in fieldCfg.enum_values" :key="val" :value="val">
                          {{ val }}
                        </option>
                      </select>

                      <input
                        v-else
                        v-model="action.fields[fieldName]"
                        type="text"
                        :placeholder="fieldCfg.examples?.[0] || '请输入'"
                      >
                    </div>
                  </div>
                </td>
                <td>{{ Math.round(action.confidence * 100) }}%</td>
                <td>
                  <button
                    type="button"
                    class="auto-copilot__remove"
                    aria-label="删除草稿"
                    @click="removeAction(action.action_id)"
                  >
                    x
                  </button>
                </td>
              </tr>
            </tbody>
          </table>
        </div>

        <button
          type="button"
          class="auto-copilot__commit"
          :disabled="!canCommit || commitLoading"
          @click="commitDraft"
        >
          {{ commitLoading ? '提交中...' : `确认创建 ${editableActions.length} 条事项` }}
        </button>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onUnmounted, ref } from 'vue';
import { useAiBusinessCaseCopilot, type CopilotBatchStatusResponse, type CopilotDraftAction, type CopilotDraftDiagnosticResponse, type CopilotDraftResponse, type CopilotOperationalMetrics } from '../../composables/useAiBusinessCaseCopilot';
import { useRealtimeAudioSession } from '../../composables/useRealtimeAudioSession';
import { DEFAULT_UTTERANCE_FINAL_GRACE_MS, useUtteranceSession, type UtteranceSessionStatus } from '../../composables/useUtteranceSession';
import type { BusinessCaseTypeDefinition, BusinessCaseAiExtractionConfig } from '../../types/backend';
import {
  resolveCaseTypeConfig,
  resolveExtraInfoFields,
  type CaseTypeResolvedConfig,
  type CaseFieldConfig,
} from './helpers';

const props = defineProps<{
  entityId?: string;
  selectedFlightId?: string | null;
  selectedFlightNo?: string | null;
  businessCaseTypes?: BusinessCaseTypeDefinition[];
}>();

const emit = defineEmits<{
  (event: 'created', payload: { caseIds: string[]; notificationGroupCount: number }): void;
}>();

interface EditableDraftAction extends CopilotDraftAction {
  selected_flight_key: string;
  selected_flight_id: string;
  selected_flight_no: string;
  bound_leg_type: string;
  fields: Record<string, unknown>;
}

const DEFAULT_ENTITY_ID = 'flight-monitor-copilot';
const TARGET_SAMPLE_RATE = 16000;
const SILENCE_RMS_THRESHOLD = 0.012;
const MAX_SILENT_CHUNKS_BEFORE_END = 24;

const copilot = useAiBusinessCaseCopilot();
const isOpen = ref(false);
const listening = ref(false);
const draftLoading = ref(false);
const commitLoading = ref(false);
const failedBatchesLoading = ref(false);
const metricsLoading = ref(false);
const errorMessage = ref('');
const resultMessage = ref('');
const diagnostic = ref<CopilotDraftDiagnosticResponse | null>(null);
const failedBatches = ref<CopilotBatchStatusResponse[]>([]);
const operationalMetrics = ref<CopilotOperationalMetrics | null>(null);
const draft = ref<CopilotDraftResponse | null>(null);
const editableActions = ref<EditableDraftAction[]>([]);
const draftCommitIdempotencyKey = ref<string | null>(null);
let mediaStream: MediaStream | null = null;
let audioContext: AudioContext | null = null;
let sourceNode: MediaStreamAudioSourceNode | null = null;
let processorNode: ScriptProcessorNode | null = null;
let silentChunks = 0;

const entityId = computed(() => props.entityId?.trim() || DEFAULT_ENTITY_ID);
const isBusy = computed(() => draftLoading.value || commitLoading.value);

function readPositiveNumber(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : null;
}

function resolveConfiguredFinalGraceMs(): number {
  for (const type of props.businessCaseTypes || []) {
    const configured = readPositiveNumber(type.case_properties?.auto_copilot?.utterance_final_grace_ms)
      ?? readPositiveNumber(type.ai_extraction_config?.utterance_session?.final_grace_ms);
    if (configured !== null) {
      return configured;
    }
  }
  return DEFAULT_UTTERANCE_FINAL_GRACE_MS;
}

const utterance = useUtteranceSession({
  finalGraceMs: resolveConfiguredFinalGraceMs,
  onSegmentReady: () => {
    void generateDraft();
  },
});

const manualTranscript = computed({
  get: () => utterance.transcript.value,
  set: (value: string) => utterance.updateTranscript(value),
});
const partialTranscript = computed(() => utterance.partial.value);
const canDraft = computed(() => utterance.canFlush.value);
const transcriptNeedsConfirmation = computed(() => utterance.transcriptNeedsConfirmation.value);
function getCaseTypeConfig(caseTypeCode: string): CaseTypeResolvedConfig | null {
  const found = normalizedCaseTypes.value.find((t) => t.code === caseTypeCode);
  if (!found) return null;
  return resolveCaseTypeConfig(found.case_properties, found.ai_extraction_config);
}

function getExtraInfoFields(caseTypeCode: string): Record<string, CaseFieldConfig> {
  const found = normalizedCaseTypes.value.find((t) => t.code === caseTypeCode);
  if (!found) return {};
  return resolveExtraInfoFields(found.case_properties, found.ai_extraction_config);
}

const canCommit = computed(() => {
  if (editableActions.value.length === 0) {
    return false;
  }
  return editableActions.value.every((action) => {
    if (!action.case_type.trim() || !action.selected_flight_id.trim() || !action.selected_flight_no.trim()) {
      return false;
    }
    const hasType = normalizedCaseTypes.value.some((t) => t.code === action.case_type);
    if (!hasType) {
      return false;
    }
    const cfg = getCaseTypeConfig(action.case_type);
    if (cfg) {
      if (cfg.fields) {
        for (const [fieldName, fieldCfg] of Object.entries(cfg.fields)) {
          if (fieldCfg.required) {
            const val = action.fields?.[fieldName];
            if (val === undefined || val === null || String(val).trim() === '') {
              return false;
            }
          }
        }
      }
      const boundLeg = action.bound_leg_type || 'outbound';
      if (cfg.leg_binding && Array.isArray(cfg.leg_binding.allowed) && cfg.leg_binding.allowed.length > 0) {
        if (!cfg.leg_binding.allowed.includes(boundLeg)) {
          return false;
        }
      }
    }
    return true;
  });
});

const normalizedCaseTypes = computed(() => {
  return (props.businessCaseTypes || [])
    .filter((type) => {
      const code = String(type.code || '').trim();
      if (!code) return false;
      const aiCfg = type.ai_extraction_config;
      return Boolean(aiCfg && typeof aiCfg === 'object' && aiCfg.enabled === true);
    })
    .map((type) => ({
      code: String(type.code).trim(),
      name: String(type.name || type.code).trim(),
      ai_extraction_config: type.ai_extraction_config as BusinessCaseAiExtractionConfig,
      case_properties: type.case_properties || null,
    }));
});

const statusLabel = computed(() => {
  if (commitLoading.value) return '提交业务事项中';
  if (draftLoading.value) return '整理语音草稿中';
  if (editableActions.value.length) return '待人工确认';
  if (utterance.status.value === 'finalizing') return '正在收尾识别';
  if (utterance.status.value === 'segment_ready') return '语音片段已就绪';
  if (utterance.status.value === 'collecting') return '聚合语音片段中';
  if (utterance.status.value === 'error') return '语音片段处理异常';
  if (listening.value) return partialTranscript.value ? '识别中' : '监听中';
  return '未启用';
});

const utteranceStatusLabel = computed(() => {
  const labels: Record<UtteranceSessionStatus, string> = {
    idle: 'idle · 等待输入',
    collecting: 'collecting · 聚合片段',
    finalizing: 'finalizing · 收尾识别',
    segment_ready: 'segment_ready · 可生成草稿',
    drafting: 'drafting · 生成草稿',
    needs_confirmation: 'needs_confirmation · 待确认',
    error: 'error · 需处理异常',
  };
  return labels[utterance.status.value];
});

const draftButtonDisabledReason = computed(() => {
  if (draftLoading.value) return '正在生成草稿';
  if (commitLoading.value) return '正在提交业务事项';
  if (utterance.status.value === 'finalizing') return '正在收尾识别，请稍候';
  if (!canDraft.value) return '请先输入或识别有效语音片段';
  return '';
});

const draftHint = computed(() => {
  if (draftButtonDisabledReason.value) return draftButtonDisabledReason.value;
  if (utterance.hasPendingPartial.value) return '存在临时识别文本，生成前会短暂等待最终识别，可能需要人工确认';
  if (utterance.transcriptNeedsConfirmation.value) return '识别文本包含低置信片段，请人工确认后提交';
  if (utterance.status.value === 'collecting') return '正在等待短静音窗口合并后续语音，仍可手动生成';
  return '';
});

const realtime = useRealtimeAudioSession({
  entityId: entityId.value,
  onAsrPartial: (text) => {
    utterance.acceptPartial(text);
  },
  onAsrFinal: (text) => {
    utterance.acceptFinal(text);
  },
});

function hasCaseType(caseType: string): boolean {
  return normalizedCaseTypes.value.some((type) => type.code === caseType);
}

function scoreLabel(score: number): string {
  return `${Math.round(score * 100)}%`;
}

function closePanel(): void {
  if (listening.value) {
    void stopListening();
  }
  isOpen.value = false;
}

function floatToPcm16Base64(input: Float32Array, inputSampleRate: number): string {
  const ratio = inputSampleRate / TARGET_SAMPLE_RATE;
  const outputLength = Math.max(1, Math.floor(input.length / ratio));
  const pcm = new Int16Array(outputLength);
  for (let index = 0; index < outputLength; index += 1) {
    const sample = input[Math.min(input.length - 1, Math.floor(index * ratio))] ?? 0;
    const clamped = Math.max(-1, Math.min(1, sample));
    pcm[index] = clamped < 0 ? clamped * 0x8000 : clamped * 0x7fff;
  }
  const bytes = new Uint8Array(pcm.buffer);
  let binary = '';
  const batchSize = 0x8000;
  for (let index = 0; index < bytes.length; index += batchSize) {
    binary += String.fromCharCode(...bytes.subarray(index, index + batchSize));
  }
  return window.btoa(binary);
}

function rms(input: Float32Array): number {
  if (!input.length) {
    return 0;
  }
  let sum = 0;
  for (const sample of input) {
    sum += sample * sample;
  }
  return Math.sqrt(sum / input.length);
}

async function startListening(): Promise<void> {
  errorMessage.value = '';
  resultMessage.value = '';
  try {
    await realtime.connect();
    mediaStream = await navigator.mediaDevices.getUserMedia({
      audio: {
        channelCount: 1,
        noiseSuppression: true,
        echoCancellation: true,
        autoGainControl: true,
      },
    });
    audioContext = new AudioContext();
    sourceNode = audioContext.createMediaStreamSource(mediaStream);
    processorNode = audioContext.createScriptProcessor(4096, 1, 1);
    silentChunks = 0;

    processorNode.onaudioprocess = (event) => {
      if (!listening.value || !audioContext) {
        return;
      }
      const input = event.inputBuffer.getChannelData(0);
      if (rms(input) < SILENCE_RMS_THRESHOLD) {
        silentChunks += 1;
        if (silentChunks === MAX_SILENT_CHUNKS_BEFORE_END) {
          realtime.endAudio();
        }
        return;
      }
      silentChunks = 0;
      realtime.sendAudioChunk(floatToPcm16Base64(input, audioContext.sampleRate), Date.now());
    };

    sourceNode.connect(processorNode);
    processorNode.connect(audioContext.destination);
    listening.value = true;
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : '麦克风启用失败';
    await stopListening();
  }
}

async function stopListening(): Promise<void> {
  listening.value = false;
  try {
    realtime.endAudio();
  } catch {
    // Connection may already be closed.
  }
  processorNode?.disconnect();
  sourceNode?.disconnect();
  mediaStream?.getTracks().forEach((track) => track.stop());
  await audioContext?.close().catch(() => undefined);
  processorNode = null;
  sourceNode = null;
  mediaStream = null;
  audioContext = null;
  void utterance.finalizeAndFlush();
}

function generateDraftCommitIdempotencyKey(batchId: string): string {
  const random = typeof crypto !== 'undefined' && 'randomUUID' in crypto
    ? crypto.randomUUID()
    : `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return `${batchId}-${random}`;
}

function getDraftCommitIdempotencyKey(batchId: string): string {
  if (!draftCommitIdempotencyKey.value?.startsWith(`${batchId}-`)) {
    draftCommitIdempotencyKey.value = generateDraftCommitIdempotencyKey(batchId);
  }
  return draftCommitIdempotencyKey.value;
}

function toEditableAction(action: CopilotDraftAction): EditableDraftAction {
  const matched = action.matched_flight || action.candidates?.[0] || null;
  const legType = matched?.leg_type || action.leg_type_hint || 'outbound';
  return {
    ...action,
    fields: action.fields || {},
    candidates: action.candidates?.length ? action.candidates : (matched ? [matched] : []),
    selected_flight_key: matched ? `${matched.flight_id}:${matched.flight_no}:${legType}` : '',
    selected_flight_id: matched?.flight_id || '',
    selected_flight_no: matched?.flight_no || '',
    bound_leg_type: legType,
  };
}

async function generateDraft(): Promise<void> {
  if (draftLoading.value || commitLoading.value) {
    return;
  }

  const transcript = await utterance.finalizeAndFlush({ notify: false });
  if (!transcript || draftLoading.value || commitLoading.value) {
    return;
  }

  draftLoading.value = true;
  utterance.markDrafting();
  errorMessage.value = '';
  resultMessage.value = '';
  diagnostic.value = null;
  try {
    const response = await copilot.createDraft({
      entity_id: entityId.value,
      transcript,
      source_page: 'flight_monitor',
      context: {
        selected_flight_id: props.selectedFlightId || undefined,
        selected_flight_no: props.selectedFlightNo || undefined,
      },
    });
    draft.value = response;
    draftCommitIdempotencyKey.value = generateDraftCommitIdempotencyKey(response.batch_id);
    editableActions.value = response.actions.map(toEditableAction);
    utterance.markNeedsConfirmation();
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : '生成草稿失败';
    utterance.markError();
    await diagnoseCurrentTranscript(false);
  } finally {
    draftLoading.value = false;
  }
}

async function diagnoseCurrentTranscript(showLoadingError = true): Promise<void> {
  const transcript = manualTranscript.value.trim();
  if (!transcript || commitLoading.value) {
    return;
  }
  try {
    diagnostic.value = await copilot.diagnoseDraft({
      entity_id: entityId.value,
      transcript,
      source_page: 'flight_monitor',
      context: {
        selected_flight_id: props.selectedFlightId || undefined,
        selected_flight_no: props.selectedFlightNo || undefined,
      },
    });
  } catch (error) {
    if (showLoadingError) {
      errorMessage.value = error instanceof Error ? error.message : '草稿诊断失败';
    }
  }
}

function formatCommitError(value: unknown): string {
  if (!value || typeof value !== 'object') {
    return '';
  }
  const record = value as Record<string, unknown>;
  const previousError = record.previous_error as Record<string, unknown> | undefined;
  const message = String(record.message || previousError?.message || '');
  const stage = String(record.stage || previousError?.stage || '');
  return [stage, message].filter(Boolean).join(': ');
}

async function loadFailedBatches(): Promise<void> {
  failedBatchesLoading.value = true;
  errorMessage.value = '';
  try {
    const [commitFailures, workflowFailures] = await Promise.all([
      copilot.listBatches({ status: 'failed', limit: 10 }),
      copilot.listBatches({ workflow_dispatch_status: 'failed', limit: 10 }),
    ]);
    const byId = new Map<string, CopilotBatchStatusResponse>();
    for (const item of [...commitFailures.items, ...workflowFailures.items]) {
      byId.set(item.batch_id, item);
    }
    failedBatches.value = Array.from(byId.values());
    if (failedBatches.value.length === 0) {
      resultMessage.value = '当前没有失败批次';
    }
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : '读取失败批次失败';
  } finally {
    failedBatchesLoading.value = false;
  }
}

async function loadOperationalMetrics(): Promise<void> {
  metricsLoading.value = true;
  errorMessage.value = '';
  try {
    operationalMetrics.value = await copilot.getOperationalMetrics({
      recent_error_limit: 10,
      max_workflow_dispatch_attempts: 5,
    });
    await loadFailedBatches();
    resultMessage.value = `运行指标已更新，失败批次 ${failedBatches.value.length} 个`;
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : '读取运行指标失败';
  } finally {
    metricsLoading.value = false;
  }
}

async function retryWorkflowDispatch(batchId: string): Promise<void> {
  failedBatchesLoading.value = true;
  errorMessage.value = '';
  try {
    const updated = await copilot.retryWorkflowDispatch(batchId);
    if (updated.workflow_dispatch_status === 'succeeded') {
      failedBatches.value = failedBatches.value.filter((batch) => batch.batch_id !== batchId);
      resultMessage.value = '流程派发已重试成功';
    } else {
      failedBatches.value = failedBatches.value.map((batch) => (
        batch.batch_id === batchId ? updated : batch
      ));
      resultMessage.value = '流程派发重试已完成，但仍未成功，请查看错误信息';
    }
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : '重试流程派发失败';
  } finally {
    failedBatchesLoading.value = false;
  }
}

async function resolveBatch(batchId: string, action: 'mark_resolved' | 'reset_to_draft'): Promise<void> {
  failedBatchesLoading.value = true;
  errorMessage.value = '';
  try {
    await copilot.resolveFailedBatch({
      batchId,
      action,
      note: action === 'mark_resolved' ? '前端确认已人工处理' : '前端确认无部分事项后重置',
    });
    failedBatches.value = failedBatches.value.filter((batch) => batch.batch_id !== batchId);
    resultMessage.value = action === 'mark_resolved' ? '失败批次已标记处理完成' : '失败批次已重置为草稿';
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : '处理失败批次失败';
  } finally {
    failedBatchesLoading.value = false;
  }
}

function applySelectedFlight(action: EditableDraftAction): void {
  const [flightId, flightNo, legType] = action.selected_flight_key.split(':');
  action.selected_flight_id = flightId || '';
  action.selected_flight_no = flightNo || '';
  action.bound_leg_type = legType || 'outbound';
  action.needs_review = !action.selected_flight_id;
  action.review_reason = action.needs_review ? action.review_reason : null;
}

function removeAction(actionId: string): void {
  editableActions.value = editableActions.value.filter((action) => action.action_id !== actionId);
}

async function commitDraft(): Promise<void> {
  if (commitLoading.value || !draft.value || !canCommit.value) {
    return;
  }

  commitLoading.value = true;
  errorMessage.value = '';
  diagnostic.value = null;
  try {
    const response = await copilot.commitBatch(
      draft.value.batch_id,
      editableActions.value.map((action) => ({
        action_id: action.action_id,
        case_type: action.case_type,
        flight_id: action.selected_flight_id,
        flight_no: action.selected_flight_no,
        bound_leg_type: action.bound_leg_type || 'outbound',
        bound_flight_no: action.selected_flight_no,
        description: action.description,
        remarks: action.remarks,
        fields: action.fields || {},
        status: 'INITIAL',
      })),
      {
        idempotencyKey: getDraftCommitIdempotencyKey(draft.value.batch_id),
      },
    );
    const dispatchSuffix = response.workflow_dispatch_status === 'failed'
      ? '，流程派发失败，可在失败批次中重试'
      : '';
    resultMessage.value = `已创建 ${response.case_ids.length} 条事项，通知组 ${response.notification_groups.length} 组${dispatchSuffix}`;
    emit('created', {
      caseIds: response.case_ids,
      notificationGroupCount: response.notification_groups.length,
    });
    utterance.clear();
    draft.value = null;
    draftCommitIdempotencyKey.value = null;
    editableActions.value = [];
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : '批量创建失败';
  } finally {
    commitLoading.value = false;
  }
}

function clearSession(): void {
  utterance.clear();
  draft.value = null;
  draftCommitIdempotencyKey.value = null;
  editableActions.value = [];
  diagnostic.value = null;
  failedBatches.value = [];
  operationalMetrics.value = null;
  errorMessage.value = '';
  resultMessage.value = '';
}

onUnmounted(() => {
  void stopListening();
  realtime.cancel('component_unmounted');
});
</script>

<style scoped>
.auto-copilot {
  position: fixed;
  right: 24px;
  bottom: 92px;
  z-index: 980;
  font-size: 13px;
}

.auto-copilot__launcher,
.auto-copilot__panel {
  border: 1px solid var(--admin-border);
  box-shadow: 0 16px 44px rgba(15, 23, 42, 0.18);
  background: var(--admin-card-bg);
  color: var(--admin-text);
}

.auto-copilot__launcher {
  display: inline-flex;
  align-items: center;
  gap: 9px;
  min-height: 44px;
  padding: 0 16px;
  border-radius: 8px;
  font-weight: 700;
  cursor: pointer;
}

.auto-copilot__pulse {
  width: 10px;
  height: 10px;
  border-radius: 999px;
  background: #1d9a6c;
}

.auto-copilot__panel {
  width: min(720px, calc(100vw - 32px));
  max-height: min(720px, calc(100vh - 128px));
  display: flex;
  flex-direction: column;
  border-radius: 8px;
  overflow: hidden;
}

.auto-copilot__header,
.auto-copilot__controls,
.auto-copilot__section-title,
.auto-copilot__summary {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.auto-copilot__header {
  padding: 14px 16px;
  border-bottom: 1px solid var(--admin-border);
  background: var(--bg-page);
}

.auto-copilot__header h2 {
  margin: 0;
  font-size: 16px;
  line-height: 1.3;
}

.auto-copilot__header p {
  margin: 2px 0 0;
  color: var(--admin-text-subtle);
}

.auto-copilot__icon-btn,
.auto-copilot__remove,
.auto-copilot__section-title button {
  border: 1px solid var(--admin-border);
  background: var(--admin-card-bg);
  border-radius: 6px;
  cursor: pointer;
}

.auto-copilot__icon-btn,
.auto-copilot__remove {
  width: 30px;
  height: 30px;
}

.auto-copilot__controls {
  padding: 12px 16px;
  border-bottom: 1px solid var(--admin-border);
  flex-wrap: wrap;
}

.auto-copilot__primary,
.auto-copilot__secondary,
.auto-copilot__diagnose,
.auto-copilot__commit {
  min-height: 34px;
  border-radius: 6px;
  border: 1px solid transparent;
  padding: 0 13px;
  font-weight: 700;
  cursor: pointer;
}

.auto-copilot__primary,
.auto-copilot__commit {
  background: #125f9f;
  color: var(--admin-card-bg);
}

.auto-copilot__secondary {
  background: #eef6ff;
  color: var(--ws-primary);
  border-color: var(--admin-border);
}

.auto-copilot__diagnose {
  background: var(--admin-card-bg);
  color: var(--admin-text);
  border-color: var(--admin-border);
}

button:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.auto-copilot__alert,
.auto-copilot__success {
  margin: 12px 16px 0;
  padding: 9px 10px;
  border-radius: 6px;
}

.auto-copilot__alert {
  color: #8a1f11;
  background: #fff0ed;
  border: 1px solid #ffc9c0;
}

.auto-copilot__success {
  color: #115c3f;
  background: #ebf8f1;
  border: 1px solid var(--admin-border);
}

.auto-copilot__diagnostic {
  margin: 12px 16px 0;
  padding: 10px;
  border: 1px solid var(--admin-border);
  border-radius: 6px;
  background: var(--bg-page);
}

.auto-copilot__diagnostic-head,
.auto-copilot__diagnostic-grid {
  display: flex;
  align-items: center;
  gap: 8px;
}

.auto-copilot__diagnostic-head {
  justify-content: space-between;
  margin-bottom: 8px;
  font-weight: 700;
}

.auto-copilot__diagnostic-head button {
  border: 1px solid var(--admin-border);
  border-radius: 6px;
  background: var(--admin-card-bg);
  color: var(--admin-text);
  cursor: pointer;
}

.auto-copilot__diagnostic-grid {
  display: grid;
  grid-template-columns: 72px 1fr;
  margin-bottom: 8px;
  color: var(--admin-text-subtle);
}

.auto-copilot__diagnostic-grid strong {
  color: var(--admin-text);
  font-weight: 600;
}

.auto-copilot__diagnostic pre {
  max-height: 180px;
  margin: 8px 0 0;
  overflow: auto;
  white-space: pre-wrap;
  word-break: break-word;
  font-size: 12px;
  line-height: 1.5;
  color: var(--admin-text);
}

.auto-copilot__metrics {
  margin: 12px 16px 0;
  padding: 10px;
  border: 1px solid var(--admin-border);
  border-radius: 6px;
  background: var(--dh-signal-accent-soft);
}

.auto-copilot__metric-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
}

.auto-copilot__metric-grid div {
  min-width: 0;
  padding: 8px;
  border: 1px solid var(--admin-border);
  border-radius: 6px;
  background: var(--admin-card-bg);
}

.auto-copilot__metric-grid span,
.auto-copilot__metric-grid strong {
  display: block;
}

.auto-copilot__metric-grid span {
  color: var(--admin-text-muted);
  font-size: 12px;
}

.auto-copilot__metric-grid strong {
  margin-top: 3px;
  color: var(--admin-text-subtle);
  font-size: 18px;
}

.auto-copilot__recent-errors {
  margin-top: 10px;
  border-top: 1px solid var(--admin-border);
}

.auto-copilot__recent-error {
  display: grid;
  gap: 3px;
  padding: 8px 0;
  border-bottom: 1px solid var(--admin-border);
}

.auto-copilot__recent-error strong {
  color: var(--admin-text);
}

.auto-copilot__recent-error small {
  color: var(--admin-text-subtle);
}

.auto-copilot__recent-error span {
  color: var(--admin-text);
  word-break: break-word;
}

.auto-copilot__failed-batches {
  margin: 12px 16px 0;
  padding: 10px;
  border: 1px solid #ffd6a8;
  border-radius: 6px;
  background: #fffaf0;
}

.auto-copilot__failed-item {
  display: flex;
  justify-content: space-between;
  gap: 10px;
  padding: 9px 0;
  border-top: 1px solid #ffe3bd;
}

.auto-copilot__failed-item:first-of-type {
  border-top: 0;
}

.auto-copilot__failed-item strong,
.auto-copilot__failed-item small {
  display: block;
}

.auto-copilot__failed-item small {
  margin-top: 3px;
  color: #7a5a2a;
  word-break: break-word;
}

.auto-copilot__failed-actions {
  display: flex;
  flex-direction: column;
  gap: 6px;
  flex: 0 0 86px;
}

.auto-copilot__failed-actions button {
  min-height: 28px;
  border: 1px solid var(--admin-border);
  border-radius: 6px;
  background: var(--admin-card-bg);
  color: var(--admin-text);
  cursor: pointer;
}

.auto-copilot__transcript,
.auto-copilot__draft {
  padding: 14px 16px;
}

.auto-copilot__section-title {
  margin-bottom: 8px;
  font-weight: 700;
}

.auto-copilot__session-meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin-bottom: 8px;
  color: var(--admin-text-subtle);
}

.auto-copilot__session-meta small {
  text-align: right;
}

.auto-copilot__session-state {
  flex: 0 0 auto;
  padding: 3px 8px;
  border: 1px solid var(--admin-border);
  border-radius: 999px;
  background: var(--bg-page);
  color: var(--admin-text);
  font-size: 12px;
  font-weight: 700;
}

.auto-copilot__session-state.is-collecting {
  border-color: var(--admin-border);
  background: #eef6ff;
  color: var(--ws-primary);
}

.auto-copilot__session-state.is-segment_ready {
  border-color: var(--admin-border);
  background: #ebf8f1;
  color: #115c3f;
}

.auto-copilot__session-state.is-finalizing {
  border-color: #ffd6a8;
  background: #fffaf0;
  color: #7a5a2a;
}

.auto-copilot__session-state.is-drafting,
.auto-copilot__session-state.is-needs_confirmation {
  border-color: #ffd6a8;
  background: #fffaf0;
  color: #7a5a2a;
}

.auto-copilot__session-state.is-error {
  border-color: #ffc9c0;
  background: #fff0ed;
  color: #8a1f11;
}

.auto-copilot textarea,
.auto-copilot input,
.auto-copilot select {
  width: 100%;
  min-width: 0;
  border: 1px solid var(--admin-border);
  border-radius: 6px;
  background: var(--admin-card-bg);
  color: var(--admin-text);
  font: inherit;
}

.auto-copilot textarea {
  resize: vertical;
  padding: 9px 10px;
}

.auto-copilot input,
.auto-copilot select {
  min-height: 32px;
  padding: 0 8px;
}

.auto-copilot__partial {
  margin: 8px 0 0;
  color: var(--admin-text-subtle);
}

.auto-copilot__partial--warning {
  color: #9a5a00;
}

.auto-copilot__summary {
  align-items: flex-start;
  margin-bottom: 10px;
  color: var(--admin-text-subtle);
}

.auto-copilot__summary strong {
  flex: 1;
  color: var(--admin-text);
  font-weight: 600;
}

.auto-copilot__table-wrap {
  overflow: auto;
  border: 1px solid var(--admin-border);
  border-radius: 8px;
}

.auto-copilot__table {
  width: 100%;
  min-width: 680px;
  border-collapse: collapse;
}

.auto-copilot__table th,
.auto-copilot__table td {
  padding: 8px;
  border-bottom: 1px solid var(--admin-border);
  text-align: left;
  vertical-align: top;
}

.auto-copilot__table th {
  background: var(--bg-page);
  color: var(--admin-text-subtle);
  font-size: 12px;
}

.auto-copilot__table tr.needs-review td {
  background: #fffaf0;
}

.auto-copilot__table small {
  display: block;
  margin-top: 4px;
  color: #a15c00;
}

.auto-copilot__commit {
  width: 100%;
  margin-top: 12px;
}

@media (max-width: 720px) {
  .auto-copilot {
    right: 12px;
    bottom: 76px;
  }

  .auto-copilot__panel {
    width: calc(100vw - 24px);
  }
}

/* Dynamic AI Fields Styles */
.dynamic-fields-container {
  display: flex;
  flex-direction: column;
  gap: 6px;
  background: var(--bg-page);
  border-radius: 6px;
  padding: 6px;
  margin-top: 4px;
}
.dynamic-field-item {
  display: flex;
  flex-direction: column;
  gap: 3px;
  text-align: left;
}
.field-label {
  font-size: 11px;
  color: var(--admin-text-subtle);
  font-weight: 500;
}
.required-star {
  color: var(--system-red);
  margin-left: 2px;
}
</style>
