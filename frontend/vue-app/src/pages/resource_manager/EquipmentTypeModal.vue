<script setup lang="ts">
import { computed } from 'vue';
import type { EquipmentType, EquipmentTypeFormData } from '@/composables/useResourceManager';

const props = defineProps<{
  show: boolean;
  editing: EquipmentType | null;
  form: EquipmentTypeFormData;
  saving: boolean;
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
  <Teleport to="body">
    <div v-if="show" class="modal-overlay" @click.self="emit('close')">
      <div
        class="modal-content"
        role="dialog"
        aria-modal="true"
        aria-labelledby="equip-type-modal-title"
      >
        <header class="modal-header">
          <h3 id="equip-type-modal-title">
            {{ editing ? '编辑设备类型' : '新建设备类型' }}
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
          <div class="form-group form-row">
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
            <label for="et-driver-team">司机班组类型 ID</label>
            <input
              id="et-driver-team"
              type="text"
              :value="form.driver_team_type_id"
              placeholder="可选，driver_team_type_id"
              @input="patch('driver_team_type_id', ($event.target as HTMLInputElement).value)"
            >
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
.form-group input[type="text"],
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
