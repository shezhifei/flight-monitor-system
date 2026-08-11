<script setup lang="ts">
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
  <!--
    Global `.modal { display: none }` in layout.css wins over bare v-show.
    Force display:block when open so the dialog is actually visible.
  -->
  <div
    v-if="isOpen"
    id="flightBatchEditModal"
    class="modal"
    role="dialog"
    aria-modal="true"
    aria-labelledby="flightBatchEditTitle"
    aria-hidden="false"
    style="display: block;"
  >
    <div class="modal-content modal-sm">
      <div class="modal-header">
        <h2 id="flightBatchEditTitle">
          批量修改 {{ label }}
        </h2>
        <button
          type="button"
          class="close close-modal close-modal-compact"
          aria-label="关闭批量编辑弹窗"
          @click="emit('close')"
        >
          &times;
        </button>
      </div>
      <form @submit.prevent="emit('submit')">
        <p class="batch-edit-hint">
          将把同一值写入已选中的 <strong>{{ flightCount }}</strong> 个单元格（同一列）。
        </p>
        <div class="form-group" style="margin-top: 12px;">
          <input
            v-if="valueType === 'datetime'"
            :value="value"
            type="datetime-local"
            class="form-control"
            required
            style="width:100%; border:1px solid var(--border-light, #E5E5EA); padding:8px; background:var(--bg-app, #F5F5F7); color:var(--text-primary, #1D1D1F);"
            @input="onInput"
          >
          <textarea
            v-else-if="label.includes('备注')"
            :value="value"
            class="form-control"
            rows="4"
            :maxlength="maxLength ?? undefined"
            placeholder="输入批量备注内容..."
            style="width:100%; border:1px solid var(--border-light, #E5E5EA); padding:8px; background:var(--bg-app, #F5F5F7); color:var(--text-primary, #1D1D1F); resize: vertical;"
            @input="onInput"
          />
          <input
            v-else
            :value="value"
            type="text"
            class="form-control"
            :maxlength="maxLength ?? undefined"
            style="width:100%; border:1px solid var(--border-light, #E5E5EA); padding:8px; background:var(--bg-app, #F5F5F7); color:var(--text-primary, #1D1D1F);"
            @input="onInput"
          >
        </div>
        <p v-if="error" class="batch-edit-error" role="alert">
          {{ error }}
        </p>
        <div class="modal-footer" style="padding-top: 16px;">
          <button type="button" class="flight-text-btn" :disabled="saving" @click="emit('close')">
            取消
          </button>
          <button type="submit" class="flight-text-btn" :disabled="!canSubmit || saving">
            {{ saving ? '提交中...' : `应用到 ${flightCount} 项` }}
          </button>
        </div>
      </form>
    </div>
  </div>
</template>

<style scoped>
.batch-edit-hint {
  margin: 12px 0 0;
  font-size: 13px;
  color: var(--text-secondary, #6e6e73);
  line-height: 1.4;
}

.batch-edit-error {
  margin: 10px 0 0;
  font-size: 12px;
  color: var(--system-red, #FF3B30);
}
</style>
