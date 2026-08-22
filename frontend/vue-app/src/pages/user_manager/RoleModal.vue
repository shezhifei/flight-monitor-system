<script setup lang="ts">
import { computed, ref } from 'vue';
import {
  permissionNameOf,
  type Permission,
  type PermissionTemplate,
  type RoleFormState,
  type TemplateApplyMode,
  type UserRole,
} from '@/composables/useUserManager';
import UiButton from '@/components/ui/UiButton.vue';
import UiModal from '@/components/ui/UiModal.vue';
import UiPermissionTree, { type PermissionItem } from '@/components/ui/UiPermissionTree.vue';
import UiSelect from '@/components/ui/UiSelect.vue';

const props = defineProps<{
  show: boolean;
  editing: UserRole | null;
  form: RoleFormState;
  permissions: Permission[];
  templates?: PermissionTemplate[];
  saving: boolean;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'save'): void;
  (e: 'update:form', value: RoleFormState): void;
  (e: 'apply-template', templateId: string, mode: TemplateApplyMode): void;
}>();

const selectedTemplateId = ref('');

const permItems = computed<PermissionItem[]>(() =>
  props.permissions.map((p) => ({
    key: permissionNameOf(p),
    description: p.description ?? undefined,
  })),
);

const permissionCodes = computed<string[]>({
  get: () => props.form.permission_codes,
  set: (value) => update('permission_codes', value),
});

const validationError = computed(() => {
  if (!props.form.name.trim()) return '请输入角色名称';
  return '';
});

const canSubmit = computed(() => !validationError.value && !props.saving);

const templateOptions = computed(() => [
  { value: '', label: '选择模板...' },
  ...(props.templates ?? []).map((tmpl) => ({
    value: tmpl.id,
    label: `[${tmpl.category || '其他'}] ${tmpl.name}`,
  })),
]);

function update<K extends keyof RoleFormState>(key: K, value: RoleFormState[K]): void {
  emit('update:form', { ...props.form, [key]: value });
}

function applyTemplate(mode: TemplateApplyMode): void {
  emit('apply-template', selectedTemplateId.value, mode);
}

function onSubmit(): void {
  if (!canSubmit.value) return;
  emit('save');
}
</script>

<template>
  <UiModal
    :open="show"
    :title="editing ? '编辑角色' : '创建角色'"
    :width="640"
    @close="emit('close')"
  >
    <form class="role-form" @submit.prevent="onSubmit">
      <div v-if="validationError" class="validation-summary" role="alert">
        {{ validationError }}
      </div>

      <div class="form-group">
        <label for="role-name">角色名称<span class="required">*</span></label>
        <input
          id="role-name"
          type="text"
          :value="form.name"
          required
          placeholder="例如：调度主管"
          @input="update('name', ($event.target as HTMLInputElement).value)"
        >
      </div>

      <div class="form-group">
        <label for="role-description">描述</label>
        <textarea
          id="role-description"
          :value="form.description"
          rows="2"
          placeholder="角色职责说明，可选"
          @input="update('description', ($event.target as HTMLTextAreaElement).value)"
        />
      </div>

      <div class="form-group template-apply">
        <label>快速应用模板</label>
        <div class="template-apply-row">
          <UiSelect
            v-model="selectedTemplateId"
            :options="templateOptions"
            label="选择权限模板"
            min-width="180px"
          />
          <UiButton size="sm" @click="applyTemplate('replace')">
            替换
          </UiButton>
          <UiButton size="sm" @click="applyTemplate('append')">
            追加
          </UiButton>
          <UiButton size="sm" @click="applyTemplate('clear')">
            清空
          </UiButton>
        </div>
      </div>

      <div class="form-group">
        <UiPermissionTree
          v-model="permissionCodes"
          :items="permItems"
          label="权限分配"
        />
      </div>
    </form>

    <template #footer>
      <UiButton @click="emit('close')">
        取消
      </UiButton>
      <UiButton
        variant="primary"
        :disabled="!canSubmit"
        @click="onSubmit"
      >
        {{ saving ? '保存中...' : '保存' }}
      </UiButton>
    </template>
  </UiModal>
</template>

<style scoped>
/* 信号面：帽、幕、Esc、关归 UiModal，权限树归 UiPermissionTree；这里只留表单与模板行。 */
.role-form {
  display: flex;
  flex-direction: column;
}

.validation-summary {
  background: var(--danger-soft);
  border: 1px solid color-mix(in srgb, var(--danger) 32%, transparent);
  color: var(--danger);
  padding: var(--s2) var(--s3);
  border-radius: var(--r-control);
  font-size: var(--fs-body);
  margin-bottom: var(--s3);
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

.form-group input:focus-visible,
.form-group textarea:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}

.required {
  color: var(--danger);
}

.template-apply {
  background: color-mix(in srgb, var(--ink) 6%, transparent);
  padding: var(--s3);
  border-radius: var(--r-control);
}

.template-apply-row {
  display: flex;
  gap: var(--s2);
  flex-wrap: wrap;
  align-items: center;
}
</style>
