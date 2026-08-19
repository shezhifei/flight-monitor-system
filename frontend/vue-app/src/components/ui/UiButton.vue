<script setup lang="ts">
import { computed } from 'vue';

const props = withDefaults(defineProps<{
  variant?: 'ghost' | 'primary' | 'danger' | 'quiet';
  size?: 'sm' | 'md';
  pressed?: boolean;
  disabled?: boolean;
  nativeType?: 'button' | 'submit';
}>(), {
  variant: 'ghost',
  size: 'sm',
  nativeType: 'button',
  pressed: undefined,
});

const pressedAttr = computed(() => (
  props.pressed === undefined ? {} : { 'aria-pressed': (props.pressed ? 'true' : 'false') as 'true' | 'false' }
));
</script>

<template>
  <button
    :type="nativeType"
    class="ui-btn"
    :data-variant="variant"
    :data-size="size"
    v-bind="pressedAttr"
    :disabled="disabled"
  >
    <slot />
  </button>
</template>

<style scoped>
.ui-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: var(--h-sm);
  padding: 0 12px;
  border-radius: var(--r-control);
  border: 1px solid transparent;
  background: transparent;
  color: var(--ink);
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  font-family: inherit;
  cursor: pointer;
}

.ui-btn[data-size='md'] {
  height: var(--h-md);
  padding: 0 14px;
}

.ui-btn:hover {
  border-color: var(--ink-muted);
}

.ui-btn:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
}

.ui-btn[data-variant='ghost'] {
  border-color: var(--line-strong);
}

.ui-btn[data-variant='ghost']:hover {
  border-color: var(--ink-muted);
  background: transparent;
}

.ui-btn[data-variant='primary'] {
  background: var(--act);
  color: var(--act-on);
  border-color: var(--act);
}

.ui-btn[data-variant='primary']:hover {
  filter: brightness(1.06);
  background: var(--act);
}

.ui-btn[data-variant='danger'] {
  background: var(--danger-soft);
  color: var(--danger);
  border-color: var(--danger);
}

.ui-btn[data-variant='quiet'] {
  color: var(--ink-subtle);
}

.ui-btn[data-variant='quiet']:hover {
  color: var(--ink);
  border-color: transparent;
}

.ui-btn[aria-pressed='true'] {
  background: var(--act-soft);
  color: var(--act);
  border-color: var(--act);
}

.ui-btn[aria-pressed='true']:hover {
  background: var(--act-soft);
  color: var(--act);
  border-color: var(--act);
  filter: none;
}

.ui-btn:disabled {
  cursor: not-allowed;
  background: color-mix(in srgb, var(--ink) 7%, transparent);
  color: var(--ink-muted);
  border-color: transparent;
  filter: none;
}
</style>
