/**
 * Dispatch board replan → proposal bridge (Task I4).
 *
 * The dispatch board must not act as a second agent loop: applying a replan
 * preview may only enter the proposal approval flow (`POST /api/v2/ai/proposals`),
 * never the direct `replan-apply` write path — unless the escape-hatch feature
 * flag is enabled for a human operator.
 */
import type { ReplanStrategy } from '@/composables/useDispatchBoardData';
import type { ReplanSuggestion } from '@/composables/useDispatchReplan';

export interface DispatchReplanProposalPayload {
  object_type: string;
  object_id: string;
  action_name: string;
  arguments: Record<string, unknown>;
  reasoning: string;
  confidence: number;
}

/**
 * Build the proposal payload for a solver replan preview. The first
 * suggestion's dispatch order becomes the proposal object; the full
 * suggestion list is carried in `arguments` so the approver can review it.
 */
export function buildDispatchReplanProposalPayload(
  strategy: ReplanStrategy,
  suggestions: ReadonlyArray<ReplanSuggestion>,
): DispatchReplanProposalPayload {
  const normalizedStrategy = String(strategy || 'balanced');
  const orderIds = suggestions
    .map((item) => String(item.orderId || '').trim())
    .filter(Boolean);
  return {
    object_type: 'DispatchOrder',
    object_id: orderIds[0] || 'dispatch-board',
    action_name: 'recommend_replan',
    arguments: {
      strategy: normalizedStrategy,
      order_ids: orderIds,
      suggestions: suggestions.map((item) => ({
        order_id: item.orderId,
        description: item.description,
        suggestion_type: item.suggestionType || '',
        changes: item.changes || [],
      })),
      source: 'dispatch_board',
    },
    reasoning: `派工看板重排预览（策略=${normalizedStrategy}），共 ${suggestions.length} 条建议，待人工审批后执行`,
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
