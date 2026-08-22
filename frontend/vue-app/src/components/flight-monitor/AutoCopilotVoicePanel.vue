<template>
  <UiFloatPanel
    :open="isOpen"
    title="Auto Copilot"
    :subtitle="statusLabel"
    width="min(720px, calc(100vw - 32px))"
    height="min(720px, calc(100vh - 128px))"
    @close="closePanel"
  >
    <!-- 偶尔才用的一串命令走锚浮菜单（§2.6），不占常驻的谓词带 -->
    <template #meta>
      <div class="copilot__overflow">
        <UiButton
          variant="quiet"
          title="更多操作"
          aria-label="更多操作"
          :pressed="showMenu"
          @click="showMenu = !showMenu"
        >
          ⋮
        </UiButton>
        <UiMenu v-if="showMenu" class="copilot__menu" label="Auto Copilot 操作">
          <UiMenuItem
            :disabled="!canDraft || draftLoading || commitLoading"
            @click="runDiagnostic"
          >
            诊断模型输出
          </UiMenuItem>
          <UiMenuItem
            :disabled="draftLoading || commitLoading || failedBatchesLoading || metricsLoading"
            @click="runOperationalMetrics"
          >
            运行指标
          </UiMenuItem>
        </UiMenu>
      </div>
    </template>

    <!-- 谓词带：麦克风是持守开关，生成草稿是这一步的主谓词 -->
    <div class="copilot__band copilot__verbs">
      <UiButton
        :pressed="listening"
        :disabled="isBusy"
        @click="listening ? stopListening() : startListening()"
      >
        {{ listening ? '停止监听' : '启用麦克风' }}
      </UiButton>
      <UiButton
        :variant="draft ? 'ghost' : 'primary'"
        :disabled="Boolean(draftButtonDisabledReason)"
        :title="draftHint || '生成当前语音片段草稿'"
        @click="generateDraft"
      >
        {{ draftLoading ? '生成中…' : '生成草稿' }}
      </UiButton>
    </div>

    <div v-if="errorMessage" class="copilot__band">
      <UiBanner tone="danger">
        {{ errorMessage }}
      </UiBanner>
    </div>

    <div v-if="resultMessage" class="copilot__band">
      <UiBanner tone="ok">
        {{ resultMessage }}
      </UiBanner>
    </div>

    <div v-if="diagnostic" class="copilot__band">
      <UiInset
        title="模型诊断"
        :tone="diagnostic.ok ? 'mute' : 'danger'"
        dismissible
        @dismiss="diagnostic = null"
      >
        <dl class="copilot__pairs">
          <dt>状态</dt>
          <dd>{{ diagnostic.ok ? '通过' : '失败' }}</dd>
          <dt>阶段</dt>
          <dd>{{ diagnostic.error_stage || '—' }}</dd>
          <dt>候选事项</dt>
          <dd>{{ diagnostic.candidate_case_types.map((item) => item.name || item.code).join('、') || '—' }}</dd>
        </dl>
        <pre v-if="diagnostic.error_message" class="copilot__raw">{{ diagnostic.error_message }}</pre>
        <details v-if="diagnostic.llm_raw_preview">
          <summary>模型原始输出</summary>
          <pre class="copilot__raw">{{ diagnostic.llm_raw_preview }}</pre>
        </details>
      </UiInset>
    </div>

    <div v-if="operationalMetrics" class="copilot__band">
      <UiInset title="运行指标" dismissible @dismiss="operationalMetrics = null">
        <UiReadoutStrip :items="metricItems" label="Auto Copilot 运行指标" />
        <div v-if="operationalMetrics.recent_errors.length" class="copilot__errors">
          <div
            v-for="item in operationalMetrics.recent_errors"
            :key="item.batch_id"
            class="copilot__error"
          >
            <strong>{{ item.stage || item.workflow_dispatch_status || item.status }}</strong>
            <small>{{ item.batch_id }} · {{ item.updated_at }}</small>
            <span>{{ item.message || '无错误详情' }}</span>
          </div>
        </div>
      </UiInset>
    </div>

    <div v-if="failedBatches.length" class="copilot__band">
      <UiInset
        title="失败批次"
        tone="warn"
        dismissible
        @dismiss="failedBatches = []"
      >
        <div
          v-for="batch in failedBatches"
          :key="batch.batch_id"
          class="copilot__batch"
        >
          <div class="copilot__batch-body">
            <strong>{{ batch.transcript_summary || batch.batch_id }}</strong>
            <small>{{ batch.batch_id }} · {{ batch.updated_at }}</small>
            <small v-if="batch.committed_case_ids.length">已创建事项 {{ batch.committed_case_ids.join(', ') }}</small>
            <small v-if="formatCommitError(batch.commit_error)" class="is-warn">{{ formatCommitError(batch.commit_error) }}</small>
            <small v-if="batch.workflow_dispatch_status === 'failed'" class="is-warn">
              流程派发失败 {{ formatCommitError(batch.workflow_dispatch_error) }}
            </small>
            <small v-if="batch.workflow_dispatch_status === 'failed'">
              已重试 {{ batch.workflow_dispatch_attempts }} 次<span v-if="batch.workflow_dispatch_next_retry_at"> · 下次自动重试 {{ batch.workflow_dispatch_next_retry_at }}</span>
            </small>
          </div>
          <div class="copilot__batch-verbs">
            <UiButton
              v-if="batch.workflow_dispatch_status === 'failed'"
              :disabled="failedBatchesLoading"
              @click="retryWorkflowDispatch(batch.batch_id)"
            >
              重试派发
            </UiButton>
            <UiButton
              v-if="batch.status === 'failed'"
              variant="quiet"
              :disabled="failedBatchesLoading || batch.committed_case_ids.length > 0"
              @click="resolveBatch(batch.batch_id, 'reset_to_draft')"
            >
              重置草稿
            </UiButton>
            <UiButton
              v-if="batch.status === 'failed'"
              variant="quiet"
              :disabled="failedBatchesLoading"
              @click="resolveBatch(batch.batch_id, 'mark_resolved')"
            >
              已处理
            </UiButton>
          </div>
        </div>
      </UiInset>
    </div>

    <div class="copilot__band">
      <div class="copilot__section-head">
        <span class="copilot__section-title">识别文本</span>
        <span class="copilot__section-tools">
          <UiPill :tone="sessionTone">{{ utteranceStatusLabel }}</UiPill>
          <UiButton
            variant="quiet"
            :disabled="listening || draftLoading || commitLoading"
            @click="clearSession"
          >
            清空
          </UiButton>
        </span>
      </div>
      <UiField>
        <textarea
          v-model="manualTranscript"
          rows="4"
          placeholder="开启麦克风后自动填充，也可以粘贴 ASR 文本后生成草稿。"
          :disabled="listening || draftLoading || commitLoading"
        />
      </UiField>
      <UiBanner v-if="transcriptNeedsConfirmation" tone="warn" class="copilot__inline-banner">
        文本包含低置信收尾片段，请人工确认航班号、座位号等关键信息。
      </UiBanner>
      <p v-if="partialTranscript" class="copilot__partial">
        {{ partialTranscript }}
      </p>
    </div>

    <div v-if="draft" class="copilot__band">
      <div class="copilot__section-head">
        <span class="copilot__section-title">草稿摘要</span>
      </div>
      <p class="copilot__summary">
        {{ draft.summary }}
      </p>

      <div class="copilot__table-wrap">
        <UiTable label="语音草稿事项" :sticky-head="false">
          <thead>
            <tr>
              <th>事项</th>
              <th>航班</th>
              <th>航段</th>
              <th>额外信息</th>
              <th data-align="end">
                置信度
              </th>
              <th><span class="sr-only">操作</span></th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="action in editableActions"
              :key="action.action_id"
              :data-tone="action.needs_review ? 'warn' : undefined"
            >
              <td>
                <UiField>
                  <select v-model="action.case_type" aria-label="事项类型">
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
                </UiField>
              </td>
              <td>
                <UiField :hint="action.review_reason ?? undefined">
                  <select
                    v-model="action.selected_flight_key"
                    aria-label="绑定航班"
                    @change="applySelectedFlight(action)"
                  >
                    <option value="">
                      请选择
                    </option>
                    <option
                      v-for="candidate in action.candidates"
                      :key="candidate.flight_id + ':' + candidate.leg_type"
                      :value="candidate.flight_id + ':' + candidate.flight_no + ':' + candidate.leg_type"
                    >
                      {{ candidate.flight_no }} · {{ scoreLabel(candidate.score) }}
                    </option>
                  </select>
                </UiField>
              </td>
              <td>{{ action.bound_leg_type || action.leg_type_hint || 'outbound' }}</td>
              <td>
                <div class="copilot__fields">
                  <UiField>
                    <input v-model="action.remarks" aria-label="额外信息" placeholder="额外信息">
                  </UiField>

                  <!-- extra_info 由事项类型配置动态给出，字段的形交给 UiField -->
                  <UiField
                    v-for="(fieldCfg, fieldName) in getExtraInfoFields(action.case_type)"
                    :key="fieldName"
                    :label="fieldCfg.label || fieldName"
                    :required="fieldCfg.required"
                  >
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
                  </UiField>
                </div>
              </td>
              <td data-align="end" data-mono>
                {{ Math.round(action.confidence * 100) }}%
              </td>
              <td data-align="center">
                <UiButton
                  variant="quiet"
                  aria-label="删除草稿"
                  @click="removeAction(action.action_id)"
                >
                  ×
                </UiButton>
              </td>
            </tr>
          </tbody>
        </UiTable>
      </div>
    </div>

    <template v-if="draft" #footer>
      <UiButton
        variant="primary"
        size="md"
        class="copilot__commit"
        :disabled="!canCommit || commitLoading"
        @click="commitDraft"
      >
        {{ commitLoading ? '提交中…' : '确认创建 ' + editableActions.length + ' 条事项' }}
      </UiButton>
    </template>
  </UiFloatPanel>
</template>

<script setup lang="ts">
import { computed, onUnmounted, ref } from 'vue';
import UiBanner from '../ui/UiBanner.vue';
import UiButton from '../ui/UiButton.vue';
import UiField from '../ui/UiField.vue';
import UiFloatPanel from '../ui/UiFloatPanel.vue';
import UiInset from '../ui/UiInset.vue';
import UiMenu from '../ui/UiMenu.vue';
import UiMenuItem from '../ui/UiMenuItem.vue';
import UiPill from '../ui/UiPill.vue';
import UiReadoutStrip from '../ui/UiReadoutStrip.vue';
import UiTable from '../ui/UiTable.vue';
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

/** 溢出菜单是持守（aria-pressed 绑在 ⋮ 上），点完一项就收起。 */
const showMenu = ref(false);

const props = defineProps<{
  open: boolean;
  entityId?: string;
  selectedFlightId?: string | null;
  selectedFlightNo?: string | null;
  businessCaseTypes?: BusinessCaseTypeDefinition[];
}>();

const emit = defineEmits<{
  (event: 'created', payload: { caseIds: string[]; notificationGroupCount: number }): void;
  (event: 'update:open', value: boolean): void;
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
const isOpen = computed({
  get: () => props.open,
  set: (value: boolean) => emit('update:open', value),
});
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

/**
 * 帽下那一行只报这一步在做什么。语音片段本身的事态画在「识别文本」那颗胶囊上，
 * 两处不要报同一件事（§4.4 不要重复芯片）。
 */
const statusLabel = computed(() => {
  if (commitLoading.value) return '提交业务事项中';
  if (draftLoading.value) return '整理语音草稿中';
  if (editableActions.value.length) return '待人工确认';
  if (listening.value) return partialTranscript.value ? '识别中' : '监听中';
  return '未启用';
});

/** 值班的人读中文，不读后端枚举 —— 枚举名不进界面。 */
const utteranceStatusLabel = computed(() => {
  const labels: Record<UtteranceSessionStatus, string> = {
    idle: '等待输入',
    collecting: '聚合片段',
    finalizing: '收尾识别',
    segment_ready: '可生成草稿',
    drafting: '生成草稿',
    needs_confirmation: '待确认',
    error: '需处理异常',
  };
  return labels[utterance.status.value];
});

/** 语音片段状态 → 四声：进行 act / 就绪 ok / 待人看 warn / 异常 danger。 */
const sessionTone = computed(() => {
  switch (utterance.status.value) {
    case 'collecting':
      return 'act' as const;
    case 'segment_ready':
      return 'ok' as const;
    case 'finalizing':
    case 'drafting':
    case 'needs_confirmation':
      return 'warn' as const;
    case 'error':
      return 'danger' as const;
    default:
      return 'mute' as const;
  }
});

/** 运行指标 = 一排读数：只有真的非零才让它出声。 */
const metricItems = computed(() => {
  const metrics = operationalMetrics.value;
  if (!metrics) return [];
  return [
    { label: '草稿批次', value: metrics.batch_status.draft, tone: 'ink' as const },
    { label: '已创建', value: metrics.batch_status.committed, tone: 'ok' as const },
    { label: '创建失败', value: metrics.batch_status.failed, tone: 'danger' as const },
    { label: '派发失败', value: metrics.workflow_dispatch.failed, tone: 'danger' as const },
    { label: '待自动重试', value: metrics.workflow_dispatch.retry_due, tone: 'warn' as const },
    { label: '重试耗尽', value: metrics.workflow_dispatch.retry_exhausted, tone: 'danger' as const },
  ];
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
  showMenu.value = false;
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

/* 菜单里的两颗谓词：点完收起菜单，动作本身还是原来那两个。 */
function runDiagnostic(): void {
  showMenu.value = false;
  void diagnoseCurrentTranscript();
}

function runOperationalMetrics(): void {
  showMenu.value = false;
  void loadOperationalMetrics();
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
/* 浮舱的形、帽、脚、关闭键全部在 UiFloatPanel 里；
   嵌板、读数、表、字段、谓词各归其组件。
   这里只剩这一页自己的排布与两三种叙事文本。 */

/* 带：舱内每一节的留白，彼此只用一根线分开，不描框 */
.copilot__band {
  padding: var(--s3);
}

.copilot__band + .copilot__band {
  border-top: 1px solid var(--line);
}

.copilot__verbs {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--s2);
}

/* 锚浮：菜单贴着 ⋮ 落下，层序用 --z-menu，不自己发明数字 */
.copilot__overflow {
  position: relative;
  display: inline-flex;
}

.copilot__menu {
  position: absolute;
  top: calc(100% + var(--s1));
  right: 0;
  z-index: var(--z-menu);
}

.copilot__section-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s3);
  margin-bottom: var(--s2);
}

.copilot__section-title {
  font-size: var(--fs-section);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.copilot__section-tools {
  display: inline-flex;
  align-items: center;
  gap: var(--s2);
}

/* 读数条落在嵌板里，横向留白已由嵌板给过一次 */
.copilot__band :deep(.ui-readouts) {
  padding: 0;
}

.copilot__inline-banner {
  margin-top: var(--s2);
}

/* 名 / 值成对：诊断里的三行事实 */
.copilot__pairs {
  display: grid;
  grid-template-columns: 68px 1fr;
  gap: var(--s1) var(--s2);
  margin: 0;
}

.copilot__pairs dt {
  color: var(--ink-subtle);
}

.copilot__pairs dd {
  margin: 0;
  color: var(--ink);
  font-weight: var(--fw-medium);
  word-break: break-word;
}

/* 原文：模型吐出来的东西照抄，等宽 */
.copilot__raw {
  max-height: 180px;
  margin: var(--s2) 0 0;
  overflow: auto;
  white-space: pre-wrap;
  word-break: break-word;
  font-family: var(--mono);
  font-size: var(--fs-label);
  line-height: 1.5;
  color: var(--ink);
}

.copilot__errors {
  margin-top: var(--s2);
  border-top: 1px solid var(--line);
}

.copilot__error {
  display: grid;
  gap: 3px;
  padding: var(--s2) 0;
}

.copilot__error + .copilot__error {
  border-top: 1px solid var(--line);
}

.copilot__error strong {
  color: var(--ink);
}

.copilot__error small {
  color: var(--ink-subtle);
  font-variant-numeric: tabular-nums;
}

.copilot__error span {
  color: var(--ink);
  word-break: break-word;
}

.copilot__batch {
  display: flex;
  justify-content: space-between;
  gap: var(--s3);
  padding: 9px 0;
  border-top: 1px solid var(--line);
}

.copilot__batch:first-of-type {
  padding-top: 0;
  border-top: 0;
}

.copilot__batch-body {
  display: grid;
  gap: 3px;
  min-width: 0;
}

/* 批号、时刻是标识不是事态：只有真的失败那几行才出声 */
.copilot__batch-body small {
  color: var(--ink-subtle);
  word-break: break-word;
  font-variant-numeric: tabular-nums;
}

.copilot__batch-body small.is-warn {
  color: var(--warn);
}

.copilot__batch-verbs {
  display: flex;
  flex-direction: column;
  gap: var(--s1);
  flex: 0 0 88px;
}

.copilot__partial {
  margin: var(--s2) 0 0;
  color: var(--ink-subtle);
}

.copilot__summary {
  margin: 0;
  color: var(--ink);
  font-weight: var(--fw-medium);
  line-height: 1.5;
}

/* 表接着舱面铺下去：只留横向滚动口，不再描第二道边、换第二个圆角（§4.21） */
.copilot__table-wrap {
  overflow-x: auto;
}

.copilot__table-wrap :deep(.ui-table) {
  min-width: 680px;
}

.copilot__table-wrap :deep(td) {
  vertical-align: top;
}

/* 格内字段竖排；每个字段的形由 UiField 给 */
.copilot__fields {
  display: grid;
  gap: var(--s2);
  min-width: 168px;
}

.copilot__commit {
  width: 100%;
}
</style>
