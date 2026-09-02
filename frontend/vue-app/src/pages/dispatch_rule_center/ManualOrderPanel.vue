<script setup lang="ts">
import { computed, ref } from 'vue';
import UiButton from '@/components/ui/UiButton.vue';
import UiSelect from '@/components/ui/UiSelect.vue';
import FieldOverlayForm from '@/components/FieldOverlayForm.vue';
import type { FieldOverlay, FieldReferenceEntry } from '@/composables/useFieldOverlays';
import type { DepartmentResponse, TaskTypeResponse } from './dispatchRuleWorkbenchApi';
import type { ManualOrderDraft } from './useDispatchRuleWorkbench';

const props = defineProps<{
  draft: ManualOrderDraft;
  departments: DepartmentResponse[];
  taskTypes: TaskTypeResponse[];
  saving: boolean;
  disabled: boolean;
  lastCreatedOrderId?: string | null;
  fieldOverlays?: FieldOverlay[];
  fieldCatalogEntries?: Record<string, Array<{ code: string; name: string }>>;
  fieldReferenceEntries?: Record<string, FieldReferenceEntry[]>;
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

/* UiSelect 收 string，桥回受控表单 */
function stringBridge(key: 'task_type' | 'department_id' | 'publication_state' | 'leg_scope') {
  return computed<string>({
    get: () => props.draft[key],
    set: (value) => update(key, value),
  });
}

const taskTypeModel = stringBridge('task_type');
const departmentModel = stringBridge('department_id');
const publicationModel = stringBridge('publication_state');
const legScopeModel = stringBridge('leg_scope');

const taskTypeOptions = computed(() => [
  { value: '', label: '— 选择任务类型 —' },
  ...props.taskTypes.map((t) => ({ value: t.code, label: `${t.name} (${t.code})` })),
]);

const departmentOptions = computed(() => [
  { value: '', label: '— 默认当前科室 —' },
  ...props.departments.map((d) => ({ value: d.id, label: d.name })),
]);

const publicationOptions = [
  { value: 'prepublished', label: '预发布' },
  { value: 'published', label: '已发布' },
  { value: 'draft', label: '草稿' },
];

const legScopeOptions = [
  { value: 'outbound', label: '出港' },
  { value: 'inbound', label: '进港' },
  { value: 'both', label: '双向' },
];

const payloadPreview = computed(() => {
  const draft = props.draft;
  return {
    flight_id: draft.flight_id || null,
    task_type: draft.task_type || null,
    department_id: draft.department_id || null,
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
    attributes: draft.attributes,
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
        <UiSelect
          v-model="taskTypeModel"
          :options="taskTypeOptions"
          label="任务类型"
          min-width="100%"
        />
      </label>
      <label>承担科室
        <UiSelect
          v-model="departmentModel"
          :options="departmentOptions"
          label="承担科室"
          min-width="100%"
        />
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
        <UiSelect
          v-model="publicationModel"
          :options="publicationOptions"
          label="发布状态"
          min-width="100%"
        />
      </label>
      <label>航段
        <UiSelect
          v-model="legScopeModel"
          :options="legScopeOptions"
          label="航段"
          min-width="100%"
        />
      </label>
      <label class="checkbox-label">
        <input :checked="localDraft.manual_lock" type="checkbox" @change="update('manual_lock', ($event.target as HTMLInputElement).checked)">
        手动锁定
      </label>
      <label class="full-row">备注
        <textarea :value="localDraft.remarks" rows="2" @input="update('remarks', ($event.target as HTMLTextAreaElement).value)" />
      </label>
      <FieldOverlayForm
        class="full-row overlay-fields"
        :model-value="localDraft.attributes"
        :overlays="props.fieldOverlays ?? []"
        :catalog-entries="props.fieldCatalogEntries ?? {}"
        :reference-entries="props.fieldReferenceEntries ?? {}"
        @update:model-value="update('attributes', $event)"
      />

      <div class="form-actions full-row">
        <UiButton @click="showPayloadPreview = !showPayloadPreview">
          {{ showPayloadPreview ? '隐藏 Payload' : '查看 Payload' }}
        </UiButton>
        <UiButton native-type="submit" variant="primary" :disabled="!canSubmit">
          {{ saving ? '提交中…' : '创建派工单' }}
        </UiButton>
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
/* 按钮归 UiButton、下拉归 UiSelect；这里只留表单与页内反馈。 */
.manual-order-panel {
  display: flex;
  flex-direction: column;
  gap: var(--s3);
}

.head h3 {
  margin: 0;
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.muted {
  font-size: var(--fs-label);
  color: var(--ink-muted);
  margin: var(--s1) 0 0;
}

.muted code {
  font-family: var(--mono);
  background: var(--face-page);
  padding: 1px 4px;
  border-radius: var(--r-cell);
}

.form-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: var(--s3);
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
  min-height: var(--h-md);
  padding: var(--s2) var(--s3);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-cell);
  font-size: var(--fs-body);
  color: var(--ink);
  background: var(--face-page);
  font-family: inherit;
  box-sizing: border-box;
}

.form-grid textarea {
  min-height: 64px;
  resize: vertical;
}

.form-grid input:focus-visible,
.form-grid textarea:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}

.full-row {
  grid-column: span 3;
}

.checkbox-label {
  flex-direction: row !important;
  align-items: center;
  gap: var(--s2) !important;
}

.checkbox-label input {
  min-height: auto;
  accent-color: var(--act);
}

.required {
  color: var(--danger);
}

.form-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--s2);
  border-top: 1px dashed var(--line-strong);
  padding-top: var(--s2);
}

.payload-preview {
  /* 开发者预览刻意用深色码块，不随主题翻转 */
  background: #0f172a;
  color: #f1f5f9;
  padding: var(--s3);
  border-radius: var(--r-control);
  font-family: var(--mono);
  font-size: var(--fs-label);
  max-height: 240px;
  overflow: auto;
}

.success-banner {
  background: var(--ok-soft);
  border: 1px solid color-mix(in srgb, var(--ok) 40%, transparent);
  color: var(--ok);
  padding: var(--s2) var(--s3);
  border-radius: var(--r-control);
  font-size: var(--fs-body);
}

.success-banner a {
  color: var(--ok);
  margin-left: var(--s2);
}
</style>
