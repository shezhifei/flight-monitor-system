<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';

/**
 * 选择器（信号面 §5.2）：触发器像输入，列表像菜单，不露系统选择器。
 * 持守：当前值写在触发器上；列表用抬起面。
 */
const props = withDefaults(defineProps<{
  id?: string;
  modelValue: string;
  options: { value: string; label: string }[];
  /** 触发器与列表的无障碍名称（组件内绑到 aria-label） */
  label: string;
  minWidth?: string;
}>(), {
  id: undefined,
  minWidth: '128px',
});

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
}>();

const open = ref(false);
const activeIndex = ref(0);
const rootRef = ref<HTMLElement | null>(null);
const triggerRef = ref<HTMLButtonElement | null>(null);

const selectedLabel = computed(() => (
  props.options.find((option) => option.value === props.modelValue)?.label ?? props.options[0]?.label ?? ''
));

function optionDomId(index: number): string {
  return props.id ? `${props.id}-option-${index}` : `ui-select-option-${index}`;
}

function select(value: string): void {
  emit('update:modelValue', value);
  close();
  triggerRef.value?.focus();
}

function close(): void {
  open.value = false;
}

function toggle(): void {
  open.value = !open.value;
  if (open.value) {
    const current = props.options.findIndex((option) => option.value === props.modelValue);
    activeIndex.value = current >= 0 ? current : 0;
  }
}

function moveActive(delta: number): void {
  if (!open.value) {
    open.value = true;
    const current = props.options.findIndex((option) => option.value === props.modelValue);
    activeIndex.value = current >= 0 ? current : 0;
    return;
  }
  const count = props.options.length;
  if (!count) return;
  activeIndex.value = (activeIndex.value + delta + count) % count;
}

function onTriggerKeydown(event: KeyboardEvent): void {
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault();
    moveActive(event.key === 'ArrowDown' ? 1 : -1);
  } else if (event.key === 'Enter' || event.key === ' ') {
    if (open.value) {
      event.preventDefault();
      select(props.options[activeIndex.value]?.value ?? props.modelValue);
    }
  } else if (event.key === 'Escape' && open.value) {
    event.preventDefault();
    close();
  }
}

function onDocumentPointerDown(event: PointerEvent): void {
  if (!open.value) return;
  if (rootRef.value && !rootRef.value.contains(event.target as Node)) {
    close();
  }
}

watch(() => props.options, () => {
  const current = props.options.findIndex((option) => option.value === props.modelValue);
  if (current >= 0) {
    activeIndex.value = current;
  }
});

if (typeof document !== 'undefined') {
  document.addEventListener('pointerdown', onDocumentPointerDown, true);
}

onBeforeUnmount(() => {
  if (typeof document !== 'undefined') {
    document.removeEventListener('pointerdown', onDocumentPointerDown, true);
  }
});
</script>

<template>
  <div ref="rootRef" class="ui-select" :style="{ minWidth }">
    <button
      :id="id"
      ref="triggerRef"
      type="button"
      class="ui-select__btn"
      :aria-haspopup="'listbox'"
      :aria-expanded="open"
      :aria-label="label"
      @click="toggle"
      @keydown="onTriggerKeydown"
    >
      <span class="ui-select__value">{{ selectedLabel }}</span>
    </button>
    <ul
      v-if="open"
      class="ui-select__list"
      role="listbox"
      :aria-label="label"
    >
      <li
        v-for="(option, index) in options"
        :id="optionDomId(index)"
        :key="option.value"
        role="option"
        :aria-selected="option.value === modelValue"
        :class="{ 'is-active': index === activeIndex }"
        @click="select(option.value)"
        @pointerenter="activeIndex = index"
      >
        {{ option.label }}
      </li>
    </ul>
  </div>
</template>

<style scoped>
.ui-select {
  position: relative;
  display: inline-flex;
}

.ui-select__btn {
  display: inline-flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  width: 100%;
  height: var(--h-sm);
  padding: 0 10px;
  border: 1px solid var(--line-strong);
  border-radius: var(--r-control);
  background: var(--face-page);
  color: var(--ink);
  font-size: var(--fs-label);
  font-family: inherit;
  font-weight: var(--fw-regular);
  cursor: pointer;
  transition: border-color var(--t-fast) var(--ease);
  font-variant-numeric: tabular-nums;
}

.ui-select__btn::after {
  content: "";
  width: 12px;
  height: 12px;
  flex-shrink: 0;
  background: currentColor;
  opacity: 0.55;
  mask: url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'><path fill='none' stroke='black' stroke-width='1.5' stroke-linecap='round' stroke-linejoin='round' d='M2.5 4.5 6 8l3.5-3.5'/></svg>") center / contain no-repeat;
  -webkit-mask: url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'><path fill='none' stroke='black' stroke-width='1.5' stroke-linecap='round' stroke-linejoin='round' d='M2.5 4.5 6 8l3.5-3.5'/></svg>") center / contain no-repeat;
  transition: transform var(--t-fast) var(--ease);
}

.ui-select__btn:hover {
  border-color: var(--ink-subtle);
}

.ui-select__btn:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
  border-color: var(--act);
}

.ui-select__btn[aria-expanded='true']::after {
  transform: rotate(180deg);
}

.ui-select__value {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ui-select__list {
  position: absolute;
  left: 0;
  right: 0;
  top: calc(100% + 4px);
  margin: 0;
  padding: 4px;
  list-style: none;
  background: var(--face-raised);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-panel);
  box-shadow: var(--shadow-md);
  z-index: 30;
  animation: ui-select-pop-in var(--t-mid) var(--ease);
}

.ui-select__list li {
  height: 32px;
  padding: 0 10px;
  border-radius: var(--r-cell);
  display: flex;
  align-items: center;
  cursor: pointer;
  font-size: var(--fs-label);
  color: var(--ink);
  font-variant-numeric: tabular-nums;
}

.ui-select__list li.is-active {
  background: color-mix(in srgb, var(--ink) 10%, transparent);
}

.ui-select__list li[aria-selected='true'] {
  color: var(--act);
  font-weight: 550;
}

@keyframes ui-select-pop-in {
  from {
    opacity: 0;
    transform: translateY(4px);
  }
  to {
    opacity: 1;
    transform: none;
  }
}
</style>
