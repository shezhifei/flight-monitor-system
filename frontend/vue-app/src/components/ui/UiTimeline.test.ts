import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import UiTimeline, { type UiTimelineItem } from './UiTimeline.vue';

const items: UiTimelineItem[] = [
  { key: '1', title: 'CA1234 滑出', time: '08:30', tone: 'ok' },
  { key: '2', title: 'CA1234 延误预警', time: '09:05', tone: 'warn' },
  { key: '3', title: 'CA1234 取消', time: '10:00', tone: 'danger' },
  { key: '4', title: '备注更新' },
];

describe('UiTimeline', () => {
  it('renders all items with titles', () => {
    const wrapper = mount(UiTimeline, { props: { items } });
    expect(wrapper.findAll('.ui-timeline-item')).toHaveLength(4);
  });

  it('maps tone to data-tone, defaulting to mute', () => {
    const wrapper = mount(UiTimeline, { props: { items } });
    const tones = wrapper.findAll('.ui-timeline-item').map((n) => n.attributes('data-tone'));
    expect(tones).toEqual(['ok', 'warn', 'danger', 'mute']);
  });

  it('renders time only when provided', () => {
    const wrapper = mount(UiTimeline, { props: { items } });
    expect(wrapper.findAll('.ui-timeline-time')).toHaveLength(3);
  });
});
