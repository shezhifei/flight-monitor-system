<script setup lang="ts">
import { computed } from 'vue';
import type { FieldOverlay, FieldReferenceEntry } from '@/composables/useFieldOverlays';

const props = defineProps<{
  modelValue: Record<string, unknown>;
  overlays: FieldOverlay[];
  catalogEntries?: Record<string, Array<{ code: string; name: string }>>;
  referenceEntries?: Record<string, FieldReferenceEntry[]>;
}>();

const emit = defineEmits<{ (event: 'update:modelValue', value: Record<string, unknown>): void }>();

const activeFields = computed(() => props.overlays.filter(field => field.is_active));

// Mirrors the server contract: field_overlay_service::validate_visible_when
// only accepts { field, op, value } and attribute_validation::field_is_visible
// evaluates it with op defaulting to eq. Malformed shapes are treated as
// always visible, and a dependency missing from the model is visible too —
// the server returns the same permissive verdict so the form never hides a
// field the API would still validate.
function visible(field: FieldOverlay) {
  const condition = field.visible_when;
  if (!condition || typeof condition !== 'object' || typeof condition.field !== 'string' || !condition.field) return true;
  if (!(condition.field in props.modelValue)) return true;
  return evaluate(condition.op || 'eq', props.modelValue[condition.field], condition.value);
}

function evaluate(op: string, actual: unknown, expected: unknown): boolean {
  switch (op) {
    case 'eq': return jsonEquals(actual, expected);
    case 'neq': return !jsonEquals(actual, expected);
    case 'in': return Array.isArray(expected) && expected.some(item => jsonEquals(actual, item));
    case 'not_in': return Array.isArray(expected) ? expected.every(item => !jsonEquals(actual, item)) : true;
    case 'gt': return compareNumbers(actual, expected, (a, b) => a > b);
    case 'gte': return compareNumbers(actual, expected, (a, b) => a >= b);
    case 'lt': return compareNumbers(actual, expected, (a, b) => a < b);
    case 'lte': return compareNumbers(actual, expected, (a, b) => a <= b);
    default: return true;
  }
}

// Typed equality like serde_json::Value: "true" (string) does not satisfy a
// boolean condition, 1 does not equal "1".
function jsonEquals(actual: unknown, expected: unknown): boolean {
  if (Array.isArray(actual) && Array.isArray(expected)) {
    return actual.length === expected.length && actual.every((item, index) => jsonEquals(item, expected[index]));
  }
  return actual === expected;
}

function compareNumbers(actual: unknown, expected: unknown, predicate: (a: number, b: number) => boolean): boolean {
  return typeof actual === 'number' && typeof expected === 'number' && predicate(actual, expected);
}

// object_ref stores the target's business key: the code when the object has
// one (Stand / Gate / Terminal / BaggageCarousel / Equipment), otherwise its
// id (Personnel / Department / Team may lack codes).
function optionValue(entry: FieldReferenceEntry): string {
  return entry.code ?? entry.id;
}

function value(field: FieldOverlay) {
  return props.modelValue[field.field_name] ?? (field.field_type === 'boolean' ? false : '');
}

function textValue(field: FieldOverlay): string {
  const current = value(field);
  if ((field.field_type === 'object_ref[]' || field.field_type === 'catalog_ref[]') && Array.isArray(current)) return current.join(', ');
  return String(current);
}

function referenceOptions(field: FieldOverlay): FieldReferenceEntry[] {
  const target = field.object_name_target;
  return target ? (props.referenceEntries?.[target] ?? []) : [];
}

function catalogOptions(field: FieldOverlay): Array<{ code: string; name: string }> {
  return field.catalog_code ? (props.catalogEntries?.[field.catalog_code] ?? []) : [];
}

function normalizeStringList(raw: unknown): string[] {
  const values = Array.isArray(raw) ? raw.map(item => String(item)) : String(raw ?? '').split(/[\n,]+/);
  return [...new Set(values.map(item => item.trim()).filter(Boolean))];
}

function update(field: FieldOverlay, raw: unknown) {
  let next = raw;
  if (field.field_type === 'number') next = raw === '' ? null : Number(raw);
  if (field.field_type === 'boolean') next = Boolean(raw);
  if (field.field_type === 'object_ref') next = Array.isArray(raw) ? String(raw[0] ?? '').trim() : String(raw ?? '').trim();
  if (field.field_type === 'object_ref[]' || field.field_type === 'catalog_ref[]') next = normalizeStringList(raw);
  const model = { ...props.modelValue };
  // An optional singular object reference is absent when cleared. Sending an
  // empty string would be treated as a real reference by the server-side
  // validator and fail target resolution.
  if (field.field_type === 'object_ref' && next === '') delete model[field.field_name];
  else model[field.field_name] = next;
  emit('update:modelValue', model);
}
</script>

<template>
  <div v-if="activeFields.some(visible)" class="field-overlay-form">
    <div v-for="field in activeFields" v-show="visible(field)" :key="field.field_name" class="form-group">
      <label :for="`overlay-${field.object_name}-${field.field_name}`">
        {{ field.description || field.field_name }}
        <span v-if="field.required" class="required">*</span>
      </label>
      <small v-if="field.field_type === 'object_ref' || field.field_type === 'object_ref[]'" class="reference-hint">
        引用对象：{{ field.object_name_target || '未指定' }}{{ field.field_type === 'object_ref[]' ? '（多个编码以逗号或换行分隔）' : '' }}
      </small>
      <select
        v-if="(field.field_type === 'object_ref' || field.field_type === 'object_ref[]') && referenceOptions(field).length"
        :id="`overlay-${field.object_name}-${field.field_name}`"
        :multiple="field.field_type === 'object_ref[]'"
        :value="field.field_type === 'object_ref[]' ? (Array.isArray(value(field)) ? value(field) : []) : value(field)"
        @change="update(field, field.field_type === 'object_ref[]'
          ? Array.from(($event.target as HTMLSelectElement).selectedOptions).map(option => option.value)
          : ($event.target as HTMLSelectElement).value)"
      >
        <option v-if="field.field_type === 'object_ref'" value="">请选择</option>
        <option v-for="entry in referenceOptions(field)" :key="entry.id" :value="optionValue(entry)">
          {{ entry.name || entry.code || entry.id }}
        </option>
      </select>
      <select
        v-else-if="field.field_type === 'catalog_ref' && field.catalog_code"
        :id="`overlay-${field.object_name}-${field.field_name}`"
        :value="value(field)"
        @change="update(field, ($event.target as HTMLSelectElement).value)"
      >
        <option value="">请选择</option>
        <option v-for="entry in catalogOptions(field)" :key="entry.code" :value="entry.code">
          {{ entry.name }}
        </option>
      </select>
      <select
        v-else-if="field.field_type === 'catalog_ref[]' && field.catalog_code && catalogOptions(field).length"
        :id="`overlay-${field.object_name}-${field.field_name}`"
        multiple
        :value="Array.isArray(value(field)) ? value(field) : []"
        @change="update(field, Array.from(($event.target as HTMLSelectElement).selectedOptions).map(option => option.value))"
      >
        <option v-for="entry in catalogOptions(field)" :key="entry.code" :value="entry.code">
          {{ entry.name }}
        </option>
      </select>
      <textarea
        v-else-if="field.widget === 'textarea' || field.field_type === 'object_ref[]' || field.field_type === 'catalog_ref[]' || field.field_type === 'string' && (field.max_length ?? 0) > 200"
        :id="`overlay-${field.object_name}-${field.field_name}`"
        :value="textValue(field)"
        :maxlength="field.max_length ?? undefined"
        @input="update(field, ($event.target as HTMLTextAreaElement).value)"
      />
      <input
        v-else-if="field.field_type === 'boolean'"
        :id="`overlay-${field.object_name}-${field.field_name}`"
        type="checkbox"
        :checked="Boolean(value(field))"
        @change="update(field, ($event.target as HTMLInputElement).checked)"
      >
      <input
        v-else
        :id="`overlay-${field.object_name}-${field.field_name}`"
        :type="field.field_type === 'number' ? 'number' : field.field_type === 'datetime' ? 'datetime-local' : 'text'"
        :value="value(field) as string | number"
        :maxlength="field.max_length ?? undefined"
        :min="field.min ?? undefined"
        :max="field.max ?? undefined"
        @input="update(field, ($event.target as HTMLInputElement).value)"
      >
    </div>
  </div>
</template>

<style scoped>
.reference-hint {
  display: block;
  color: var(--text-secondary, #6b7280);
  font-size: 0.78rem;
  margin: -0.2rem 0 0.35rem;
}
</style>
