<script setup lang="ts">
import UiSteps, { type UiStep } from '@/components/ui/UiSteps.vue';
import type { PlanBoardModel, PlanStepStatus } from '@/lib/ai/planBoardModel';

withDefaults(defineProps<{
  model: PlanBoardModel | null;
  title?: string;
}>(), {
  title: '执行计划',
});

function stepStatus(status: PlanStepStatus): UiStep['status'] {
  if (status === 'in_progress') return 'process';
  if (status === 'done') return 'finish';
  if (status === 'blocked') return 'error';
  return 'wait';
}

function statusLabel(status: PlanStepStatus): string {
  if (status === 'in_progress') return '进行中';
  if (status === 'done') return '已完成';
  if (status === 'blocked') return '受阻';
  return '待执行';
}

function toSteps(model: PlanBoardModel | null): UiStep[] {
  return (model?.steps ?? []).map((step) => ({
    key: step.id,
    title: step.assignedTo ? `${step.description || step.id} · ${step.assignedTo}` : (step.description || step.id),
    description: [statusLabel(step.status), step.error].filter(Boolean).join(' · '),
    status: stepStatus(step.status),
  }));
}
</script>

<template>
  <section class="ai-plan-board" data-testid="plan-board">
    <h4 class="ai-panel-title">{{ title }}</h4>
    <p v-if="model?.description" class="ai-plan-desc">{{ model.description }}</p>
    <UiSteps v-if="model?.steps.length" :steps="toSteps(model)" />
    <p v-else class="ai-panel-empty">暂无计划步骤</p>
  </section>
</template>

<style scoped>
.ai-plan-board {
  border: 1px solid var(--line);
  border-radius: var(--r-panel);
  background: var(--face-work);
  padding: 12px 14px;
}

.ai-panel-title {
  margin: 0 0 8px;
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.ai-plan-desc {
  margin: 0 0 10px;
  font-size: var(--fs-label);
  color: var(--ink-subtle);
  line-height: 1.5;
}

.ai-panel-empty {
  margin: 0;
  font-size: var(--fs-label);
  color: var(--ink-muted);
  text-align: center;
  padding: 12px 0;
}
</style>
