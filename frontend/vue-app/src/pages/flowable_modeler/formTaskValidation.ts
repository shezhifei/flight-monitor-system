import type { FormFieldDefinition, FormTaskBindingConfig } from './types';

function safeTrim(value: unknown): string {
  return typeof value === 'string' ? value.trim() : '';
}

function validateFieldOptions(taskId: string, field: FormFieldDefinition): void {
  if (field.type !== 'select' && field.type !== 'radio') {
    return;
  }

  const options = Array.isArray(field.options) ? field.options : [];
  if (options.length === 0) {
    throw new Error(`节点 ${taskId} 的字段 "${safeTrim(field.label) || safeTrim(field.key) || '?'}" 至少需要一个选项`);
  }

  const seenValues = new Set<string>();
  options.forEach((option) => {
    const value = safeTrim(option?.value);
    if (!value) {
      throw new Error(`节点 ${taskId} 的字段 "${safeTrim(field.label) || safeTrim(field.key)}" 存在空选项值`);
    }
    if (seenValues.has(value)) {
      throw new Error(`节点 ${taskId} 的字段 "${safeTrim(field.label) || safeTrim(field.key)}" 存在重复选项值 "${value}"`);
    }
    seenValues.add(value);
  });
}

export function shouldPersistFormTask(config: FormTaskBindingConfig | null | undefined): boolean {
  if (!config) return false;
  // 有 formCode 才持久化绑定；空配置的表单任务跳过，避免保存时报 trim 错误
  return Boolean(safeTrim(config.formCode));
}

export function validatePersistedFormTask(taskId: string, config: FormTaskBindingConfig): void {
  if (!safeTrim(config.title)) {
    throw new Error(`节点 ${taskId} 缺少标题`);
  }
  if (!safeTrim(config.templateCode)) {
    throw new Error(`节点 ${taskId} 缺少 templateCode`);
  }
  if (!safeTrim(config.formCode)) {
    throw new Error(`节点 ${taskId} 缺少 formCode`);
  }
  if (!Number.isInteger(config.version) || config.version <= 0) {
    throw new Error(`节点 ${taskId} 的 version 必须是大于 0 的整数`);
  }
  if (!safeTrim(config.writeBackKey)) {
    throw new Error(`节点 ${taskId} 缺少 writeBackKey`);
  }
  const roles = Array.isArray(config.roles) ? config.roles : [];
  if (!safeTrim(config.department) && roles.length === 0) {
    throw new Error(`节点 ${taskId} 至少需要配置一个 department 或 roles`);
  }

  const seenFieldKeys = new Set<string>();
  const fields = Array.isArray(config.fields) ? config.fields : [];
  fields.forEach((field) => {
    const label = safeTrim(field?.label);
    const key = safeTrim(field?.key);

    if (!label) {
      throw new Error(`节点 ${taskId} 存在缺少 label 的表单字段`);
    }
    if (!key) {
      throw new Error(`节点 ${taskId} 的字段 "${label}" 缺少 key`);
    }
    if (seenFieldKeys.has(key)) {
      throw new Error(`节点 ${taskId} 存在重复字段 key "${key}"`);
    }
    seenFieldKeys.add(key);
    validateFieldOptions(taskId, field);
  });
}
