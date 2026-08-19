import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import ChatSender from './ChatSender.vue';

describe('ChatSender', () => {
  it('emits update:modelValue on input', async () => {
    const wrapper = mount(ChatSender, { props: { modelValue: '' } });
    const input = wrapper.find('textarea');
    await input.setValue('查询今日航班');
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual(['查询今日航班']);
  });

  it('emits send on Enter with non-empty text', async () => {
    const wrapper = mount(ChatSender, { props: { modelValue: '你好' } });
    await wrapper.find('textarea').trigger('keydown', { key: 'Enter' });
    expect(wrapper.emitted('send')).toHaveLength(1);
  });

  it('does not emit send on Enter when text empty', async () => {
    const wrapper = mount(ChatSender, { props: { modelValue: '  ' } });
    await wrapper.find('textarea').trigger('keydown', { key: 'Enter' });
    expect(wrapper.emitted('send')).toBeUndefined();
  });

  it('does not emit send on Shift+Enter', async () => {
    const wrapper = mount(ChatSender, { props: { modelValue: '你好' } });
    await wrapper.find('textarea').trigger('keydown', { key: 'Enter', shiftKey: true });
    expect(wrapper.emitted('send')).toBeUndefined();
  });

  it('send button disabled when text empty', () => {
    const wrapper = mount(ChatSender, { props: { modelValue: '' } });
    expect(wrapper.find('.is-send').attributes('disabled')).toBeDefined();
  });

  it('shows stop button and emits cancel when loading', async () => {
    const wrapper = mount(ChatSender, { props: { modelValue: 'x', loading: true } });
    expect(wrapper.find('.is-send').exists()).toBe(false);
    const stop = wrapper.find('.is-stop');
    expect(stop.exists()).toBe(true);
    await stop.trigger('click');
    expect(wrapper.emitted('cancel')).toHaveLength(1);
  });
});
