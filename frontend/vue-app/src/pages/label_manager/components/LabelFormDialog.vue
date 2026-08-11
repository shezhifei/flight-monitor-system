<script setup lang="ts">
import { ref, watch, computed } from 'vue';
import type { LabelDefinition, CreateLabelRequest, UpdateLabelRequest } from '../../../types/backend';

interface Props {
  visible: boolean;
  label?: LabelDefinition | null;
  loading?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  label: null,
  loading: false,
});

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'save', data: CreateLabelRequest | UpdateLabelRequest): void;
}>();

const isEdit = computed(() => !!props.label);
const canSubmit = computed(() => Boolean(formData.value.code.trim() && formData.value.name.trim()));

const formData = ref<CreateLabelRequest>({
  code: '',
  name: '',
  color: '#6B7280',
  icon: null,
  scope: 'flight',
});

watch(
  () => props.visible,
  (visible) => {
    if (visible) {
      if (props.label) {
        formData.value = {
          code: props.label.code,
          name: props.label.name,
          color: props.label.color,
          icon: props.label.icon || null,
          scope: props.label.scope,
        };
      } else {
        formData.value = {
          code: '',
          name: '',
          color: '#6B7280',
          icon: null,
          scope: 'flight',
        };
      }
    }
  }
);

function handleColorInput(e: Event) {
  const target = e.target as HTMLInputElement;
  formData.value.color = target.value;
}

function handleColorTextInput(e: Event) {
  const target = e.target as HTMLInputElement;
  const value = target.value.trim();
  if (/^#[0-9A-Fa-f]{6}$/.test(value)) {
    formData.value.color = value;
  }
}

function handleSubmit() {
  if (!canSubmit.value) {
    return;
  }
  emit('save', { ...formData.value });
}

function handleActiveToggle(e: Event) {
  if (!canSubmit.value) {
    return;
  }
  emit('save', { ...formData.value, is_active: (e.target as HTMLInputElement).checked });
}

function handleOverlayClick(e: MouseEvent) {
  if ((e.target as HTMLElement).classList.contains('modal-overlay')) {
    emit('close');
  }
}
</script>

<template>
  <Teleport to="body">
    <div
      v-if="props.visible"
      class="modal-overlay"
      @click="handleOverlayClick"
    >
      <div class="modal-content">
        <div class="modal-header">
          <h2 class="modal-title">
            {{ isEdit ? '编辑标签' : '添加标签' }}
          </h2>
          <button class="modal-close" @click="emit('close')">
            &times;
          </button>
        </div>

        <form class="modal-body" @submit.prevent="handleSubmit">
          <div class="form-group">
            <label class="form-label" for="labelCode">标签代码</label>
            <input
              id="labelCode"
              v-model="formData.code"
              type="text"
              class="form-input"
              placeholder="如: vip"
              :disabled="isEdit"
              required
            >
            <small class="form-hint">英文、数字、下划线，3-20字符</small>
          </div>

          <div class="form-group">
            <label class="form-label" for="labelName">显示名称</label>
            <input
              id="labelName"
              v-model="formData.name"
              type="text"
              class="form-input"
              placeholder="如: VIP"
              required
            >
          </div>

          <div class="form-group">
            <label class="form-label">颜色</label>
            <div class="color-picker-row">
              <input
                type="color"
                class="color-picker"
                :value="formData.color"
                @input="handleColorInput"
              >
              <input
                type="text"
                class="form-input color-text"
                :value="formData.color"
                @input="handleColorTextInput"
              >
              <span
                class="color-preview"
                :style="{ background: formData.color }"
              />
            </div>
          </div>

          <div class="form-group">
            <label class="form-label" for="labelIcon">图标 (可选)</label>
            <input
              id="labelIcon"
              v-model="formData.icon"
              type="text"
              class="form-input"
              placeholder="emoji 或图标名称"
            >
          </div>

          <div class="form-group">
            <label class="form-label" for="labelScope">适用范围</label>
            <select
              id="labelScope"
              v-model="formData.scope"
              class="form-input"
              :disabled="isEdit"
            >
              <option value="flight">
                航班级
              </option>
              <option value="leg">
                航段级
              </option>
              <option value="both">
                两者
              </option>
            </select>
          </div>

          <div v-if="isEdit" class="form-group">
            <label class="form-checkbox">
              <input
                type="checkbox"
                :checked="(props.label as LabelDefinition)?.is_active"
                :disabled="props.loading || !canSubmit"
                @change="handleActiveToggle"
              >
              启用标签
            </label>
          </div>
        </form>

        <div class="modal-footer">
          <button
            type="button"
            class="btn btn-secondary"
            @click="emit('close')"
          >
            取消
          </button>
          <button
            type="button"
            class="btn btn-primary"
            :disabled="props.loading || !canSubmit"
            @click="handleSubmit"
          >
            {{ props.loading ? '保存中...' : '保存' }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  padding: 20px;
}

.modal-content {
  background: var(--admin-card-bg, var(--bg-card, var(--ws-surface-strong)));
  color: var(--admin-text, var(--text-primary));
  border: 1px solid var(--admin-border, var(--border-light));
  border-radius: 12px;
  width: 100%;
  max-width: 480px;
  max-height: 90vh;
  overflow-y: auto;
  box-shadow: var(--ws-shadow-md, 0 20px 40px rgba(0, 0, 0, 0.28));
}

.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 18px 22px;
  border-bottom: 1px solid var(--admin-border, var(--border-light));
}

.modal-title {
  margin: 0;
  font-size: 17px;
  font-weight: 600;
  color: var(--admin-text, var(--text-primary));
}

.modal-close {
  background: none;
  border: none;
  font-size: 24px;
  color: var(--admin-text-subtle, var(--text-secondary));
  cursor: pointer;
  padding: 0;
  line-height: 1;
}

.modal-close:hover {
  color: var(--admin-text, var(--text-primary));
}

.modal-body {
  padding: 22px;
}

.form-group {
  margin-bottom: 18px;
}

.form-group:last-child {
  margin-bottom: 0;
}

.form-label {
  display: block;
  margin-bottom: 8px;
  font-size: 13px;
  font-weight: 500;
  color: var(--admin-text, var(--text-primary));
}

.form-input {
  width: 100%;
  padding: 10px 14px;
  border: 1px solid var(--admin-border, var(--border-light));
  border-radius: 8px;
  font-size: 14px;
  color: var(--admin-text, var(--text-primary));
  background: var(--ws-surface-muted, var(--bg-secondary));
  transition: border-color 0.2s, box-shadow 0.2s;
  box-sizing: border-box;
}

.form-input:focus {
  outline: none;
  border-color: var(--ws-primary, var(--primary-color, #3b82f6));
  box-shadow: 0 0 0 3px var(--focus-ring-blue, rgba(10, 124, 255, 0.2));
}

.form-input:disabled {
  opacity: 0.65;
  cursor: not-allowed;
}

.form-hint {
  display: block;
  margin-top: 4px;
  font-size: 12px;
  color: var(--admin-text-muted, var(--text-secondary));
}

.color-picker-row {
  display: flex;
  gap: 12px;
  align-items: center;
}

.color-picker {
  width: 48px;
  height: 40px;
  padding: 2px;
  border: 1px solid var(--admin-border, var(--border-light));
  border-radius: 6px;
  cursor: pointer;
  background: transparent;
}

.color-text {
  flex: 1;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
}

.color-preview {
  width: 32px;
  height: 32px;
  border-radius: 6px;
  border: 1px solid var(--admin-border, var(--border-light));
  flex-shrink: 0;
}

.form-checkbox {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 14px;
  color: var(--admin-text, var(--text-primary));
  cursor: pointer;
}

.form-checkbox input {
  width: 16px;
  height: 16px;
  cursor: pointer;
}

.modal-footer {
  display: flex;
  justify-content: flex-end;
  gap: 12px;
  padding: 14px 22px;
  border-top: 1px solid var(--admin-border, var(--border-light));
  background: var(--ws-surface-muted, var(--bg-secondary));
}

.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 9px 18px;
  border-radius: 8px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  border: none;
  transition: background 0.15s ease, opacity 0.15s ease;
}

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-secondary {
  background: var(--ws-surface-muted, var(--bg-secondary));
  color: var(--admin-text, var(--text-primary));
  border: 1px solid var(--admin-border, transparent);
}

.btn-secondary:hover:not(:disabled) {
  background: color-mix(in srgb, var(--ws-primary, #0a7cff) 10%, transparent);
}

.btn-primary {
  background: var(--ws-primary, var(--primary-color, #3b82f6));
  color: var(--text-inverse, #fff);
}

.btn-primary:hover:not(:disabled) {
  background: var(--ws-primary-strong, var(--primary-hover, #2563eb));
}
</style>
