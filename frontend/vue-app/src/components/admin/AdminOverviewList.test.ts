import { describe, expect, it } from 'vitest';
import { mount } from '@vue/test-utils';
import AdminOverviewList from './AdminOverviewList.vue';
import type { AdminOverviewItem } from './adminOverviewTypes';

const items: AdminOverviewItem[] = [
  { id: 'a', title: '拖飞机', meta: 'TOWING · 机坪', deletable: true },
  { id: 'b', title: '廊桥', meta: 'BRIDGE', description: '靠桥保障' },
];

describe('AdminOverviewList', () => {
  it('renders items and highlights selection', () => {
    const wrapper = mount(AdminOverviewList, {
      props: { items, selectedId: 'b' },
    });
    expect(wrapper.text()).toContain('拖飞机');
    expect(wrapper.text()).toContain('廊桥');
    expect(wrapper.text()).toContain('BRIDGE');
    expect(wrapper.text()).toContain('靠桥保障');
    const options = wrapper.findAll('[role="option"]');
    expect(options).toHaveLength(2);
    expect(options[1].classes()).toContain('active');
  });

  it('emits select on item click', async () => {
    const wrapper = mount(AdminOverviewList, {
      props: { items, selectedId: null },
    });
    await wrapper.findAll('.admin-overview-item__body')[0].trigger('click');
    expect(wrapper.emitted('select')?.[0]).toEqual(['a']);
  });

  it('shows empty and error states', () => {
    const empty = mount(AdminOverviewList, {
      props: { items: [], emptyText: '暂无任务类型' },
    });
    expect(empty.text()).toContain('暂无任务类型');

    const err = mount(AdminOverviewList, {
      props: { items: [], errorText: '加载失败' },
    });
    expect(err.text()).toContain('加载失败');
  });

  it('emits delete when delete button is used', async () => {
    const wrapper = mount(AdminOverviewList, {
      props: { items, showDelete: true, selectedId: 'a' },
    });
    const del = wrapper.find('.admin-overview-item__delete');
    expect(del.exists()).toBe(true);
    await del.trigger('click');
    expect(wrapper.emitted('delete')?.[0]).toEqual(['a']);
  });

  it('does not show delete when showDelete is false', () => {
    const wrapper = mount(AdminOverviewList, {
      props: { items, showDelete: false },
    });
    expect(wrapper.find('.admin-overview-item__delete').exists()).toBe(false);
  });

  it('deprecate mode: × on active, restore on deprecated', async () => {
    const flowItems: AdminOverviewItem[] = [
      { id: 'a', title: '正常', meta: 'A' },
      { id: 'b', title: '旧事项', meta: 'B', deprecated: true },
    ];
    const wrapper = mount(AdminOverviewList, {
      props: {
        items: flowItems,
        showDelete: true,
        actionMode: 'deprecate',
        deleteTitle: '弃用该类型',
        restoreTitle: '恢复使用',
      },
    });
    expect(wrapper.findAll('.admin-overview-item__delete')).toHaveLength(1);
    expect(wrapper.findAll('.admin-overview-item__restore')).toHaveLength(1);
    expect(wrapper.text()).toContain('已弃用');
    await wrapper.find('.admin-overview-item__delete').trigger('click');
    expect(wrapper.emitted('delete')?.[0]).toEqual(['a']);
    await wrapper.find('.admin-overview-item__restore').trigger('click');
    expect(wrapper.emitted('restore')?.[0]).toEqual(['b']);
  });
});
