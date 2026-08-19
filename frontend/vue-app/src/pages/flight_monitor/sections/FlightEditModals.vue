<script setup lang="ts">
import { ref } from 'vue';
import { BASE_COLUMNS } from '../../../components/flight-monitor/FlightList.vue';
import UiModal from '../../../components/ui/UiModal.vue';
import UiButton from '../../../components/ui/UiButton.vue';

const COLUMN_LABELS: Readonly<Record<string, string>> = Object.fromEntries(
  BASE_COLUMNS.map((column) => [column.key, column.label]),
);

function columnLabel(key: string): string {
  return COLUMN_LABELS[key] ?? key;
}

defineProps<{
  columnConfigIsOpen: boolean;
  columnConfigItems: string[];
  columnConfigVisibleColumns: Record<string, boolean>;
  fieldEditIsOpen: boolean;
  fieldEditLabel: string;
  fieldEditType: string;
  fieldEditValue: string;
  fieldEditSaving: boolean;
  remarkEditIsOpen: boolean;
  remarkEditLabel: string;
  remarkEditValue: string;
  remarkEditSaving: boolean;
}>();

const emit = defineEmits<{
  (e: 'update:columnConfigVisibleColumns', value: Record<string, boolean>): void;
  (e: 'reorder-column-config', fromKey: string, toKey: string): void;
  (e: 'save-column-config'): void;
  (e: 'close-column-config'): void;
  (e: 'reset-column-config'): void;
  (e: 'update:fieldEditValue', value: string): void;
  (e: 'save-field-edit'): void;
  (e: 'close-field-edit'): void;
  (e: 'update:remarkEditValue', value: string): void;
  (e: 'save-remark-edit'): void;
  (e: 'close-remark-edit'): void;
}>();

const dragSrcKey = ref<string | null>(null);
const dragOverKey = ref<string | null>(null);

function onConfigDragStart(event: DragEvent, key: string): void {
  dragSrcKey.value = key;
  dragOverKey.value = null;
  if (event.dataTransfer) {
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData('text/plain', key);
  }
}

function onConfigDragOver(event: DragEvent, key: string): void {
  event.preventDefault();
  if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';
  if (dragSrcKey.value && dragSrcKey.value !== key) {
    dragOverKey.value = key;
  }
}

function onConfigDragLeave(key: string): void {
  if (dragOverKey.value === key) dragOverKey.value = null;
}

function onConfigDrop(event: DragEvent, targetKey: string): void {
  event.preventDefault();
  event.stopPropagation();
  const fromKey = dragSrcKey.value || event.dataTransfer?.getData('text/plain') || null;
  if (fromKey && fromKey !== targetKey) {
    emit('reorder-column-config', fromKey, targetKey);
  }
  dragSrcKey.value = null;
  dragOverKey.value = null;
}

function onConfigDragEnd(): void {
  dragSrcKey.value = null;
  dragOverKey.value = null;
}
</script>

<template>
  <UiModal
    :open="columnConfigIsOpen"
    title="表格列"
    :width="560"
    id="columnConfigModal"
    @close="emit('close-column-config')"
  >
    <p class="column-config-hint">
      勾选显示列；拖拽行可调整列顺序。
    </p>
    <div id="columnConfigList" class="column-config-list" role="list" aria-label="表格列配置">
      <div
        v-for="key in columnConfigItems"
        :key="key"
        class="column-config-item"
        :class="{
          dragging: dragSrcKey === key,
          'drag-over': dragOverKey === key && dragSrcKey !== key,
        }"
        :data-column-id="key"
        draggable="true"
        role="listitem"
        @dragstart="onConfigDragStart($event, key)"
        @dragover="onConfigDragOver($event, key)"
        @dragleave="onConfigDragLeave(key)"
        @drop="onConfigDrop($event, key)"
        @dragend="onConfigDragEnd"
      >
        <span class="column-handle" aria-hidden="true" title="拖拽排序">⋮⋮</span>
        <input
          :checked="columnConfigVisibleColumns[key]"
          class="column-checkbox"
          type="checkbox"
          :aria-label="`显示${columnLabel(key)}`"
          @click.stop
          @change="emit('update:columnConfigVisibleColumns', { ...columnConfigVisibleColumns, [key]: ($event.target as HTMLInputElement).checked })"
        >
        <span class="column-label">{{ columnLabel(key) }}</span>
      </div>
    </div>
    <template #footer>
      <UiButton id="resetColumnsBtn" variant="ghost" @click="emit('reset-column-config')">恢复默认</UiButton>
      <UiButton id="saveColumnsBtn" variant="primary" @click="emit('save-column-config')">保存配置</UiButton>
    </template>
  </UiModal>

  <UiModal
    :open="fieldEditIsOpen"
    :title="fieldEditLabel"
    :width="560"
    id="fieldEditModal"
    @close="emit('close-field-edit')"
  >
    <form id="fieldEditForm" @submit.prevent="emit('save-field-edit')">
      <div class="form-group">
        <input
          v-if="fieldEditType === 'datetime-local'"
          :value="fieldEditValue"
          type="datetime-local"
          class="form-control field-input"
          required
          @input="emit('update:fieldEditValue', ($event.target as HTMLInputElement).value)"
        >
        <input
          v-else
          :value="fieldEditValue"
          type="text"
          class="form-control field-input"
          required
          @input="emit('update:fieldEditValue', ($event.target as HTMLInputElement).value)"
        >
      </div>
    </form>
    <template #footer>
      <UiButton variant="ghost" @click="emit('close-field-edit')">取消</UiButton>
      <UiButton variant="primary" :disabled="fieldEditSaving" @click="emit('save-field-edit')">
        {{ fieldEditSaving ? '保存中...' : '保存修改' }}
      </UiButton>
    </template>
  </UiModal>

  <UiModal
    :open="remarkEditIsOpen"
    :title="remarkEditLabel"
    :width="640"
    id="remarkEditModal"
    @close="emit('close-remark-edit')"
  >
    <form id="remarkEditForm" @submit.prevent="emit('save-remark-edit')">
      <div class="form-group">
        <textarea
          id="remarkInput"
          :value="remarkEditValue"
          class="form-control field-input"
          rows="6"
          placeholder="输入长备注内容..."
          @input="emit('update:remarkEditValue', ($event.target as HTMLTextAreaElement).value)"
        />
      </div>
    </form>
    <template #footer>
      <UiButton variant="ghost" @click="emit('close-remark-edit')">取消</UiButton>
      <UiButton id="saveRemarkBtn" variant="primary" :disabled="remarkEditSaving" @click="emit('save-remark-edit')">
        {{ remarkEditSaving ? '保存中...' : '保存修改' }}
      </UiButton>
    </template>
  </UiModal>
</template>

<style scoped>
.column-config-hint {
  margin: 0 0 8px;
  font-size: var(--fs-label);
  color: var(--ink-subtle);
  line-height: 1.4;
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
