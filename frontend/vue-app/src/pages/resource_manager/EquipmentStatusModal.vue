<script setup lang="ts">
import { computed } from 'vue';
import type { Equipment, EquipmentStatusFormData } from '@/composables/useResourceManager';

const props = defineProps<{
  show: boolean;
  equipment: Equipment | null;
  form: EquipmentStatusFormData;
  saving: boolean;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'save'): void;
  (e: 'update:form', value: EquipmentStatusFormData): void;
}>();

const canSave = computed(() => Boolean(props.form.status) && !props.saving);

function patch<K extends keyof EquipmentStatusFormData>(field: K, value: EquipmentStatusFormData[K]) {
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
        aria-labelledby="equip-status-modal-title"
      >
        <header class="modal-header">
          <div>
            <div class="modal-eyebrow">
              设备状态
            </div>
            <h3 id="equip-status-modal-title">
              {{ equipment?.name || '更新设备状态' }}
            </h3>
          </div>
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
            <label for="es-status">状态 <span class="required">*</span></label>
            <select
              id="es-status"
              :value="form.status"
              @change="patch('status', ($event.target as HTMLSelectElement).value)"
            >
              <option value="available">
                可用
              </option>
              <option value="in_use">
                使用中
              </option>
              <option value="maintenance">
                维护中
              </option>
              <option value="retired">
                已报废
              </option>
            </select>
          </div>
          <div class="form-group">
            <label for="es-terminal">航站楼 / 位置</label>
            <input
              id="es-terminal"
              type="text"
              :value="form.terminal"
              placeholder="例如：T1"
              @input="patch('terminal', ($event.target as HTMLInputElement).value)"
            >
          </div>
          <div class="form-group">
            <label for="es-next">下次保养日期</label>
            <input
              id="es-next"
              type="date"
              :value="form.next_maintenance_date"
              @input="patch('next_maintenance_date', ($event.target as HTMLInputElement).value)"
            >
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
  align-items: flex-start;
  padding: 16px 20px;
  border-bottom: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
}
.modal-eyebrow {
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text-tertiary);
}
.modal-header h3 {
  margin: 4px 0 0;
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
.form-group input,
.form-group textarea,
.form-group select {
  width: 100%;
  padding: 8px 12px;
  border: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
  border-radius: 6px;
  font-size: 14px;
  box-sizing: border-box;
}
.form-group textarea {
  min-height: 60px;
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
