// @vitest-environment node
import { effectScope, nextTick, ref, type EffectScope, type Ref } from 'vue';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Stakeholder } from './useMentionStakeholders';
import { MENTION_ALL_ID, useMentionPicker, type MentionPicker } from './useMentionPicker';

const stakeholders: Stakeholder[] = [
  { user_id: 'u1', username: '张三', is_dispatcher: true, is_assignee: false },
  { user_id: 'u2', username: '李四', is_dispatcher: false, is_assignee: true },
  { user_id: 'u3', username: '王五', is_dispatcher: false, is_assignee: false },
];

type FakeTextarea = {
  value: string;
  selectionStart: number;
  selectionEnd: number;
  setSelectionRange: (start: number, end: number) => void;
  focus: ReturnType<typeof vi.fn>;
};

function keyEvent(key: string, extra: { shiftKey?: boolean } = {}): KeyboardEvent {
  return {
    key,
    shiftKey: extra.shiftKey ?? false,
    preventDefault: vi.fn(),
  } as unknown as KeyboardEvent;
}

describe('useMentionPicker', () => {
  let scope: EffectScope;

  beforeEach(() => {
    scope = effectScope();
  });

  afterEach(() => {
    scope.stop();
  });

  function createPicker(includeAll = false): {
    picker: MentionPicker;
    value: Ref<string>;
    textarea: FakeTextarea;
    type: (text: string, cursor?: number) => void;
  } {
    const value = ref('');
    const textarea: FakeTextarea = {
      value: '',
      selectionStart: 0,
      selectionEnd: 0,
      setSelectionRange(start: number, end: number) {
        this.selectionStart = start;
        this.selectionEnd = end;
      },
      focus: vi.fn(),
    };

    const picker = scope.run(() =>
      useMentionPicker({
        getValue: () => value.value,
        setValue: (v) => {
          value.value = v;
          textarea.value = v;
        },
        getTextarea: () => textarea as unknown as HTMLTextAreaElement,
        stakeholders: () => stakeholders,
        includeAll,
      }),
    )!;

    function type(text: string, cursor = text.length): void {
      textarea.value = text;
      textarea.selectionStart = cursor;
      textarea.selectionEnd = cursor;
      picker.onInput({ target: textarea } as unknown as Event);
    }

    return { picker, value, textarea, type };
  }

  it('opens when @ is typed and stakeholders exist', () => {
    const { picker, type } = createPicker();
    expect(picker.isOpen.value).toBe(false);
    type('@');
    expect(picker.isOpen.value).toBe(true);
    expect(picker.filtered.value).toHaveLength(3);
    expect(picker.selectedIndex.value).toBe(0);
  });

  it('filters by keyword after @', () => {
    const { picker, type } = createPicker();
    type('@李');
    expect(picker.isOpen.value).toBe(true);
    expect(picker.filtered.value.map((s) => s.username)).toEqual(['李四']);
  });

  it('shuts when nothing matches instead of showing an empty list', () => {
    const { picker, type } = createPicker();
    type('@没有这个人');
    expect(picker.filtered.value).toHaveLength(0);
    expect(picker.isOpen.value).toBe(false);
  });

  it('selecting a person inserts @username and appends user_id', async () => {
    const { picker, value, textarea, type } = createPicker();
    type('收到 @');
    picker.selectAt(picker.filtered.value[0]);
    expect(value.value).toBe('收到 @张三 ');
    expect(picker.mentionIds.value).toEqual(['u1']);
    await nextTick();
    expect(textarea.selectionStart).toBe('收到 @张三 '.length);
    expect(textarea.focus).toHaveBeenCalled();
  });

  it('includeAll puts 全体 first; selecting it sets atAll and skips mentionIds', async () => {
    const { picker, value, type } = createPicker(true);
    type('@');
    expect(picker.filtered.value[0]).toMatchObject({
      user_id: MENTION_ALL_ID,
      username: '全体',
    });
    picker.selectAt(picker.filtered.value[0]);
    expect(value.value).toBe('@全体 ');
    expect(picker.atAll.value).toBe(true);
    expect(picker.mentionIds.value).not.toContain(MENTION_ALL_ID);
    expect(picker.mentionIds.value).toEqual([]);
    await nextTick();
    expect(picker.isOpen.value).toBe(false);
  });

  it('includeAll keeps 全体 when the keyword matches it', () => {
    const { picker, type } = createPicker(true);
    type('@全');
    expect(picker.filtered.value[0]?.username).toBe('全体');
    type('@all');
    expect(picker.filtered.value[0]?.user_id).toBe(MENTION_ALL_ID);
    type('@张');
    expect(picker.filtered.value.map((s) => s.user_id)).toEqual(['u1']);
  });

  it('without includeAll, filtered does not include 全体', () => {
    const { picker, type } = createPicker(false);
    type('@');
    expect(picker.filtered.value.map((s) => s.username)).toEqual(['张三', '李四', '王五']);
    expect(picker.filtered.value.some((s) => s.user_id === MENTION_ALL_ID)).toBe(false);
  });

  it('clearing value resets mentionIds and atAll', () => {
    const { picker, value, type } = createPicker(true);
    type('@');
    picker.selectAt(picker.filtered.value[0]);
    type('@张');
    picker.selectAt(picker.filtered.value[0]);
    expect(picker.atAll.value).toBe(true);
    expect(picker.mentionIds.value).toEqual(['u1']);

    value.value = '';
    expect(picker.mentionIds.value).toEqual([]);
    expect(picker.atAll.value).toBe(false);
  });

  it('resetMentions clears ids, atAll, and closes', () => {
    const { picker, type } = createPicker(true);
    type('@');
    picker.selectAt(1);
    expect(picker.mentionIds.value).toEqual(['u1']);
    type('@');
    expect(picker.isOpen.value).toBe(true);
    picker.resetMentions();
    expect(picker.mentionIds.value).toEqual([]);
    expect(picker.atAll.value).toBe(false);
    expect(picker.isOpen.value).toBe(false);
  });

  it('arrow keys and Enter select; Escape closes', () => {
    const { picker, value, type } = createPicker();
    type('@');

    const down = keyEvent('ArrowDown');
    picker.onKeydown(down);
    expect(down.preventDefault).toHaveBeenCalled();
    expect(picker.selectedIndex.value).toBe(1);

    const up = keyEvent('ArrowUp');
    picker.onKeydown(up);
    expect(picker.selectedIndex.value).toBe(0);

    picker.onKeydown(keyEvent('ArrowDown'));
    const enter = keyEvent('Enter');
    picker.onKeydown(enter);
    expect(enter.preventDefault).toHaveBeenCalled();
    expect(value.value).toBe('@李四 ');
    expect(picker.mentionIds.value).toEqual(['u2']);
    expect(picker.isOpen.value).toBe(false);
  });

  it('Escape closes without selecting; keys do nothing when shut', () => {
    const { picker, type } = createPicker();
    type('@');
    picker.onKeydown(keyEvent('Escape'));
    expect(picker.isOpen.value).toBe(false);

    const down = keyEvent('ArrowDown');
    picker.onKeydown(down);
    expect(down.preventDefault).not.toHaveBeenCalled();
  });
});
