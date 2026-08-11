import { useApi } from './useApi';
import { useAuth } from './useAuth';

export interface CopilotMatchedFlight {
  flight_id: string;
  flight_no: string;
  leg_type: string;
  score: number;
  scheduled_departure?: string | null;
  estimated_departure?: string | null;
  status?: string | null;
}

export interface CopilotDraftAction {
  action_id: string;
  case_type: string;
  case_type_name?: string | null;
  flight_number_raw: string;
  leg_type_hint: string;
  description: string;
  remarks: string;
  fields?: Record<string, unknown>;
  confidence: number;
  needs_review: boolean;
  review_reason?: string | null;
  matched_flight?: CopilotMatchedFlight | null;
  candidates?: CopilotMatchedFlight[];
}

export interface CopilotDraftResponse {
  batch_id: string;
  summary: string;
  transcript: string;
  actions: CopilotDraftAction[];
  expires_at: string;
}

export interface CopilotCaseTypeDiagnostic {
  code: string;
  name: string;
}

export interface CopilotDraftDiagnosticResponse {
  ok: boolean;
  entity_id: string;
  transcript_summary: string;
  candidate_case_types: CopilotCaseTypeDiagnostic[];
  llm_raw_preview?: string | null;
  parsed_payload?: unknown;
  error_stage?: string | null;
  error_message?: string | null;
}

export interface CopilotApprovedAction {
  action_id: string;
  case_type: string;
  flight_id: string;
  flight_no: string;
  bound_leg_type?: string | null;
  bound_flight_no?: string | null;
  description?: string | null;
  remarks?: string | null;
  fields?: Record<string, unknown>;
  status?: string | null;
}

export interface CopilotNotificationGroup {
  group_id: string;
  case_type: string;
  case_ids: string[];
  title: string;
  body: string;
}

export interface CopilotCommitResponse {
  batch_id: string;
  case_ids: string[];
  notification_groups: CopilotNotificationGroup[];
  already_committed: boolean;
  workflow_dispatch_status: CopilotWorkflowDispatchStatus;
}

export type CopilotBatchStatus =
  | 'draft'
  | 'committing'
  | 'committed'
  | 'failed'
  | 'failed_resolved'
  | 'expired';

export type CopilotWorkflowDispatchStatus =
  | 'not_required'
  | 'pending'
  | 'failed'
  | 'succeeded';

export interface CopilotBatchStatusResponse {
  batch_id: string;
  entity_id: string;
  source_page: string;
  transcript_summary: string;
  draft_actions: unknown;
  status: CopilotBatchStatus;
  created_by: string;
  committed_case_ids: string[];
  notification_groups: unknown;
  commit_error?: unknown;
  committed_at?: string | null;
  workflow_dispatch_status: CopilotWorkflowDispatchStatus;
  workflow_dispatch_error?: unknown;
  workflow_dispatch_attempts: number;
  workflow_dispatch_next_retry_at?: string | null;
  workflow_dispatched_at?: string | null;
  created_at: string;
  updated_at: string;
  expires_at: string;
}

export interface CopilotBatchListResponse {
  items: CopilotBatchStatusResponse[];
  limit: number;
  offset: number;
}

export interface CopilotOperationalMetrics {
  generated_at: string;
  batch_status: {
    total: number;
    draft: number;
    committing: number;
    committed: number;
    failed: number;
    failed_resolved: number;
    expired: number;
  };
  workflow_dispatch: {
    not_required: number;
    pending: number;
    succeeded: number;
    failed: number;
    retry_due: number;
    retry_exhausted: number;
    max_attempts: number;
  };
  recent_errors: Array<{
    batch_id: string;
    status: CopilotBatchStatus;
    workflow_dispatch_status: CopilotWorkflowDispatchStatus;
    stage?: string | null;
    message?: string | null;
    attempts: number;
    next_retry_at?: string | null;
    updated_at: string;
  }>;
}

interface ApiEnvelope<T> {
  success?: boolean;
  data?: T;
  message?: string;
  error?: unknown;
}

function unwrapEnvelope<T>(payload: ApiEnvelope<T> | T | null, fallback: string): T {
  if (!payload) {
    throw new Error(fallback);
  }
  if (typeof payload === 'object' && 'success' in payload && (payload as ApiEnvelope<T>).success === false) {
    throw new Error((payload as ApiEnvelope<T>).message || fallback);
  }
  if (typeof payload === 'object' && 'data' in payload) {
    const data = (payload as ApiEnvelope<T>).data;
    if (data !== undefined) {
      return data;
    }
  }
  return payload as T;
}

function idempotencyKey(batchId: string): string {
  const random = typeof crypto !== 'undefined' && 'randomUUID' in crypto
    ? crypto.randomUUID()
    : `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return `${batchId}-${random}`;
}

export function useAiBusinessCaseCopilot() {
  const api = useApi();
  const auth = useAuth();

  async function createDraft(input: {
    entity_id: string;
    transcript: string;
    source_page?: string;
    context?: Record<string, unknown>;
  }): Promise<CopilotDraftResponse> {
    const { ok, status, data } = await api.post<ApiEnvelope<CopilotDraftResponse>>(
      `${auth.apiBase.value}/ai/copilot/business-case-drafts`,
      {
        entity_id: input.entity_id,
        transcript: input.transcript,
        source_page: input.source_page ?? 'flight_monitor',
        context: input.context ?? {},
      },
    );
    if (!ok) {
      throw new Error((data as ApiEnvelope<unknown> | null)?.message || `生成草稿失败 (${status})`);
    }
    return unwrapEnvelope<CopilotDraftResponse>(data, '生成草稿失败');
  }

  async function diagnoseDraft(input: {
    entity_id: string;
    transcript: string;
    source_page?: string;
    context?: Record<string, unknown>;
  }): Promise<CopilotDraftDiagnosticResponse> {
    const { ok, status, data } = await api.post<ApiEnvelope<CopilotDraftDiagnosticResponse>>(
      `${auth.apiBase.value}/ai/copilot/business-case-draft-diagnostics`,
      {
        entity_id: input.entity_id,
        transcript: input.transcript,
        source_page: input.source_page ?? 'flight_monitor',
        context: input.context ?? {},
      },
    );
    if (!ok) {
      throw new Error((data as ApiEnvelope<unknown> | null)?.message || `草稿诊断失败 (${status})`);
    }
    return unwrapEnvelope<CopilotDraftDiagnosticResponse>(data, '草稿诊断失败');
  }

  async function commitBatch(
    batchId: string,
    actions: CopilotApprovedAction[],
    options: { idempotencyKey?: string } = {},
  ): Promise<CopilotCommitResponse> {
    const { ok, status, data } = await api.post<ApiEnvelope<CopilotCommitResponse>>(
      `${auth.apiBase.value}/ai/copilot/business-case-batches/${encodeURIComponent(batchId)}/commit`,
      {
        idempotency_key: options.idempotencyKey || idempotencyKey(batchId),
        actions,
      },
    );
    if (!ok) {
      throw new Error((data as ApiEnvelope<unknown> | null)?.message || `批量创建失败 (${status})`);
    }
    return unwrapEnvelope<CopilotCommitResponse>(data, '批量创建失败');
  }

  async function listBatches(input: {
    status?: CopilotBatchStatus;
    workflow_dispatch_status?: CopilotWorkflowDispatchStatus;
    limit?: number;
    offset?: number;
  } = {}): Promise<CopilotBatchListResponse> {
    const params = new URLSearchParams();
    if (input.status) params.set('status', input.status);
    if (input.workflow_dispatch_status) {
      params.set('workflow_dispatch_status', input.workflow_dispatch_status);
    }
    params.set('limit', String(input.limit ?? 20));
    params.set('offset', String(input.offset ?? 0));
    const { ok, status, data } = await api.get<ApiEnvelope<CopilotBatchListResponse>>(
      `${auth.apiBase.value}/ai/copilot/business-case-batches?${params.toString()}`,
    );
    if (!ok) {
      throw new Error((data as ApiEnvelope<unknown> | null)?.message || `读取批次列表失败 (${status})`);
    }
    return unwrapEnvelope<CopilotBatchListResponse>(data, '读取批次列表失败');
  }

  async function resolveFailedBatch(input: {
    batchId: string;
    action: 'mark_resolved' | 'reset_to_draft';
    note?: string;
  }): Promise<CopilotBatchStatusResponse> {
    const { ok, status, data } = await api.post<ApiEnvelope<CopilotBatchStatusResponse>>(
      `${auth.apiBase.value}/ai/copilot/business-case-batches/${encodeURIComponent(input.batchId)}/failed-resolution`,
      {
        action: input.action,
        note: input.note,
      },
    );
    if (!ok) {
      throw new Error((data as ApiEnvelope<unknown> | null)?.message || `处理失败批次失败 (${status})`);
    }
    return unwrapEnvelope<CopilotBatchStatusResponse>(data, '处理失败批次失败');
  }

  async function getOperationalMetrics(input: {
    recent_error_limit?: number;
    max_workflow_dispatch_attempts?: number;
  } = {}): Promise<CopilotOperationalMetrics> {
    const params = new URLSearchParams();
    params.set('recent_error_limit', String(input.recent_error_limit ?? 10));
    params.set(
      'max_workflow_dispatch_attempts',
      String(input.max_workflow_dispatch_attempts ?? 5),
    );
    const { ok, status, data } = await api.get<ApiEnvelope<CopilotOperationalMetrics>>(
      `${auth.apiBase.value}/ai/copilot/business-case-operational-metrics?${params.toString()}`,
    );
    if (!ok) {
      throw new Error((data as ApiEnvelope<unknown> | null)?.message || `读取运行指标失败 (${status})`);
    }
    return unwrapEnvelope<CopilotOperationalMetrics>(data, '读取运行指标失败');
  }

  async function retryWorkflowDispatch(batchId: string): Promise<CopilotBatchStatusResponse> {
    const { ok, status, data } = await api.post<ApiEnvelope<CopilotBatchStatusResponse>>(
      `${auth.apiBase.value}/ai/copilot/business-case-batches/${encodeURIComponent(batchId)}/workflow-dispatch/retry`,
      {},
    );
    if (!ok) {
      throw new Error((data as ApiEnvelope<unknown> | null)?.message || `重试流程派发失败 (${status})`);
    }
    return unwrapEnvelope<CopilotBatchStatusResponse>(data, '重试流程派发失败');
  }

  return {
    createDraft,
    diagnoseDraft,
    commitBatch,
    listBatches,
    resolveFailedBatch,
    getOperationalMetrics,
    retryWorkflowDispatch,
  };
}
