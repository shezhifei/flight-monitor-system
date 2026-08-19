<script setup lang="ts">
import type { PendingActionCardModel, PendingActionConstraint } from '@/lib/ai/pendingActionDiff';

const props = withDefaults(defineProps<{
  action: PendingActionCardModel;
  busy?: boolean;
}>(), {
  busy: false,
});

const emit = defineEmits<{
  approve: [actionId: string];
  reject: [actionId: string];
}>();

function constraintText(item: PendingActionConstraint): string {
  return item.message ? `${item.name}: ${item.message}` : item.name;
}

function hardViolations() {
  return Array.isArray(props.action.hardViolations) ? props.action.hardViolations : [];
}
function softViolations() {
  return Array.isArray(props.action.softViolations) ? props.action.softViolations : [];
}
function diffRows() {
  return Array.isArray(props.action.diffRows) ? props.action.diffRows : [];
}
</script>

<template>
  <div class="pa-card" data-testid="pending-action-card">
    <div class="pa-header">
      <span class="pa-tool-name">{{ action.toolName || action.actionId }}</span>
      <span v-if="action.irreversible" class="pa-tag is-danger">不可逆操作</span>
      <span v-if="hardViolations().length" class="pa-tag is-danger">硬约束违规</span>
    </div>

    <p v-if="action.message" class="pa-alert is-warn">{{ action.message }}</p>

    <p v-if="action.objectType || action.objectId" class="pa-meta">
      对象: {{ action.objectType || 'Unknown' }} / {{ action.objectId || '-' }}
    </p>

    <div v-if="hardViolations().length" class="pa-alert is-danger">
      <div class="pa-alert-title">硬约束违规</div>
      <div v-for="(item, i) in hardViolations()" :key="`h_${i}`">{{ constraintText(item) }}</div>
    </div>

    <div v-if="softViolations().length" class="pa-alert is-warn">
      <div class="pa-alert-title">软约束提示</div>
      <div v-for="(item, i) in softViolations()" :key="`s_${i}`">{{ constraintText(item) }}</div>
    </div>

    <table v-if="diffRows().length" class="pa-diff">
      <thead>
        <tr><th>字段</th><th>变更前</th><th>变更后</th></tr>
      </thead>
      <tbody>
        <tr v-for="row in diffRows()" :key="row.field">
          <td class="pa-diff-field">{{ row.field }}</td>
          <td>{{ row.before }}</td>
          <td>{{ row.after }}</td>
        </tr>
      </tbody>
    </table>

    <p class="pa-meta">状态: {{ action.status || 'pending' }}</p>
    <p v-if="action.sourceRunId || action.sourceTool" class="pa-meta">
      来源: {{ [action.sourceTool, action.sourceRunId].filter(Boolean).join(' / ') }}
    </p>
    <p v-if="action.createdAt" class="pa-meta">创建: {{ action.createdAt }}</p>
    <p v-if="action.expiresAt" class="pa-meta">过期: {{ action.expiresAt }}</p>

    <div class="pa-actions">
      <button
        type="button"
        class="pa-btn is-approve"
        :disabled="busy"
        @click="emit('approve', action.actionId)"
      >
        批准
      </button>
      <button
        type="button"
        class="pa-btn is-reject"
        :disabled="busy"
        @click="emit('reject', action.actionId)"
      >
        拒绝
      </button>
    </div>
  </div>
</template>

<style scoped>
.pa-card {
  border: 1px solid var(--line);
  border-radius: var(--r-panel);
  background: var(--face-raised);
  padding: 12px 14px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.pa-header {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.pa-tool-name {
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.pa-tag {
  font-size: var(--fs-label);
  padding: 1px 8px;
  border-radius: var(--r-cell);
}

.pa-tag.is-danger {
  color: var(--danger);
  background: var(--danger-soft);
}

.pa-alert {
  margin: 0;
  padding: 8px 10px;
  border-radius: var(--r-control);
  font-size: var(--fs-label);
  line-height: 1.5;
}

.pa-alert.is-warn {
  background: var(--warn-soft);
  color: var(--warn);
}

.pa-alert.is-danger {
  background: var(--danger-soft);
  color: var(--danger);
}

.pa-alert-title {
  font-weight: var(--fw-semibold);
  margin-bottom: 2px;
}

.pa-meta {
  margin: 0;
  font-size: var(--fs-label);
  color: var(--ink-subtle);
}

.pa-diff {
  width: 100%;
  border-collapse: collapse;
  font-size: var(--fs-label);
}

.pa-diff th {
  text-align: left;
  color: var(--ink-subtle);
  font-weight: var(--fw-medium);
  padding: 4px 8px;
  border-bottom: 1px solid var(--line-strong);
}

.pa-diff td {
  padding: 4px 8px;
  border-bottom: 1px solid var(--line);
  color: var(--ink);
  word-break: break-all;
}

.pa-diff-field {
  font-family: var(--mono);
  white-space: nowrap;
}

.pa-actions {
  display: flex;
  gap: 8px;
}

.pa-btn {
  min-height: var(--h-sm);
  padding: 0 16px;
  border-radius: var(--r-control);
  border: none;
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  cursor: pointer;
}

.pa-btn.is-approve {
  background: var(--act);
  color: var(--act-on);
}

.pa-btn.is-reject {
  background: var(--danger-soft);
  color: var(--danger);
}

.pa-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.pa-btn:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}
</style>
