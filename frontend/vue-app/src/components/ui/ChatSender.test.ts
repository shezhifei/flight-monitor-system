import { describe, it, expect } from 'vitest';
import { unref } from 'vue';
import { mount, type VueWrapper } from '@vue/test-utils';
import ChatSender from './ChatSender.vue';

const stakeholders = [
  { user_id: 'u1', username: '张三', is_dispatcher: true, is_assignee: false },
  { user_id: 'u2', username: '李四', is_dispatcher: false, is_assignee: true },
];

async function typeAndSync(wrapper: VueWrapper, text: string) {
  await wrapper.find('textarea').setValue(text);
  await wrapper.setProps({
    modelValue: (wrapper.emitted('update:modelValue')?.at(-1)?.[0] as string | undefined) ?? text,
  });
}

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

  it('does not open a listbox when stakeholders are omitted', async () => {
    const wrapper = mount(ChatSender, { props: { modelValue: '' } });
    await wrapper.find('textarea').setValue('@');
    expect(wrapper.find('[role="listbox"]').exists()).toBe(false);
  });

  it('does not emit send on Enter when mention picker is open', async () => {
    const wrapper = mount(ChatSender, {
      props: { modelValue: '', stakeholders },
      global: { stubs: { teleport: true } },
    });
    await typeAndSync(wrapper, '@');
    expect(wrapper.find('[role="listbox"]').exists()).toBe(true);
    await wrapper.find('textarea').trigger('keydown', { key: 'Enter' });
    expect(wrapper.emitted('send')).toBeUndefined();
    expect(wrapper.emitted('update:modelValue')?.at(-1)).toEqual(['@张三 ']);
  });

  it('selecting a person exposes mentionIds', async () => {
    const wrapper = mount(ChatSender, {
      props: { modelValue: '', stakeholders },
      global: { stubs: { teleport: true } },
    });
    await typeAndSync(wrapper, '@');
    await wrapper.findAll('[role="option"]')[0].trigger('mousedown');
    expect(unref(wrapper.vm.mentionIds)).toEqual(expect.arrayContaining(['u1']));
    expect(unref(wrapper.vm.atAll)).toBe(false);
  });

  it('includeAllMention puts 全体 first; selecting it sets atAll', async () => {
    const wrapper = mount(ChatSender, {
      props: { modelValue: '', stakeholders, includeAllMention: true },
      global: { stubs: { teleport: true } },
    });
    await typeAndSync(wrapper, '@');
    const options = wrapper.findAll('[role="option"]');
    expect(options[0].text()).toContain('全体');
    await options[0].trigger('mousedown');
    expect(unref(wrapper.vm.atAll)).toBe(true);
    expect(unref(wrapper.vm.mentionIds)).not.toContain('@all');
  });

  it('resetMentions clears ids and atAll', async () => {
    const wrapper = mount(ChatSender, {
      props: { modelValue: '', stakeholders, includeAllMention: true },
      global: { stubs: { teleport: true } },
    });
    await typeAndSync(wrapper, '@');
    await wrapper.findAll('[role="option"]')[1].trigger('mousedown');
    expect(unref(wrapper.vm.mentionIds)).toEqual(expect.arrayContaining(['u1']));
    await typeAndSync(wrapper, '@');
    await wrapper.findAll('[role="option"]')[0].trigger('mousedown');
    expect(unref(wrapper.vm.atAll)).toBe(true);
    wrapper.vm.resetMentions();
    expect(unref(wrapper.vm.mentionIds)).toEqual([]);
    expect(unref(wrapper.vm.atAll)).toBe(false);
  });
});
