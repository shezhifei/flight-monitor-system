import { describe, expect, it } from 'vitest';
import {
  buildReplanProposalPayload,
  isDirectReplanApplyEnabled,
} from './dispatchBoardProposal';

describe('buildReplanProposalPayload', () => {
  const request = {
    strategy: 'balanced',
    max_suggestions: 20,
    window_start: '2026-08-18T00:00:00Z',
    window_end: '2026-08-18T06:00:00Z',
  };

  it('targets the first preview row dispatch order', () => {
    const payload = buildReplanProposalPayload(request, [
      { order_id: 'DO-101', description: 'reassign' },
      { orderId: 'DO-102' },
    ]);
    expect(payload.object_type).toBe('DispatchOrder');
    expect(payload.object_id).toBe('DO-101');
    expect(payload.action_name).toBe('recommend_replan');
    expect(payload.arguments.order_ids).toEqual(['DO-101', 'DO-102']);
    expect(payload.arguments.strategy).toBe('balanced');
    expect(payload.arguments.suggestions).toHaveLength(2);
  });

  it('falls back to the board sentinel when no rows carry an order id', () => {
    const payload = buildReplanProposalPayload({ strategy: '', max_suggestions: 0 }, []);
    expect(payload.object_id).toBe('dispatch-board');
    expect(payload.arguments.strategy).toBe('balanced');
    expect(payload.arguments.max_suggestions).toBe(20);
  });

  it('accepts dispatch_order_id keys from solver previews', () => {
    const payload = buildReplanProposalPayload(request, [{ dispatch_order_id: 'DO-9' }]);
    expect(payload.object_id).toBe('DO-9');
  });
});

describe('isDirectReplanApplyEnabled', () => {
  it('defaults to proposal-first when the flag is absent', () => {
    expect(isDirectReplanApplyEnabled({})).toBe(false);
    expect(isDirectReplanApplyEnabled({ VITE_DISPATCH_DIRECT_REPLAN_APPLY: '' })).toBe(false);
    expect(isDirectReplanApplyEnabled({ VITE_DISPATCH_DIRECT_REPLAN_APPLY: 'false' })).toBe(false);
  });

  it('enables the escape hatch only for truthy flag values', () => {
    expect(isDirectReplanApplyEnabled({ VITE_DISPATCH_DIRECT_REPLAN_APPLY: '1' })).toBe(true);
    expect(isDirectReplanApplyEnabled({ VITE_DISPATCH_DIRECT_REPLAN_APPLY: 'true' })).toBe(true);
    expect(isDirectReplanApplyEnabled({ VITE_DISPATCH_DIRECT_REPLAN_APPLY: ' Yes ' })).toBe(true);
  });
});
