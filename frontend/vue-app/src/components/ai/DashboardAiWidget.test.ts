import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { ref } from 'vue';
import DashboardAiWidget from './DashboardAiWidget.vue';

type Handler = (event: Event | MessageEvent<string>) => void;

const listeners = new Map<string, Set<Handler>>();
const connectMock = vi.fn().mockResolvedValue(null);

vi.mock('@/composables/useSSE', () => ({
  useSSE: () => ({
    source: ref(null),
    status: ref('idle'),
    error: ref(null),
    lastMessage: ref(null),
    isConnected: ref(false),
    connect: connectMock,
    disconnect: vi.fn(),
    reconnect: vi.fn(),
    on: (name: string, handler: Handler) => {
      const set = listeners.get(name) ?? new Set<Handler>();
      set.add(handler);
      listeners.set(name, set);
      return () => set.delete(handler);
    },
    off: vi.fn(),
  }),
}));

function emitSse(name: string, payload: unknown) {
  const event = new MessageEvent<string>(name, { data: JSON.stringify(payload) });
  listeners.get(name)?.forEach((handler) => handler(event));
}

function makeExecution(overrides: Record<string, unknown> = {}) {
  return { type: 'ai_execution', payload: { event: 'tool_start', tool_name: 'query_flights', ...overrides } };
}

describe('DashboardAiWidget', () => {
  beforeEach(() => {
    listeners.clear();
    connectMock.mockClear();
    document.body.innerHTML = '';
  });

  it('connects SSE on mount with dashboard scope', () => {
    mount(DashboardAiWidget);
    expect(connectMock).toHaveBeenCalledTimes(1);
  });

  it('shows floating button without badge initially', () => {
    const wrapper = mount(DashboardAiWidget);
    expect(wrapper.find('.ai-widget-fab').exists()).toBe(true);
    expect(wrapper.find('.ai-widget-badge').exists()).toBe(false);
  });

  it('increments unread badge for visible event types while drawer closed', async () => {
    const wrapper = mount(DashboardAiWidget);
    emitSse('ai_execution', makeExecution());
    emitSse('ai_execution', makeExecution({ event: 'execution_end', message: '执行完成' }));
    await wrapper.vm.$nextTick();
    expect(wrapper.find('.ai-widget-badge').text()).toBe('2');
  });

  it('ignores non-visible semantic types', async () => {
    const wrapper = mount(DashboardAiWidget);
    emitSse('ai_execution', makeExecution({ event: 'chunk' }));
    await wrapper.vm.$nextTick();
    expect(wrapper.find('.ai-widget-badge').exists()).toBe(false);
  });

  it('opens drawer with events and clears unread', async () => {
    const wrapper = mount(DashboardAiWidget, { global: { stubs: { teleport: true } } });
    emitSse('ai_execution', makeExecution());
    await wrapper.find('.ai-widget-fab').trigger('click');
    expect(wrapper.find('.ui-drawer').exists()).toBe(true);
    expect(wrapper.findAll('.ui-timeline-item')).toHaveLength(1);
    expect(wrapper.find('.ai-widget-badge').exists()).toBe(false);
  });

  it('records stream error as danger event', async () => {
    const wrapper = mount(DashboardAiWidget, { global: { stubs: { teleport: true } } });
    listeners.get('error')?.forEach((handler) => handler(new Event('error')));
    await wrapper.vm.$nextTick();
    await wrapper.find('.ai-widget-fab').trigger('click');
    const items = wrapper.findAll('.ui-timeline-item');
    expect(items).toHaveLength(1);
    expect(items[0]!.attributes('data-tone')).toBe('danger');
  });
});
