import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import AiPlanBoard from './AiPlanBoard.vue';
import AiSubagentTree from './AiSubagentTree.vue';
import AiCompressionNotice from './AiCompressionNotice.vue';
import type { PlanBoardModel } from '@/lib/ai/planBoardModel';
import type { SubagentNodeModel } from '@/lib/ai/subagentTreeModel';

const board: PlanBoardModel = {
  description: '查询并汇总',
  steps: [
    { id: 's1', description: '查询航班', status: 'done' },
    { id: 's2', description: '汇总结果', status: 'in_progress', assignedTo: 'agent-a' },
    { id: 's3', description: '生成报告', status: 'pending' },
    { id: 's4', description: '失败步骤', status: 'blocked', error: 'timeout' },
  ],
};

describe('AiPlanBoard', () => {
  it('renders steps with mapped statuses', () => {
    const wrapper = mount(AiPlanBoard, { props: { model: board } });
    const statuses = wrapper.findAll('.ui-step').map((n) => n.attributes('data-status'));
    expect(statuses).toEqual(['finish', 'process', 'wait', 'error']);
    expect(wrapper.text()).toContain('查询并汇总');
    expect(wrapper.text()).toContain('agent-a');
    expect(wrapper.text()).toContain('timeout');
  });

  it('shows empty state when no steps', () => {
    const wrapper = mount(AiPlanBoard, { props: { model: null } });
    expect(wrapper.text()).toContain('暂无计划步骤');
  });
});

describe('AiSubagentTree', () => {
  const nodes: SubagentNodeModel[] = [
    { id: 'n1', depth: 1, label: 'planner', status: 'done', proposalOnly: true, toolCalls: 3, lastActivity: 'completed' },
    { id: 'n2', depth: 2, label: 'worker', status: 'running', proposalOnly: true, toolCalls: 1 },
  ];

  it('renders nodes with status labels and meta', () => {
    const wrapper = mount(AiSubagentTree, { props: { nodes } });
    expect(wrapper.findAll('.ai-sub-node')).toHaveLength(2);
    expect(wrapper.text()).toContain('planner');
    expect(wrapper.text()).toContain('已完成');
    expect(wrapper.text()).toContain('运行中');
    expect(wrapper.text()).toContain('proposal_only');
    expect(wrapper.text()).toContain('工具调用 3');
  });

  it('indents by depth', () => {
    const wrapper = mount(AiSubagentTree, { props: { nodes } });
    const items = wrapper.findAll('.ai-sub-node');
    expect(items[1]!.attributes('style')).toContain('padding-left: 20px');
  });

  it('shows empty state', () => {
    const wrapper = mount(AiSubagentTree, { props: { nodes: [] } });
    expect(wrapper.text()).toContain('暂无子代理');
  });
});

describe('AiCompressionNotice', () => {
  it('renders nothing when notice is null', () => {
    const wrapper = mount(AiCompressionNotice, { props: { notice: null } });
    expect(wrapper.find('[data-testid="compression-notice"]').exists()).toBe(false);
  });

  it('renders token delta when provided', () => {
    const wrapper = mount(AiCompressionNotice, {
      props: { notice: { strategy: 'summary', beforeTokens: 12000, afterTokens: 3000, at: '10:00:00' } },
    });
    expect(wrapper.text()).toContain('12000 → 3000 tokens');
    expect(wrapper.text()).toContain('summary');
  });
});
