<script setup lang="ts">
import { computed } from 'vue';
import type { Equipment, EquipmentStatusFormData } from '@/composables/useResourceManager';
import UiButton from '@/components/ui/UiButton.vue';
import UiModal from '@/components/ui/UiModal.vue';
import UiSelect from '@/components/ui/UiSelect.vue';

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

const statusOptions = [
  { value: 'available', label: '可用' },
  { value: 'in_use', label: '使用中' },
  { value: 'maintenance', label: '维护中' },
  { value: 'retired', label: '已报废' },
];

/* UiSelect 收 string，桥回受控表单 */
const statusModel = computed<string>({
  get: () => props.form.status,
  set: (value) => patch('status', value as EquipmentStatusFormData['status']),
});
</script>

<template>
  <UiModal
    :open="show"
    :title="equipment?.name || '更新设备状态'"
    :width="480"
    @close="emit('close')"
  >
    <template #header>
      <div class="status-heading">
        <div class="modal-eyebrow">
          设备状态
        </div>
        <h3 id="equip-status-modal-title" class="modal-title">
          {{ equipment?.name || '更新设备状态' }}
        </h3>
      </div>
    </template>

    <div class="form-group">
      <label>状态 <span class="required">*</span></label>
      <UiSelect
        v-model="statusModel"
        :options="statusOptions"
        label="设备状态"
        min-width="100%"
      />
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
.status-heading {
  min-width: 0;
}

.modal-eyebrow {
  font-size: var(--fs-label);
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--ink-muted);
}

.modal-title {
  margin: var(--s1) 0 0;
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

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

.form-group input {
  width: 100%;
  height: var(--h-md);
  padding: 0 var(--s3);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-control);
  font-size: var(--fs-body);
  background: var(--face-page);
  color: var(--ink);
  box-sizing: border-box;
  font-family: inherit;
}

.form-group input:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}
</style>
