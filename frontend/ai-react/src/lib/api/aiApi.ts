import { requestEnvelope } from '@/lib/http/apiClient';
import type { AiCapabilities, PendingActionsResponse } from '@/lib/types/apiModels';

const AI_BASE = '/api/v2/ai';

export async function getAiCapabilities(): Promise<AiCapabilities> {
  return requestEnvelope<AiCapabilities>(`${AI_BASE}/capabilities`);
}

export async function listPendingActions(params: {
  status?: string;
  limit?: number;
  offset?: number;
} = {}): Promise<PendingActionsResponse> {
  const search = new URLSearchParams();
  if (params.status) search.set('status', params.status);
  if (params.limit) search.set('limit', String(params.limit));
  if (params.offset) search.set('offset', String(params.offset));
  return requestEnvelope<PendingActionsResponse>(`${AI_BASE}/pending-actions?${search.toString()}`);
}

export async function approvePendingAction(actionId: string): Promise<Record<string, unknown>> {
  return requestEnvelope<Record<string, unknown>>(`${AI_BASE}/pending-actions/${encodeURIComponent(actionId)}/approve`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
  });
}

export async function rejectPendingAction(actionId: string, reason = ''): Promise<Record<string, unknown>> {
  return requestEnvelope<Record<string, unknown>>(`${AI_BASE}/pending-actions/${encodeURIComponent(actionId)}/reject`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(reason ? { reason } : {}),
  });
}

export async function executeTool(toolName: string, toolArgs: Record<string, unknown>): Promise<Record<string, unknown>> {
  return requestEnvelope<Record<string, unknown>>(`${AI_BASE}/tools/execute`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      tool_name: toolName,
      tool_args: toolArgs,
    }),
  });
}

export async function getExecutionDetail(runId: string): Promise<Record<string, unknown>> {
  return requestEnvelope<Record<string, unknown>>(`${AI_BASE}/executions/${encodeURIComponent(runId)}`);
}

export async function getRoutingMetrics(): Promise<Record<string, unknown>> {
  return requestEnvelope<Record<string, unknown>>(`${AI_BASE}/metrics/query-routing`);
}

export async function getReportSchemaMetrics(): Promise<Record<string, unknown>> {
  return requestEnvelope<Record<string, unknown>>(`${AI_BASE}/metrics/report-schema`);
}

export async function getExecutionVisibilityMetrics(): Promise<Record<string, unknown>> {
  return requestEnvelope<Record<string, unknown>>(`${AI_BASE}/metrics/execution-visibility`);
}

export async function listEntities(): Promise<{ entities: Array<{ id: string }> }> {
  return requestEnvelope<{ entities: Array<{ id: string }> }>(`${AI_BASE}/entities`);
}

export async function getEntity(entityId: string): Promise<Record<string, unknown>> {
  return requestEnvelope<Record<string, unknown>>(`${AI_BASE}/entities/${encodeURIComponent(entityId)}`);
}

export async function saveEntity(
  entityId: string,
  payload: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  return requestEnvelope<Record<string, unknown>>(`${AI_BASE}/entities/${encodeURIComponent(entityId)}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
}

export async function testConnection(payload: Record<string, unknown>): Promise<Record<string, unknown>> {
  return requestEnvelope<Record<string, unknown>>(`${AI_BASE}/connection/test`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
}

export async function listModels(): Promise<{ models: Array<Record<string, unknown>> }> {
  return requestEnvelope<{ models: Array<Record<string, unknown>> }>(`${AI_BASE}/models`);
}

export async function listTools(): Promise<Array<Record<string, unknown>>> {
  return requestEnvelope<Array<Record<string, unknown>>>(`${AI_BASE}/tools`);
}

export async function listToolCategories(): Promise<{ categories: Array<Record<string, unknown>> }> {
  return requestEnvelope<{ categories: Array<Record<string, unknown>> }>(`${AI_BASE}/tools/categories`);
}

// ---- Proposals API ----

export async function listProposals(params: {
  object_type?: string;
  object_id?: string;
  action_name?: string;
  status?: string;
  limit?: number;
  offset?: number;
} = {}): Promise<Array<Record<string, unknown>>> {
  const search = new URLSearchParams();
  if (params.object_type) search.set('object_type', params.object_type);
  if (params.object_id) search.set('object_id', params.object_id);
  if (params.action_name) search.set('action_name', params.action_name);
  if (params.status) search.set('status', params.status);
  if (params.limit) search.set('limit', String(params.limit));
  if (params.offset) search.set('offset', String(params.offset));
  return requestEnvelope<Array<Record<string, unknown>>>(`${AI_BASE}/proposals?${search.toString()}`);
}

export async function approveProposal(proposalId: string, modifiedArguments?: Record<string, unknown>): Promise<Record<string, unknown>> {
  return requestEnvelope<Record<string, unknown>>(`${AI_BASE}/proposals/${encodeURIComponent(proposalId)}/approve`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ modified_arguments: modifiedArguments || {} }),
  });
}

export async function rejectProposal(proposalId: string, reason: string): Promise<Record<string, unknown>> {
  return requestEnvelope<Record<string, unknown>>(`${AI_BASE}/proposals/${encodeURIComponent(proposalId)}/reject`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ reason }),
  });
}

export async function getProposalStats(): Promise<Record<string, unknown>> {
  return requestEnvelope<Record<string, unknown>>(`${AI_BASE}/proposals/stats`);
}

// ---- AI Jobs API ----

export async function listAiJobs(params: {
  status?: string;
  job_type?: string;
  limit?: number;
  offset?: number;
} = {}): Promise<Array<Record<string, unknown>>> {
  const search = new URLSearchParams();
  if (params.status) search.set('status', params.status);
  if (params.job_type) search.set('job_type', params.job_type);
  if (params.limit) search.set('limit', String(params.limit));
  if (params.offset) search.set('offset', String(params.offset));
  return requestEnvelope<Array<Record<string, unknown>>>(`${AI_BASE}/jobs?${search.toString()}`);
}

export async function getAiJob(jobId: string): Promise<Record<string, unknown>> {
  return requestEnvelope<Record<string, unknown>>(`${AI_BASE}/jobs/${encodeURIComponent(jobId)}`);
}

export async function listAiJobRuns(jobId: string): Promise<Array<Record<string, unknown>>> {
  return requestEnvelope<Array<Record<string, unknown>>>(`${AI_BASE}/jobs/${encodeURIComponent(jobId)}/runs`);
}

export async function listAiRunEvents(jobId: string, runId: string): Promise<Array<Record<string, unknown>>> {
  return requestEnvelope<Array<Record<string, unknown>>>(`${AI_BASE}/jobs/${encodeURIComponent(jobId)}/runs/${encodeURIComponent(runId)}/events`);
}

export async function getAiJobStats(): Promise<Record<string, unknown>> {
  return requestEnvelope<Record<string, unknown>>(`${AI_BASE}/jobs/stats`);
}

// ---- Flight Risk Micro Model ----

export async function executeFlightRiskModel(flightId: string): Promise<Record<string, unknown>> {
  return requestEnvelope<Record<string, unknown>>(`${AI_BASE}/micro-models/flight_risk_v1/execute`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ flight_id: flightId }),
  });
}

// ---- Rollout Gate APIs ----

export type ExecutionMode = 'disabled' | 'allow_all' | 'allowlist';

export interface RolloutStatusResponse {
  execution_enabled: boolean;
  execution_mode: ExecutionMode;
  readiness_override: string | null;
  readiness: {
    overall_status: 'Ready' | 'NotReady';
    checks: Array<{ name: string; status: 'Pass' | 'Fail' | 'Warn'; message: string }>;
    generated_at: string;
  };
  metrics: {
    pending_proposals: number;
    failed_proposals_24h: number;
    executed_proposals_24h: number;
    outbox_unprocessed: number;
    outbox_oldest_age_seconds: number | null;
  };
  recent_smoke: {
    last_run_at: string | null;
    total_smoke_proposals: number;
    succeeded: number;
    failed: number;
    blocked_by_readiness: number;
  } | null;
  allowed_actions: string[];
  generated_at: string;
}

export interface ProposalRow {
  proposal_id: string;
  object_type: string;
  object_id: string;
  action_name: string;
  status: string;
  created_at?: string;
  updated_at?: string;
}

export async function getRolloutStatus(): Promise<RolloutStatusResponse> {
  return requestEnvelope<RolloutStatusResponse>(`${AI_BASE}/execution-readiness/rollout-status`);
}

export async function executeProposal(proposalId: string): Promise<Record<string, unknown>> {
  return requestEnvelope<Record<string, unknown>>(
    `${AI_BASE}/proposals/${encodeURIComponent(proposalId)}/execute`,
    { method: 'POST' },
  );
}
