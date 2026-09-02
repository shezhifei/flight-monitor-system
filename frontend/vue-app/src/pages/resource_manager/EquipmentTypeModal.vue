<script setup lang="ts">
import { computed } from 'vue';
import type { EquipmentType, EquipmentTypeFormData } from '@/composables/useResourceManager';
import UiButton from '@/components/ui/UiButton.vue';
import UiModal from '@/components/ui/UiModal.vue';
import FieldOverlayForm from '@/components/FieldOverlayForm.vue';
import type { FieldOverlay, FieldReferenceEntry } from '@/composables/useFieldOverlays';

const props = defineProps<{
  show: boolean;
  editing: EquipmentType | null;
  form: EquipmentTypeFormData;
  saving: boolean;
  fieldOverlays?: FieldOverlay[];
  fieldCatalogEntries?: Record<string, Array<{ code: string; name: string }>>;
  fieldReferenceEntries?: Record<string, FieldReferenceEntry[]>;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'save'): void;
  (e: 'update:form', value: EquipmentTypeFormData): void;
}>();

const canSave = computed(() => Boolean(props.form.name.trim()) && !props.saving);

function patch<K extends keyof EquipmentTypeFormData>(field: K, value: EquipmentTypeFormData[K]) {
  emit('update:form', { ...props.form, [field]: value });
}
</script>

<template>
  <UiModal
    :open="show"
    :title="editing ? '编辑设备类型' : '新建设备类型'"
    :width="480"
    @close="emit('close')"
  >
    <div class="form-group">
      <label for="et-name">名称 <span class="required">*</span></label>
      <input
        id="et-name"
        type="text"
        :value="form.name"
        placeholder="例如：牵引车"
        @input="patch('name', ($event.target as HTMLInputElement).value)"
      >
    </div>
    <div class="form-group">
      <label for="et-code">代码</label>
      <input
        id="et-code"
        type="text"
        :value="form.code"
        placeholder="例如：TUG"
        @input="patch('code', ($event.target as HTMLInputElement).value)"
      >
    </div>
    <div class="form-group">
      <label for="et-category">分类</label>
      <input
        id="et-category"
        type="text"
        :value="form.category"
        placeholder="例如：vehicle"
        @input="patch('category', ($event.target as HTMLInputElement).value)"
      >
    </div>
    <div class="form-group">
      <label class="checkbox-label">
        <input
          type="checkbox"
          :checked="form.requires_driver"
          @change="patch('requires_driver', ($event.target as HTMLInputElement).checked)"
        >
        需要司机
      </label>
    </div>
    <div class="form-group">
      <label for="et-icon">图标</label>
      <input
        id="et-icon"
        type="text"
        :value="form.icon"
        placeholder="例如：tractor"
        @input="patch('icon', ($event.target as HTMLInputElement).value)"
      >
    </div>
    <div class="form-group">
      <label for="et-desc">描述</label>
      <textarea
        id="et-desc"
        :value="form.description"
        placeholder="可选，描述设备用途"
        @input="patch('description', ($event.target as HTMLTextAreaElement).value)"
      />
    </div>
    <FieldOverlayForm
      :model-value="form.attributes ?? {}"
      :overlays="fieldOverlays ?? []"
      :catalog-entries="fieldCatalogEntries ?? {}"
      :reference-entries="fieldReferenceEntries ?? {}"
      @update:model-value="patch('attributes', $event)"
    />

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
</style>
