import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import EmptyState from './EmptyState.vue';

describe('EmptyState', () => {
  it('renders title text', () => {
    const wrapper = mount(EmptyState, { props: { title: '暂无数据' } });
    expect(wrapper.find('.empty-state-title').text()).toBe('暂无数据');
  });

  it('renders description when provided', () => {
    const wrapper = mount(EmptyState, {
      props: { title: '空', description: '请尝试其他搜索条件' },
    });
    expect(wrapper.find('.empty-state-desc').text()).toBe('请尝试其他搜索条件');
  });

  it('hides description when not provided', () => {
    const wrapper = mount(EmptyState, { props: { title: '空' } });
    expect(wrapper.find('.empty-state-desc').exists()).toBe(false);
  });

  it('renders action button when actionLabel is provided', () => {
    const wrapper = mount(EmptyState, {
      props: { title: '空', actionLabel: '重试' },
    });
    const btn = wrapper.find('.empty-state-action');
    expect(btn.exists()).toBe(true);
    expect(btn.text()).toBe('重试');
  });

  it('emits action event when button clicked', async () => {
    const wrapper = mount(EmptyState, {
      props: { title: '空', actionLabel: '重试' },
    });
    await wrapper.find('.empty-state-action').trigger('click');
    expect(wrapper.emitted('action')).toHaveLength(1);
  });

  it('disables action button when actionDisabled is true', () => {
    const wrapper = mount(EmptyState, {
      props: { title: '空', actionLabel: '重试', actionDisabled: true },
    });
    expect(wrapper.find('.empty-state-action').attributes('disabled')).toBeDefined();
  });

  it('renders correct icon path for each icon variant', () => {
    for (const icon of ['search', 'plane', 'alert', 'filter', 'data'] as const) {
      const wrapper = mount(EmptyState, { props: { title: 't', icon } });
      expect(wrapper.find('.empty-state-icon svg path').attributes('d')).toBeTruthy();
    }
  });

  it('has accessible role and aria-live', () => {
    const wrapper = mount(EmptyState, { props: { title: 't' } });
    const root = wrapper.find('.empty-state');
    expect(root.attributes('role')).toBe('status');
    expect(root.attributes('aria-live')).toBe('polite');
  });
});
