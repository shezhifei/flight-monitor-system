<script setup lang="ts">
import { computed, ref } from 'vue';

/**
 * 搜索器（信号面 §2.5「搜索有值」/ §5.2）：
 * - 长得像输入：face-page 凹底 + line-strong 边，聚焦亮行动色。
 * - 有值是持守：框内出现清除，不在底下再挂一枚芯片。
 * - 清除是动词：点完回到静，不留蓝。
 */
const props = withDefaults(defineProps<{
  modelValue: string;
  /** 无障碍名称；同时作为 placeholder 兜底 */
  label: string;
  placeholder?: string;
  id?: string;
  disabled?: boolean;
  /** 在寻簇中是否吃满剩余宽度（窄栏里通常独占一行） */
  grow?: boolean;
}>(), {
  placeholder: undefined,
  id: undefined,
  disabled: false,
  grow: true,
});

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
  (e: 'submit'): void;
  (e: 'clear'): void;
}>();

const inputRef = ref<HTMLInputElement | null>(null);
const hasValue = computed(() => props.modelValue.trim().length > 0);

function onInput(event: Event): void {
  emit('update:modelValue', (event.target as HTMLInputElement).value);
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === 'Enter') {
    event.preventDefault();
    emit('submit');
  } else if (event.key === 'Escape' && hasValue.value) {
    event.preventDefault();
    clear();
  }
}

function clear(): void {
  emit('update:modelValue', '');
  emit('clear');
  inputRef.value?.focus();
}
</script>

<template>
  <div class="ui-search" :data-grow="grow ? 'true' : undefined" :data-has-value="hasValue ? 'true' : undefined">
    <span class="ui-search__icon" aria-hidden="true">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none">
        <circle cx="11" cy="11" r="8" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
        <line x1="21" y1="21" x2="16.65" y2="16.65" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
    </span>
    <input
      :id="id"
      ref="inputRef"
      type="search"
      class="ui-search__input"
      :value="modelValue"
      :placeholder="placeholder ?? label"
      :aria-label="label"
      :disabled="disabled"
      autocomplete="off"
      @input="onInput"
      @keydown="onKeydown"
    >
    <button
      v-if="hasValue"
      type="button"
      class="ui-search__clear"
      aria-label="清除搜索"
      @click="clear"
    >
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" aria-hidden="true">
        <path d="M6 6l12 12M18 6L6 18" />
      </svg>
    </button>
    <slot name="after" />
  </div>
</template>

<style scoped>
.ui-search {
  display: inline-flex;
  align-items: center;
  gap: var(--s2);
  box-sizing: border-box;
  min-width: 0;
  height: var(--h-sm);
  padding: 0 8px 0 10px;
  background: var(--face-page);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-control);
  transition: border-color var(--t-fast) var(--ease), box-shadow var(--t-fast) var(--ease);
}

.ui-search[data-grow='true'] {
  flex: 1 1 220px;
}

.ui-search:hover {
  border-color: var(--ink-subtle);
}

.ui-search:focus-within {
  border-color: var(--act);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--act) 18%, transparent);
}

.ui-search__icon {
  display: inline-flex;
  flex: none;
  color: var(--ink-muted);
}

/* 有值是持守：图标转到行动色，手离开仍在 */
.ui-search[data-has-value='true'] .ui-search__icon {
  color: var(--act);
}

.ui-search__input {
  flex: 1 1 auto;
  min-width: 0;
  width: 100%;
  border: none;
  outline: none;
  background: none;
  color: var(--ink);
  caret-color: var(--act);
  font-size: var(--fs-body);
  font-family: inherit;
}

/* 环画在外框，内层不再画一圈 */
.ui-search__input:focus-visible {
  outline: none;
  box-shadow: none;
}

.ui-search__input::placeholder {
  color: var(--ink-muted);
}

/* 系统自带的清除叉与装饰按 §4.22 收掉，清除由框内动词承担 */
.ui-search__input::-webkit-search-cancel-button,
.ui-search__input::-webkit-search-decoration {
  appearance: none;
  -webkit-appearance: none;
}

.ui-search__clear {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  width: 18px;
  height: 18px;
  padding: 0;
  border: none;
  border-radius: var(--r-pill);
  background: none;
  color: var(--ink-muted);
  cursor: pointer;
}

.ui-search__clear:hover {
  color: var(--ink);
  background: color-mix(in srgb, var(--ink) 10%, transparent);
}

.ui-search__clear:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}
</style>
