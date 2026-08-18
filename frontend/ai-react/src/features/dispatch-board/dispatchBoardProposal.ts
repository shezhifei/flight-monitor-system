/**
 * Dispatch board replan → proposal bridge (Task I4).
 *
 * The dispatch board must not act as a second agent loop: "apply replan"
 * may only enter the proposal approval flow (`POST /api/v2/ai/proposals`),
 * never call the direct replan-apply path — unless the escape-hatch feature
 * flag is enabled for a human operator.
 */
import type { DispatchReplanRequest } from '@/lib/api/dispatchApi';

export interface ReplanProposalPayload {
  object_type: string;
  object_id: string;
  action_name: string;
  arguments: Record<string, unknown>;
  reasoning: string;
  confidence: number;
}

/**
 * Build the proposal payload for a replan preview. The first preview row's
 * dispatch order becomes the proposal object; the full preview is carried in
 * `arguments` so the approver card can render the diff.
 */
export function buildReplanProposalPayload(
  request: DispatchReplanRequest,
  rows: ReadonlyArray<Record<string, unknown>>,
): ReplanProposalPayload {
  const strategy = String(request.strategy || 'balanced');
  const orderIds = rows
    .map((row) => String(row.order_id || row.orderId || row.dispatch_order_id || '').trim())
    .filter(Boolean);
  return {
    object_type: 'DispatchOrder',
    object_id: orderIds[0] || 'dispatch-board',
    action_name: 'recommend_replan',
    arguments: {
      strategy,
      max_suggestions: request.max_suggestions || 20,
      window_start: request.window_start,
      window_end: request.window_end,
      order_ids: orderIds,
      suggestions: rows,
      source: 'dispatch_board',
    },
    reasoning: `派工看板重排预览（策略=${strategy}），共 ${rows.length} 条建议，待人工审批后执行`,
    confidence: 0.8,
  };
}

export const DIRECT_REPLAN_APPLY_FLAG = 'VITE_DISPATCH_DIRECT_REPLAN_APPLY';

/**
 * Escape hatch (Task I4): direct replan-apply is only allowed behind an
 * explicit feature flag, and only for human operators — never the agent.
 */
export function isDirectReplanApplyEnabled(
  env: Record<string, unknown> = import.meta.env,
): boolean {
  const value = String(env?.[DIRECT_REPLAN_APPLY_FLAG] ?? '').trim().toLowerCase();
  return value === '1' || value === 'true' || value === 'yes';
}
