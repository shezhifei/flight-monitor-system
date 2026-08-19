import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import UiButton from './UiButton.vue';
import UiSegment from './UiSegment.vue';
import UiSwitch from './UiSwitch.vue';
import UiPill from './UiPill.vue';
import UiBanner from './UiBanner.vue';
import UiField from './UiField.vue';

describe('UiButton', () => {
  it('exposes pressed via aria-pressed', () => {
    const on = mount(UiButton, { props: { pressed: true }, slots: { default: '异常' } });
    expect(on.attributes('aria-pressed')).toBe('true');
    const off = mount(UiButton, { props: { pressed: false }, slots: { default: '异常' } });
    expect(off.attributes('aria-pressed')).toBe('false');
  });

  it('does not set aria-pressed for verbs', () => {
    const wrapper = mount(UiButton, { slots: { default: '导出' } });
    expect(wrapper.attributes('aria-pressed')).toBeUndefined();
  });
});

describe('UiSegment', () => {
  it('is a radiogroup', () => {
    const wrapper = mount(UiSegment, {
      props: { label: '视图' },
      slots: { default: '<button aria-checked="true">卡片</button>' },
    });
    expect(wrapper.attributes('role')).toBe('radiogroup');
    expect(wrapper.attributes('aria-label')).toBe('视图');
    expect(wrapper.attributes('data-inset')).toBe('page');
  });

  it('nests one step down on a raised surface', () => {
    const wrapper = mount(UiSegment, { props: { label: '页签', inset: 'work' } });
    expect(wrapper.attributes('data-inset')).toBe('work');
  });
});

describe('UiSwitch', () => {
  it('toggles via update:checked', async () => {
    const wrapper = mount(UiSwitch, { props: { checked: false, label: '回执' } });
    expect(wrapper.attributes('aria-checked')).toBe('false');
    await wrapper.trigger('click');
    expect(wrapper.emitted('update:checked')![0]).toEqual([true]);
  });
});

describe('UiPill', () => {
  it('maps tone', () => {
    const wrapper = mount(UiPill, { props: { tone: 'danger' }, slots: { default: 'CRITICAL' } });
    expect(wrapper.attributes('data-tone')).toBe('danger');
    expect(wrapper.text()).toBe('CRITICAL');
  });
});

describe('UiBanner', () => {
  it('renders status role', () => {
    const wrapper = mount(UiBanner, { props: { tone: 'warn' }, slots: { default: '重连中' } });
    expect(wrapper.attributes('role')).toBe('status');
    expect(wrapper.text()).toBe('重连中');
  });
});

describe('UiField', () => {
  it('shows error as alert', () => {
    const wrapper = mount(UiField, {
      props: { label: '标题', error: '必填' },
      slots: { default: '<input>' },
    });
    expect(wrapper.find('[role="alert"]').text()).toBe('必填');
  });
});
