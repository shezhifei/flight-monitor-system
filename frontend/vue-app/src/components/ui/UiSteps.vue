<script setup lang="ts">
export interface UiStep {
  key: string;
  title: string;
  description?: string;
  status: 'wait' | 'process' | 'finish' | 'error';
}

defineProps<{
  steps: UiStep[];
}>();
</script>

<template>
  <ol class="ui-steps">
    <li v-for="step in steps" :key="step.key" class="ui-step" :data-status="step.status">
      <span class="ui-step-dot" aria-hidden="true" />
      <div class="ui-step-content">
        <div class="ui-step-title">{{ step.title }}</div>
        <div v-if="step.description" class="ui-step-desc">{{ step.description }}</div>
      </div>
    </li>
  </ol>
</template>

<style scoped>
.ui-steps {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
}

.ui-step {
  position: relative;
  display: flex;
  gap: 10px;
  padding-bottom: 16px;
}

.ui-step:last-child {
  padding-bottom: 0;
}

.ui-step:not(:last-child)::before {
  content: '';
  position: absolute;
  left: 5px;
  top: 16px;
  bottom: 2px;
  width: 1px;
  background: var(--line);
}

.ui-step-dot {
  flex-shrink: 0;
  width: 11px;
  height: 11px;
  margin-top: 3px;
  border-radius: 50%;
  border: 2px solid var(--line-strong);
  background: var(--face-page);
  box-sizing: border-box;
}

.ui-step[data-status='process'] .ui-step-dot {
  border-color: var(--act);
  background: var(--act-soft);
}

.ui-step[data-status='finish'] .ui-step-dot {
  border-color: var(--ok);
  background: var(--ok);
}

.ui-step[data-status='error'] .ui-step-dot {
  border-color: var(--danger);
  background: var(--danger);
}

.ui-step-title {
  font-size: var(--fs-body);
  font-weight: var(--fw-medium);
  color: var(--ink);
}

.ui-step[data-status='wait'] .ui-step-title {
  color: var(--ink-muted);
  font-weight: var(--fw-regular);
}

.ui-step[data-status='error'] .ui-step-title {
  color: var(--danger);
}

.ui-step-desc {
  margin-top: 2px;
  font-size: var(--fs-label);
  color: var(--ink-subtle);
  line-height: 1.5;
}
</style>
