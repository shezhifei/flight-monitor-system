<script setup lang="ts">
import { ref, computed, nextTick, watch } from 'vue';
import type { Stakeholder } from '../../composables/useMentionStakeholders';

const props = defineProps<{
  modelValue: string;
  stakeholders: Stakeholder[];
  placeholder?: string;
  maxlength?: number;
  rows?: number;
}>();

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
  (e: 'update:mentionIds', ids: string[]): void;
}>();

const textareaRef = ref<HTMLTextAreaElement | null>(null);
const showDropdown = ref(false);
const searchKeyword = ref('');
const selectedIndex = ref(0);
const mentionedIds = ref<Set<string>>(new Set());
const mentionTriggerPos = ref(-1);

const filteredStakeholders = computed(() => {
  const keyword = searchKeyword.value.toLowerCase();
  if (!keyword) return props.stakeholders;
  return props.stakeholders.filter(
    (s) =>
      s.username.toLowerCase().includes(keyword) ||
      s.user_id.toLowerCase().includes(keyword),
  );
});

function onInput(event: Event) {
  const target = event.target as HTMLTextAreaElement;
  const value = target.value;
  emit('update:modelValue', value);

  const cursorPos = target.selectionStart || 0;
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

function selectStakeholder(stakeholder: Stakeholder) {
  if (!textareaRef.value || mentionTriggerPos.value < 0) return;

  const textarea = textareaRef.value;
  const value = props.modelValue;
  const cursorPos = textarea.selectionStart || 0;

  const beforeAt = value.substring(0, mentionTriggerPos.value);
  const afterCursor = value.substring(cursorPos);
  const mentionText = `@${stakeholder.username} `;
  const newValue = beforeAt + mentionText + afterCursor;

  emit('update:modelValue', newValue);
  mentionedIds.value.add(stakeholder.user_id);
  emit('update:mentionIds', Array.from(mentionedIds.value));

  showDropdown.value = false;
  searchKeyword.value = '';
  mentionTriggerPos.value = -1;

  nextTick(() => {
    if (textarea) {
      const newCursorPos = beforeAt.length + mentionText.length;
      textarea.setSelectionRange(newCursorPos, newCursorPos);
      textarea.focus();
    }
  });
}

function onKeydown(event: KeyboardEvent) {
  if (!showDropdown.value || filteredStakeholders.value.length === 0) return;

  if (event.key === 'ArrowDown') {
    event.preventDefault();
    selectedIndex.value = Math.min(selectedIndex.value + 1, filteredStakeholders.value.length - 1);
  } else if (event.key === 'ArrowUp') {
    event.preventDefault();
    selectedIndex.value = Math.max(selectedIndex.value - 1, 0);
  } else if (event.key === 'Enter' && !event.shiftKey) {
    event.preventDefault();
    selectStakeholder(filteredStakeholders.value[selectedIndex.value]);
  } else if (event.key === 'Escape') {
    showDropdown.value = false;
  }
}

function closeDropdown() {
  showDropdown.value = false;
}

// Re-emit mention IDs whenever modelValue changes externally (clear scenario)
watch(
  () => props.modelValue,
  (val) => {
    if (!val) {
      mentionedIds.value.clear();
      emit('update:mentionIds', []);
    }
  },
);
</script>

<template>
  <div class="mention-input-wrapper" @click.stop>
    <textarea
      ref="textareaRef"
      :value="modelValue"
      :placeholder="placeholder || '填写回复内容，输入 @ 可提醒相关人员。'"
      :maxlength="maxlength || 2000"
      :rows="rows || 4"
      class="mention-textarea"
      @input="onInput"
      @keydown="onKeydown"
      @blur="closeDropdown"
    />
    <Teleport to="body">
      <div
        v-if="showDropdown && filteredStakeholders.length > 0"
        class="mention-dropdown"
        @mousedown.prevent
      >
        <div class="mention-dropdown-header">
          选择提醒人员
        </div>
        <div
          v-for="(s, i) in filteredStakeholders"
          :key="s.user_id"
          class="mention-option"
          :class="{ 'mention-option-active': i === selectedIndex }"
          @mousedown.prevent="selectStakeholder(s)"
          @mouseenter="selectedIndex = i"
        >
          <span class="mention-username">{{ s.username }}</span>
          <span v-if="s.is_dispatcher" class="mention-role-tag">调度</span>
          <span v-if="s.is_assignee" class="mention-role-tag mention-role-assignee">责任人</span>
        </div>
        <div v-if="filteredStakeholders.length === 0" class="mention-empty">
          无匹配成员
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.mention-input-wrapper {
  position: relative;
  width: 100%;
}

.mention-textarea {
  width: 100%;
  resize: vertical;
  border: 1px solid var(--border-light, #d7e0e8);
  border-radius: 12px;
  padding: 12px 14px;
  font-size: 13px;
  line-height: 1.6;
  box-sizing: border-box;
  font-family: inherit;
  transition: border-color 0.15s;
}

.mention-textarea:focus {
  outline: none;
  border-color: var(--service-blue, #007AFF);
  box-shadow: 0 0 0 3px var(--focus-ring-blue);
}

.mention-dropdown {
  position: fixed;
  z-index: 100000;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  width: 320px;
  max-height: 280px;
  overflow-y: auto;
  background: var(--admin-card-bg);
  border: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
  border-radius: 12px;
  box-shadow: 0 12px 40px rgba(15, 23, 42, 0.18);
  padding: 4px 0;
}

.mention-dropdown-header {
  padding: 8px 14px;
  font-size: 11px;
  font-weight: 600;
  color: var(--text-tertiary, #8E8E93);
  border-bottom: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
}

.mention-option {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 14px;
  cursor: pointer;
  font-size: 13px;
  transition: background 0.1s;
}

.mention-option:hover,
.mention-option-active {
  background: var(--system-blue-subtle);
}

.mention-username {
  font-weight: 500;
  color: var(--text-primary, #102132);
}

.mention-role-tag {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 999px;
  background: var(--system-blue-subtle);
  color: var(--service-blue, #007AFF);
  font-weight: 600;
}

.mention-role-assignee {
  background: var(--dh-signal-warn-soft);
  color: var(--ws-warn);
}

.mention-empty {
  padding: 12px 14px;
  font-size: 12px;
  color: var(--text-tertiary, #8E8E93);
  text-align: center;
}
</style>
