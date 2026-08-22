import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import MentionInput from './MentionInput.vue';

const stakeholders = [
  { user_id: 'u1', username: '张三', is_dispatcher: true, is_assignee: false },
  { user_id: 'u2', username: '李四', is_dispatcher: false, is_assignee: true },
  { user_id: 'u3', username: '王五', is_dispatcher: false, is_assignee: false },
];

function mountInput(modelValue = '') {
  return mount(MentionInput, {
    props: { modelValue, stakeholders },
    global: { stubs: { teleport: true } },
  });
}

describe('MentionInput', () => {
  it('keeps the picker shut until an @ is typed', () => {
    const wrapper = mountInput();
    expect(wrapper.find('[role="listbox"]').exists()).toBe(false);
    expect(wrapper.find('textarea').attributes('aria-activedescendant')).toBeUndefined();
  });

  it('opens a listbox of options, not a menu of verbs', async () => {
    const wrapper = mountInput();
    await wrapper.find('textarea').setValue('@');
    const list = wrapper.find('[role="listbox"]');
    expect(list.exists()).toBe(true);
    expect(list.attributes('aria-label')).toBe('提醒人员');
    expect(wrapper.findAll('[role="option"]')).toHaveLength(3);
  });

  it('reports the keyboard cursor as aria-selected, and the textarea points at it', async () => {
    const wrapper = mountInput();
    const textarea = wrapper.find('textarea');
    await textarea.setValue('@');

    const optionIds = wrapper.findAll('[role="option"]').map((o) => o.attributes('id'));
    expect(wrapper.findAll('[role="option"]')[0].attributes('aria-selected')).toBe('true');
    expect(textarea.attributes('aria-activedescendant')).toBe(optionIds[0]);

    await textarea.trigger('keydown', { key: 'ArrowDown' });
    expect(wrapper.findAll('[role="option"]')[0].attributes('aria-selected')).toBe('false');
    expect(wrapper.findAll('[role="option"]')[1].attributes('aria-selected')).toBe('true');
    expect(textarea.attributes('aria-activedescendant')).toBe(optionIds[1]);
  });

  it('narrows the list by the keyword after the @', async () => {
    const wrapper = mountInput();
    await wrapper.find('textarea').setValue('@李');
    const options = wrapper.findAll('[role="option"]');
    expect(options).toHaveLength(1);
    expect(options[0].text()).toContain('李四');
  });

  it('shuts the picker when nothing matches instead of showing an empty list', async () => {
    const wrapper = mountInput();
    await wrapper.find('textarea').setValue('@没有这个人');
    expect(wrapper.find('[role="listbox"]').exists()).toBe(false);
  });

  it('writes the mention back and reports the mentioned id', async () => {
    // modelValue 是父级持有的：这里先把它摆成「已经打到 @」的那一刻
    const wrapper = mountInput('收到 @');
    await wrapper.find('textarea').setValue('收到 @');
    await wrapper.findAll('[role="option"]')[0].trigger('mousedown');
    expect(wrapper.emitted('update:modelValue')?.at(-1)).toEqual(['收到 @张三 ']);
    expect(wrapper.emitted('update:mentionIds')?.at(-1)).toEqual([['u1']]);
  });

  it('closes on Escape', async () => {
    const wrapper = mountInput();
    const textarea = wrapper.find('textarea');
    await textarea.setValue('@');
    await textarea.trigger('keydown', { key: 'Escape' });
    expect(wrapper.find('[role="listbox"]').exists()).toBe(false);
  });
});
