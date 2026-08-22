<script setup lang="ts">
import { computed, ref } from 'vue';
import { useToast } from '@/composables/useToast';
import UiButton from '@/components/ui/UiButton.vue';
import UiPill from '@/components/ui/UiPill.vue';
import UiSelect from '@/components/ui/UiSelect.vue';
import type {
  GenerationAdjustmentRulePayload,
  GenerationAdjustmentRuleResponse,
} from './dispatchRuleWorkbenchApi';

const props = defineProps<{
  rules: GenerationAdjustmentRuleResponse[];
  taskTypeCode: string;
  saving: boolean;
  disabled: boolean;
}>();

const emit = defineEmits<{
  (e: 'save', payload: GenerationAdjustmentRulePayload): void;
  (e: 'dirty', value: boolean): void;
}>();

const toast = useToast();

interface Draft {
  rule_id: string;
  rule_name: string;
  status: string;
  conditions_json: string;
  actions_json: string;
  notes: string;
}

function emptyDraft(): Draft {
  return {
    rule_id: '',
    rule_name: '',
    status: 'draft',
    conditions_json: '{}',
    actions_json: '[]',
    notes: '',
  };
}

const draft = ref<Draft>(emptyDraft());

const rulesForTaskType = computed(() =>
  props.rules.filter((r) => r.task_type === props.taskTypeCode),
);

function loadRule(rule: GenerationAdjustmentRuleResponse): void {
  draft.value = {
    rule_id: rule.id,
    rule_name: rule.rule_name ?? '',
    status: rule.status,
    conditions_json: JSON.stringify(rule.conditions ?? {}, null, 2),
    actions_json: JSON.stringify(rule.actions ?? [], null, 2),
    notes: rule.notes ?? '',
  };
}

function clearDraft(): void {
  draft.value = emptyDraft();
  emit('dirty', false);
}

function submit(): void {
  let conditions: Record<string, unknown> = {};
  let actions: unknown[] = [];
  try { conditions = JSON.parse(draft.value.conditions_json || '{}'); }
  catch (e) {
    toast.showToast('error', `条件 JSON 无效: ${e instanceof Error ? e.message : '未知错误'}`, { duration: 5000 });
    return;
  }
  try {
    const parsedActions = JSON.parse(draft.value.actions_json || '[]');
    if (!Array.isArray(parsedActions)) throw new Error('动作必须是数组');
    actions = parsedActions;
  } catch (e) {
    toast.showToast('error', `动作 JSON 无效: ${e instanceof Error ? e.message : '未知错误'}`, { duration: 5000 });
    return;
  }

  emit('save', {
    rule_id: draft.value.rule_id || null,
    rule_name: draft.value.rule_name || null,
    task_type: props.taskTypeCode,
    status: draft.value.status,
    conditions,
    actions,
    notes: draft.value.notes || null,
  });
}

function onFieldChange(): void {
  emit('dirty', true);
}

type PillTone = 'act' | 'ok' | 'warn' | 'danger' | 'mute';

function ruleStatusTone(status: string): PillTone {
  if (status === 'active') return 'ok';
  return 'mute';
}

/* UiSelect 收 string，桥回本地 draft 并标脏 */
const statusModel = computed<string>({
  get: () => draft.value.status,
  set: (value) => {
    draft.value.status = value;
    onFieldChange();
  },
});

const statusOptions = [
  { value: 'draft', label: '草稿' },
  { value: 'active', label: '启用' },
  { value: 'archived', label: '归档' },
];
</script>

<template>
  <section class="adjustment-rule-panel" aria-label="生成调整规则">
    <header class="head">
      <h4>生成调整规则</h4>
      <span class="muted">共 {{ rulesForTaskType.length }} 条</span>
    </header>

    <ul v-if="rulesForTaskType.length" class="rule-list">
      <li v-for="rule in rulesForTaskType" :key="rule.id">
        <div class="rule-summary">
          <strong>{{ rule.rule_name || rule.id }}</strong>
          <UiPill :tone="ruleStatusTone(rule.status)">
            {{ rule.status }}
          </UiPill>
          <span class="muted">v{{ rule.version_no }} · {{ rule.actions.length }} 个动作</span>
        </div>
        <UiButton
          :disabled="disabled"
          @click="loadRule(rule)"
        >
          编辑
        </UiButton>
      </li>
    </ul>
    <div v-else class="empty">
      尚未配置生成调整规则。
    </div>

    <form class="draft-form" @submit.prevent="submit">
      <h5>{{ draft.rule_id ? '编辑调整规则' : '新增调整规则' }}</h5>
      <div class="form-grid">
        <label>规则名称 <input v-model="draft.rule_name" type="text" @input="onFieldChange"></label>
        <label>状态
          <UiSelect
            v-model="statusModel"
            :options="statusOptions"
            label="状态"
            min-width="100%"
          />
        </label>
        <label class="full">条件 JSON
          <textarea v-model="draft.conditions_json" rows="3" @input="onFieldChange" />
        </label>
        <label class="full">动作 JSON (数组)
          <textarea v-model="draft.actions_json" rows="4" @input="onFieldChange" />
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
.adjustment-rule-panel {
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
  grid-template-columns: repeat(2, 1fr);
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
  grid-column: span 2;
}

.form-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--s2);
}
</style>
