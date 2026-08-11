<script setup lang="ts">
import { computed } from 'vue';

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
        <div class="field-designer-title">字段设计器</div>
        <div class="field-designer-hint">
          增删改与排序字段；保存流程时会写入表单模板 schema。
        </div>
      </div>
      <button class="field-add-button" type="button" @click="addField">+ 新增字段</button>
    </div>

    <div v-if="fields.length === 0" class="field-designer-empty">
      暂无字段，点击「新增字段」开始设计。
    </div>

    <div v-for="(field, index) in fields" :key="field.id" class="field-card">
      <div class="field-card-toolbar">
        <div class="field-card-index">
          <span class="field-index-badge">{{ index + 1 }}</span>
          字段 {{ index + 1 }}
          <span v-if="field.required" class="field-req-tag">必填</span>
        </div>
        <div class="field-card-actions">
          <button class="icon-btn" type="button" title="上移" :disabled="index === 0" @click="moveField(index, -1)">↑</button>
          <button class="icon-btn" type="button" title="下移" :disabled="index === fields.length - 1" @click="moveField(index, 1)">↓</button>
          <button class="icon-btn danger" type="button" title="删除" @click="removeField(index)">删除</button>
        </div>
      </div>

      <div class="field-grid">
        <label class="field-item">
          <span>显示名称</span>
          <input type="text" :value="field.label" maxlength="80" placeholder="例如：座位号" @input="updateField(index, { label: readInputValue($event) })">
        </label>

        <label class="field-item">
          <span>字段键 Key</span>
          <input type="text" class="mono" :value="field.key" maxlength="80" placeholder="seat_no" @input="updateField(index, { key: readInputValue($event) })">
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
          <input type="text" :value="field.placeholder" maxlength="120" placeholder="输入提示文案" @input="updateField(index, { placeholder: readInputValue($event) })">
        </label>

        <label class="field-item">
          <span>默认值</span>
          <input type="text" :value="field.defaultValue" maxlength="120" placeholder="可选" @input="updateField(index, { defaultValue: readInputValue($event) })">
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
  gap: 12px;
}

.field-designer-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}

.field-designer-title {
  font-size: 14px;
  font-weight: 700;
  color: var(--admin-text);
}

.field-designer-hint {
  margin-top: 4px;
  font-size: 12px;
  color: var(--admin-text-muted);
  line-height: 1.5;
}

.field-add-button {
  white-space: nowrap;
  border: none;
  border-radius: 10px;
  padding: 8px 12px;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  background: var(--ws-primary);
  color: var(--text-inverse, #fff);
}

.field-designer-empty {
  border: 1px dashed var(--admin-border);
  border-radius: 12px;
  padding: 18px;
  text-align: center;
  color: var(--admin-text-muted);
  font-size: 12px;
  background: var(--ws-surface-muted);
}

.field-card {
  border: 1px solid var(--admin-border);
  border-radius: 12px;
  background: var(--admin-card-bg);
  padding: 12px;
  box-shadow: 0 1px 2px rgba(15, 23, 42, 0.03);
}

.field-card-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 12px;
}

.field-card-index {
  font-size: 12px;
  font-weight: 600;
  color: var(--admin-text-subtle);
  display: inline-flex;
  align-items: center;
  gap: 8px;
}

.field-index-badge {
  width: 20px;
  height: 20px;
  border-radius: 6px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: var(--system-blue-subtle);
  color: var(--ws-primary);
  font-size: 11px;
  font-weight: 700;
}

.field-req-tag {
  font-size: 10px;
  font-weight: 700;
  color: var(--ws-warn);
  background: color-mix(in srgb, var(--ws-warn) 14%, transparent);
  padding: 1px 6px;
  border-radius: 999px;
}

.field-card-actions {
  display: flex;
  gap: 6px;
}

.icon-btn {
  min-width: 32px;
  height: 30px;
  border: 1px solid var(--admin-border);
  border-radius: 8px;
  background: var(--ws-surface-muted);
  color: var(--admin-text-subtle);
  cursor: pointer;
  font-size: 12px;
  font-weight: 600;
  padding: 0 8px;
}

.icon-btn:hover:not(:disabled) {
  border-color: color-mix(in srgb, var(--ws-primary) 35%, var(--admin-border));
  color: var(--ws-primary);
}

.icon-btn:disabled {
  cursor: not-allowed;
  opacity: 0.45;
}

.icon-btn.danger {
  color: var(--ws-danger);
  border-color: color-mix(in srgb, var(--ws-danger) 28%, var(--admin-border));
  background: color-mix(in srgb, var(--ws-danger) 8%, transparent);
}

.field-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
}

.field-item {
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-size: 12px;
  font-weight: 600;
  color: var(--admin-text-subtle);
}

.field-item input,
.field-item select,
.field-item textarea {
  width: 100%;
  border: 1px solid var(--admin-border);
  border-radius: 10px;
  padding: 8px 10px;
  font-size: 13px;
  font-weight: 400;
  color: var(--admin-text);
  background: var(--ws-surface-muted);
  box-sizing: border-box;
}

.field-item input:focus,
.field-item select:focus,
.field-item textarea:focus {
  outline: none;
  border-color: var(--ws-primary);
  box-shadow: 0 0 0 3px var(--focus-ring-blue, var(--system-blue-subtle));
  background: var(--admin-card-bg);
}

.field-item input.mono {
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 12px;
}

.checkbox-item {
  justify-content: center;
}

.req-switch {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  min-height: 38px;
  padding: 0 10px;
  border: 1px solid var(--admin-border);
  border-radius: 10px;
  background: var(--ws-surface-muted);
  font-weight: 500;
  color: var(--admin-text);
  cursor: pointer;
}

.req-switch input {
  width: 15px;
  height: 15px;
  accent-color: var(--ws-primary);
  padding: 0;
}

.option-textarea {
  margin-top: 10px;
}

.option-textarea textarea {
  min-height: 88px;
  resize: vertical;
  line-height: 1.45;
}
</style>
