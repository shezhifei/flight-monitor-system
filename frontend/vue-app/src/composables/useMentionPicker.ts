import { computed, nextTick, ref, watch, type ComputedRef, type Ref } from 'vue';
import type { Stakeholder } from './useMentionStakeholders';

export const MENTION_ALL_ID = '@all';

const MENTION_ALL: Stakeholder = {
  user_id: MENTION_ALL_ID,
  username: '全体',
  is_assignee: false,
  is_dispatcher: false,
};

export interface MentionPickerOptions {
  getValue: () => string;
  setValue: (v: string) => void;
  getTextarea: () => HTMLTextAreaElement | null;
  stakeholders: () => Stakeholder[];
  includeAll?: boolean;
}

export interface MentionPicker {
  isOpen: ComputedRef<boolean>;
  filtered: ComputedRef<Stakeholder[]>;
  selectedIndex: Ref<number>;
  onInput: (event: Event) => void;
  onKeydown: (event: KeyboardEvent) => void;
  selectAt: (target: Stakeholder | number) => void;
  close: () => void;
  mentionIds: Ref<string[]>;
  atAll: Ref<boolean>;
  resetMentions: () => void;
  syncFromValue: () => void;
}

function matchesKeyword(stakeholder: Stakeholder, keyword: string): boolean {
  return (
    stakeholder.username.toLowerCase().includes(keyword) ||
    stakeholder.user_id.toLowerCase().includes(keyword)
  );
}

export function useMentionPicker(options: MentionPickerOptions): MentionPicker {
  const includeAll = options.includeAll === true;
  const showDropdown = ref(false);
  const searchKeyword = ref('');
  const selectedIndex = ref(0);
  const mentionTriggerPos = ref(-1);
  const mentionedIds = new Set<string>();
  const mentionIds = ref<string[]>([]);
  const atAll = ref(false);

  const filtered = computed(() => {
    const keyword = searchKeyword.value.toLowerCase();
    const people = options.stakeholders();
    const matched = keyword ? people.filter((s) => matchesKeyword(s, keyword)) : people;
    if (!includeAll) return matched;
    const allMatches = !keyword || matchesKeyword(MENTION_ALL, keyword);
    return allMatches ? [MENTION_ALL, ...matched] : matched;
  });

  const isOpen = computed(() => showDropdown.value && filtered.value.length > 0);

  function publishMentionIds(): void {
    mentionIds.value = Array.from(mentionedIds);
  }

  function close(): void {
    showDropdown.value = false;
  }

  function resetMentions(): void {
    mentionedIds.clear();
    publishMentionIds();
    atAll.value = false;
    close();
  }

  function syncFromValue(): void {
    if (!options.getValue()) {
      mentionedIds.clear();
      publishMentionIds();
      atAll.value = false;
    }
  }

  function detectTrigger(value: string, cursorPos: number): void {
    const textBeforeCursor = value.substring(0, cursorPos);
    const atIndex = textBeforeCursor.lastIndexOf('@');

    if (atIndex >= 0) {
      const charBeforeAt = atIndex > 0 ? textBeforeCursor[atIndex - 1] : ' ';
      if (charBeforeAt === ' ' || charBeforeAt === '\n' || atIndex === 0) {
        const query = textBeforeCursor.substring(atIndex + 1);
        if (!query.includes(' ') && !query.includes('\n')) {
          showDropdown.value = true;
          searchKeyword.value = query;
          mentionTriggerPos.value = atIndex;
          selectedIndex.value = 0;
          return;
        }
      }
    }

    showDropdown.value = false;
    searchKeyword.value = '';
    mentionTriggerPos.value = -1;
  }

  function onInput(event: Event): void {
    const target = event.target as HTMLTextAreaElement;
    const value = target.value;
    options.setValue(value);
    detectTrigger(value, target.selectionStart || 0);
  }

  function selectAt(target: Stakeholder | number): void {
    const stakeholder = typeof target === 'number' ? filtered.value[target] : target;
    const textarea = options.getTextarea();
    if (!stakeholder || !textarea || mentionTriggerPos.value < 0) return;

    const value = options.getValue();
    const cursorPos = textarea.selectionStart || 0;
    const beforeAt = value.substring(0, mentionTriggerPos.value);
    const afterCursor = value.substring(cursorPos);
    const mentionText = `@${stakeholder.username} `;
    const newValue = beforeAt + mentionText + afterCursor;

    options.setValue(newValue);

    if (stakeholder.user_id === MENTION_ALL_ID) {
      atAll.value = true;
    } else {
      mentionedIds.add(stakeholder.user_id);
      publishMentionIds();
    }

    showDropdown.value = false;
    searchKeyword.value = '';
    mentionTriggerPos.value = -1;

    nextTick(() => {
      const newCursorPos = beforeAt.length + mentionText.length;
      textarea.setSelectionRange(newCursorPos, newCursorPos);
      textarea.focus();
    });
  }

  function onKeydown(event: KeyboardEvent): void {
    if (!isOpen.value) return;

    if (event.key === 'ArrowDown') {
      event.preventDefault();
      selectedIndex.value = Math.min(selectedIndex.value + 1, filtered.value.length - 1);
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      selectedIndex.value = Math.max(selectedIndex.value - 1, 0);
    } else if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      selectAt(selectedIndex.value);
    } else if (event.key === 'Escape') {
      close();
    }
  }

  watch(() => options.getValue(), syncFromValue, { flush: 'sync' });

  return {
    isOpen,
    filtered,
    selectedIndex,
    onInput,
    onKeydown,
    selectAt,
    close,
    mentionIds,
    atAll,
    resetMentions,
    syncFromValue,
  };
}
