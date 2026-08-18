import { describe, expect, it } from 'vitest';
import {
  buildDispatchReplanProposalPayload,
  isDirectReplanApplyEnabled,
} from './dispatchReplanProposal';

describe('buildDispatchReplanProposalPayload', () => {
  it('targets the first suggestion dispatch order', () => {
    const payload = buildDispatchReplanProposalPayload('balanced', [
      { id: 'replan-0', orderId: 'DO-101', description: '换人', suggestionType: 'assigned_conflict_resolution' },
      { id: 'replan-1', orderId: 'DO-102', description: '补派' },
    ]);
    expect(payload.object_type).toBe('DispatchOrder');
    expect(payload.object_id).toBe('DO-101');
    expect(payload.action_name).toBe('recommend_replan');
    expect(payload.arguments.order_ids).toEqual(['DO-101', 'DO-102']);
    expect(payload.arguments.strategy).toBe('balanced');
    expect(payload.arguments.suggestions).toHaveLength(2);
  });

  it('falls back to the board sentinel without suggestions', () => {
    const payload = buildDispatchReplanProposalPayload('' as never, []);
    expect(payload.object_id).toBe('dispatch-board');
    expect(payload.arguments.strategy).toBe('balanced');
    expect(payload.arguments.order_ids).toEqual([]);
  });
});

describe('isDirectReplanApplyEnabled', () => {
  it('defaults to proposal-first when the flag is absent', () => {
    expect(isDirectReplanApplyEnabled({})).toBe(false);
    expect(isDirectReplanApplyEnabled({ VITE_DISPATCH_DIRECT_REPLAN_APPLY: 'false' })).toBe(false);
  });

  it('enables the escape hatch only for truthy flag values', () => {
    expect(isDirectReplanApplyEnabled({ VITE_DISPATCH_DIRECT_REPLAN_APPLY: '1' })).toBe(true);
    expect(isDirectReplanApplyEnabled({ VITE_DISPATCH_DIRECT_REPLAN_APPLY: 'TRUE' })).toBe(true);
    expect(isDirectReplanApplyEnabled({ VITE_DISPATCH_DIRECT_REPLAN_APPLY: ' yes ' })).toBe(true);
  });
});
