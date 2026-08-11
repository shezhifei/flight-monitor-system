export type FormFieldType = 'text' | 'textarea' | 'number' | 'select' | 'radio' | 'date';

export interface FormFieldOption {
  id: string;
  label: string;
  value: string;
}

export interface FormFieldDefinition {
  id: string;
  label: string;
  key: string;
  type: FormFieldType;
  required: boolean;
  placeholder: string;
  defaultValue: string;
  options: FormFieldOption[];
}

export interface FormTaskBindingConfig {
  title: string;
  templateCode: string;
  formCode: string;
  version: number;
  department: string;
  roles: string[];
  writeBackKey: string;
  completeTaskOnSubmit: boolean;
  allowResubmit: boolean;
  description: string;
  fields: FormFieldDefinition[];
}

export interface WorkflowFormTemplateResponse {
  id: string;
  form_code: string;
  name: string;
  version: number;
  schema_json: Record<string, unknown>;
  ui_schema_json: Record<string, unknown>;
  status: string;
  description?: string | null;
  created_by: string;
  created_at: string;
  updated_at: string;
}

export interface CaseTypeItem {
  id: string;
  name: string;
  code: string;
  description?: string;
  bpmn_xml?: string;
  xml_data?: string;
  /** false = legacy 弃用 (is_active=false) */
  is_active?: boolean;
  ai_extraction_config?: Record<string, unknown> | null;
  case_properties?: Record<string, unknown> | null;
}

export interface ParsedFormTaskBindingConfig {
  title?: string;
  templateCode?: string;
  formCode?: string;
  version?: number;
  department?: string;
  roles?: string[];
  writeBackKey?: string;
  completeTaskOnSubmit?: boolean;
  allowResubmit?: boolean;
  description?: string;
}
