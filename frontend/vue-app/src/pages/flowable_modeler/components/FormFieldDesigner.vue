<script setup lang="ts">
import { computed } from 'vue';

import UiButton from '@/components/ui/UiButton.vue';
import UiPill from '@/components/ui/UiPill.vue';
import { createDefaultField } from '../formTaskDesigner';
import type { FormFieldDefinition, FormFieldOption, FormFieldType } from '../types';

const props = defineProps<{
  modelValue: FormFieldDefinition[];
}>();

const emit = defineEmits<{
  'update:modelValue': [value: FormFieldDefinition[]];
}>();

const fieldTypeOptions: Array<{ label: string; value: FormFieldType }> = [
  { label: '单行文本', value: 'text' },
  { label: '多行文本', value: 'textarea' },
  { label: '数字', value: 'number' },
  { label: '下拉选择', value: 'select' },
  { label: '单选', value: 'radio' },
  { label: '日期', value: 'date' },
];

const fields = computed(() => props.modelValue ?? []);

function cloneOption(option: FormFieldOption): FormFieldOption {
  return {
    id: option.id,
    label: option.label,
    value: option.value,
  };
}

function cloneField(field: FormFieldDefinition): FormFieldDefinition {
  return {
    id: field.id,
    label: field.label,
    key: field.key,
    type: field.type,
    required: field.required,
    placeholder: field.placeholder,
    defaultValue: field.defaultValue,
    options: field.options.map(cloneOption),
  };
}

function emitFields(nextFields: FormFieldDefinition[]): void {
  emit('update:modelValue', nextFields.map(cloneField));
}

function updateField(index: number, patch: Partial<FormFieldDefinition>): void {
  const nextFields = fields.value.map((field, currentIndex) => (
    currentIndex === index
      ? {
        ...cloneField(field),
        ...patch,
        options: patch.options
          ? patch.options.map(cloneOption)
          : field.options.map(cloneOption),
      }
      : cloneField(field)
  ));
  emitFields(nextFields);
}

function addField(): void {
  emitFields([
    ...fields.value.map(cloneField),
    createDefaultField(fields.value.length + 1),
  ]);
}

function removeField(index: number): void {
  emitFields(fields.value.filter((_, currentIndex) => currentIndex !== index));
}

function moveField(index: number, direction: -1 | 1): void {
  const targetIndex = index + direction;
  if (targetIndex < 0 || targetIndex >= fields.value.length) {
    return;
  }

  const nextFields = fields.value.map(cloneField);
  const [item] = nextFields.splice(index, 1);
  nextFields.splice(targetIndex, 0, item);
  emitFields(nextFields);
}

function serializeOptions(options: FormFieldOption[]): string {
  return options
    .map((option) => (
      option.label.trim() === option.value.trim()
        ? option.label.trim()
        : `${option.label.trim()}:${option.value.trim()}`
    ))
    .join('\n');
}

function parseOptions(value: string): FormFieldOption[] {
  return value
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line, index) => {
      const [labelPart, valuePart] = line.includes(':')
        ? line.split(/:(.+)/, 2)
        : [line, line];
      const label = labelPart.trim() || `选项 ${index + 1}`;
      const optionValue = (valuePart ?? labelPart).trim() || label;

      return {
        id: `option_${Date.now().toString(36)}_${index}`,
        label,
        value: optionValue,
      };
    });
}

function readInputValue(event: Event): string {
  return (event.target as HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement | null)?.value ?? '';
}

function readCheckboxValue(event: Event): boolean {
  return Boolean((event.target as HTMLInputElement | null)?.checked);
}
</script>

<template>
  <div class="field-designer">
    <div class="field-designer-header">
      <div>
        <div class="field-designer-title">
          字段设计器
        </div>
        <div class="field-designer-hint">
          增删改与排序字段；保存流程时会写入表单模板 schema。
        </div>
      </div>
      <UiButton variant="primary" @click="addField">
        + 新增字段
      </UiButton>
    </div>

    <div v-if="fields.length === 0" class="field-designer-empty">
      暂无字段，点击「新增字段」开始设计。
    </div>

    <div v-for="(field, index) in fields" :key="field.id" class="field-card">
      <div class="field-card-toolbar">
        <div class="field-card-index">
          <span class="field-index-badge">{{ index + 1 }}</span>
          字段 {{ index + 1 }}
          <UiPill v-if="field.required" tone="warn">
            必填
          </UiPill>
        </div>
        <div class="field-card-actions">
          <button
            class="icon-btn"
            type="button"
            title="上移"
            :disabled="index === 0"
            @click="moveField(index, -1)"
          >
            ↑
          </button>
          <button
            class="icon-btn"
            type="button"
            title="下移"
            :disabled="index === fields.length - 1"
            @click="moveField(index, 1)"
          >
            ↓
          </button>
          <button
            class="icon-btn danger"
            type="button"
            title="删除"
            @click="removeField(index)"
          >
            删除
          </button>
        </div>
      </div>

      <div class="field-grid">
        <label class="field-item">
          <span>显示名称</span>
          <input
            type="text"
            :value="field.label"
            maxlength="80"
            placeholder="例如：座位号"
            @input="updateField(index, { label: readInputValue($event) })"
          >
        </label>

        <label class="field-item">
          <span>字段键 Key</span>
          <input
            type="text"
            class="mono"
            :value="field.key"
            maxlength="80"
            placeholder="seat_no"
            @input="updateField(index, { key: readInputValue($event) })"
          >
        </label>

        <label class="field-item">
          <span>类型</span>
          <select :value="field.type" @change="updateField(index, { type: readInputValue($event) as FormFieldType })">
            <option v-for="option in fieldTypeOptions" :key="option.value" :value="option.value">{{ option.label }}</option>
          </select>
        </label>

        <label class="field-item checkbox-item">
          <span>是否必填</span>
          <label class="req-switch">
            <input type="checkbox" :checked="field.required" @change="updateField(index, { required: readCheckboxValue($event) })">
            <span>{{ field.required ? '必填' : '选填' }}</span>
          </label>
        </label>

        <label class="field-item">
          <span>占位提示</span>
          <input
            type="text"
            :value="field.placeholder"
            maxlength="120"
            placeholder="输入提示文案"
            @input="updateField(index, { placeholder: readInputValue($event) })"
          >
        </label>

        <label class="field-item">
          <span>默认值</span>
          <input
            type="text"
            :value="field.defaultValue"
            maxlength="120"
            placeholder="可选"
            @input="updateField(index, { defaultValue: readInputValue($event) })"
          >
        </label>
      </div>

      <label v-if="field.type === 'select' || field.type === 'radio'" class="field-item option-textarea">
        <span>选项（每行一个，支持 标签:值）</span>
        <textarea
          rows="4"
          :value="serializeOptions(field.options)"
          placeholder="选项A&#10;选项B:value_b"
          @input="updateField(index, { options: parseOptions(readInputValue($event)) })"
        />
      </label>
    </div>
  </div>
</template>

<style scoped>
.field-designer {
  display: flex;
  flex-direction: column;
  gap: var(--s3);
}

.field-designer-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--s3);
}

.field-designer-title {
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.field-designer-hint {
  margin-top: 4px;
  font-size: var(--fs-label);
  color: var(--ink-muted);
  line-height: 1.5;
}

.field-designer-empty {
  border: 1px dashed var(--line);
  border-radius: var(--r-panel);
  padding: var(--s4);
  text-align: center;
  color: var(--ink-muted);
  font-size: var(--fs-label);
  background: color-mix(in srgb, var(--ink) 6%, transparent);
}

.field-card {
  border: 1px solid var(--line);
  border-radius: var(--r-panel);
  background: var(--face-work);
  padding: var(--s3);
  box-shadow: var(--shadow-sm);
}

.field-card-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s2);
  margin-bottom: var(--s3);
}

.field-card-index {
  font-size: var(--fs-label);
  font-weight: var(--fw-semibold);
  color: var(--ink-subtle);
  display: inline-flex;
  align-items: center;
  gap: var(--s2);
}

.field-index-badge {
  width: 20px;
  height: 20px;
  border-radius: var(--r-cell);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: var(--act-soft);
  color: var(--act);
  font-size: var(--fs-label);
  font-weight: var(--fw-semibold);
}

.field-card-actions {
  display: flex;
  gap: 6px;
}

.icon-btn {
  min-width: 32px;
  height: var(--h-sm);
  border: 1px solid var(--line);
  border-radius: var(--r-control);
  background: color-mix(in srgb, var(--ink) 6%, transparent);
  color: var(--ink-subtle);
  cursor: pointer;
  font-size: var(--fs-label);
  font-weight: var(--fw-semibold);
  padding: 0 var(--s2);
  transition: border-color var(--t-fast) var(--ease), color var(--t-fast) var(--ease);
}

.icon-btn:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
}

.icon-btn:hover:not(:disabled) {
  border-color: color-mix(in srgb, var(--act) 35%, var(--line));
  color: var(--act);
}

.icon-btn:disabled {
  cursor: not-allowed;
  opacity: 0.45;
}

.icon-btn.danger {
  color: var(--danger);
  border-color: color-mix(in srgb, var(--danger) 28%, var(--line));
  background: color-mix(in srgb, var(--danger) 8%, transparent);
}

.field-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--s3);
}

.field-item {
  display: flex;
  flex-direction: column;
  gap: var(--s2);
  font-size: var(--fs-label);
  font-weight: var(--fw-semibold);
  color: var(--ink-subtle);
}

.field-item input,
.field-item select,
.field-item textarea {
  width: 100%;
  border: 1px solid var(--line);
  border-radius: var(--r-control);
  padding: var(--s2) var(--s3);
  font-size: var(--fs-body);
  font-weight: 400;
  color: var(--ink);
  background: color-mix(in srgb, var(--ink) 6%, transparent);
  box-sizing: border-box;
  transition: border-color var(--t-fast) var(--ease);
}

.field-item input:focus,
.field-item select:focus,
.field-item textarea:focus {
  outline: 2px solid var(--act);
  outline-offset: 2px;
  border-color: var(--act);
  background: var(--face-work);
}

.field-item input.mono {
  font-family: var(--mono);
  font-size: var(--fs-label);
}

.checkbox-item {
  justify-content: center;
}

.req-switch {
  display: inline-flex;
  align-items: center;
  gap: var(--s2);
  min-height: var(--h-md);
  padding: 0 var(--s3);
  border: 1px solid var(--line);
  border-radius: var(--r-control);
  background: color-mix(in srgb, var(--ink) 6%, transparent);
  font-weight: var(--fw-medium);
  color: var(--ink);
  cursor: pointer;
}

.req-switch input {
  width: 15px;
  height: 15px;
  accent-color: var(--act);
  padding: 0;
}

.option-textarea {
  margin-top: var(--s3);
}

.option-textarea textarea {
  min-height: 88px;
  resize: vertical;
  line-height: 1.45;
}
</style>
