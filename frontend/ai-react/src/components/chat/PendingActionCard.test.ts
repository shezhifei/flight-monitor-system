import { describe, expect, it } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';

import { PendingActionCard } from '@/components/chat/PendingActionCard';
import { toPendingActionCardModel, type PendingActionCardModel } from '@/components/chat/pendingActionDiff';

function renderCard(action: PendingActionCardModel): string {
  return renderToStaticMarkup(
    createElement(PendingActionCard, {
      action,
      busy: false,
      onApprove: () => undefined,
      onReject: () => undefined,
    }),
  );
}

describe('K3: PendingActionCard rendering', () => {
  it('shows object identity, diff rows, hard/soft constraints and source', () => {
    const model = toPendingActionCardModel({
      action_id: 'pa-10',
      tool_name: 'ontology.propose_action',
      run_id: 'run-77',
      proposal: {
        object_type: 'Flight',
        object_id: 'flt-100',
        run_id: 'run-77',
        tool_name: 'ontology.propose_action',
        simulate: {
          before: { stand: 'A10' },
          after: { stand: 'A12' },
          violations: [
            { rule_id: 'no_occupation_overlap', severity: 'hard', message: 'A12 已被占用' },
            { rule_id: 'prefer_near_stand', severity: 'soft', message: '建议使用近机位' },
          ],
          availability: { is_available: true },
        },
      },
    });

    const html = renderCard(model);

    expect(html).toContain('Flight');
    expect(html).toContain('flt-100');
    expect(html).toContain('A10');
    expect(html).toContain('A12');
    expect(html).toContain('硬约束违规');
    expect(html).toContain('no_occupation_overlap');
    expect(html).toContain('软约束提示');
    expect(html).toContain('prefer_near_stand');
    expect(html).toContain('run-77');
  });

  it('renders the irreversible marker for CRITICAL_WRITE actions', () => {
    const model = toPendingActionCardModel({
      action_id: 'pa-11',
      tool_name: 'change_stand',
      operation_level: 'CRITICAL_WRITE',
      entity_type: 'flight',
      entity_id: 'flt-200',
      before_snapshot: { stand: 'B2' },
      after_snapshot: { stand: 'B7' },
    });

    const html = renderCard(model);

    expect(html).toContain('不可逆操作');
    expect(html).toContain('对象');
    expect(html).toContain('flt-200');
  });

  it('does not crash when the payload carries no constraints or diff', () => {
    const model = toPendingActionCardModel({
      action_id: 'pa-12',
      tool_name: 'notify_teams',
      status: 'pending',
      message: 'approval required',
    });

    const html = renderCard(model);

    // Title prefers toolName over actionId when both exist.
    expect(html).toContain('notify_teams');
    // antd inserts a space between two CJK button characters ("批 准").
    expect(html).toMatch(/批\s*准/);
    expect(html).toMatch(/拒\s*绝/);
    expect(html).not.toContain('硬约束违规');
    expect(html).not.toContain('软约束提示');
    expect(html).not.toContain('不可逆操作');
  });

  it('renders a bare minimal model without throwing', () => {
    const html = renderCard({ actionId: 'pa-13' });
    expect(html).toContain('pa-13');
  });
});
