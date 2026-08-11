<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue';
import type {
  WorkflowFormJsonSchemaObject,
  WorkflowFormJsonSchemaProperty,
  WorkflowFormUiSchemaField,
  WorkflowFormUiSchemaObject,
} from '../../types/backend';

type PrimitiveOptionValue = string | number | boolean;
type FieldKind =
  | 'text'
  | 'textarea'
  | 'integer'
  | 'number'
  | 'boolean'
  | 'date-time'
  | 'single-select'
  | 'multi-select'
  | 'unsupported';

interface FieldOption {
  label: string;
  value: PrimitiveOptionValue;
  serialized: string;
}

interface ResolvedField {
  key: string;
  label: string;
  description: string;
  required: boolean;
  kind: FieldKind;
  placeholder: string;
  property: WorkflowFormJsonSchemaProperty;
  options: FieldOption[];
}

const props = withDefaults(defineProps<{
  schema?: WorkflowFormJsonSchemaObject | null;
  uiSchema?: WorkflowFormUiSchemaObject | null;
  initialValue?: Record<string, unknown> | null;
  readonly?: boolean;
  submitting?: boolean;
  submitText?: string;
  emptyText?: string;
}>(), {
  schema: null,
  uiSchema: null,
  initialValue: null,
  readonly: false,
  submitting: false,
  submitText: '提交表单',
  emptyText: '暂无可展示内容',
});

const emit = defineEmits<{
  (e: 'submit', payload: Record<string, unknown>): void;
}>();

const formState = reactive<Record<string, unknown>>({});
const validationError = ref('');

const schemaProperties = computed<Record<string, WorkflowFormJsonSchemaProperty>>(
  () => props.schema?.properties || {},
);
const requiredFields = computed<Set<string>>(
  () => new Set(Array.isArray(props.schema?.required) ? props.schema?.required : []),
);

const resolvedFields = computed<ResolvedField[]>(() => {
  const properties = schemaProperties.value;
  const uiSchema = props.uiSchema || {};
  const knownKeys = Object.keys(properties);
  const order = Array.isArray(uiSchema['ui:order']) ? uiSchema['ui:order'] : [];
  const orderedKeys = [
    ...order.filter((item): item is string => item !== '*'),
    ...knownKeys.filter((key) => !order.includes(key)),
  ];

  return orderedKeys
    .filter((key) => key in properties)
    .map((key) => {
      const property = properties[key];
      const uiField = getUiSchemaField(uiSchema, key);
      const kind = resolveFieldKind(property, uiField);
      return {
        key,
        label: String(property.title || key),
        description: String(uiField?.['ui:help'] || property.description || ''),
        required: requiredFields.value.has(key),
        kind,
        placeholder: String(uiField?.['ui:placeholder'] || ''),
        property,
        options: buildOptions(property),
      };
    });
});

const readonlyEntries = computed(() => {
  const source = props.initialValue || {};
  const fields = resolvedFields.value;

  if (fields.length > 0) {
    return fields
      .map((field) => ({
        key: field.key,
        label: field.label,
        kind: field.kind,
        value: source[field.key],
        options: field.options,
      }))
      .filter((entry) => hasDisplayValue(entry.value));
  }

  return Object.entries(source)
    .filter(([, value]) => hasDisplayValue(value))
    .map(([key, value]) => ({
      key,
      label: key,
      kind: 'unsupported' as FieldKind,
      value,
      options: [] as FieldOption[],
    }));
});

watch(
  () => [props.schema, props.initialValue, props.uiSchema],
  () => {
    syncFormState();
    validationError.value = '';
  },
  { deep: true, immediate: true },
);

function getUiSchemaField(uiSchema: WorkflowFormUiSchemaObject, key: string): WorkflowFormUiSchemaField | null {
  const candidate = uiSchema[key];
  if (candidate && typeof candidate === 'object' && !Array.isArray(candidate)) {
    return candidate as WorkflowFormUiSchemaField;
  }
  return null;
}

function serializeOptionValue(value: PrimitiveOptionValue): string {
  return JSON.stringify(value);
}

function buildOptions(property: WorkflowFormJsonSchemaProperty): FieldOption[] {
  const enumValues = Array.isArray(property.enum)
    ? property.enum
    : Array.isArray(property.items?.enum)
      ? property.items.enum
      : [];

  return enumValues
    .filter((value): value is PrimitiveOptionValue => ['string', 'number', 'boolean'].includes(typeof value))
    .map((value) => ({
      label: String(value),
      value,
      serialized: serializeOptionValue(value),
    }));
}

function resolveSchemaType(property: WorkflowFormJsonSchemaProperty): string {
  const candidate = property.type;
  if (Array.isArray(candidate)) {
    return candidate.find((item) => item !== 'null') || candidate[0] || '';
  }
  return String(candidate || '');
}

function resolveFieldKind(
  property: WorkflowFormJsonSchemaProperty,
  uiField: WorkflowFormUiSchemaField | null,
): FieldKind {
  const widget = String(uiField?.['ui:widget'] || '').trim().toLowerCase();
  const schemaType = resolveSchemaType(property);

  if (widget === 'textarea') {
    return 'textarea';
  }
  if (widget === 'select' && Array.isArray(property.enum)) {
    return 'single-select';
  }
  if ((widget === 'checkboxes' || widget === 'select') && schemaType === 'array') {
    return 'multi-select';
  }
  if (schemaType === 'string' && property.format === 'date-time') {
    return 'date-time';
  }
  if (schemaType === 'string' && Array.isArray(property.enum)) {
    return 'single-select';
  }
  if (schemaType === 'array' && Array.isArray(property.items?.enum)) {
    return 'multi-select';
  }
  if (schemaType === 'string') {
    return 'text';
  }
  if (schemaType === 'integer') {
    return 'integer';
  }
  if (schemaType === 'number') {
    return 'number';
  }
  if (schemaType === 'boolean') {
    return 'boolean';
  }
  return 'unsupported';
}

function clearFormState(): void {
  Object.keys(formState).forEach((key) => {
    delete formState[key];
  });
}

function toDateTimeLocalValue(value: unknown): string {
  if (typeof value !== 'string' || !value.trim()) {
    return '';
  }
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return '';
  }

  const offset = parsed.getTimezoneOffset() * 60000;
  const local = new Date(parsed.getTime() - offset);
  return local.toISOString().slice(0, 16);
}

function syncFormState(): void {
  clearFormState();
  const source = props.initialValue || {};

  resolvedFields.value.forEach((field) => {
    const rawValue = source[field.key] ?? field.property.default;

    if (field.kind === 'boolean') {
      formState[field.key] = typeof rawValue === 'boolean' ? rawValue : false;
      return;
    }
    if (field.kind === 'multi-select') {
      const values = Array.isArray(rawValue) ? rawValue : [];
      formState[field.key] = values
        .filter((value): value is PrimitiveOptionValue => ['string', 'number', 'boolean'].includes(typeof value))
        .map((value) => serializeOptionValue(value));
      return;
    }
    if (field.kind === 'single-select') {
      formState[field.key] = ['string', 'number', 'boolean'].includes(typeof rawValue)
        ? serializeOptionValue(rawValue as PrimitiveOptionValue)
        : '';
      return;
    }
    if (field.kind === 'date-time') {
      formState[field.key] = toDateTimeLocalValue(rawValue);
      return;
    }
    if (field.kind === 'integer' || field.kind === 'number') {
      formState[field.key] = rawValue === null || rawValue === undefined ? '' : String(rawValue);
      return;
    }
    formState[field.key] = rawValue === null || rawValue === undefined ? '' : String(rawValue);
  });
}

function parseSingleSelectValue(field: ResolvedField, rawValue: unknown): PrimitiveOptionValue | null {
  const selected = String(rawValue || '');
  if (!selected) {
    return null;
  }
  return field.options.find((option) => option.serialized === selected)?.value ?? null;
}

function buildPayload(): Record<string, unknown> {
  const payload: Record<string, unknown> = {};

  resolvedFields.value.forEach((field) => {
    const rawValue = formState[field.key];

    if (field.kind === 'boolean') {
      payload[field.key] = Boolean(rawValue);
      return;
    }
    if (field.kind === 'multi-select') {
      const selected = Array.isArray(rawValue) ? rawValue : [];
      payload[field.key] = selected
        .map((item) => field.options.find((option) => option.serialized === item)?.value)
        .filter((value): value is PrimitiveOptionValue => value !== undefined);
      return;
    }
    if (field.kind === 'single-select') {
      const selected = parseSingleSelectValue(field, rawValue);
      if (selected !== null) {
        payload[field.key] = selected;
      }
      return;
    }
    if (field.kind === 'date-time') {
      const normalized = String(rawValue || '').trim();
      if (normalized) {
        payload[field.key] = new Date(normalized).toISOString();
      }
      return;
    }
    if (field.kind === 'integer') {
      const normalized = String(rawValue || '').trim();
      if (normalized) {
        payload[field.key] = Number.parseInt(normalized, 10);
      }
      return;
    }
    if (field.kind === 'number') {
      const normalized = String(rawValue || '').trim();
      if (normalized) {
        payload[field.key] = Number(normalized);
      }
      return;
    }

    const normalized = String(rawValue || '').trim();
    if (normalized) {
      payload[field.key] = normalized;
    }
  });

  return payload;
}

function hasValueForField(field: ResolvedField, value: unknown): boolean {
  if (field.kind === 'boolean') {
    return typeof value === 'boolean';
  }
  if (field.kind === 'multi-select') {
    return Array.isArray(value) && value.length > 0;
  }
  return value !== null && value !== undefined && String(value).trim() !== '';
}

function validateBeforeSubmit(payload: Record<string, unknown>): string {
  for (const field of resolvedFields.value) {
    if (!field.required) {
      continue;
    }
    if (!hasValueForField(field, payload[field.key])) {
      return `${field.label}为必填项`;
    }
  }
  return '';
}

function updateFieldValue(key: string, value: unknown): void {
  formState[key] = value;
  validationError.value = '';
}

function getTextLikeValue(key: string): string {
  const value = formState[key];
  return typeof value === 'string' ? value : value === null || value === undefined ? '' : String(value);
}

function getCheckboxValue(key: string): boolean {
  return Boolean(formState[key]);
}

function getMultiSelectValue(key: string): string[] {
  const value = formState[key];
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : [];
}

function onMultiSelectChange(key: string, event: Event): void {
  const target = event.target as HTMLSelectElement | null;
  updateFieldValue(
    key,
    target ? Array.from(target.selectedOptions).map((option) => option.value) : [],
  );
}

function onSubmit(): void {
  const payload = buildPayload();
  const error = validateBeforeSubmit(payload);
  validationError.value = error;
  if (!error) {
    emit('submit', payload);
  }
}

function formatDateTimeDisplay(value: unknown): string {
  if (typeof value !== 'string' || !value.trim()) {
    return '--';
  }
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return String(value);
  }
  return parsed.toLocaleString('zh-CN');
}

function hasDisplayValue(value: unknown): boolean {
  if (Array.isArray(value)) {
    return value.length > 0;
  }
  if (value && typeof value === 'object') {
    return Object.keys(value as Record<string, unknown>).length > 0;
  }
  return value !== null && value !== undefined && String(value).trim() !== '';
}

function formatReadonlyValue(kind: FieldKind, value: unknown, options: FieldOption[]): string {
  if (value === null || value === undefined) {
    return '--';
  }
  if (kind === 'boolean') {
    return value ? '是' : '否';
  }
  if (kind === 'multi-select' && Array.isArray(value)) {
    return value
      .map((item) => options.find((option) => option.value === item)?.label || String(item))
      .join(' / ') || '--';
  }
  if (kind === 'single-select') {
    return options.find((option) => option.value === value)?.label || String(value);
  }
  if (kind === 'date-time') {
    return formatDateTimeDisplay(value);
  }
  if (Array.isArray(value)) {
    return value.join(' / ');
  }
  if (typeof value === 'object') {
    return JSON.stringify(value);
  }
  return String(value);
}
</script>

<template>
  <div class="workflow-form">
    <div v-if="readonly">
      <div v-if="readonlyEntries.length > 0" class="workflow-form-readonly">
        <div
          v-for="entry in readonlyEntries"
          :key="entry.key"
          class="workflow-form-readonly-row"
        >
          <div class="workflow-form-readonly-label">
            {{ entry.label }}
          </div>
          <div class="workflow-form-readonly-value">
            {{ formatReadonlyValue(entry.kind, entry.value, entry.options) }}
          </div>
        </div>
      </div>
      <div v-else class="workflow-form-empty">
        {{ emptyText }}
      </div>
    </div>

    <form v-else class="workflow-form-editable" @submit.prevent="onSubmit">
      <div v-if="resolvedFields.length > 0" class="workflow-form-fields">
        <div v-for="field in resolvedFields" :key="field.key" class="workflow-form-field">
          <label class="workflow-form-label" :for="`workflow-form-${field.key}`">
            {{ field.label }}
            <span v-if="field.required" class="workflow-form-required">*</span>
          </label>
          <div v-if="field.description" class="workflow-form-description">
            {{ field.description }}
          </div>

          <textarea
            v-if="field.kind === 'textarea'"
            :id="`workflow-form-${field.key}`"
            :value="getTextLikeValue(field.key)"
            class="workflow-form-input workflow-form-textarea"
            :placeholder="field.placeholder"
            rows="4"
            @input="updateFieldValue(field.key, ($event.target as HTMLTextAreaElement).value)"
          />

          <input
            v-else-if="field.kind === 'text'"
            :id="`workflow-form-${field.key}`"
            :value="getTextLikeValue(field.key)"
            class="workflow-form-input"
            type="text"
            :placeholder="field.placeholder"
            @input="updateFieldValue(field.key, ($event.target as HTMLInputElement).value)"
          >

          <input
            v-else-if="field.kind === 'integer'"
            :id="`workflow-form-${field.key}`"
            :value="getTextLikeValue(field.key)"
            class="workflow-form-input"
            type="number"
            step="1"
            :placeholder="field.placeholder"
            @input="updateFieldValue(field.key, ($event.target as HTMLInputElement).value)"
          >

          <input
            v-else-if="field.kind === 'number'"
            :id="`workflow-form-${field.key}`"
            :value="getTextLikeValue(field.key)"
            class="workflow-form-input"
            type="number"
            step="any"
            :placeholder="field.placeholder"
            @input="updateFieldValue(field.key, ($event.target as HTMLInputElement).value)"
          >

          <label
            v-else-if="field.kind === 'boolean'"
            class="workflow-form-checkbox"
            :for="`workflow-form-${field.key}`"
          >
            <input
              :id="`workflow-form-${field.key}`"
              :checked="getCheckboxValue(field.key)"
              type="checkbox"
              @change="updateFieldValue(field.key, ($event.target as HTMLInputElement).checked)"
            >
            <span>是/否</span>
          </label>

          <input
            v-else-if="field.kind === 'date-time'"
            :id="`workflow-form-${field.key}`"
            :value="getTextLikeValue(field.key)"
            class="workflow-form-input"
            type="datetime-local"
            @input="updateFieldValue(field.key, ($event.target as HTMLInputElement).value)"
          >

          <select
            v-else-if="field.kind === 'single-select'"
            :id="`workflow-form-${field.key}`"
            :value="getTextLikeValue(field.key)"
            class="workflow-form-input"
            @change="updateFieldValue(field.key, ($event.target as HTMLSelectElement).value)"
          >
            <option value="">
              请选择
            </option>
            <option
              v-for="option in field.options"
              :key="option.serialized"
              :value="option.serialized"
            >
              {{ option.label }}
            </option>
          </select>

          <select
            v-else-if="field.kind === 'multi-select'"
            :id="`workflow-form-${field.key}`"
            :value="getMultiSelectValue(field.key)"
            class="workflow-form-input workflow-form-multi-select"
            multiple
            @change="onMultiSelectChange(field.key, $event)"
          >
            <option
              v-for="option in field.options"
              :key="option.serialized"
              :value="option.serialized"
            >
              {{ option.label }}
            </option>
          </select>

          <div v-else class="workflow-form-unsupported">
            当前字段类型暂不支持编辑
          </div>
        </div>
      </div>

      <div v-else class="workflow-form-empty">
        {{ emptyText }}
      </div>

      <div v-if="validationError" class="workflow-form-error">
        {{ validationError }}
      </div>

      <div class="workflow-form-actions">
        <button
          class="btn btn-primary"
          type="submit"
          :disabled="submitting || resolvedFields.length === 0"
        >
          {{ submitting ? '提交中...' : submitText }}
        </button>
      </div>
    </form>
  </div>
</template>

<style scoped>
.workflow-form {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.workflow-form-fields,
.workflow-form-readonly {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.workflow-form-field,
.workflow-form-readonly-row {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.workflow-form-label,
.workflow-form-readonly-label {
  font-size: 12px;
  font-weight: 700;
  color: var(--text-primary, #102132);
}

.workflow-form-description {
  font-size: 12px;
  color: var(--text-tertiary, #8E8E93);
  line-height: 1.5;
}

.workflow-form-input,
.workflow-form-textarea {
  width: 100%;
  padding: 9px 10px;
  border: 1px solid var(--border-light, #d8e0e8);
  border-radius: 8px;
  background: var(--bg-card);
  color: var(--text-primary, #102132);
  font-size: 13px;
  line-height: 1.5;
  box-sizing: border-box;
}

.workflow-form-textarea {
  resize: vertical;
  min-height: 96px;
}

.workflow-form-checkbox {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--text-secondary, #526477);
}

.workflow-form-multi-select {
  min-height: 112px;
}

.workflow-form-required {
  color: var(--status-text-boarding-urge);
}

.workflow-form-actions {
  display: flex;
  justify-content: flex-end;
}

.workflow-form-error {
  font-size: 12px;
  color: var(--ws-danger);
}

.workflow-form-readonly-value {
  font-size: 13px;
  color: var(--text-secondary, #526477);
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-word;
}

.workflow-form-empty,
.workflow-form-unsupported {
  padding: 12px;
  border: 1px dashed var(--border-light, #d8e0e8);
  border-radius: 10px;
  background: var(--bg-secondary, #F8FAFC);
  color: var(--text-tertiary, #8E8E93);
  font-size: 12px;
  line-height: 1.6;
}
</style>
