// 搬运自 frontend/ai-react/src/components/chat/pendingActionDiff.test.ts（仅改 import 路径）。
import { describe, expect, it } from 'vitest';

import {
  extractConstraints,
  extractDiffRows,
  toPendingActionCardModel,
} from './pendingActionDiff';

describe('K3: toPendingActionCardModel — sidecar pending store shape', () => {
  it('maps entity, snapshots and irreversible marker from a CRITICAL_WRITE action', () => {
    const model = toPendingActionCardModel({
      action_id: 'pa-1',
      tool_name: 'change_stand',
      status: 'pending',
      reason: 'tool requires human approval',
      operation_level: 'CRITICAL_WRITE',
      entity_type: 'flight',
      entity_id: 'CA1234-20260819',
      before_snapshot: { stand: 'A10' },
      after_snapshot: { stand: 'A12' },
      created_at: '2026-08-19T08:00:00Z',
      expires_at: '2026-08-19T08:10:00Z',
    });

    expect(model.actionId).toBe('pa-1');
    expect(model.objectType).toBe('flight');
    expect(model.objectId).toBe('CA1234-20260819');
    expect(model.irreversible).toBe(true);
    expect(model.diffRows).toEqual([{ field: 'stand', before: 'A10', after: 'A12' }]);
    expect(model.hardViolations).toBeUndefined();
    expect(model.softViolations).toBeUndefined();
    expect(model.sourceTool).toBe('change_stand');
  });

  it('prefers json_patch over raw snapshots when both are present', () => {
    const rows = extractDiffRows({
      json_patch: [{ op: 'replace', path: '/stand', value: 'A12' }],
      before_snapshot: { stand: 'A10', gate: 'G3' },
      after_snapshot: { stand: 'A12', gate: 'G3' },
    });

    expect(rows).toEqual([{ field: 'stand', before: '', after: 'A12' }]);
  });
});

describe('K3: toPendingActionCardModel — ontology proposal shape (I3 simulate)', () => {
  const proposalPayload = {
    status: 'proposal_created',
    proposal: {
      object_type: 'Flight',
      object_id: 'flt-001',
      action_name: 'change_stand',
      run_id: 'run-42',
      tool_name: 'ontology.propose_action',
      reasoning: 'LLM requested controlled write',
      simulate: {
        action_name: 'Flight.change_stand',
        flight_id: 'flt-001',
        before: { flight_id: 'flt-001', stand: 'A10' },
        after: { stand: 'A12' },
        violations: [
          { rule_id: 'no_occupation_overlap', severity: 'hard', message: 'A12 已被占用' },
          { rule_id: 'prefer_near_stand', severity: 'soft', message: '远机位建议' },
        ],
        availability: { is_available: false, conflicts: ['order-9'] },
      },
    },
  };

  it('folds proposal + simulate into an ontology-aware model', () => {
    const model = toPendingActionCardModel(proposalPayload);

    expect(model.status).toBe('pending');
    expect(model.objectType).toBe('Flight');
    expect(model.objectId).toBe('flt-001');
    expect(model.sourceRunId).toBe('run-42');
    expect(model.sourceTool).toBe('ontology.propose_action');
    expect(model.diffRows).toContainEqual({ field: 'stand', before: 'A10', after: 'A12' });
    expect(model.hardViolations?.map((item) => item.name)).toContain('no_occupation_overlap');
    expect(model.softViolations?.map((item) => item.name)).toContain('prefer_near_stand');
  });

  it('splits hard/soft constraint outcomes and hides passed ones', () => {
    const { hard, soft } = extractConstraints({
      proposal: {
        constraint_results: [
          { constraint_name: 'stand_available', kind: 'business', passed: false },
          { constraint_name: 'walk_distance', kind: 'soft', passed: false },
          { constraint_name: 'version_check', kind: 'hard', passed: true },
        ],
      },
    });

    expect(hard.map((item) => item.name)).toEqual(['stand_available']);
    expect(soft.map((item) => item.name)).toEqual(['walk_distance']);
  });

  it('treats unavailable target as a hard violation', () => {
    const { hard } = extractConstraints({
      proposal: {
        simulate: {
          violations: [],
          availability: { is_available: false, conflicts: ['order-9'] },
        },
      },
    });

    expect(hard).toHaveLength(1);
    expect(hard[0].name).toBe('stand_available');
    expect(hard[0].message).toContain('order-9');
  });
});

describe('K3: defensive behavior — no constraints, no crash', () => {
  it('survives a minimal payload without diff or constraint fields', () => {
    const model = toPendingActionCardModel({
      action_id: 'pa-2',
      tool_name: 'notify_teams',
      status: 'pending',
      message: 'approval required',
    });

    expect(model.actionId).toBe('pa-2');
    expect(model.diffRows).toBeUndefined();
    expect(model.hardViolations).toBeUndefined();
    expect(model.softViolations).toBeUndefined();
    expect(model.irreversible).toBeUndefined();
    expect(model.objectType).toBeUndefined();
  });

  it('survives an empty payload', () => {
    const model = toPendingActionCardModel({});
    expect(model.actionId).toBe('');
    expect(model.diffRows).toBeUndefined();
  });

  it('ignores malformed simulate/violation entries instead of throwing', () => {
    const model = toPendingActionCardModel({
      action_id: 'pa-3',
      proposal: {
        object_type: 'Flight',
        object_id: 'flt-002',
        simulate: {
          before: 'not-an-object',
          after: null,
          violations: [null, 'garbage', { severity: 'hard' }],
          availability: 'broken',
        },
      },
    });

    expect(model.actionId).toBe('pa-3');
    expect(model.hardViolations).toHaveLength(1);
    expect(model.hardViolations?.[0].name).toBe('constraint');
  });
});
