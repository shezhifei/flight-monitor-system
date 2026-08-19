import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import UiSteps, { type UiStep } from './UiSteps.vue';

const steps: UiStep[] = [
  { key: 'a', title: '提交任务', status: 'finish' },
  { key: 'b', title: '执行中', description: '预计 2 分钟', status: 'process' },
  { key: 'c', title: '生成报告', status: 'wait' },
];

describe('UiSteps', () => {
  it('renders all steps with titles', () => {
    const wrapper = mount(UiSteps, { props: { steps } });
    expect(wrapper.findAll('.ui-step')).toHaveLength(3);
    expect(wrapper.findAll('.ui-step-title').map((n) => n.text())).toEqual([
      '提交任务',
      '执行中',
      '生成报告',
    ]);
  });

  it('maps status to data-status attribute', () => {
    const wrapper = mount(UiSteps, { props: { steps } });
    const items = wrapper.findAll('.ui-step');
    expect(items[0]!.attributes('data-status')).toBe('finish');
    expect(items[1]!.attributes('data-status')).toBe('process');
    expect(items[2]!.attributes('data-status')).toBe('wait');
  });

  it('renders description only when provided', () => {
    const wrapper = mount(UiSteps, { props: { steps } });
    expect(wrapper.findAll('.ui-step-desc')).toHaveLength(1);
    expect(wrapper.find('.ui-step-desc').text()).toBe('预计 2 分钟');
  });

  it('renders nothing when steps empty', () => {
    const wrapper = mount(UiSteps, { props: { steps: [] } });
    expect(wrapper.findAll('.ui-step')).toHaveLength(0);
  });
});
