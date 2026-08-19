// AI 相关 REST/流式 API，合并搬运自 frontend/ai-react/src/lib/api/ 下四个模块。
// 适配点：原模块级函数通过 requestEnvelope(url, init)（authFetch 传输）发请求；
// 这里改为工厂函数注入 ApiLike（页面侧传 useApi()），信封解包语义由
// ./envelope.ts 的 requestEnvelope 保留。函数参数与返回类型不变。
// 未搬运：flowableApi.ts（死代码）；aiApi.ts 的 entities/models/tools/connection
// 一组（listEntities/getEntity/saveEntity/testConnection/listModels/listTools/
// listToolCategories，已退役功能）。
import type { ApiLike } from './envelope';
import { requestEnvelope } from './envelope';
import { consumeSSEBody, safeJson } from './streamParser';

// ---------------------------------------------------------------------------
// 共享类型（搬运自 frontend/ai-react/src/lib/types/apiModels.ts，仅保留本文件用到的）
// ---------------------------------------------------------------------------

export interface AiCapabilities {
  ai_ready: boolean;
  ai_execute_permission: boolean;
  ai_chat_permission: boolean;
  missing_reasons: string[];
}

export interface PendingActionsResponse {
  items: Array<Record<string, unknown>>;
  total: number;
  total_count: number;
  pagination?: {
    limit: number;
    offset: number;
    next_offset?: number;
    has_more: boolean;
  };
}

export interface NLQueryResult {
  conversation_id?: string;
  summary?: string;
  structured_data?: unknown;
  visualization_hint?: string;
}

export interface EvalJobSummary {
  job_id: string;
  name?: string;
  dataset_path?: string;
  status: string;
  progress_percent?: number;
  total_runs?: number;
  completed_runs?: number;
  created_at?: string;
}

export interface EvalGateRow {
  metric_name: string;
  value: number;
  threshold: number;
  status: string; // pass | fail | warn
}

export interface EvalJobDetail extends EvalJobSummary {
  description?: string;
  metrics_config?: Record<string, unknown>;
  started_at?: string;
  completed_at?: string;
  error_message?: string | null;
  gates?: EvalGateRow[];
}

export interface EvalJobCreatePayload {
  name: string;
  dataset_path: string;
  description?: string;
  metrics_config?: Record<string, unknown>;
  run?: boolean;
}

// ---------------------------------------------------------------------------
// 来源：lib/api/aiApi.ts —— capabilities / pending actions / tools / executions /
// metrics / proposals / jobs / checkpoints / micro-models / rollout gates
// ---------------------------------------------------------------------------

const AI_BASE = '/api/v2/ai';

export interface ProposalCreatePayload {
  object_type: string;
  object_id: string;
  action_name: string;
  arguments: Record<string, unknown>;
  reasoning?: string;
  confidence?: number;
  job_id?: string;
  run_id?: string;
  correlation_id?: string;
  idempotency_key?: string;
  expected_object_version?: number | string;
}

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

export function createAiApi(api: ApiLike) {
  return {
    getAiCapabilities(): Promise<AiCapabilities> {
      return requestEnvelope<AiCapabilities>(api, `${AI_BASE}/capabilities`);
    },

    listPendingActions(
      params: {
        status?: string;
        limit?: number;
        offset?: number;
      } = {},
    ): Promise<PendingActionsResponse> {
      const search = new URLSearchParams();
      if (params.status) search.set('status', params.status);
      if (params.limit) search.set('limit', String(params.limit));
      if (params.offset) search.set('offset', String(params.offset));
      return requestEnvelope<PendingActionsResponse>(api, `${AI_BASE}/pending-actions?${search.toString()}`);
    },

    approvePendingAction(actionId: string): Promise<Record<string, unknown>> {
      return requestEnvelope<Record<string, unknown>>(
        api,
        `${AI_BASE}/pending-actions/${encodeURIComponent(actionId)}/approve`,
        'POST',
      );
    },

    rejectPendingAction(actionId: string, reason = ''): Promise<Record<string, unknown>> {
      return requestEnvelope<Record<string, unknown>>(
        api,
        `${AI_BASE}/pending-actions/${encodeURIComponent(actionId)}/reject`,
        'POST',
        reason ? { reason } : {},
      );
    },

    executeTool(toolName: string, toolArgs: Record<string, unknown>): Promise<Record<string, unknown>> {
      return requestEnvelope<Record<string, unknown>>(api, `${AI_BASE}/tools/execute`, 'POST', {
        tool_name: toolName,
        tool_args: toolArgs,
      });
    },

    getExecutionDetail(runId: string): Promise<Record<string, unknown>> {
      return requestEnvelope<Record<string, unknown>>(api, `${AI_BASE}/executions/${encodeURIComponent(runId)}`);
    },

    getRoutingMetrics(): Promise<Record<string, unknown>> {
      return requestEnvelope<Record<string, unknown>>(api, `${AI_BASE}/metrics/query-routing`);
    },

    getReportSchemaMetrics(): Promise<Record<string, unknown>> {
      return requestEnvelope<Record<string, unknown>>(api, `${AI_BASE}/metrics/report-schema`);
    },

    getExecutionVisibilityMetrics(): Promise<Record<string, unknown>> {
      return requestEnvelope<Record<string, unknown>>(api, `${AI_BASE}/metrics/execution-visibility`);
    },

    // ---- Proposals API ----

    createProposal(payload: ProposalCreatePayload): Promise<Record<string, unknown>> {
      return requestEnvelope<Record<string, unknown>>(api, `${AI_BASE}/proposals`, 'POST', payload);
    },

    listProposals(
      params: {
        object_type?: string;
        object_id?: string;
        action_name?: string;
        status?: string;
        limit?: number;
        offset?: number;
      } = {},
    ): Promise<Array<Record<string, unknown>>> {
      const search = new URLSearchParams();
      if (params.object_type) search.set('object_type', params.object_type);
      if (params.object_id) search.set('object_id', params.object_id);
      if (params.action_name) search.set('action_name', params.action_name);
      if (params.status) search.set('status', params.status);
      if (params.limit) search.set('limit', String(params.limit));
      if (params.offset) search.set('offset', String(params.offset));
      return requestEnvelope<Array<Record<string, unknown>>>(api, `${AI_BASE}/proposals?${search.toString()}`);
    },

    approveProposal(
      proposalId: string,
      modifiedArguments?: Record<string, unknown>,
    ): Promise<Record<string, unknown>> {
      return requestEnvelope<Record<string, unknown>>(
        api,
        `${AI_BASE}/proposals/${encodeURIComponent(proposalId)}/approve`,
        'POST',
        { modified_arguments: modifiedArguments || {} },
      );
    },

    rejectProposal(proposalId: string, reason: string): Promise<Record<string, unknown>> {
      return requestEnvelope<Record<string, unknown>>(
        api,
        `${AI_BASE}/proposals/${encodeURIComponent(proposalId)}/reject`,
        'POST',
        { reason },
      );
    },

    getProposalStats(): Promise<Record<string, unknown>> {
      return requestEnvelope<Record<string, unknown>>(api, `${AI_BASE}/proposals/stats`);
    },

    // ---- AI Jobs API ----

    listAiJobs(
      params: {
        status?: string;
        job_type?: string;
        limit?: number;
        offset?: number;
      } = {},
    ): Promise<Array<Record<string, unknown>>> {
      const search = new URLSearchParams();
      if (params.status) search.set('status', params.status);
      if (params.job_type) search.set('job_type', params.job_type);
      if (params.limit) search.set('limit', String(params.limit));
      if (params.offset) search.set('offset', String(params.offset));
      return requestEnvelope<Array<Record<string, unknown>>>(api, `${AI_BASE}/jobs?${search.toString()}`);
    },

    getAiJob(jobId: string): Promise<Record<string, unknown>> {
      return requestEnvelope<Record<string, unknown>>(api, `${AI_BASE}/jobs/${encodeURIComponent(jobId)}`);
    },

    listAiJobRuns(jobId: string): Promise<Array<Record<string, unknown>>> {
      return requestEnvelope<Array<Record<string, unknown>>>(
        api,
        `${AI_BASE}/jobs/${encodeURIComponent(jobId)}/runs`,
      );
    },

    listAiRunEvents(jobId: string, runId: string): Promise<Array<Record<string, unknown>>> {
      return requestEnvelope<Array<Record<string, unknown>>>(
        api,
        `${AI_BASE}/jobs/${encodeURIComponent(jobId)}/runs/${encodeURIComponent(runId)}/events`,
      );
    },

    getAiJobStats(): Promise<Record<string, unknown>> {
      return requestEnvelope<Record<string, unknown>>(api, `${AI_BASE}/jobs/stats`);
    },

    // ---- Run resume / checkpoints (crates/api/src/routes/ai_resume.rs) ----

    async listAiRunCheckpoints(jobId: string, runId: string): Promise<Array<Record<string, unknown>>> {
      const payload = await requestEnvelope<{ items: Array<Record<string, unknown>> }>(
        api,
        `${AI_BASE}/jobs/${encodeURIComponent(jobId)}/runs/${encodeURIComponent(runId)}/checkpoints`,
      );
      return payload.items || [];
    },

    resumeAiRun(runId: string, fromCheckpointId?: string): Promise<Record<string, unknown>> {
      return requestEnvelope<Record<string, unknown>>(
        api,
        `${AI_BASE}/runs/${encodeURIComponent(runId)}/resume`,
        'POST',
        fromCheckpointId ? { from_checkpoint_id: fromCheckpointId } : {},
      );
    },

    cancelAiJob(jobId: string): Promise<Record<string, unknown>> {
      return requestEnvelope<Record<string, unknown>>(
        api,
        `${AI_BASE}/jobs/${encodeURIComponent(jobId)}`,
        'DELETE',
      );
    },

    // ---- Flight Risk Micro Model ----

    executeFlightRiskModel(flightId: string): Promise<Record<string, unknown>> {
      return requestEnvelope<Record<string, unknown>>(
        api,
        `${AI_BASE}/micro-models/flight_risk_v1/execute`,
        'POST',
        { flight_id: flightId },
      );
    },

    // ---- Rollout Gate APIs ----

    getRolloutStatus(): Promise<RolloutStatusResponse> {
      return requestEnvelope<RolloutStatusResponse>(api, `${AI_BASE}/execution-readiness/rollout-status`);
    },

    executeProposal(proposalId: string): Promise<Record<string, unknown>> {
      return requestEnvelope<Record<string, unknown>>(
        api,
        `${AI_BASE}/proposals/${encodeURIComponent(proposalId)}/execute`,
        'POST',
      );
    },
  };
}

export type AiApi = ReturnType<typeof createAiApi>;

// ---------------------------------------------------------------------------
// 来源：lib/api/nlQueryApi.ts —— 自然语言查询（含 POST-SSE 流式查询）
// 适配点：streamQuery 原先直接用 authFetch 拿原始 Response，这里改用
// ApiLike.raw()（useApi 提供，同样走 auth.fetch 但不做 JSON 解析）。
// ---------------------------------------------------------------------------

const NL_QUERY_BASE = '/api/v2/ai/nl-query';

export interface NlQueryStreamRequestPayload {
  question: string;
  conversation_id?: string;
  request_id: string;
  context?: Record<string, unknown>;
  /**
   * Task I4: pin the sidecar policy template for embedded shells (e.g.
   * `dispatch_ops` for the dispatch board assistant). Rust validates the
   * value against the registered task templates.
   */
  task_type?: string;
}

export function createNlQueryApi(api: ApiLike) {
  return {
    async listSuggestions(): Promise<Array<{ label?: string; text?: string }>> {
      const payload = await requestEnvelope<{ suggestions: Array<{ label?: string; text?: string }> }>(
        api,
        `${NL_QUERY_BASE}/suggestions`,
      );
      return payload.suggestions || [];
    },

    async listConversations(limit = 50, offset = 0): Promise<Array<Record<string, unknown>>> {
      const payload = await requestEnvelope<{ items: Array<Record<string, unknown>> }>(
        api,
        `${NL_QUERY_BASE}/conversations?limit=${encodeURIComponent(String(limit))}&offset=${encodeURIComponent(String(offset))}`,
      );
      return payload.items || [];
    },

    async listConversationMessages(
      conversationId: string,
      limit = 50,
      offset = 0,
      order: 'asc' | 'desc' = 'desc',
    ): Promise<Array<Record<string, unknown>>> {
      const payload = await requestEnvelope<{ items: Array<Record<string, unknown>> }>(
        api,
        `${NL_QUERY_BASE}/conversations/${encodeURIComponent(conversationId)}/messages?limit=${encodeURIComponent(String(limit))}&offset=${encodeURIComponent(String(offset))}&order=${encodeURIComponent(order)}`,
      );
      return payload.items || [];
    },

    async deleteConversation(conversationId: string): Promise<void> {
      await requestEnvelope<Record<string, unknown>>(
        api,
        `${NL_QUERY_BASE}/${encodeURIComponent(conversationId)}`,
        'DELETE',
      );
    },

    async streamQuery(
      payload: NlQueryStreamRequestPayload,
      onEvent: (eventName: string, data: Record<string, unknown>) => void,
    ): Promise<NLQueryResult> {
      const endpoint = `${NL_QUERY_BASE}/stream`;
      const response = await api.raw(endpoint, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Accept: 'text/event-stream',
        },
        body: JSON.stringify(payload),
      });
      if (!response.ok) {
        const text = await response.text().catch(() => '');
        throw new Error(text || `HTTP ${response.status}`);
      }

      let finalResult: NLQueryResult | null = null;
      let streamError = '';
      await consumeSSEBody(response, (event) => {
        const parsed = safeJson<Record<string, unknown>>(event.data) || {};
        if (event.event === 'final_result') {
          finalResult = {
            ...(parsed as unknown as NLQueryResult),
            conversation_id: String(parsed.conversation_id || payload.conversation_id || ''),
            summary: String(parsed.summary || parsed.answer || ''),
          };
          return;
        }
        if (event.event === 'error') {
          streamError = String(parsed.message || parsed.detail || '流式查询失败');
          return;
        }
        onEvent(event.event, parsed);
      });

      if (finalResult) {
        return finalResult;
      }
      if (streamError) {
        throw new Error(streamError);
      }
      throw new Error('流式查询未返回最终结果');
    },
  };
}

export type NlQueryApi = ReturnType<typeof createNlQueryApi>;

// ---------------------------------------------------------------------------
// 来源：lib/api/dispatchApi.ts —— 调度冲突 / 重规划
// ---------------------------------------------------------------------------

export interface DispatchReplanRequest {
  strategy: string;
  max_suggestions: number;
  window_start?: string;
  window_end?: string;
  apply_changes?: boolean;
  scope?: Record<string, unknown>;
}

function readScopeString(scope: Record<string, unknown> | undefined, keys: string[]): string | undefined {
  if (!scope) {
    return undefined;
  }
  for (const key of keys) {
    const value = scope[key];
    if (typeof value === 'string' && value.trim()) {
      return value.trim();
    }
  }
  return undefined;
}

function defaultWindow(): { window_start: string; window_end: string } {
  const now = Date.now();
  return {
    window_start: new Date(now - 2 * 60 * 60 * 1000).toISOString(),
    window_end: new Date(now + 4 * 60 * 60 * 1000).toISOString(),
  };
}

function buildReplanPayload(input: DispatchReplanRequest, applyChanges: boolean): Record<string, unknown> {
  const fallback = defaultWindow();
  const windowStart =
    input.window_start ||
    readScopeString(input.scope, ['window_start', 'windowStart', 'windowStartIso']) ||
    fallback.window_start;
  const windowEnd =
    input.window_end ||
    readScopeString(input.scope, ['window_end', 'windowEnd', 'windowEndIso']) ||
    fallback.window_end;

  return {
    window_start: windowStart,
    window_end: windowEnd,
    strategy: input.strategy || 'balanced',
    max_suggestions: Math.max(1, Math.trunc(input.max_suggestions || 20)),
    apply_changes: applyChanges,
  };
}

export function createDispatchApi(api: ApiLike) {
  return {
    async loadDispatchConflicts(
      params: {
        start_time?: string;
        end_time?: string;
        severity?: string;
        type?: string;
        query?: string;
      } = {},
    ): Promise<Array<Record<string, unknown>>> {
      const search = new URLSearchParams();
      Object.entries(params).forEach(([key, value]) => {
        if (value) {
          const normalizedKey = key === 'start_time' ? 'window_start' : key === 'end_time' ? 'window_end' : key;
          search.set(normalizedKey, value);
        }
      });
      const payload = await requestEnvelope<{ conflicts: Array<Record<string, unknown>> }>(
        api,
        `/api/v2/dispatch-orders/conflicts?${search.toString()}`,
      );
      return payload.conflicts || [];
    },

    previewReplan(payload: DispatchReplanRequest): Promise<Record<string, unknown>> {
      return requestEnvelope<Record<string, unknown>>(
        api,
        '/api/v2/dispatch-orders/replan',
        'POST',
        buildReplanPayload(payload, false),
      );
    },

    applyReplan(payload: DispatchReplanRequest): Promise<Record<string, unknown>> {
      return requestEnvelope<Record<string, unknown>>(
        api,
        '/api/v2/dispatch-orders/replan',
        'POST',
        buildReplanPayload(payload, true),
      );
    },
  };
}

export type DispatchApi = ReturnType<typeof createDispatchApi>;

// ---------------------------------------------------------------------------
// 来源：lib/api/llmEvalApi.ts —— LLM 评估任务
// ---------------------------------------------------------------------------

const EVAL_BASE = '/api/v2/ai/eval';

export function createLlmEvalApi(api: ApiLike) {
  return {
    createEvalJob(payload: EvalJobCreatePayload): Promise<{ job_id: string; status: string }> {
      return requestEnvelope<{ job_id: string; status: string }>(api, `${EVAL_BASE}/jobs`, 'POST', payload);
    },

    async listEvalJobs(limit = 20): Promise<EvalJobSummary[]> {
      const data = await requestEnvelope<{ items: EvalJobSummary[] }>(
        api,
        `${EVAL_BASE}/jobs?limit=${encodeURIComponent(String(limit))}`,
      );
      return data.items || [];
    },

    getEvalJob(jobId: string): Promise<EvalJobDetail> {
      return requestEnvelope<EvalJobDetail>(api, `${EVAL_BASE}/jobs/${encodeURIComponent(jobId)}`);
    },

    cancelEvalJob(jobId: string): Promise<Record<string, unknown>> {
      return requestEnvelope<Record<string, unknown>>(
        api,
        `${EVAL_BASE}/jobs/${encodeURIComponent(jobId)}/cancel`,
        'POST',
      );
    },
  };
}

export type LlmEvalApi = ReturnType<typeof createLlmEvalApi>;
