import { describe, expect, it } from 'vitest';

import { applyPendingActionEvent, type PendingRow } from '@/features/ai-monitor/pendingActionRows';

describe('applyPendingActionEvent', () => {
  it('adds a pending action without rebuilding unrelated rows', () => {
    const rows: PendingRow[] = [];

    const next = applyPendingActionEvent(rows, 'approval_required', {
      action_id: 'act-1',
      tool_name: 'approve_slot',
      status: 'pending',
      message: 'Need approval',
    });

    expect(next).toEqual([
      {
        key: 'act-1',
        actionId: 'act-1',
        toolName: 'approve_slot',
        status: 'pending',
        createdAt: '',
        expiresAt: '',
        raw: {
          action_id: 'act-1',
          tool_name: 'approve_slot',
          status: 'pending',
          message: 'Need approval',
        },
      },
    ]);
  });

  it('removes a pending action on approval result', () => {
    const rows: PendingRow[] = [
      {
        key: 'act-1',
        actionId: 'act-1',
        toolName: 'approve_slot',
        status: 'pending',
        createdAt: '',
        expiresAt: '',
        raw: {},
      },
      {
        key: 'act-2',
        actionId: 'act-2',
        toolName: 'other_tool',
        status: 'pending',
        createdAt: '',
        expiresAt: '',
        raw: {},
      },
    ];

    const next = applyPendingActionEvent(rows, 'approval_result', {
      action_id: 'act-1',
    });

    expect(next).toEqual([rows[1]]);
  });
});
