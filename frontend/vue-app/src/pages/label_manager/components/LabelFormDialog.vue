<script setup lang="ts">
import { ref, watch, computed } from 'vue';
import type { LabelDefinition, CreateLabelRequest, UpdateLabelRequest } from '../../../types/backend';
import UiButton from '@/components/ui/UiButton.vue';
import UiSelect from '@/components/ui/UiSelect.vue';

const scopeOptions = [
  { value: 'flight', label: '航班级' },
  { value: 'leg', label: '航段级' },
  { value: 'both', label: '两者' },
];

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

/* UiSelect 收 string，桥接回联合类型 */
const scopeValue = computed<string>({
  get: () => formData.value.scope,
  set: (value) => {
    formData.value.scope = value as CreateLabelRequest['scope'];
  },
});

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
            <UiSelect
              id="labelScope"
              v-model="scopeValue"
              :options="scopeOptions"
              label="适用范围"
              min-width="100%"
              :disabled="isEdit"
            />
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
          <UiButton variant="ghost" @click="emit('close')">
            取消
          </UiButton>
          <UiButton
            variant="primary"
            :disabled="props.loading || !canSubmit"
            @click="handleSubmit"
          >
            {{ props.loading ? '保存中...' : '保存' }}
          </UiButton>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
/* 信号面：弹窗用抬起面 + 幕布 scrim；层序用 --z-modal，不再发明 1000 */
.modal-overlay {
  position: fixed;
  inset: 0;
  background: var(--scrim);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: var(--z-modal);
  padding: var(--s4);
}

.modal-content {
  background: var(--face-raised);
  color: var(--ink);
  border: 1px solid var(--line);
  border-radius: var(--r-panel);
  width: 100%;
  max-width: 480px;
  max-height: 90vh;
  overflow-y: auto;
  box-shadow: var(--shadow-md);
}

.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--s4) var(--s4) var(--s3);
  border-bottom: 1px solid var(--line);
}

.modal-title {
  margin: 0;
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.modal-close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: var(--h-sm);
  height: var(--h-sm);
  background: none;
  border: none;
  border-radius: var(--r-cell);
  font-size: var(--fs-page);
  color: var(--ink-muted);
  cursor: pointer;
  padding: 0;
  line-height: 1;
  transition: background var(--t-fast) var(--ease), color var(--t-fast) var(--ease);
}

.modal-close:hover {
  color: var(--ink);
  background: color-mix(in srgb, var(--ink) 8%, transparent);
}

.modal-body {
  padding: var(--s4);
}

.form-group {
  margin-bottom: var(--s3);
}

.form-group:last-child {
  margin-bottom: 0;
}

.form-label {
  display: block;
  margin-bottom: var(--s2);
  font-size: var(--fs-body);
  font-weight: var(--fw-medium);
  color: var(--ink);
}

.form-input {
  width: 100%;
  height: var(--h-md);
  padding: 0 var(--s3);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-control);
  font-size: var(--fs-body);
  font-family: inherit;
  color: var(--ink);
  background: var(--face-page);
  transition: border-color var(--t-fast) var(--ease);
  box-sizing: border-box;
}

.form-input:focus {
  outline: 2px solid var(--act);
  outline-offset: 2px;
  border-color: var(--act);
}

.form-input:disabled {
  color: var(--ink-muted);
  cursor: not-allowed;
}

.form-hint {
  display: block;
  margin-top: var(--s1);
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

.color-picker-row {
  display: flex;
  gap: var(--s3);
  align-items: center;
}

.color-picker {
  width: 48px;
  height: var(--h-md);
  padding: 2px;
  border: 1px solid var(--line-strong);
  border-radius: var(--r-control);
  cursor: pointer;
  background: var(--face-page);
}

.color-text {
  flex: 1;
  font-family: var(--mono);
}

.color-preview {
  width: var(--h-sm);
  height: var(--h-sm);
  border-radius: var(--r-cell);
  border: 1px solid var(--line);
  flex-shrink: 0;
}

.form-checkbox {
  display: flex;
  align-items: center;
  gap: var(--s2);
  font-size: var(--fs-body);
  color: var(--ink);
  cursor: pointer;
}

.form-checkbox input {
  width: 16px;
  height: 16px;
  cursor: pointer;
  accent-color: var(--act);
}

.modal-footer {
  display: flex;
  justify-content: flex-end;
  gap: var(--s3);
  padding: var(--s3) var(--s4);
  border-top: 1px solid var(--line);
  background: var(--face-work);
}
</style>
