<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue';
import type {
  WorkflowFormJsonSchemaObject,
  WorkflowFormJsonSchemaProperty,
  WorkflowFormUiSchemaField,
  WorkflowFormUiSchemaObject,
} from '../../types/backend';
import UiBanner from '../ui/UiBanner.vue';
import UiButton from '../ui/UiButton.vue';
import UiCheckChip from '../ui/UiCheckChip.vue';
import UiFacts, { type Fact } from '../ui/UiFacts.vue';
import UiField from '../ui/UiField.vue';
import UiSelect from '../ui/UiSelect.vue';

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

/** 只读态就是一叠属性：名值对只此一套配方（§3.2）。时刻用等宽（§2.4）。 */
const readonlyFacts = computed<Fact[]>(() => readonlyEntries.value.map((entry) => ({
  label: entry.label,
  value: formatReadonlyValue(entry.kind, entry.value, entry.options),
  mono: entry.kind === 'date-time',
})));

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

/** 空值那一条必须真的在 options 里：UiSelect 的兜底会拿第一条顶上来（§4.22）。 */
function singleSelectOptions(field: ResolvedField): { value: string; label: string }[] {
  return [
    { value: '', label: '请选择' },
    ...field.options.map((option) => ({ value: option.serialized, label: option.label })),
  ];
}

/** 裸控件才能给 UiField 传 for-id；button/芯片组不是 labelable（UiSelect 说明第 3 点）。 */
function fieldControlId(field: ResolvedField): string | undefined {
  if (
    field.kind === 'textarea'
    || field.kind === 'text'
    || field.kind === 'integer'
    || field.kind === 'number'
    || field.kind === 'boolean'
    || field.kind === 'date-time'
  ) {
    return `workflow-form-${field.key}`;
  }
  return undefined;
}

function getCheckboxValue(key: string): boolean {
  return Boolean(formState[key]);
}

function getMultiSelectValue(key: string): string[] {
  const value = formState[key];
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : [];
}

function toggleMultiSelect(key: string, serialized: string, checked: boolean): void {
  const current = getMultiSelectValue(key);
  const next = checked
    ? (current.includes(serialized) ? current : [...current, serialized])
    : current.filter((item) => item !== serialized);
  updateFieldValue(key, next);
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
    return '';
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
    return '';
  }
  if (kind === 'boolean') {
    return value ? '是' : '否';
  }
  if (kind === 'multi-select' && Array.isArray(value)) {
    return value
      .map((item) => options.find((option) => option.value === item)?.label || String(item))
      .join(' / ') || '';
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
      <UiFacts v-if="readonlyFacts.length > 0" :items="readonlyFacts" :columns="1" />
      <p v-else class="workflow-form-note">
        {{ emptyText }}
      </p>
    </div>

    <form v-else class="workflow-form-editable" @submit.prevent="onSubmit">
      <div v-if="resolvedFields.length > 0" class="workflow-form-fields">
        <UiField
          v-for="field in resolvedFields"
          :key="field.key"
          :label="field.label"
          :required="field.required"
          :hint="field.description"
          :for-id="fieldControlId(field)"
        >
          <textarea
            v-if="field.kind === 'textarea'"
            :id="`workflow-form-${field.key}`"
            :value="getTextLikeValue(field.key)"
            :placeholder="field.placeholder"
            rows="4"
            @input="updateFieldValue(field.key, ($event.target as HTMLTextAreaElement).value)"
          />

          <input
            v-else-if="field.kind === 'text'"
            :id="`workflow-form-${field.key}`"
            :value="getTextLikeValue(field.key)"
            type="text"
            :placeholder="field.placeholder"
            @input="updateFieldValue(field.key, ($event.target as HTMLInputElement).value)"
          >

          <input
            v-else-if="field.kind === 'integer'"
            :id="`workflow-form-${field.key}`"
            :value="getTextLikeValue(field.key)"
            type="number"
            step="1"
            :placeholder="field.placeholder"
            @input="updateFieldValue(field.key, ($event.target as HTMLInputElement).value)"
          >

          <input
            v-else-if="field.kind === 'number'"
            :id="`workflow-form-${field.key}`"
            :value="getTextLikeValue(field.key)"
            type="number"
            step="any"
            :placeholder="field.placeholder"
            @input="updateFieldValue(field.key, ($event.target as HTMLInputElement).value)"
          >

          <UiCheckChip
            v-else-if="field.kind === 'boolean'"
            :id="`workflow-form-${field.key}`"
            label="是"
            :aria-label="field.label"
            :checked="getCheckboxValue(field.key)"
            @update:checked="updateFieldValue(field.key, $event)"
          />

          <input
            v-else-if="field.kind === 'date-time'"
            :id="`workflow-form-${field.key}`"
            :value="getTextLikeValue(field.key)"
            type="datetime-local"
            @input="updateFieldValue(field.key, ($event.target as HTMLInputElement).value)"
          >

          <UiSelect
            v-else-if="field.kind === 'single-select'"
            :model-value="getTextLikeValue(field.key)"
            :options="singleSelectOptions(field)"
            :label="field.label"
            @update:model-value="updateFieldValue(field.key, $event)"
          />

          <div
            v-else-if="field.kind === 'multi-select'"
            class="workflow-form-chips"
            role="group"
            :aria-label="field.label"
          >
            <UiCheckChip
              v-for="(option, optionIndex) in field.options"
              :id="`workflow-form-${field.key}-${optionIndex}`"
              :key="option.serialized"
              :label="option.label"
              :checked="getMultiSelectValue(field.key).includes(option.serialized)"
              @update:checked="toggleMultiSelect(field.key, option.serialized, $event)"
            />
          </div>

          <p v-else class="workflow-form-note">
            当前字段类型暂不支持编辑
          </p>
        </UiField>
      </div>

      <p v-else class="workflow-form-note">
        {{ emptyText }}
      </p>

      <UiBanner v-if="validationError" tone="danger">
        {{ validationError }}
      </UiBanner>

      <div class="workflow-form-actions">
        <UiButton
          variant="primary"
          type="submit"
          :disabled="submitting || resolvedFields.length === 0"
        >
          {{ submitting ? '提交中...' : submitText }}
        </UiButton>
      </div>
    </form>
  </div>
</template>

<style scoped>
/* 表单本身只管排布：名/器/说明由 UiField 给，动词由 UiButton 给，错由 UiBanner 给。 */
.workflow-form,
.workflow-form-editable,
.workflow-form-fields {
  display: flex;
  flex-direction: column;
  gap: var(--s3);
}

.workflow-form-chips {
  display: flex;
  flex-wrap: wrap;
  gap: var(--s2);
}

.workflow-form-actions {
  display: flex;
  justify-content: flex-end;
}

/* 一句话就别开嵌板（§3.7）：淡墨小字，不描边、不铺面 */
.workflow-form-note {
  margin: 0;
  color: var(--ink-muted);
  font-size: var(--fs-label);
  line-height: 1.6;
}
</style>
