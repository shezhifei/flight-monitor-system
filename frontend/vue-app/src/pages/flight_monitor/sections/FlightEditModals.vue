<script setup lang="ts">
import { ref } from 'vue';
import { BASE_COLUMNS } from '../../../components/flight-monitor/FlightList.vue';

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

/** 配置列拖拽：对齐 legacy handleConfigDrag* */
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
  <!--
    全局 `.modal { display: none }`（layout.css）会盖过裸 v-show，
    与 FlightBatchEditModal 相同：v-if + 显式 display:block。
  -->
  <div
    v-if="columnConfigIsOpen"
    id="columnConfigModal"
    class="modal"
    role="dialog"
    aria-modal="true"
    aria-labelledby="columnConfigTitle"
    aria-hidden="false"
    style="display: block;"
  >
    <div class="modal-content modal-sm">
      <div class="modal-header">
        <h2 id="columnConfigTitle">
          配置表格列
        </h2>
        <button
          id="closeColumnConfig"
          type="button"
          class="close close-modal close-modal-compact"
          aria-label="关闭列配置弹窗"
          @click="emit('close-column-config')"
        >
          &times;
        </button>
      </div>
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
      <div class="modal-footer">
        <button id="resetColumnsBtn" type="button" class="flight-text-btn" @click="emit('reset-column-config')">
          恢复默认
        </button>
        <button id="saveColumnsBtn" type="button" class="flight-text-btn" @click="emit('save-column-config')">
          保存配置
        </button>
      </div>
    </div>
  </div>

  <div
    v-if="fieldEditIsOpen"
    id="fieldEditModal"
    class="modal"
    role="dialog"
    aria-modal="true"
    aria-labelledby="editFieldTitle"
    aria-hidden="false"
    style="display: block;"
  >
    <div class="modal-content modal-sm">
      <div class="modal-header">
        <h2 id="editFieldTitle">
          修改 {{ fieldEditLabel }}
        </h2>
        <button
          type="button"
          class="close close-modal close-modal-compact"
          aria-label="关闭编辑弹窗"
          @click="emit('close-field-edit')"
        >
          &times;
        </button>
      </div>
      <form @submit.prevent="emit('save-field-edit')">
        <div class="form-group" style="margin-top: 16px;">
          <input
            v-if="fieldEditType === 'datetime-local'"
            :value="fieldEditValue"
            type="datetime-local"
            class="form-control"
            required
            style="width:100%; border:1px solid var(--border-light, #E5E5EA); padding:8px; background:var(--bg-app, #F5F5F7); color:var(--text-primary, #1D1D1F);"
            @input="emit('update:fieldEditValue', ($event.target as HTMLInputElement).value)"
          >
          <input
            v-else
            :value="fieldEditValue"
            type="text"
            class="form-control"
            required
            style="width:100%; border:1px solid var(--border-light, #E5E5EA); padding:8px; background:var(--bg-app, #F5F5F7); color:var(--text-primary, #1D1D1F);"
            @input="emit('update:fieldEditValue', ($event.target as HTMLInputElement).value)"
          >
        </div>
        <div class="modal-footer" style="padding-top: 16px;">
          <button type="button" class="flight-text-btn" @click="emit('close-field-edit')">
            取消
          </button>
          <button type="submit" class="flight-text-btn" :disabled="fieldEditSaving">
            {{ fieldEditSaving ? '保存中...' : '保存修改' }}
          </button>
        </div>
      </form>
    </div>
  </div>

  <div
    v-if="remarkEditIsOpen"
    id="remarkEditModal"
    class="modal"
    role="dialog"
    aria-modal="true"
    aria-labelledby="remarkEditTitle"
    aria-hidden="false"
    style="display: block;"
  >
    <div class="modal-content">
      <div class="modal-header">
        <h2 id="remarkEditTitle">
          修改 {{ remarkEditLabel }}
        </h2>
        <button
          type="button"
          class="close close-modal close-modal-compact"
          aria-label="关闭编辑弹窗"
          @click="emit('close-remark-edit')"
        >
          &times;
        </button>
      </div>
      <form @submit.prevent="emit('save-remark-edit')">
        <div class="form-group" style="margin-top: 16px;">
          <textarea
            id="remarkInput"
            :value="remarkEditValue"
            class="form-control"
            rows="6"
            placeholder="输入长备注内容..."
            style="width:100%; border:1px solid var(--border-light, #E5E5EA); padding:8px; background:var(--bg-app, #F5F5F7); color:var(--text-primary, #1D1D1F); resize: vertical;"
            @input="emit('update:remarkEditValue', ($event.target as HTMLTextAreaElement).value)"
          />
        </div>
        <div class="modal-footer" style="padding-top: 16px;">
          <button type="button" class="flight-text-btn" @click="emit('close-remark-edit')">
            取消
          </button>
          <button id="saveRemarkBtn" type="submit" class="flight-text-btn" :disabled="remarkEditSaving">
            {{ remarkEditSaving ? '保存中...' : '保存修改' }}
          </button>
        </div>
      </form>
    </div>
  </div>
</template>

<style scoped>
.column-config-hint {
  margin: 0;
  padding: 0 16px 8px;
  font-size: 12px;
  color: var(--text-secondary, var(--admin-text-subtle));
  line-height: 1.4;
}
</style>
