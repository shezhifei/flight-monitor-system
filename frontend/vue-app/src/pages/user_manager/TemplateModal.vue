<script setup lang="ts">
import { computed } from 'vue';
import {
  permissionNameOf,
  type Permission,
  type PermissionTemplate,
  type TemplateFormState,
} from '@/composables/useUserManager';
import UiButton from '@/components/ui/UiButton.vue';
import UiModal from '@/components/ui/UiModal.vue';
import UiPermissionTree, { type PermissionItem } from '@/components/ui/UiPermissionTree.vue';

const props = defineProps<{
  show: boolean;
  editing: PermissionTemplate | null;
  form: TemplateFormState;
  permissions: Permission[];
  saving: boolean;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'save'): void;
  (e: 'update:form', value: TemplateFormState): void;
}>();

const permItems = computed<PermissionItem[]>(() =>
  props.permissions.map((p) => ({
    key: permissionNameOf(p),
    description: p.description ?? undefined,
  })),
);

const selectedPerms = computed<string[]>({
  get: () => props.form.permissions,
  set: (value) => update('permissions', value),
});

const validationError = computed(() => {
  if (!props.form.name.trim()) return '请输入模板名称';
  if (!props.form.code.trim()) return '请输入模板代码';
  return '';
});

const canSubmit = computed(() => !validationError.value && !props.saving);

function update<K extends keyof TemplateFormState>(key: K, value: TemplateFormState[K]): void {
  emit('update:form', { ...props.form, [key]: value });
}

function onSubmit(): void {
  if (!canSubmit.value) return;
  emit('save');
}
</script>

<template>
  <UiModal
    :open="show"
    :title="editing ? '编辑权限模板' : '创建权限模板'"
    :width="680"
    @close="emit('close')"
  >
    <form class="tmpl-form" @submit.prevent="onSubmit">
      <div v-if="validationError" class="validation-summary" role="alert">
        {{ validationError }}
      </div>

      <div class="form-row">
        <div class="form-group">
          <label for="tmpl-name">名称<span class="required">*</span></label>
          <input
            id="tmpl-name"
            type="text"
            :value="form.name"
            required
            placeholder="例如：调度操作员"
            @input="update('name', ($event.target as HTMLInputElement).value)"
          >
        </div>
        <div class="form-group">
          <label for="tmpl-code">代码<span class="required">*</span></label>
          <input
            id="tmpl-code"
            type="text"
            :value="form.code"
            required
            placeholder="例如：dispatch_operator"
            @input="update('code', ($event.target as HTMLInputElement).value)"
          >
        </div>
      </div>

      <div class="form-row">
        <div class="form-group">
          <label for="tmpl-category">分类</label>
          <input
            id="tmpl-category"
            type="text"
            :value="form.category"
            placeholder="例如：调度"
            @input="update('category', ($event.target as HTMLInputElement).value)"
          >
        </div>
        <div class="form-group">
          <label for="tmpl-desc">描述</label>
          <input
            id="tmpl-desc"
            type="text"
            :value="form.description"
            placeholder="模板用途说明，可选"
            @input="update('description', ($event.target as HTMLInputElement).value)"
          >
        </div>
      </div>

      <div class="form-group">
        <UiPermissionTree
          v-model="selectedPerms"
          :items="permItems"
          label="权限选择"
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
/* 信号面：帽、幕、Esc、关归 UiModal，权限树归 UiPermissionTree；这里只留表单。 */
.tmpl-form {
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

.form-row {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--s3);
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

.form-group input[type="text"] {
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

.required {
  color: var(--danger);
}
</style>
