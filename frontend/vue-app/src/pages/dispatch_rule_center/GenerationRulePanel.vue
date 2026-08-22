<script setup lang="ts">
import { computed, ref } from 'vue';
import { useToast } from '@/composables/useToast';
import UiButton from '@/components/ui/UiButton.vue';
import UiPill from '@/components/ui/UiPill.vue';
import UiSelect from '@/components/ui/UiSelect.vue';
import type {
  CompletionTimeMode,
  FlightGenerationRulePayload,
  FlightGenerationRuleResponse,
} from './dispatchRuleWorkbenchApi';

const props = defineProps<{
  rules: FlightGenerationRuleResponse[];
  taskTypeCode: string;
  saving: boolean;
  disabled: boolean;
}>();

const emit = defineEmits<{
  (e: 'save', payload: FlightGenerationRulePayload): void;
  (e: 'delete', ruleId: string): void;
  (e: 'dirty', value: boolean): void;
}>();

const toast = useToast();

interface Draft {
  rule_id: string;
  rule_name: string;
  leg_scope: string;
  status: string;
  generation_anchor_type: string;
  start_offset_minutes: number;
  completion_time_mode: CompletionTimeMode;
  completion_anchor_type: string | null;
  completion_offset_minutes: number | null;
  duration_minutes: number | null;
  start_flex_minutes: number | null;
  completion_warning_lead_minutes: number | null;
  duration_by_crew_size_json: string;
  publication_state: string;
  publish_trigger_mode: string;
  publish_offset_minutes: number | null;
  publish_event_code: string | null;
  notes: string;
  conditions_json: string;
}

function emptyDraft(): Draft {
  return {
    rule_id: '',
    rule_name: '',
    leg_scope: 'outbound',
    status: 'draft',
    generation_anchor_type: 'scheduled_time',
    start_offset_minutes: 0,
    completion_time_mode: 'start_plus_duration',
    completion_anchor_type: null,
    completion_offset_minutes: null,
    duration_minutes: 30,
    start_flex_minutes: null,
    completion_warning_lead_minutes: null,
    duration_by_crew_size_json: '',
    publication_state: 'prepublished',
    publish_trigger_mode: 'time',
    publish_offset_minutes: null,
    publish_event_code: null,
    notes: '',
    conditions_json: '{}',
  };
}

const draft = ref<Draft>(emptyDraft());

const rulesForTaskType = computed(() =>
  props.rules.filter((r) => r.task_type === props.taskTypeCode),
);

function loadRule(rule: FlightGenerationRuleResponse): void {
  draft.value = {
    rule_id: rule.id,
    rule_name: rule.rule_name ?? '',
    leg_scope: rule.leg_scope,
    status: rule.status,
    generation_anchor_type: rule.generation_anchor_type,
    start_offset_minutes: rule.start_offset_minutes,
    completion_time_mode: rule.completion_time_mode ?? 'start_plus_duration',
    completion_anchor_type: rule.completion_anchor_type ?? null,
    completion_offset_minutes: rule.completion_offset_minutes ?? null,
    duration_minutes: rule.duration_minutes ?? null,
    start_flex_minutes: rule.start_flex_minutes ?? null,
    completion_warning_lead_minutes: rule.completion_warning_lead_minutes ?? null,
    duration_by_crew_size_json: rule.duration_by_crew_size
      ? JSON.stringify(rule.duration_by_crew_size, null, 2)
      : '',
    publication_state: rule.publication_state,
    publish_trigger_mode: rule.publish_trigger_mode,
    publish_offset_minutes: rule.publish_offset_minutes ?? null,
    publish_event_code: rule.publish_event_code ?? null,
    notes: rule.notes ?? '',
    conditions_json: JSON.stringify(rule.conditions ?? {}, null, 2),
  };
}

function clearDraft(): void {
  draft.value = emptyDraft();
  emit('dirty', false);
}

function onCompletionModeChange(): void {
  if (draft.value.completion_time_mode === 'start_plus_duration') {
    draft.value.completion_anchor_type = null;
    draft.value.completion_offset_minutes = null;
    draft.value.duration_minutes ??= 30;
  } else {
    draft.value.duration_minutes = null;
    draft.value.duration_by_crew_size_json = '';
    draft.value.completion_anchor_type ??= 'scheduled_departure';
    draft.value.completion_offset_minutes ??= 0;
  }
  onFieldChange();
}

/**
 * 解析人数->时长映射。空串表示"本部门不配置"，发 null 让重排回退时长常量。
 * 抛错交给 submit 转成提示，不静默丢配置。
 */
function parseDurationByCrewSize(text: string): Record<string, number> | null {
  const trimmed = text.trim();
  if (!trimmed) {
    return null;
  }
  const parsed: unknown = JSON.parse(trimmed);
  if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
    throw new Error('必须是形如 {"1":45,"2":30} 的对象');
  }
  const result: Record<string, number> = {};
  for (const [crewSize, minutes] of Object.entries(parsed)) {
    if (!/^[1-9]\d*$/.test(crewSize.trim())) {
      throw new Error(`人数 "${crewSize}" 必须是正整数`);
    }
    if (typeof minutes !== 'number' || !Number.isInteger(minutes) || minutes <= 0) {
      throw new Error(`人数 ${crewSize} 对应的时长必须是正整数分钟`);
    }
    result[crewSize.trim()] = minutes;
  }
  return Object.keys(result).length > 0 ? result : null;
}

function submit(): void {
  let conditions: Record<string, unknown> = {};
  try {
    conditions = JSON.parse(draft.value.conditions_json || '{}');
  } catch (e) {
    toast.showToast('error', `条件 JSON 格式无效: ${e instanceof Error ? e.message : '未知错误'}`, { duration: 5000 });
    return;
  }
  let durationByCrewSize: Record<string, number> | null = null;
  if (draft.value.completion_time_mode === 'start_plus_duration') {
    try {
      durationByCrewSize = parseDurationByCrewSize(draft.value.duration_by_crew_size_json);
    } catch (e) {
      toast.showToast('error', `人数时长表无效: ${e instanceof Error ? e.message : '未知错误'}`, { duration: 5000 });
      return;
    }
  }
  const payload: FlightGenerationRulePayload = {
    rule_id: draft.value.rule_id || null,
    rule_name: draft.value.rule_name || null,
    task_type: props.taskTypeCode,
    leg_scope: draft.value.leg_scope,
    status: draft.value.status,
    conditions,
    generation_anchor_type: draft.value.generation_anchor_type,
    start_offset_minutes: draft.value.start_offset_minutes,
    completion_time_mode: draft.value.completion_time_mode,
    completion_anchor_type: draft.value.completion_time_mode === 'completion_anchor_offset'
      ? draft.value.completion_anchor_type
      : null,
    completion_offset_minutes: draft.value.completion_time_mode === 'completion_anchor_offset'
      ? draft.value.completion_offset_minutes
      : null,
    duration_minutes: draft.value.completion_time_mode === 'start_plus_duration'
      ? draft.value.duration_minutes
      : null,
    // 清空输入框意味着"本部门不配置"，要发 null 而不是空串，后端才会回退默认值。
    start_flex_minutes: Number.isFinite(draft.value.start_flex_minutes as number)
      ? draft.value.start_flex_minutes
      : null,
    completion_warning_lead_minutes: Number.isFinite(draft.value.completion_warning_lead_minutes as number)
      ? draft.value.completion_warning_lead_minutes
      : null,
    duration_by_crew_size: draft.value.completion_time_mode === 'start_plus_duration'
      ? durationByCrewSize
      : null,
    publication_state: draft.value.publication_state,
    publish_trigger_mode: draft.value.publish_trigger_mode,
    publish_offset_minutes: draft.value.publish_offset_minutes,
    publish_event_code: draft.value.publish_event_code,
    notes: draft.value.notes || null,
  };
  emit('save', payload);
}

function confirmDelete(rule: FlightGenerationRuleResponse): void {
  if (!window.confirm(`删除规则 ${rule.rule_name || rule.id}?`)) return;
  emit('delete', rule.id);
}

function onFieldChange(): void {
  emit('dirty', true);
}

type PillTone = 'act' | 'ok' | 'warn' | 'danger' | 'mute';

function ruleStatusTone(status: string): PillTone {
  if (status === 'published') return 'ok';
  return 'mute';
}

/* UiSelect 收 string，桥回本地 draft 并标脏 */
type SelectField = 'leg_scope' | 'status' | 'generation_anchor_type'
  | 'completion_anchor_type' | 'publication_state' | 'publish_trigger_mode';

function fieldBridge(key: SelectField) {
  return computed<string>({
    get: () => draft.value[key] ?? '',
    set: (value) => {
      draft.value[key] = value;
      onFieldChange();
    },
  });
}

const legScopeModel = fieldBridge('leg_scope');
const statusModel = fieldBridge('status');
const anchorModel = fieldBridge('generation_anchor_type');
const completionAnchorModel = fieldBridge('completion_anchor_type');
const publicationModel = fieldBridge('publication_state');
const publishTriggerModel = fieldBridge('publish_trigger_mode');

const completionModeModel = computed<string>({
  get: () => draft.value.completion_time_mode,
  set: (value) => {
    draft.value.completion_time_mode = value as CompletionTimeMode;
    onCompletionModeChange();
  },
});

const legScopeOptions = [
  { value: 'outbound', label: '出港' },
  { value: 'inbound', label: '进港' },
];

/* 后端 DepartmentRuleStatus 只认 draft/published/archived；
   曾经送出的 "active" 会被判为未知状态而保存失败。 */
const statusOptions = [
  { value: 'draft', label: '草稿' },
  { value: 'published', label: '启用' },
  { value: 'archived', label: '归档' },
];

const anchorOptions = [
  { value: 'scheduled_time', label: '计划时间' },
  { value: 'actual_arrival', label: '实际到达' },
  { value: 'estimated_arrival', label: '预计到达' },
  { value: 'scheduled_arrival', label: '计划到达' },
  { value: 'actual_departure', label: '实际出发' },
  { value: 'estimated_departure', label: '预计出发' },
  { value: 'scheduled_departure', label: '计划出发' },
];

const completionModeOptions = [
  { value: 'start_plus_duration', label: '预计开始 + 作业时长' },
  { value: 'completion_anchor_offset', label: '完成锚点 + 完成偏移' },
];

const publicationOptions = [
  { value: 'prepublished', label: '预发布' },
  { value: 'published', label: '已发布' },
];

const publishTriggerOptions = [
  { value: 'time', label: '按时间' },
  { value: 'event', label: '按事件' },
  { value: 'either', label: '时间或事件任一满足' },
  { value: 'both_required', label: '时间和事件都满足' },
];
</script>

<template>
  <section class="generation-rule-panel" aria-label="航班生成规则">
    <header class="head">
      <h4>航班生成规则</h4>
      <span class="muted">共 {{ rulesForTaskType.length }} 条规则</span>
    </header>

    <ul v-if="rulesForTaskType.length" class="rule-list">
      <li v-for="rule in rulesForTaskType" :key="rule.id">
        <div class="rule-summary">
          <strong>{{ rule.rule_name || rule.id }}</strong>
          <UiPill :tone="ruleStatusTone(rule.status)">
            {{ rule.status }}
          </UiPill>
          <span class="muted">v{{ rule.version_no }} · {{ rule.leg_scope }}</span>
          <span class="muted">
            {{ rule.completion_time_mode === 'completion_anchor_offset' ? '完成锚点' : '开始 + 时长' }}
          </span>
        </div>
        <div class="rule-actions">
          <UiButton
            :disabled="disabled"
            @click="loadRule(rule)"
          >
            编辑
          </UiButton>
          <UiButton
            variant="danger"
            :disabled="disabled || saving"
            @click="confirmDelete(rule)"
          >
            删除
          </UiButton>
        </div>
      </li>
    </ul>
    <div v-else class="empty">
      尚未为该任务类型配置生成规则。点击下方表单添加。
    </div>

    <form class="draft-form" @submit.prevent="submit">
      <h5>{{ draft.rule_id ? '编辑规则' : '新增规则' }}</h5>
      <div class="form-grid">
        <label>规则名称 <input v-model="draft.rule_name" type="text" @input="onFieldChange"></label>
        <label>航段
          <UiSelect
            v-model="legScopeModel"
            :options="legScopeOptions"
            label="航段"
            min-width="100%"
          />
        </label>
        <label>状态
          <UiSelect
            v-model="statusModel"
            :options="statusOptions"
            label="状态"
            min-width="100%"
          />
        </label>
        <label>开始锚点
          <UiSelect
            v-model="anchorModel"
            :options="anchorOptions"
            label="开始锚点"
            min-width="100%"
          />
        </label>
        <label>开始偏移 (分)
          <input v-model.number="draft.start_offset_minutes" type="number" @input="onFieldChange">
        </label>
        <label>预计完成方式
          <UiSelect
            v-model="completionModeModel"
            :options="completionModeOptions"
            label="预计完成方式"
            min-width="100%"
          />
        </label>
        <label v-if="draft.completion_time_mode === 'start_plus_duration'">时长 (分)
          <input
            v-model.number="draft.duration_minutes"
            type="number"
            min="1"
            @input="onFieldChange"
          >
        </label>
        <label v-if="draft.completion_time_mode === 'completion_anchor_offset'">完成锚点
          <UiSelect
            v-model="completionAnchorModel"
            :options="anchorOptions"
            label="完成锚点"
            min-width="100%"
          />
        </label>
        <label v-if="draft.completion_time_mode === 'completion_anchor_offset'">完成偏移 (分)
          <input
            v-model.number="draft.completion_offset_minutes"
            type="number"
            title="可为负数，例如计划离港前 10 分钟填 -10"
            @input="onFieldChange"
          >
        </label>
        <label>开始时间弹性 (分)
          <input
            v-model.number="draft.start_flex_minutes"
            type="number"
            min="0"
            placeholder="留空默认 5 分钟"
            title="重排时该作业开始时间允许后滑的分钟数，留空使用系统默认 5 分钟"
            @input="onFieldChange"
          >
        </label>
        <label>完工预警提前量 (分)
          <input
            v-model.number="draft.completion_warning_lead_minutes"
            type="number"
            min="0"
            max="60"
            placeholder="留空默认 5 分钟"
            title="预排冲突预警提前分钟数（0~60），0 表示下一单到计划开始时才触发；留空使用系统默认 5 分钟"
            @input="onFieldChange"
          >
        </label>
        <label>发布状态
          <UiSelect
            v-model="publicationModel"
            :options="publicationOptions"
            label="发布状态"
            min-width="100%"
          />
        </label>
        <label>发布触发
          <UiSelect
            v-model="publishTriggerModel"
            :options="publishTriggerOptions"
            label="发布触发"
            min-width="100%"
          />
        </label>
        <label>发布偏移 (分)
          <input v-model.number="draft.publish_offset_minutes" type="number" @input="onFieldChange">
        </label>
        <label>发布事件码
          <input v-model="draft.publish_event_code" type="text" @input="onFieldChange">
        </label>
        <label v-if="draft.completion_time_mode === 'start_plus_duration'" class="full">人数时长表（人数 → 分钟，如 {"1":45,"2":30}）
          <textarea
            v-model="draft.duration_by_crew_size_json"
            rows="3"
            placeholder="留空表示本部门不配置，重排时一律用上方的时长"
            @input="onFieldChange"
          />
        </label>
        <label class="full">条件 JSON
          <textarea v-model="draft.conditions_json" rows="4" @input="onFieldChange" />
        </label>
        <label class="full">备注 <input v-model="draft.notes" type="text" @input="onFieldChange"></label>
      </div>
      <div class="form-actions">
        <UiButton @click="clearDraft">
          清空
        </UiButton>
        <UiButton native-type="submit" variant="primary" :disabled="disabled || saving || !taskTypeCode">
          {{ saving ? '保存中…' : (draft.rule_id ? '保存修改' : '新增规则') }}
        </UiButton>
      </div>
    </form>
  </section>
</template>

<style scoped>
/* 按钮归 UiButton、状态章归 UiPill、下拉归 UiSelect */
.generation-rule-panel {
  display: flex;
  flex-direction: column;
  gap: var(--s3);
}

.head {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.head h4 {
  margin: 0;
  font-size: var(--fs-section);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.muted {
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

.rule-list {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: var(--s2);
}

.rule-list li {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: var(--s2) var(--s3);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-control);
  background: var(--face-page);
}

.rule-summary {
  display: flex;
  align-items: center;
  gap: var(--s2);
  flex-wrap: wrap;
}

.rule-actions {
  display: flex;
  gap: var(--s2);
}

.empty {
  padding: var(--s4);
  text-align: center;
  font-size: var(--fs-label);
  color: var(--ink-muted);
  border: 1px dashed var(--line-strong);
  border-radius: var(--r-control);
}

.draft-form {
  border: 1px solid var(--line-strong);
  border-radius: var(--r-panel);
  padding: var(--s3);
  display: flex;
  flex-direction: column;
  gap: var(--s3);
}

.draft-form h5 {
  margin: 0;
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.form-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: var(--s2);
}

.form-grid label {
  display: flex;
  flex-direction: column;
  gap: var(--s1);
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  color: var(--ink-subtle);
}

.form-grid input,
.form-grid textarea {
  padding: var(--s2);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-cell);
  font-size: var(--fs-body);
  color: var(--ink);
  background: var(--face-page);
  font-family: inherit;
}

.form-grid input:focus-visible,
.form-grid textarea:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}

.form-grid textarea {
  resize: vertical;
}

.full {
  grid-column: span 3;
}

.form-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--s2);
}
</style>
