// 搬运自 frontend/ai-react/src/features/dispatch-board/dispatchBoardProposal.ts（无逻辑改动；
// DispatchReplanRequest 类型改从 ./api 导入）。
import type { DispatchReplanRequest } from './api';

export interface ReplanProposalPayload {
  object_type: string;
  object_id: string;
  action_name: string;
  arguments: Record<string, unknown>;
  reasoning: string;
  confidence: number;
}

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

export function isDirectReplanApplyEnabled(
  env: Record<string, unknown> = import.meta.env,
): boolean {
  const value = String(env?.[DIRECT_REPLAN_APPLY_FLAG] ?? '').trim().toLowerCase();
  return value === '1' || value === 'true' || value === 'yes';
}
