import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import UiModal from './UiModal.vue';
import UiDockButton from './UiDockButton.vue';
import UiMenu from './UiMenu.vue';
import UiMenuItem from './UiMenuItem.vue';

describe('UiModal', () => {
  it('renders nothing when closed', () => {
    const wrapper = mount(UiModal, { props: { open: false, title: 't' }, global: { stubs: { teleport: true } } });
    expect(wrapper.find('.ui-modal').exists()).toBe(false);
  });

  it('renders title and body when open', () => {
    const wrapper = mount(UiModal, {
      props: { open: true, title: '创建事项' },
      slots: { default: '<p>body</p>', footer: '<button>确认</button>' },
      global: { stubs: { teleport: true } },
    });
    expect(wrapper.find('.ui-modal-title').text()).toBe('创建事项');
    expect(wrapper.find('.ui-modal-body').text()).toBe('body');
    expect(wrapper.find('.ui-modal-footer').exists()).toBe(true);
  });

  it('hides footer when slot absent', () => {
    const wrapper = mount(UiModal, { props: { open: true, title: 't' }, global: { stubs: { teleport: true } } });
    expect(wrapper.find('.ui-modal-footer').exists()).toBe(false);
  });

  it('emits close on scrim click and close button', async () => {
    const wrapper = mount(UiModal, { props: { open: true, title: 't' }, global: { stubs: { teleport: true } } });
    await wrapper.find('.ui-modal-scrim').trigger('click');
    await wrapper.find('.ui-modal-close').trigger('click');
    expect(wrapper.emitted('close')).toHaveLength(2);
  });

  it('does not close when clicking inside the panel', async () => {
    const wrapper = mount(UiModal, { props: { open: true, title: 't' }, global: { stubs: { teleport: true } } });
    await wrapper.find('.ui-modal').trigger('click');
    expect(wrapper.emitted('close')).toBeUndefined();
  });

  it('hides close control and ignores scrim when not closable', async () => {
    const wrapper = mount(UiModal, {
      props: { open: true, title: '关键通知', closable: false },
      global: { stubs: { teleport: true } },
    });
    expect(wrapper.find('.ui-modal-close').exists()).toBe(false);
    await wrapper.find('.ui-modal-scrim').trigger('click');
    expect(wrapper.emitted('close')).toBeUndefined();
  });

  it('forwards id onto the dialog', () => {
    const wrapper = mount(UiModal, {
      props: { open: true, title: 't', id: 'flightBatchEditModal' },
      global: { stubs: { teleport: true } },
    });
    expect(wrapper.find('.ui-modal').attributes('id')).toBe('flightBatchEditModal');
  });
});

describe('UiDockButton', () => {
  it('renders label and count', () => {
    const wrapper = mount(UiDockButton, { props: { label: '调度网关', count: 3 } });
    expect(wrapper.find('.ui-dock-label').text()).toBe('调度网关');
    expect(wrapper.find('.ui-dock-count').text()).toBe('3');
  });

  it('omits count when null', () => {
    const wrapper = mount(UiDockButton, { props: { label: 'AI 洞察', count: null } });
    expect(wrapper.find('.ui-dock-count').exists()).toBe(false);
  });

  it('reflects pressed state via aria-checked on a checkable menu entry', () => {
    const on = mount(UiDockButton, { props: { label: '异常告警', pressed: true, tone: 'danger' } });
    expect(on.attributes('role')).toBe('menuitemcheckbox');
    expect(on.attributes('aria-checked')).toBe('true');
    expect(on.attributes('data-on')).toBe('true');
    const off = mount(UiDockButton, { props: { label: '异常告警', pressed: false } });
    expect(off.attributes('aria-checked')).toBe('false');
    expect(off.attributes('data-on')).toBeUndefined();
  });

  it('is a plain menu entry when it holds nothing', () => {
    const wrapper = mount(UiDockButton, { props: { label: 'AI 洞察' } });
    expect(wrapper.attributes('role')).toBe('menuitem');
    expect(wrapper.attributes('aria-checked')).toBeUndefined();
  });

  it('emits click', async () => {
    const wrapper = mount(UiDockButton, { props: { label: 'x' } });
    await wrapper.trigger('click');
    expect(wrapper.emitted('click')).toHaveLength(1);
  });
});

describe('UiMenu', () => {
  it('is a list of verbs by default', () => {
    const wrapper = mount(UiMenu, { props: { label: '更多' } });
    expect(wrapper.attributes('role')).toBe('menu');
    expect(wrapper.attributes('aria-label')).toBe('更多');
    expect(wrapper.attributes('data-pinned')).toBeUndefined();
  });

  it('becomes a listbox for a picker and pins to viewport coordinates', () => {
    const wrapper = mount(UiMenu, { props: { role: 'listbox', label: '提醒人员', x: 12, y: 40 } });
    expect(wrapper.attributes('role')).toBe('listbox');
    expect(wrapper.attributes('data-pinned')).toBe('true');
    expect(wrapper.attributes('style')).toContain('left: 12px');
    expect(wrapper.attributes('style')).toContain('top: 40px');
  });
});

describe('UiMenuItem', () => {
  it('is a one-shot verb with no aria state by default', () => {
    const wrapper = mount(UiMenuItem, { slots: { default: '导出' } });
    expect(wrapper.attributes('role')).toBe('menuitem');
    expect(wrapper.attributes('aria-selected')).toBeUndefined();
  });

  it('reports the keyboard cursor via aria-selected when it is an option', () => {
    const on = mount(UiMenuItem, { props: { role: 'option', selected: true }, slots: { default: '张三' } });
    expect(on.attributes('role')).toBe('option');
    expect(on.attributes('aria-selected')).toBe('true');
    const off = mount(UiMenuItem, { props: { role: 'option', selected: false }, slots: { default: '李四' } });
    expect(off.attributes('aria-selected')).toBe('false');
  });
});
