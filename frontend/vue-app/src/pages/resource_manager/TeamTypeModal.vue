<script setup lang="ts">
import { computed } from 'vue';
import type { TeamType, TeamTypeFormData } from '@/composables/useResourceManager';

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
  <Teleport to="body">
    <div v-if="show" class="modal-overlay" @click.self="emit('close')">
      <div
        class="modal-content"
        role="dialog"
        aria-modal="true"
        aria-labelledby="team-type-modal-title"
      >
        <header class="modal-header">
          <h3 id="team-type-modal-title">
            {{ editing ? '编辑班组类型' : '新建班组类型' }}
          </h3>
          <button
            class="modal-close"
            type="button"
            aria-label="关闭"
            @click="emit('close')"
          >
            ×
          </button>
        </header>
        <div class="modal-body">
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
          <div class="form-group form-row">
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
        </div>
        <footer class="modal-footer">
          <button type="button" class="btn btn-secondary" @click="emit('close')">
            取消
          </button>
          <button
            type="button"
            class="btn btn-primary"
            :disabled="!canSave"
            @click="emit('save')"
          >
            {{ saving ? '保存中...' : '保存' }}
          </button>
        </footer>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(15, 23, 42, 0.4);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2100;
}
.modal-content {
  width: 480px;
  max-width: 95vw;
  background: var(--bg-card, #fff);
  border-radius: 12px;
  display: flex;
  flex-direction: column;
  box-shadow: 0 20px 40px rgba(15, 23, 42, 0.18);
}
.modal-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 20px;
  border-bottom: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
}
.modal-header h3 {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
}
.modal-close {
  background: none;
  border: none;
  font-size: 24px;
  line-height: 1;
  cursor: pointer;
  color: var(--text-tertiary);
}
.modal-body {
  padding: 20px;
  max-height: 60vh;
  overflow-y: auto;
}
.form-group {
  margin-bottom: 16px;
}
.form-group label {
  display: block;
  font-size: 13px;
  font-weight: 500;
  margin-bottom: 6px;
}
.required {
  color: var(--system-red);
}
.checkbox-label {
  display: flex;
  align-items: center;
  gap: 6px;
  cursor: pointer;
  font-size: 13px;
}
.form-group input,
.form-group textarea {
  width: 100%;
  padding: 8px 12px;
  border: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
  border-radius: 6px;
  font-size: 14px;
  box-sizing: border-box;
}
.form-group textarea {
  min-height: 72px;
  resize: vertical;
}
.form-hint {
  margin: 6px 0 0;
  font-size: 12px;
  color: var(--text-tertiary);
}
.modal-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 14px 20px;
  border-top: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
}
.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 8px 16px;
  font-size: 13px;
  font-weight: 500;
  border-radius: 8px;
  cursor: pointer;
  border: none;
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.btn-primary {
  background: var(--system-blue);
  color: var(--text-inverse);
}
.btn-secondary {
  background: rgba(60, 60, 67, 0.08);
  color: var(--text-primary);
  border: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
}
</style>
