<script setup lang="ts">
import UiModal from '../../../components/ui/UiModal.vue';
import UiButton from '../../../components/ui/UiButton.vue';
import type { BatchValueType } from '../flightBatchEditableFields';

defineProps<{
  isOpen: boolean;
  label: string;
  valueType: BatchValueType;
  value: string;
  flightCount: number;
  maxLength?: number | null;
  saving: boolean;
  canSubmit: boolean;
  error: string | null;
}>();

const emit = defineEmits<{
  (e: 'update:value', value: string): void;
  (e: 'submit'): void;
  (e: 'close'): void;
}>();

function onInput(event: Event): void {
  emit('update:value', (event.target as HTMLInputElement | HTMLTextAreaElement).value);
}
</script>

<template>
  <UiModal
    :open="isOpen"
    title="批量编辑"
    :width="560"
    id="flightBatchEditModal"
    @close="emit('close')"
  >
    <form id="flightBatchEditForm" @submit.prevent="emit('submit')">
      <p class="batch-edit-hint">
        将把同一值写入已选中的 <strong>{{ flightCount }}</strong> 个「{{ label }}」单元格（同一列）。
      </p>
      <div class="form-group">
        <input
          v-if="valueType === 'datetime'"
          :value="value"
          type="datetime-local"
          class="form-control field-input"
          required
          @input="onInput"
        >
        <textarea
          v-else-if="label.includes('备注')"
          :value="value"
          class="form-control field-input"
          rows="4"
          :maxlength="maxLength ?? undefined"
          placeholder="输入批量备注内容..."
          @input="onInput"
        />
        <input
          v-else
          :value="value"
          type="text"
          class="form-control field-input"
          :maxlength="maxLength ?? undefined"
          @input="onInput"
        >
      </div>
      <p v-if="error" class="batch-edit-error" role="alert">
        {{ error }}
      </p>
    </form>
    <template #footer>
      <UiButton variant="ghost" :disabled="saving" @click="emit('close')">取消</UiButton>
      <UiButton variant="primary" native-type="submit" form="flightBatchEditForm" :disabled="!canSubmit || saving">
        {{ saving ? '提交中...' : `应用到 ${flightCount} 项` }}
      </UiButton>
    </template>
  </UiModal>
</template>

<style scoped>
.batch-edit-hint {
  margin: 0 0 12px;
  font-size: var(--fs-body);
  color: var(--ink-subtle);
  line-height: 1.4;
}

.batch-edit-error {
  margin: 10px 0 0;
  font-size: var(--fs-label);
  color: var(--danger);
}

.field-input {
  width: 100%;
  border: 1px solid var(--line-strong);
  border-radius: var(--r-control);
  padding: 8px 10px;
  background: var(--face-work);
  color: var(--ink);
  box-sizing: border-box;
  resize: vertical;
}

.field-input:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}
</style>
