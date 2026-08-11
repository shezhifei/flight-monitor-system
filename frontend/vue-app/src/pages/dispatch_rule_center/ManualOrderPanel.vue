<script setup lang="ts">
import { computed, ref } from 'vue';
import type { DepartmentResponse, TaskTypeResponse } from './dispatchRuleWorkbenchApi';
import type { ManualOrderDraft } from './useDispatchRuleWorkbench';

const props = defineProps<{
  draft: ManualOrderDraft;
  departments: DepartmentResponse[];
  taskTypes: TaskTypeResponse[];
  saving: boolean;
  disabled: boolean;
  lastCreatedOrderId?: string | null;
}>();

const emit = defineEmits<{
  (e: 'update:draft', value: ManualOrderDraft): void;
  (e: 'submit'): void;
  (e: 'dirty', value: boolean): void;
}>();

const showPayloadPreview = ref(false);

const localDraft = computed({
  get: () => props.draft,
  set: (value) => emit('update:draft', value),
});

function update<K extends keyof ManualOrderDraft>(key: K, value: ManualOrderDraft[K]): void {
  emit('update:draft', { ...props.draft, [key]: value });
  emit('dirty', true);
}

const payloadPreview = computed(() => {
  const draft = props.draft;
  return {
    flight_id: draft.flight_id || null,
    task_type: draft.task_type || null,
    department_id: draft.department_id || null,
    team_id: draft.team_id || null,
    individual_user_id: draft.individual_user_id || null,
    stand_id: draft.stand_id || null,
    location: draft.location || null,
    planned_start_time: draft.planned_start_time
      ? new Date(draft.planned_start_time).toISOString()
      : null,
    planned_end_time: draft.planned_end_time
      ? new Date(draft.planned_end_time).toISOString()
      : null,
    priority: draft.priority,
    publication_state: draft.publication_state,
    source_type: draft.source_type,
    leg_scope: draft.leg_scope,
    manual_lock: draft.manual_lock,
    remarks: draft.remarks || null,
  };
});

const canSubmit = computed(() => Boolean(props.draft.task_type && !props.disabled && !props.saving));

function onSubmit(): void {
  if (!canSubmit.value) return;
  emit('submit');
}
</script>

<template>
  <section class="manual-order-panel" aria-label="人工创建派工单">
    <header class="head">
      <h3>人工创建派工单</h3>
      <p class="muted">
        提交至 <code>POST /api/v2/dispatch-orders</code>
      </p>
    </header>

    <form class="form-grid" @submit.prevent="onSubmit">
      <label>关联航班 ID
        <input
          :value="localDraft.flight_id"
          type="text"
          placeholder="可选"
          @input="update('flight_id', ($event.target as HTMLInputElement).value)"
        >
      </label>
      <label>任务类型 <span class="required">*</span>
        <select :value="localDraft.task_type" required @change="update('task_type', ($event.target as HTMLSelectElement).value)">
          <option value="">— 选择任务类型 —</option>
          <option v-for="t in taskTypes" :key="t.code" :value="t.code">{{ t.name }} ({{ t.code }})</option>
        </select>
      </label>
      <label>承担科室
        <select :value="localDraft.department_id" @change="update('department_id', ($event.target as HTMLSelectElement).value)">
          <option value="">— 默认当前科室 —</option>
          <option v-for="d in departments" :key="d.id" :value="d.id">{{ d.name }}</option>
        </select>
      </label>
      <label>班组 ID
        <input :value="localDraft.team_id" type="text" @input="update('team_id', ($event.target as HTMLInputElement).value)">
      </label>
      <label>个人指派
        <input :value="localDraft.individual_user_id" type="text" @input="update('individual_user_id', ($event.target as HTMLInputElement).value)">
      </label>
      <label>机位
        <input :value="localDraft.stand_id" type="text" @input="update('stand_id', ($event.target as HTMLInputElement).value)">
      </label>
      <label>位置描述
        <input :value="localDraft.location" type="text" @input="update('location', ($event.target as HTMLInputElement).value)">
      </label>
      <label>计划开始
        <input :value="localDraft.planned_start_time" type="datetime-local" @input="update('planned_start_time', ($event.target as HTMLInputElement).value)">
      </label>
      <label>计划结束
        <input :value="localDraft.planned_end_time" type="datetime-local" @input="update('planned_end_time', ($event.target as HTMLInputElement).value)">
      </label>
      <label>优先级
        <input
          :value="localDraft.priority"
          type="number"
          min="0"
          max="999"
          @input="update('priority', Number(($event.target as HTMLInputElement).value))"
        >
      </label>
      <label>发布状态
        <select :value="localDraft.publication_state" @change="update('publication_state', ($event.target as HTMLSelectElement).value)">
          <option value="prepublished">预发布</option>
          <option value="published">已发布</option>
          <option value="draft">草稿</option>
        </select>
      </label>
      <label>航段
        <select :value="localDraft.leg_scope" @change="update('leg_scope', ($event.target as HTMLSelectElement).value)">
          <option value="outbound">出港</option>
          <option value="inbound">进港</option>
          <option value="both">双向</option>
        </select>
      </label>
      <label class="checkbox-label">
        <input :checked="localDraft.manual_lock" type="checkbox" @change="update('manual_lock', ($event.target as HTMLInputElement).checked)">
        手动锁定
      </label>
      <label class="full-row">备注
        <textarea :value="localDraft.remarks" rows="2" @input="update('remarks', ($event.target as HTMLTextAreaElement).value)" />
      </label>

      <div class="form-actions full-row">
        <button type="button" class="btn ghost" @click="showPayloadPreview = !showPayloadPreview">
          {{ showPayloadPreview ? '隐藏 Payload' : '查看 Payload' }}
        </button>
        <button type="submit" class="btn primary" :disabled="!canSubmit">
          {{ saving ? '提交中…' : '创建派工单' }}
        </button>
      </div>
    </form>

    <pre v-if="showPayloadPreview" class="payload-preview">{{ JSON.stringify(payloadPreview, null, 2) }}</pre>

    <div v-if="lastCreatedOrderId" class="success-banner">
      已创建派工单 <strong>{{ lastCreatedOrderId }}</strong> ·
      <a href="/frontend/dispatch_board.html">前往派工台</a>
    </div>
  </section>
</template>

<style scoped>
.manual-order-panel {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.head h3 { margin: 0; font-size: 16px; }
.muted { font-size: 12px; color: var(--text-tertiary); margin: 4px 0 0; }
.muted code { background: var(--ws-surface-muted); padding: 1px 4px; border-radius: 4px; }
.form-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 10px;
}
.form-grid label {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
  color: var(--text-secondary);
}
.form-grid input, .form-grid select, .form-grid textarea {
  padding: 6px 10px;
  border: 1px solid var(--border-light);
  border-radius: 6px;
  font-size: 13px;
}
.full-row { grid-column: span 3; }
.checkbox-label {
  flex-direction: row !important;
  align-items: center;
  gap: 6px !important;
}
.required { color: var(--system-red); }
.form-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  border-top: 1px dashed var(--border-light);
  padding-top: 8px;
}
.btn {
  border: 1px solid var(--border-light);
  border-radius: 8px;
  background: var(--bg-card);
  padding: 8px 16px;
  cursor: pointer;
  font-size: 13px;
}
.btn.primary { background: var(--system-blue); color: var(--text-inverse); border-color: var(--system-blue); }
.btn.ghost { background: transparent; }
.btn:disabled { opacity: 0.5; cursor: not-allowed; }
.payload-preview {
  background: #0f172a;
  color: #f1f5f9;
  padding: 12px;
  border-radius: 8px;
  font-size: 11px;
  max-height: 240px;
  overflow: auto;
}
.success-banner {
  background: var(--success-bg-subtle);
  border: 1px solid var(--success-border-subtle);
  color: #14532d;
  padding: 8px 12px;
  border-radius: 8px;
  font-size: 13px;
}
.success-banner a { color: #15803d; margin-left: 6px; }
</style>
