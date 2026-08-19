<script setup lang="ts">
defineProps<{
  checked: boolean;
  label?: string;
  disabled?: boolean;
}>();

const emit = defineEmits<{
  (e: 'update:checked', value: boolean): void;
}>();
</script>

<template>
  <button
    type="button"
    class="ui-switch"
    role="switch"
    :aria-checked="checked ? 'true' : 'false'"
    :aria-label="label"
    :disabled="disabled"
    @click="emit('update:checked', !checked)"
  />
</template>

<style scoped>
.ui-switch {
  box-sizing: border-box;
  width: 36px;
  height: 20px;
  padding: 0;
  border: 0;
  border-radius: var(--r-pill);
  background: var(--line-strong);
  position: relative;
  flex-shrink: 0;
  cursor: pointer;
}

.ui-switch::after {
  content: "";
  box-sizing: border-box;
  position: absolute;
  top: 50%;
  left: 2px;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: var(--ink);
  transform: translateY(-50%);
}

.ui-switch[aria-checked='true'] {
  background: var(--act);
}

.ui-switch[aria-checked='true']::after {
  background: var(--act-on);
  transform: translate(16px, -50%);
}

.ui-switch:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
}

.ui-switch:disabled {
  cursor: not-allowed;
  background: color-mix(in srgb, var(--ink) 16%, transparent);
}
</style>
