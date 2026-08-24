<script setup lang="ts">
import { computed, onBeforeUnmount, ref, useId, watch } from 'vue';
import type { Stakeholder } from '../../composables/useMentionStakeholders';
import { useMentionPicker } from '../../composables/useMentionPicker';
import UiMenu from './UiMenu.vue';
import UiMenuItem from './UiMenuItem.vue';
import UiPill from './UiPill.vue';

/**
 * 发送器（信号面 §3.3）：会话流下面那一件。页底 + 强线，获焦时整块显形。
 * Enter 发、Shift+Enter 换行 —— 各页不要再自己写一套输入框 + 发送钮。
 *
 * - 能中断才给「停止」：loading 是「正在流，可以叫停」，不是「忙」。
 *   请求没有 AbortController 就只给 disabled，别挂假的取消。
 * - 左边的 #tools 槽放随手开关（@全体、附件），右边永远只有那一颗主谓词。
 * - 给 maxlength 就自动报字数；快满了转警声。
 * - @ 候选可选：传了 stakeholders?.length 才开 picker；不传则与今天一致（AI 助手）。
 */
const props = withDefaults(defineProps<{
  modelValue: string;
  loading?: boolean;
  disabled?: boolean;
  placeholder?: string;
  /** 上限；给了就在左下角报读数 */
  maxlength?: number;
  sendLabel?: string;
  stakeholders?: Stakeholder[];
  includeAllMention?: boolean;
}>(), {
  loading: false,
  disabled: false,
  placeholder: '输入问题，Enter 发送，Shift+Enter 换行',
  maxlength: undefined,
  sendLabel: '发送',
  stakeholders: undefined,
  includeAllMention: false,
});

const emit = defineEmits<{
  'update:modelValue': [value: string];
  send: [];
  cancel: [];
}>();

const textareaRef = ref<HTMLTextAreaElement | null>(null);

const picker = useMentionPicker({
  getValue: () => props.modelValue,
  setValue: (v) => emit('update:modelValue', v),
  getTextarea: () => textareaRef.value,
  stakeholders: () => props.stakeholders ?? [],
  includeAll: props.includeAllMention === true,
});

const { isOpen, filtered, selectedIndex, selectAt, close, mentionIds, atAll, resetMentions } = picker;

defineExpose({ mentionIds, atAll, resetMentions });

const listId = useId();
function optionId(index: number): string {
  return `${listId}-opt-${index}`;
}

const MENU_MAX_H = 280;
const MENU_MIN_H = 160;
const MENU_GAP = 4;

const menuPos = ref({ x: 0, y: 0 });

function placeMenu(): void {
  const el = textareaRef.value;
  if (!el) return;
  const rect = el.getBoundingClientRect();
  const roomBelow = window.innerHeight - rect.bottom - MENU_GAP;
  menuPos.value = {
    x: Math.round(rect.left),
    y: roomBelow >= MENU_MIN_H
      ? Math.round(rect.bottom + MENU_GAP)
      : Math.round(Math.max(MENU_GAP, rect.top - MENU_GAP - MENU_MAX_H)),
  };
}

watch(isOpen, (open) => {
  if (open) {
    placeMenu();
    window.addEventListener('scroll', placeMenu, true);
    window.addEventListener('resize', placeMenu);
  } else {
    window.removeEventListener('scroll', placeMenu, true);
    window.removeEventListener('resize', placeMenu);
  }
});

onBeforeUnmount(() => {
  window.removeEventListener('scroll', placeMenu, true);
  window.removeEventListener('resize', placeMenu);
});

function onInput(event: Event) {
  picker.onInput(event);
  if (isOpen.value) placeMenu();
}

const nearLimit = computed(() => (
  props.maxlength !== undefined && props.modelValue.length >= props.maxlength * 0.9
));

function onKeydown(e: KeyboardEvent) {
  if (isOpen.value) {
    picker.onKeydown(e);
    return;
  }
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault();
    if (props.modelValue.trim() && !props.loading && !props.disabled) {
      emit('send');
    }
  }
}
</script>

<template>
  <div class="chat-sender">
    <textarea
      ref="textareaRef"
      class="chat-sender-input"
      :value="modelValue"
      :placeholder="placeholder"
      :disabled="disabled"
      :maxlength="maxlength"
      rows="2"
      :aria-controls="isOpen ? listId : undefined"
      :aria-activedescendant="isOpen ? optionId(selectedIndex) : undefined"
      @input="onInput"
      @keydown="onKeydown"
      @blur="close"
    />
    <!-- 常在弹窗里，绝对定位会被裁；定点 Teleport 到 body。悬停不挪 selectedIndex（§2.5 / §4.2） -->
    <Teleport v-if="isOpen" to="body">
      <UiMenu
        :id="listId"
        class="chat-sender-mention-list"
        role="listbox"
        label="提醒人员"
        :x="menuPos.x"
        :y="menuPos.y"
        min-width="240px"
        @mousedown.prevent
      >
        <UiMenuItem
          v-for="(s, i) in filtered"
          :id="optionId(i)"
          :key="s.user_id"
          role="option"
          :selected="i === selectedIndex"
          @mousedown.prevent="selectAt(s)"
        >
          <span class="chat-sender-mention-opt">
            <span class="chat-sender-mention-name">{{ s.username }}</span>
            <UiPill v-if="s.is_dispatcher">调度</UiPill>
            <UiPill v-if="s.is_assignee">责任人</UiPill>
          </span>
        </UiMenuItem>
      </UiMenu>
    </Teleport>
    <div class="chat-sender-actions">
      <div class="chat-sender-tools">
        <slot name="tools" />
        <span v-if="maxlength" class="chat-sender-count" :data-tone="nearLimit ? 'warn' : 'mute'">
          {{ modelValue.length }}/{{ maxlength }}
        </span>
      </div>
      <button
        v-if="loading"
        type="button"
        class="chat-sender-btn is-stop"
        @click="emit('cancel')"
      >
        停止
      </button>
      <button
        v-else
        type="button"
        class="chat-sender-btn is-send"
        :disabled="disabled || !modelValue.trim()"
        @click="emit('send')"
      >
        {{ sendLabel }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.chat-sender {
  border: 1px solid var(--line-strong);
  border-radius: var(--r-panel);
  background: var(--face-page);
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.chat-sender:focus-within {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}

.chat-sender-input {
  width: 100%;
  border: none;
  background: transparent;
  color: var(--ink);
  font-size: var(--fs-body);
  font-family: inherit;
  resize: none;
  box-sizing: border-box;
}

.chat-sender-input:focus {
  outline: none;
}

.chat-sender-input::placeholder {
  color: var(--ink-muted);
}

.chat-sender-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s3);
}

/* 左边是随手开关与读数，右边只留主谓词 */
.chat-sender-tools {
  display: flex;
  align-items: center;
  gap: var(--s3);
  min-width: 0;
  font-size: var(--fs-label);
  color: var(--ink-subtle);
}

.chat-sender-count {
  font-family: var(--mono);
  font-variant-numeric: tabular-nums;
}

.chat-sender-count[data-tone='warn'] {
  color: var(--warn);
}

.chat-sender-btn {
  min-height: var(--h-sm);
  padding: 0 16px;
  border-radius: var(--r-control);
  border: none;
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  cursor: pointer;
}

.is-send {
  background: var(--act);
  color: var(--act-on);
}

.is-send:disabled {
  background: color-mix(in srgb, var(--ink) 8%, transparent);
  color: var(--ink-muted);
  cursor: not-allowed;
}

.is-stop {
  background: var(--danger-soft);
  color: var(--danger);
}

.chat-sender-mention-list {
  max-height: 280px;
  overflow-y: auto;
}

.chat-sender-mention-opt {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.chat-sender-mention-name {
  font-weight: var(--fw-medium);
  color: var(--ink);
}
</style>
