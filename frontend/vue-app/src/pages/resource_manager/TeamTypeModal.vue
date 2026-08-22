<script setup lang="ts">
import { computed } from 'vue';
import type { TeamType, TeamTypeFormData } from '@/composables/useResourceManager';
import UiButton from '@/components/ui/UiButton.vue';
import UiModal from '@/components/ui/UiModal.vue';

const props = defineProps<{
  show: boolean;
  editing: TeamType | null;
  form: TeamTypeFormData;
  saving: boolean;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'save'): void;
  (e: 'update:form', value: TeamTypeFormData): void;
}>();

const canSave = computed(() => Boolean(props.form.name.trim()) && !props.saving);

function patch<K extends keyof TeamTypeFormData>(field: K, value: TeamTypeFormData[K]) {
  emit('update:form', { ...props.form, [field]: value });
}
</script>

<template>
  <UiModal
    :open="show"
    :title="editing ? '编辑班组类型' : '新建班组类型'"
    :width="480"
    @close="emit('close')"
  >
    <div class="form-group">
      <label for="tt-name">名称 <span class="required">*</span></label>
      <input
        id="tt-name"
        type="text"
        :value="form.name"
        placeholder="例如：机务班组"
        @input="patch('name', ($event.target as HTMLInputElement).value)"
      >
    </div>
    <div class="form-group">
      <label for="tt-code">代码</label>
      <input
        id="tt-code"
        type="text"
        :value="form.code"
        placeholder="例如：MX"
        @input="patch('code', ($event.target as HTMLInputElement).value)"
      >
    </div>
    <div class="form-group">
      <label for="tt-color">颜色</label>
      <input
        id="tt-color"
        type="text"
        :value="form.color"
        placeholder="例如：#1677ff"
        @input="patch('color', ($event.target as HTMLInputElement).value)"
      >
    </div>
    <div class="form-group">
      <label class="checkbox-label">
        <input
          type="checkbox"
          :checked="form.is_driver_type"
          @change="patch('is_driver_type', ($event.target as HTMLInputElement).checked)"
        >
        司机班组类型
      </label>
    </div>
    <div class="form-group">
      <label for="tt-tasks">可作业类型</label>
      <input
        id="tt-tasks"
        type="text"
        :value="form.task_types"
        placeholder="使用逗号分隔，例如：boarding, cleaning"
        @input="patch('task_types', ($event.target as HTMLInputElement).value)"
      >
      <p class="form-hint">
        用逗号或空格分隔多个作业类型代码。
      </p>
    </div>
    <div class="form-group">
      <label for="tt-desc">描述</label>
      <textarea
        id="tt-desc"
        :value="form.description"
        placeholder="可选，描述班组职责"
        @input="patch('description', ($event.target as HTMLTextAreaElement).value)"
      />
    </div>

    <template #footer>
      <UiButton @click="emit('close')">
        取消
      </UiButton>
      <UiButton
        variant="primary"
        :disabled="!canSave"
        @click="emit('save')"
      >
        {{ saving ? '保存中...' : '保存' }}
      </UiButton>
    </template>
  </UiModal>
</template>

<style scoped>
/* 信号面：帽、幕、Esc、关归 UiModal；按钮归 UiButton；这里只留表单。 */
.form-group {
  margin-bottom: var(--s3);
}

.form-group > label {
  display: block;
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  margin-bottom: var(--s1);
  color: var(--ink-subtle);
}

.required {
  color: var(--danger);
}

.checkbox-label {
  display: flex;
  align-items: center;
  gap: var(--s2);
  cursor: pointer;
  font-size: var(--fs-body);
  color: var(--ink);
}

.checkbox-label input {
  accent-color: var(--act);
}

.form-group input[type="text"],
.form-group textarea {
  width: 100%;
  min-height: var(--h-md);
  padding: var(--s1) var(--s3);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-control);
  font-size: var(--fs-body);
  background: var(--face-page);
  color: var(--ink);
  box-sizing: border-box;
  font-family: inherit;
}

.form-group input[type="text"] {
  padding: 0 var(--s3);
  height: var(--h-md);
}

.form-group textarea {
  min-height: 72px; /* 三行文本的器高，不入 --h 梯 */
  resize: vertical;
}

.form-group input:focus-visible,
.form-group textarea:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}

.form-hint {
  margin: var(--s1) 0 0;
  font-size: var(--fs-label);
  color: var(--ink-muted);
}
</style>
