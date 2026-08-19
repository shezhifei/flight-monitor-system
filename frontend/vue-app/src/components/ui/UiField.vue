<script setup lang="ts">
defineProps<{
  label?: string;
  hint?: string;
  error?: string;
  forId?: string;
}>();
</script>

<template>
  <div class="ui-field" :data-error="error ? 'true' : undefined">
    <label v-if="label" class="ui-field-label" :for="forId">{{ label }}</label>
    <slot />
    <p v-if="error" class="ui-field-hint" role="alert">{{ error }}</p>
    <p v-else-if="hint" class="ui-field-hint">{{ hint }}</p>
  </div>
</template>

<style scoped>
.ui-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.ui-field-label {
  font-size: var(--fs-label);
  color: var(--ink-subtle);
  font-weight: var(--fw-medium);
}

.ui-field-hint {
  margin: 0;
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

.ui-field[data-error='true'] .ui-field-hint {
  color: var(--danger);
}

.ui-field :slotted(input),
.ui-field :slotted(select),
.ui-field :slotted(textarea) {
  width: 100%;
  height: var(--h-sm);
  padding: 0 10px;
  border-radius: var(--r-control);
  border: 1px solid var(--line-strong);
  background: var(--face-page);
  color: var(--ink);
  font: inherit;
  box-sizing: border-box;
}

.ui-field :slotted(textarea) {
  height: auto;
  min-height: 72px;
  padding: 8px 10px;
  resize: vertical;
}

.ui-field :slotted(input:hover),
.ui-field :slotted(select:hover),
.ui-field :slotted(textarea:hover) {
  border-color: var(--act);
}

.ui-field :slotted(input:focus-visible),
.ui-field :slotted(select:focus-visible),
.ui-field :slotted(textarea:focus-visible) {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}

.ui-field[data-error='true'] :slotted(input),
.ui-field[data-error='true'] :slotted(textarea) {
  border-color: var(--danger);
  background: var(--danger-soft);
}
</style>
