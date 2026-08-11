<script setup lang="ts">
import { computed, ref } from 'vue';
import {
  permissionNameOf,
  type Permission,
  type PermissionTemplate,
  type TemplateFormState,
} from '@/composables/useUserManager';

interface PermissionGroup {
  prefix: string;
  permissions: Permission[];
}

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

const searchText = ref('');

function prefixOf(name: string): string {
  const match = name.match(/^([^:._]+)/);
  return match ? match[1] : name;
}

const filteredPermissions = computed(() => {
  const q = searchText.value.trim().toLowerCase();
  if (!q) return props.permissions;
  return props.permissions.filter(
    (p) =>
      permissionNameOf(p).toLowerCase().includes(q)
      || (p.description ?? '').toLowerCase().includes(q),
  );
});

const grouped = computed<PermissionGroup[]>(() => {
  const groups = new Map<string, Permission[]>();
  for (const perm of filteredPermissions.value) {
    const key = prefixOf(permissionNameOf(perm));
    const list = groups.get(key) ?? [];
    list.push(perm);
    groups.set(key, list);
  }
  return Array.from(groups.entries())
    .map(([prefix, items]) => ({
      prefix,
      permissions: items.slice().sort((a, b) =>
        permissionNameOf(a).localeCompare(permissionNameOf(b)),
      ),
    }))
    .sort((a, b) => a.prefix.localeCompare(b.prefix));
});

const selectedCount = computed(() => props.form.permissions.length);

const validationError = computed(() => {
  if (!props.form.name.trim()) return '请输入模板名称';
  if (!props.form.code.trim()) return '请输入模板代码';
  return '';
});

const canSubmit = computed(() => !validationError.value && !props.saving);

function update<K extends keyof TemplateFormState>(key: K, value: TemplateFormState[K]): void {
  emit('update:form', { ...props.form, [key]: value });
}

function togglePermission(name: string, checked: boolean): void {
  const current = props.form.permissions;
  const next = checked
    ? Array.from(new Set([...current, name]))
    : current.filter((v) => v !== name);
  update('permissions', next);
}

function toggleGroup(group: PermissionGroup, checked: boolean): void {
  const names = group.permissions.map((p) => permissionNameOf(p));
  const current = props.form.permissions;
  const next = checked
    ? Array.from(new Set([...current, ...names]))
    : current.filter((v) => !names.includes(v));
  update('permissions', next);
}

function groupChecked(group: PermissionGroup): boolean {
  return group.permissions.every((p) =>
    props.form.permissions.includes(permissionNameOf(p)),
  );
}

function groupIndeterminate(group: PermissionGroup): boolean {
  const some = group.permissions.some((p) =>
    props.form.permissions.includes(permissionNameOf(p)),
  );
  return some && !groupChecked(group);
}

function isChecked(name: string): boolean {
  return props.form.permissions.includes(name);
}

function onSubmit(): void {
  if (!canSubmit.value) return;
  emit('save');
}
</script>

<template>
  <Teleport to="body">
    <div v-if="show" class="modal-overlay" @click.self="emit('close')">
      <div
        class="modal-content"
        role="dialog"
        aria-modal="true"
        aria-labelledby="template-modal-title"
      >
        <div class="modal-header">
          <h3 id="template-modal-title">
            {{ editing ? '编辑权限模板' : '创建权限模板' }}
          </h3>
          <button
            type="button"
            class="modal-close"
            aria-label="关闭"
            @click="emit('close')"
          >
            ×
          </button>
        </div>

        <form class="modal-body" @submit.prevent="onSubmit">
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
            <div class="permission-header">
              <label>权限选择</label>
              <span class="permission-count">已选 {{ selectedCount }} 项</span>
            </div>
            <input
              v-model="searchText"
              type="search"
              class="permission-search"
              placeholder="搜索权限名称或描述..."
            >
            <div v-if="grouped.length === 0" class="permission-empty">
              暂无可分配权限
            </div>
            <div v-else class="permission-groups">
              <div v-for="group in grouped" :key="group.prefix" class="permission-group">
                <label class="permission-group-header">
                  <input
                    type="checkbox"
                    :checked="groupChecked(group)"
                    :indeterminate.prop="groupIndeterminate(group)"
                    @change="toggleGroup(group, ($event.target as HTMLInputElement).checked)"
                  >
                  <span class="permission-group-title">{{ group.prefix }}</span>
                  <span class="permission-group-meta">{{ group.permissions.length }} 项</span>
                </label>
                <div class="permission-items">
                  <label
                    v-for="perm in group.permissions"
                    :key="perm.id || permissionNameOf(perm)"
                    class="permission-item"
                  >
                    <input
                      type="checkbox"
                      :checked="isChecked(permissionNameOf(perm))"
                      @change="togglePermission(permissionNameOf(perm), ($event.target as HTMLInputElement).checked)"
                    >
                    <span class="permission-code">{{ permissionNameOf(perm) }}</span>
                    <span v-if="perm.description" class="permission-desc">{{ perm.description }}</span>
                  </label>
                </div>
              </div>
            </div>
          </div>
        </form>

        <div class="modal-footer">
          <button type="button" class="btn btn-secondary" @click="emit('close')">
            取消
          </button>
          <button
            type="button"
            class="btn btn-primary"
            :disabled="!canSubmit"
            @click="onSubmit"
          >
            {{ saving ? '保存中...' : '保存' }}
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
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 10000;
}
.modal-content {
  background: var(--bg-card, #fff);
  border-radius: 12px;
  width: 680px;
  max-width: 92vw;
  max-height: 88vh;
  display: flex;
  flex-direction: column;
}
.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px 20px;
  border-bottom: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
}
.modal-header h3 { margin: 0; font-size: 16px; font-weight: 600; }
.modal-close {
  background: none;
  border: none;
  font-size: 24px;
  cursor: pointer;
  color: var(--text-secondary, #64748b);
  line-height: 1;
}
.modal-body {
  padding: 20px;
  overflow-y: auto;
  flex: 1;
}
.validation-summary {
  background: var(--dh-signal-critical-soft);
  border: 1px solid #fecaca;
  color: var(--ws-danger);
  padding: 8px 12px;
  border-radius: 6px;
  font-size: 13px;
  margin-bottom: 12px;
}
.form-row {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}
.form-group { margin-bottom: 16px; }
.form-group label {
  display: block;
  font-size: 13px;
  font-weight: 500;
  margin-bottom: 6px;
  color: var(--text-primary, #1D1D1F);
}
.form-group input[type="text"],
.permission-search {
  width: 100%;
  padding: 8px 12px;
  border: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
  border-radius: 6px;
  font-size: 14px;
  background: var(--bg-card);
  box-sizing: border-box;
  font-family: inherit;
}
.required { color: var(--status-text-boarding-urge); margin-left: 2px; }
.permission-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.permission-count {
  font-size: 12px;
  color: var(--text-secondary, #64748b);
  font-weight: normal;
}
.permission-search { margin-bottom: 8px; }
.permission-empty {
  padding: 24px;
  text-align: center;
  color: var(--system-gray2);
  font-size: 13px;
  border: 1px dashed var(--border-light);
  border-radius: 6px;
}
.permission-groups {
  border: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
  border-radius: 6px;
  max-height: 320px;
  overflow-y: auto;
}
.permission-group { border-bottom: 1px solid #f1f5f9; }
.permission-group:last-child { border-bottom: none; }
.permission-group-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  background: var(--bg-page);
  font-weight: 600;
  font-size: 13px;
  cursor: pointer;
  margin: 0;
}
.permission-group-title { flex: 1; }
.permission-group-meta { font-size: 11px; color: var(--system-gray2); font-weight: normal; }
.permission-items {
  padding: 6px 12px 10px 32px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.permission-item {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  cursor: pointer;
  margin: 0;
  font-weight: normal;
}
.permission-code {
  font-family: ui-monospace, SFMono-Regular, monospace;
  font-size: 12px;
  color: var(--text-primary);
}
.permission-desc { color: var(--text-tertiary); font-size: 12px; }
.modal-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 16px 20px;
  border-top: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
}
.btn {
  padding: 8px 16px;
  border-radius: 6px;
  font-size: 13px;
  font-weight: 500;
  border: 1px solid var(--border-light);
  background: var(--bg-card);
  cursor: pointer;
}
.btn-primary {
  background: var(--system-blue);
  color: var(--text-inverse);
  border-color: var(--system-blue);
}
.btn-primary:disabled {
  background: var(--dh-signal-accent-soft);
  border-color: #93c5fd;
  cursor: not-allowed;
}
.btn-secondary { background: var(--bg-page); color: var(--text-tertiary); }
</style>
