import { describe, expect, it } from 'vitest';
import { mount } from '@vue/test-utils';
import ChatMentionBody from './ChatMentionBody.vue';
import { splitChatMentionSegments } from './splitChatMentions';

describe('splitChatMentionSegments', () => {
  it('wraps @全体, @all / @All, and usernames', () => {
    expect(splitChatMentionSegments('请支援 @张三 @全体')).toEqual([
      { type: 'text', value: '请支援 ' },
      { type: 'mention', value: '@张三' },
      { type: 'text', value: ' ' },
      { type: 'mention', value: '@全体' },
    ]);
    expect(splitChatMentionSegments('hello @All there')).toEqual([
      { type: 'text', value: 'hello ' },
      { type: 'mention', value: '@All' },
      { type: 'text', value: ' there' },
    ]);
    expect(splitChatMentionSegments('ping @all')).toEqual([
      { type: 'text', value: 'ping ' },
      { type: 'mention', value: '@all' },
    ]);
  });

  it('leaves plain text unchanged', () => {
    expect(splitChatMentionSegments('hello world')).toEqual([
      { type: 'text', value: 'hello world' },
    ]);
    expect(splitChatMentionSegments('')).toEqual([]);
  });

  it('keeps HTML-looking strings as text, not markup', () => {
    expect(splitChatMentionSegments('<script>alert(1)</script>')).toEqual([
      { type: 'text', value: '<script>alert(1)</script>' },
    ]);
  });
});

describe('ChatMentionBody', () => {
  it('renders mention tokens in mark elements', () => {
    const wrapper = mount(ChatMentionBody, { props: { content: '请支援 @张三 @全体' } });
    const marks = wrapper.findAll('mark.chat-mention');
    expect(marks.map((node) => node.text())).toEqual(['@张三', '@全体']);
    expect(wrapper.text()).toBe('请支援 @张三 @全体');
  });

  it('does not inject raw HTML from message content', () => {
    const wrapper = mount(ChatMentionBody, {
      props: { content: '<script>alert(1)</script> @李四' },
    });
    expect(wrapper.find('script').exists()).toBe(false);
    expect(wrapper.text()).toContain('<script>alert(1)</script>');
    expect(wrapper.find('mark.chat-mention').text()).toBe('@李四');
  });
});
