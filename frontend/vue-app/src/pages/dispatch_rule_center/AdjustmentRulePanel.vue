<script setup lang="ts">
import { computed, ref } from 'vue';
import { useToast } from '@/composables/useToast';
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
          <span class="badge" :data-status="rule.status">{{ rule.status }}</span>
          <span class="muted">v{{ rule.version_no }} · {{ rule.actions.length }} 个动作</span>
        </div>
        <button
          type="button"
          class="ghost"
          :disabled="disabled"
          @click="loadRule(rule)"
        >
          编辑
        </button>
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
          <select v-model="draft.status" @change="onFieldChange">
            <option value="draft">草稿</option>
            <option value="active">启用</option>
            <option value="archived">归档</option>
          </select>
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
        <button type="button" class="ghost" @click="clearDraft">
          清空
        </button>
        <button type="submit" class="btn primary" :disabled="disabled || saving || !taskTypeCode">
          {{ saving ? '保存中…' : (draft.rule_id ? '保存修改' : '新增规则') }}
        </button>
      </div>
    </form>
  </section>
</template>

<style scoped>
.adjustment-rule-panel { display: flex; flex-direction: column; gap: 12px; }
.head { display: flex; justify-content: space-between; align-items: center; }
.head h4 { margin: 0; font-size: 14px; }
.muted { font-size: 12px; color: var(--text-tertiary); }
.rule-list { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 6px; }
.rule-list li {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 12px;
  border: 1px solid var(--border-light);
  border-radius: 8px;
  background: var(--ws-surface-muted);
}
.rule-summary { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.badge {
  padding: 2px 8px; border-radius: 999px; background: var(--ws-surface-muted);
  color: var(--text-secondary); font-size: 11px; font-weight: 600;
}
.badge[data-status='active'] { background: rgba(34, 197, 94, 0.15); color: #15803d; }
.ghost { background: transparent; border: 1px solid var(--border-light); padding: 4px 10px; border-radius: 6px; cursor: pointer; font-size: 12px; }
.ghost:disabled { opacity: 0.5; cursor: not-allowed; }
.empty { padding: 16px; text-align: center; font-size: 12px; color: var(--text-tertiary); border: 1px dashed var(--border-light); border-radius: 8px; }
.draft-form { border: 1px solid var(--border-light); border-radius: 10px; padding: 12px; display: flex; flex-direction: column; gap: 10px; }
.draft-form h5 { margin: 0; font-size: 13px; }
.form-grid { display: grid; grid-template-columns: repeat(2, 1fr); gap: 8px; }
.form-grid label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; }
.form-grid input, .form-grid select, .form-grid textarea {
  padding: 6px 8px; border: 1px solid var(--border-light); border-radius: 6px; font-size: 12px;
}
.full { grid-column: span 2; }
.form-actions { display: flex; justify-content: flex-end; gap: 8px; }
.btn { border: 1px solid var(--border-light); border-radius: 8px; padding: 6px 14px; background: var(--bg-card); cursor: pointer; font-size: 13px; }
.btn.primary { background: var(--system-blue); color: var(--text-inverse); border-color: var(--system-blue); }
.btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
