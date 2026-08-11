import type {
  FormFieldDefinition,
  FormFieldOption,
  FormFieldType,
  FormTaskBindingConfig,
  WorkflowFormTemplateResponse,
} from './types';

const FIELD_WIDGET_MAP: Record<FormFieldType, string> = {
  text: 'text',
  textarea: 'textarea',
  number: 'number',
  select: 'select',
  radio: 'radio',
  date: 'date',
};

function createUniqueId(prefix: string): string {
  return `${prefix}_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`;
}

function safeTrim(value: unknown): string {
  return typeof value === 'string' ? value.trim() : '';
}

export function sanitizeIdentifier(value: string | null | undefined, fallback = 'item'): string {
  const normalized = safeTrim(value)
    .replace(/[^a-zA-Z0-9_]+/g, '_')
    .replace(/_+/g, '_')
    .replace(/^_+|_+$/g, '');

  return normalized || fallback;
}

export function buildDefaultWriteBackKey(seed: string): string {
  return `forms.${sanitizeIdentifier(seed || 'workflow_form', 'workflow_form')}`;
}

export function createDefaultField(index = 1): FormFieldDefinition {
  const key = sanitizeIdentifier(`field_${index}`, `field_${index}`);
  return {
    id: createUniqueId('field'),
    label: `字段 ${index}`,
    key,
    type: 'text',
    required: false,
    placeholder: '',
    defaultValue: '',
    options: [],
  };
}

export function cloneFieldOption(option: FormFieldOption): FormFieldOption {
  return {
    id: option.id,
    label: option.label,
    value: option.value,
  };
}

export function cloneFieldDefinition(field: FormFieldDefinition): FormFieldDefinition {
  return {
    id: field.id,
    label: field.label,
    key: field.key,
    type: field.type,
    required: field.required,
    placeholder: field.placeholder,
    defaultValue: field.defaultValue,
    options: field.options.map(cloneFieldOption),
  };
}

export function cloneFormTaskConfig(config: FormTaskBindingConfig): FormTaskBindingConfig {
  return {
    title: config.title,
    templateCode: config.templateCode,
    formCode: config.formCode,
    version: config.version,
    department: config.department,
    roles: [...config.roles],
    writeBackKey: config.writeBackKey,
    completeTaskOnSubmit: config.completeTaskOnSubmit,
    allowResubmit: config.allowResubmit,
    description: config.description,
    fields: config.fields.map(cloneFieldDefinition),
  };
}

export function createDefaultFormTaskConfig(params: {
  taskId: string;
  taskName?: string;
  templateCode?: string;
  department?: string;
}): FormTaskBindingConfig {
  const taskSeed = sanitizeIdentifier(params.taskId || 'form_task', 'form_task');
  const title = safeTrim(params.taskName) || '表单任务节点';

  return {
    title,
    templateCode: safeTrim(params.templateCode),
    formCode: '',
    version: 1,
    department: safeTrim(params.department),
    roles: [],
    writeBackKey: buildDefaultWriteBackKey(taskSeed),
    completeTaskOnSubmit: true,
    allowResubmit: false,
    description: '',
    fields: [createDefaultField(1)],
  };
}

/** 把解析/局部更新的配置补全为可安全 trim 的完整结构 */
export function normalizeFormTaskConfig(
  partial: Partial<FormTaskBindingConfig> | null | undefined,
  fallback?: { taskId?: string; taskName?: string },
): FormTaskBindingConfig {
  const base = createDefaultFormTaskConfig({
    taskId: fallback?.taskId || 'form_task',
    taskName: fallback?.taskName,
  });
  if (!partial || typeof partial !== 'object') {
    return base;
  }

  const roles = Array.isArray(partial.roles)
    ? partial.roles.map((role) => safeTrim(role)).filter(Boolean)
    : base.roles;
  const fields = Array.isArray(partial.fields) && partial.fields.length > 0
    ? partial.fields.map((field, index) => {
      const cloned = cloneFieldDefinition({
        id: safeTrim(field?.id) || createUniqueId('field'),
        label: safeTrim(field?.label) || `字段 ${index + 1}`,
        key: safeTrim(field?.key) || `field_${index + 1}`,
        type: field?.type || 'text',
        required: Boolean(field?.required),
        placeholder: safeTrim(field?.placeholder),
        defaultValue: safeTrim(field?.defaultValue),
        options: Array.isArray(field?.options)
          ? field.options.map((option) => ({
            id: safeTrim(option?.id) || createUniqueId('option'),
            label: safeTrim(option?.label),
            value: safeTrim(option?.value),
          }))
          : [],
      });
      return cloned;
    })
    : base.fields;

  const versionNum = Number(partial.version);
  return {
    title: safeTrim(partial.title) || base.title,
    templateCode: safeTrim(partial.templateCode),
    formCode: safeTrim(partial.formCode),
    version: Number.isFinite(versionNum) && versionNum > 0 ? versionNum : 1,
    department: safeTrim(partial.department),
    roles,
    writeBackKey: safeTrim(partial.writeBackKey) || base.writeBackKey,
    completeTaskOnSubmit: partial.completeTaskOnSubmit !== undefined
      ? Boolean(partial.completeTaskOnSubmit)
      : true,
    allowResubmit: partial.allowResubmit !== undefined
      ? Boolean(partial.allowResubmit)
      : false,
    description: safeTrim(partial.description),
    fields,
  };
}

export function parseRolesInput(value: string): string[] {
  const seen = new Set<string>();
  return value
    .split(/[\n,，]+/)
    .map((item) => item.trim())
    .filter((item) => {
      if (!item || seen.has(item)) {
        return false;
      }
      seen.add(item);
      return true;
    });
}

export function formatRolesInput(roles: string[]): string {
  return roles.join(', ');
}

function readPrimitiveDefault(value: unknown): string {
  if (typeof value === 'string') {
    return value;
  }
  if (typeof value === 'number' || typeof value === 'boolean') {
    return String(value);
  }
  return '';
}

function buildFieldSchema(field: FormFieldDefinition): Record<string, unknown> {
  const label = safeTrim(field.label);
  const key = safeTrim(field.key);
  const defaultValue = safeTrim(field.defaultValue);
  const options = Array.isArray(field.options) ? field.options : [];
  const schema: Record<string, unknown> = {
    title: label || key,
  };

  switch (field.type) {
    case 'number': {
      schema.type = 'number';
      const numericDefault = Number(defaultValue);
      if (defaultValue && Number.isFinite(numericDefault)) {
        schema.default = numericDefault;
      }
      break;
    }
    case 'date':
      schema.type = 'string';
      schema.format = 'date';
      if (defaultValue) {
        schema.default = defaultValue;
      }
      break;
    case 'select':
    case 'radio':
      schema.type = 'string';
      if (defaultValue) {
        schema.default = defaultValue;
      }
      if (options.length > 0) {
        schema.oneOf = options.map((option) => ({
          const: safeTrim(option?.value),
          title: safeTrim(option?.label) || safeTrim(option?.value),
        }));
      }
      break;
    case 'textarea':
    case 'text':
    default:
      schema.type = 'string';
      if (defaultValue) {
        schema.default = defaultValue;
      }
      break;
  }

  return schema;
}

function buildFieldUi(field: FormFieldDefinition): Record<string, unknown> {
  const ui: Record<string, unknown> = {
    'ui:widget': FIELD_WIDGET_MAP[field.type] || 'text',
  };
  const placeholder = safeTrim(field.placeholder);
  const options = Array.isArray(field.options) ? field.options : [];

  if (placeholder) {
    ui['ui:placeholder'] = placeholder;
  }

  if ((field.type === 'select' || field.type === 'radio') && options.length > 0) {
    ui.options = options.map((option) => ({
      label: safeTrim(option?.label),
      value: safeTrim(option?.value),
    }));
  }

  return ui;
}

export function buildTemplateDefinition(config: FormTaskBindingConfig): {
  schemaJson: Record<string, unknown>;
  uiSchemaJson: Record<string, unknown>;
} {
  const properties: Record<string, unknown> = {};
  const required: string[] = [];
  const uiSchema: Record<string, unknown> = {
    'ui:order': [],
  };
  const fields = Array.isArray(config.fields) ? config.fields : [];

  fields.forEach((field) => {
    const key = safeTrim(field?.key);
    if (!key) {
      return;
    }

    properties[key] = buildFieldSchema(field);
    (uiSchema['ui:order'] as string[]).push(key);
    uiSchema[key] = buildFieldUi(field);

    if (field.required) {
      required.push(key);
    }
  });

  const schemaJson: Record<string, unknown> = {
    type: 'object',
    title: safeTrim(config.title) || '表单',
    description: safeTrim(config.description) || undefined,
    properties,
  };

  if (required.length > 0) {
    schemaJson.required = required;
  }

  return {
    schemaJson,
    uiSchemaJson: uiSchema,
  };
}

function parseFieldType(
  schemaField: Record<string, unknown>,
  uiField: Record<string, unknown>,
): FormFieldType {
  const widget = String(uiField['ui:widget'] || '').trim().toLowerCase();
  const type = String(schemaField.type || '').trim().toLowerCase();
  const format = String(schemaField.format || '').trim().toLowerCase();

  if (widget === 'textarea') {
    return 'textarea';
  }
  if (widget === 'radio') {
    return 'radio';
  }
  if (format === 'date') {
    return 'date';
  }
  if (type === 'number' || type === 'integer') {
    return 'number';
  }
  if (Array.isArray(schemaField.oneOf) || Array.isArray(schemaField.enum) || widget === 'select') {
    return 'select';
  }
  return 'text';
}

function parseFieldOptions(
  schemaField: Record<string, unknown>,
  uiField: Record<string, unknown>,
): FormFieldOption[] {
  const oneOf = Array.isArray(schemaField.oneOf) ? schemaField.oneOf : [];
  if (oneOf.length > 0) {
    return oneOf.map((item, index) => {
      const option = item && typeof item === 'object'
        ? item as Record<string, unknown>
        : {};
      const value = String(option.const ?? option.value ?? option.title ?? `option_${index + 1}`);
      const label = String(option.title ?? option.label ?? value);
      return {
        id: createUniqueId('option'),
        label,
        value,
      };
    });
  }

  const uiOptions = Array.isArray(uiField.options) ? uiField.options : [];
  if (uiOptions.length > 0) {
    return uiOptions.map((item, index) => {
      const option = item && typeof item === 'object'
        ? item as Record<string, unknown>
        : {};
      const value = String(option.value ?? option.label ?? `option_${index + 1}`);
      const label = String(option.label ?? option.value ?? value);
      return {
        id: createUniqueId('option'),
        label,
        value,
      };
    });
  }

  const enums = Array.isArray(schemaField.enum) ? schemaField.enum : [];
  if (enums.length > 0) {
    return enums.map((item, index) => {
      const value = String(item ?? `option_${index + 1}`);
      return {
        id: createUniqueId('option'),
        label: value,
        value,
      };
    });
  }

  return [];
}

export function mergeTemplateIntoConfig(
  config: FormTaskBindingConfig,
  template: WorkflowFormTemplateResponse,
): FormTaskBindingConfig {
  const schemaJson = template.schema_json && typeof template.schema_json === 'object'
    ? template.schema_json as Record<string, unknown>
    : {};
  const uiSchemaJson = template.ui_schema_json && typeof template.ui_schema_json === 'object'
    ? template.ui_schema_json as Record<string, unknown>
    : {};
  const properties = schemaJson.properties && typeof schemaJson.properties === 'object'
    ? schemaJson.properties as Record<string, unknown>
    : {};
  const requiredSet = new Set(
    Array.isArray(schemaJson.required)
      ? schemaJson.required.map((item) => String(item))
      : [],
  );
  const uiOrder = Array.isArray(uiSchemaJson['ui:order'])
    ? (uiSchemaJson['ui:order'] as unknown[]).map((item) => String(item))
    : Object.keys(properties);
  const fieldKeys = uiOrder.filter((item) => Object.prototype.hasOwnProperty.call(properties, item));

  Object.keys(properties).forEach((key) => {
    if (!fieldKeys.includes(key)) {
      fieldKeys.push(key);
    }
  });

  const fields = fieldKeys.map((key, index) => {
    const schemaField = properties[key] && typeof properties[key] === 'object'
      ? properties[key] as Record<string, unknown>
      : {};
    const uiField = uiSchemaJson[key] && typeof uiSchemaJson[key] === 'object'
      ? uiSchemaJson[key] as Record<string, unknown>
      : {};
    const type = parseFieldType(schemaField, uiField);

    return {
      id: createUniqueId(`field_${index + 1}`),
      label: String(schemaField.title || key),
      key,
      type,
      required: requiredSet.has(key),
      placeholder: String(uiField['ui:placeholder'] || ''),
      defaultValue: readPrimitiveDefault(schemaField.default),
      options: parseFieldOptions(schemaField, uiField),
    } satisfies FormFieldDefinition;
  });

  return {
    ...cloneFormTaskConfig(config),
    title: String(template.name || config.title || ''),
    version: Number.isFinite(template.version) && template.version > 0 ? template.version : config.version,
    description: String(template.description || config.description || ''),
    fields: fields.length > 0 ? fields : config.fields.map(cloneFieldDefinition),
  };
}
