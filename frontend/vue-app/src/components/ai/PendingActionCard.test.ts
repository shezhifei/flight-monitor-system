import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import PendingActionCard from './PendingActionCard.vue';
import type { PendingActionCardModel } from '@/lib/ai/pendingActionDiff';

const baseAction: PendingActionCardModel = {
  actionId: 'act-1',
  toolName: 'assign_vehicle',
  status: 'pending',
  message: '需要人工确认',
};

describe('PendingActionCard', () => {
  it('renders tool name and status', () => {
    const wrapper = mount(PendingActionCard, { props: { action: baseAction } });
    expect(wrapper.find('.pa-tool-name').text()).toBe('assign_vehicle');
    expect(wrapper.text()).toContain('状态: pending');
  });

  it('falls back to actionId when toolName missing', () => {
    const wrapper = mount(PendingActionCard, { props: { action: { actionId: 'act-9' } } });
    expect(wrapper.find('.pa-tool-name').text()).toBe('act-9');
  });

  it('renders hard/soft violations and diff rows', () => {
    const wrapper = mount(PendingActionCard, {
      props: {
        action: {
          ...baseAction,
          hardViolations: [{ kind: 'hard', passed: false, name: 'rest_rule', message: '机组执勤超限' }],
          softViolations: [{ kind: 'soft', passed: false, name: 'cost_hint' }],
          diffRows: [{ field: 'vehicle', before: 'B-1', after: 'B-2' }],
          irreversible: true,
        },
      },
    });
    expect(wrapper.text()).toContain('硬约束违规');
    expect(wrapper.text()).toContain('rest_rule: 机组执勤超限');
    expect(wrapper.text()).toContain('cost_hint');
    expect(wrapper.text()).toContain('不可逆操作');
    const cells = wrapper.findAll('.pa-diff tbody td').map((td) => td.text());
    expect(cells).toEqual(['vehicle', 'B-1', 'B-2']);
  });

  it('emits approve and reject with actionId', async () => {
    const wrapper = mount(PendingActionCard, { props: { action: baseAction } });
    await wrapper.find('.is-approve').trigger('click');
    await wrapper.find('.is-reject').trigger('click');
    expect(wrapper.emitted('approve')).toEqual([['act-1']]);
    expect(wrapper.emitted('reject')).toEqual([['act-1']]);
  });

  it('disables buttons when busy', () => {
    const wrapper = mount(PendingActionCard, { props: { action: baseAction, busy: true } });
    expect(wrapper.find('.is-approve').attributes('disabled')).toBeDefined();
    expect(wrapper.find('.is-reject').attributes('disabled')).toBeDefined();
  });
});
