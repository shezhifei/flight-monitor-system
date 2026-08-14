import type {
  BusinessCaseCreatePayload,
  BusinessCaseVisibilityInfo,
} from './useFlightDataTypes';
import type {
  BusinessCaseAiExtractionConfig,
  BusinessCaseTypeDefinition,
  BusinessCaseVisibilityScope,
} from '../types/backend';
import { normalizeFlightId } from './useFlightField';

function normalizeOptionalText(value: unknown): string | null {
  if (typeof value === 'string') {
    const normalized = value.trim();
    return normalized || null;
  }
  if (typeof value === 'number' && Number.isFinite(value)) {
    return String(value);
  }
  return null;
}

function readRecordValue(source: unknown, keys: string[]): unknown {
  if (!source || typeof source !== 'object') {
    return null;
  }
  const record = source as Record<string, unknown>;
  for (const key of keys) {
    if (record[key] !== undefined && record[key] !== null) {
      return record[key];
    }
  }
  return null;
}

function readBusinessCaseText(source: unknown, keys: string[]): string | null {
  return normalizeOptionalText(readRecordValue(source, keys));
}

export function normalizeBusinessCaseVisibilityScope(
  value: unknown,
): BusinessCaseVisibilityScope | '' {
  const normalized = normalizeOptionalText(value)?.toUpperCase() ?? '';
  if (normalized === 'COMMON' || normalized === 'DEPARTMENT') {
    return normalized;
  }
  return '';
}

export function getBusinessCaseVisibilityInfo(
  caseData: unknown,
): BusinessCaseVisibilityInfo {
  const context = readRecordValue(caseData, ['context']);
  const departmentId = readBusinessCaseText(caseData, ['department_id', 'departmentId'])
    ?? readBusinessCaseText(context, ['department_id', 'departmentId']);
  const departmentName = readBusinessCaseText(caseData, [
    'department_name_snapshot',
    'department_name',
    'departmentName',
  ]) ?? readBusinessCaseText(context, [
    'department_name_snapshot',
    'department_name',
    'departmentName',
  ]);

  const scope = normalizeBusinessCaseVisibilityScope(
    readRecordValue(caseData, ['visibility_scope', 'visibilityScope'])
      ?? readRecordValue(context, ['visibility_scope', 'visibilityScope']),
  ) || (departmentId || departmentName ? 'DEPARTMENT' : 'COMMON');

  return {
    scope,
    scopeLabel: scope === 'COMMON' ? '通用' : (departmentName ? `所属部门 · ${departmentName}` : '所属部门'),
    departmentId,
    departmentName,
    isCommon: scope === 'COMMON',
  };
}

async function parseResponsePayload(response: Response): Promise<Record<string, unknown>> {
  const rawText = await response.text();
  if (!rawText) {
    return {};
  }

  try {
    return JSON.parse(rawText);
  } catch {
    return { message: rawText };
  }
}

function buildErrorMessage(
  payload: Record<string, unknown>,
  fallbackMessage: string,
): string {
  return String(payload?.message || payload?.detail || payload?.error || fallbackMessage);
}

function sanitizeBusinessCaseCreatePayload(caseData: BusinessCaseCreatePayload): BusinessCaseCreatePayload {
  const payload: BusinessCaseCreatePayload = { ...caseData };
  const context = payload.context && typeof payload.context === 'object'
    ? { ...(payload.context as Record<string, unknown>) }
    : undefined;

  payload.case_type = String(payload.case_type || '').trim();
  payload.flight_id = String(payload.flight_id || '').trim();

  const description = normalizeOptionalText(payload.description);
  if (description) {
    payload.description = description;
  } else {
    delete payload.description;
  }

  const status = normalizeOptionalText(payload.status)?.toUpperCase();
  if (status) {
    payload.status = status;
  } else {
    delete payload.status;
  }

  const visibilityScope = normalizeBusinessCaseVisibilityScope(payload.visibility_scope);
  if (visibilityScope) {
    payload.visibility_scope = visibilityScope;
  } else {
    delete payload.visibility_scope;
  }

  const departmentId = normalizeOptionalText(payload.department_id);
  if (departmentId) {
    payload.department_id = departmentId;
  } else {
    delete payload.department_id;
  }

  const departmentNameSnapshot = normalizeOptionalText(payload.department_name_snapshot);
  if (departmentNameSnapshot) {
    payload.department_name_snapshot = departmentNameSnapshot;
  } else {
    delete payload.department_name_snapshot;
  }

  if (context) {
    Object.keys(context).forEach((key) => {
      if (context[key] === undefined) {
        delete context[key];
      }
    });
    payload.context = context;
  } else {
    delete payload.context;
  }

  return payload;
}

export async function loadBusinessCaseTypes(options: {
  apiBase: string;
  authFetch: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;
}): Promise<BusinessCaseTypeDefinition[]> {
  const response = await options.authFetch(`${options.apiBase}/business-case-types`);
  if (!response.ok) {
    throw new Error(`获取业务事项类型失败 (${response.status})`);
  }
  const payload = await parseResponsePayload(response);
  if (Array.isArray(payload?.data)) {
    return payload.data as BusinessCaseTypeDefinition[];
  }
  return Array.isArray(payload) ? payload as BusinessCaseTypeDefinition[] : [];
}

export async function updateBusinessCaseTypeAiConfigRequest(
  code: string,
  config: BusinessCaseAiExtractionConfig,
  options: {
    apiBase: string;
    authFetch: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;
  }
): Promise<Record<string, unknown>> {
  const normalizedCode = String(code || '').trim();
  const response = await options.authFetch(`${options.apiBase}/business-case-types/${encodeURIComponent(normalizedCode)}/ai-extraction-config`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(config),
  });
  const payload = await parseResponsePayload(response);
  if (!response.ok || payload?.success === false) {
    throw new Error(buildErrorMessage(payload, `更新 AI 事项抽取配置失败 (${response.status})`));
  }
  return payload;
}

export async function createBusinessCase(
  caseData: BusinessCaseCreatePayload,
  options: {
    apiBase: string;
    authFetch: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;
  }
): Promise<Record<string, unknown>> {
  const normalizedPayload = sanitizeBusinessCaseCreatePayload(caseData);
  const response = await options.authFetch(`${options.apiBase}/business-cases`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(normalizedPayload),
  });
  const payload = await parseResponsePayload(response);
  if (!response.ok || payload?.success === false) {
    throw new Error(buildErrorMessage(payload, `创建失败 (${response.status})`));
  }
  return payload;
}

export async function updateBusinessCaseStatusRequest(
  caseId: string,
  status: string,
  options: {
    apiBase: string;
    authFetch: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;
  }
): Promise<Record<string, unknown>> {
  const normalizedCaseId = String(caseId || '').trim();
  const normalizedStatus = String(status || '').trim().toUpperCase();
  const response = await options.authFetch(`${options.apiBase}/business-cases/${encodeURIComponent(normalizedCaseId)}/status`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ status: normalizedStatus }),
  });
  const payload = await parseResponsePayload(response);
  if (!response.ok || payload?.success === false) {
    throw new Error(buildErrorMessage(payload, `更新状态失败 (${response.status})`));
  }
  return payload;
}

export async function fetchFlightEventJourney(
  flightId: string | number,
  options: {
    apiBase: string;
    authFetch: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;
    hours?: number;
  }
): Promise<Record<string, unknown>> {
  const normalizedId = normalizeFlightId(flightId);
  if (!normalizedId) {
    throw new Error('航班标识缺失');
  }
  const hoursParam = typeof options.hours === 'number' ? `?hours=${options.hours}` : '';
  const response = await options.authFetch(
    `${options.apiBase}/flights/${encodeURIComponent(normalizedId)}/event-journey${hoursParam}`,
  );
  const payload = await parseResponsePayload(response);
  if (!response.ok) {
    throw new Error(buildErrorMessage(payload, `获取事件经过失败 (${response.status})`));
  }
  return payload;
}

export async function fetchFlightHistoryReport(
  flightId: string | number,
  options: {
    apiBase: string;
    authFetch: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;
    hours?: number;
  }
): Promise<Record<string, unknown>> {
  const normalizedId = normalizeFlightId(flightId);
  if (!normalizedId) {
    throw new Error('航班标识缺失');
  }
  const hoursParam = typeof options.hours === 'number' ? `?hours=${options.hours}` : '';
  const response = await options.authFetch(
    `${options.apiBase}/flights/${encodeURIComponent(normalizedId)}/history-report${hoursParam}`,
  );
  const payload = await parseResponsePayload(response);
  if (!response.ok) {
    throw new Error(buildErrorMessage(payload, `获取历史报表失败 (${response.status})`));
  }
  return payload;
}

export async function appendBusinessCase(
  caseId: string,
  appendData: { content: string; type?: string; mention_user_ids?: string[]; [key: string]: unknown },
  options: {
    apiBase: string;
    authFetch: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;
  }
): Promise<Record<string, unknown>> {
  const normalizedCaseId = String(caseId || '').trim();
  if (!normalizedCaseId) {
    throw new Error('业务事项标识缺失');
  }
  const response = await options.authFetch(
    `${options.apiBase}/business-cases/${encodeURIComponent(normalizedCaseId)}/appends`,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(appendData),
    },
  );
  const payload = await parseResponsePayload(response);
  if (!response.ok || payload?.success === false) {
    throw new Error(buildErrorMessage(payload, `追加失败 (${response.status})`));
  }
  return payload;
}

export async function acknowledgeBusinessCaseAppend(
  caseId: string,
  appendId: string,
  options: {
    apiBase: string;
    authFetch: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;
  }
): Promise<Record<string, unknown>> {
  const normalizedCaseId = String(caseId || '').trim();
  const normalizedAppendId = String(appendId || '').trim();
  if (!normalizedCaseId || !normalizedAppendId) {
    throw new Error('业务事项或追加标识缺失');
  }
  const response = await options.authFetch(
    `${options.apiBase}/business-cases/${encodeURIComponent(normalizedCaseId)}/appends/${encodeURIComponent(normalizedAppendId)}/acknowledge`,
    { method: 'POST' },
  );
  const payload = await parseResponsePayload(response);
  if (!response.ok || payload?.success === false) {
    throw new Error(buildErrorMessage(payload, `确认失败 (${response.status})`));
  }
  return payload;
}

export async function fetchCollaborationGroupByFlight(
  flightId: string | number,
  options: {
    apiBase: string;
    authFetch: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;
  }
): Promise<Record<string, unknown>> {
  const normalizedId = normalizeFlightId(flightId);
  if (!normalizedId) {
    throw new Error('航班标识缺失');
  }
  const response = await options.authFetch(
    `${options.apiBase}/dispatch/collaboration/groups/by-flight/${encodeURIComponent(normalizedId)}`,
  );
  if (!response.ok) {
    const payload = await parseResponsePayload(response);
    throw new Error(buildErrorMessage(payload, `获取航班群聊失败 (${response.status})`));
  }
  return response.json();
}

export async function patchFlightField(
  flightId: string | number,
  field: string,
  value: unknown,
  options: {
    apiBase: string;
    authFetch: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;
    /** Optimistic concurrency token from FlightResponse.version when available. */
    expectedVersion?: number | null;
  }
): Promise<Record<string, unknown>> {
  const normalizedId = normalizeFlightId(flightId);
  const diff: Record<string, unknown> = {};
  diff[field] = value;
  if (typeof options.expectedVersion === 'number' && Number.isFinite(options.expectedVersion)) {
    diff.expected_version = options.expectedVersion;
  }

  const response = await options.authFetch(`${options.apiBase}/flights/${encodeURIComponent(normalizedId)}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(diff),
  });

  if (!response.ok) {
    const text = await response.text();
    throw new Error(text || `更新失败 (${response.status})`);
  }
  return await response.json();
}

/** One flight target in a same-column batch cell edit. */
export interface FlightBatchCellTarget {
  flight_id: string;
  /** Required for snapshot (flight_patch) fields; optional for timeline writes. */
  expected_version?: number | null;
  /** Required optimistic check; null explicitly means the selected cell was empty. */
  expected_value: unknown;
}

export interface FlightBatchCellsRequest {
  field: string;
  value: unknown;
  /** Client batch idempotency key (ULID recommended). */
  client_action_id?: string;
  targets: FlightBatchCellTarget[];
}

export interface FlightBatchCellResultItem {
  flight_id: string;
  version: number;
  value: unknown;
  timeline_id?: string | null;
}

export interface FlightBatchCellsResponse {
  success: boolean;
  message?: string;
  data?: {
    batch_id?: string;
    field?: string;
    updated_count?: number;
    results?: FlightBatchCellResultItem[];
  };
  /** Flattened helpers for callers that unwrap `data`. */
  batch_id?: string;
  field?: string;
  updated_count?: number;
  results?: FlightBatchCellResultItem[];
  [key: string]: unknown;
}

function buildBatchErrorMessage(
  payload: Record<string, unknown>,
  status: number,
): string {
  const nested = payload?.error;
  if (nested && typeof nested === 'object') {
    const err = nested as Record<string, unknown>;
    const code = typeof err.code === 'string' ? err.code : '';
    const message = typeof err.message === 'string' ? err.message : '';
    if (code && message) return `${code}: ${message}`;
    if (message) return message;
  }
  return buildErrorMessage(payload, `批量更新失败 (${status})`);
}

/**
 * PATCH /flights/batch-cells — atomic same-column multi-cell write.
 * Request shape matches Rust `FlightBatchCellUpdateRequest` (`targets`, not `items`).
 */
export async function patchFlightBatchCells(
  request: FlightBatchCellsRequest,
  options: {
    apiBase: string;
    authFetch: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;
  },
): Promise<FlightBatchCellsResponse> {
  const field = String(request.field || '').trim();
  if (!field) {
    throw new Error('批量编辑字段缺失');
  }
  const targets = (request.targets || [])
    .map((item) => {
      const target: Record<string, unknown> = {
        flight_id: normalizeFlightId(item.flight_id),
      };
      if (typeof item.expected_version === 'number' && Number.isFinite(item.expected_version)) {
        target.expected_version = item.expected_version;
      }
      if (item.expected_value === undefined) {
        throw new Error(`批量编辑目标 ${target.flight_id || '(unknown)'} 缺少 expected_value`);
      }
      target.expected_value = item.expected_value;
      return target;
    })
    .filter((item) => Boolean(item.flight_id));

  if (!targets.length) {
    throw new Error('批量编辑目标航班为空');
  }

  const body: Record<string, unknown> = {
    field,
    value: request.value,
    targets,
  };
  const clientActionId = String(request.client_action_id || '').trim();
  if (clientActionId) {
    body.client_action_id = clientActionId;
  }

  const response = await options.authFetch(`${options.apiBase}/flights/batch-cells`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });

  const payload = await parseResponsePayload(response) as FlightBatchCellsResponse;
  if (!response.ok || payload?.success === false) {
    throw new Error(buildBatchErrorMessage(payload as Record<string, unknown>, response.status));
  }

  const data = (payload?.data && typeof payload.data === 'object'
    ? payload.data
    : {}) as NonNullable<FlightBatchCellsResponse['data']>;

  return {
    ...payload,
    success: true,
    batch_id: data.batch_id,
    field: data.field ?? field,
    updated_count: data.updated_count,
    results: data.results,
    data,
  };
}
