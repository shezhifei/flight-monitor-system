<script setup lang="ts">
import { computed, ref } from 'vue';
import type { ManagedUser, UserFormState, UserRole } from '@/composables/useUserManager';
import SvgIcon from '@/components/ui/SvgIcon.vue';

const props = defineProps<{
  show: boolean;
  editing: ManagedUser | null;
  form: UserFormState;
  roles: UserRole[];
  departmentSuggestions?: string[];
  saving?: boolean;
}>();
const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'save'): void;
  (e: 'update:form', value: UserFormState): void;
}>();

const showPassword = ref(false);

const emailPattern = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
const canSave = computed(() => {
  if (props.saving) return false;
  const username = props.form.username.trim();
  const email = props.form.email.trim();
  const password = props.form.password.trim();
  if (!username || !email || !emailPattern.test(email)) return false;
  if (!props.editing && !password) return false;
  return true;
});

const jobLevelOptions = [
  { value: 1, label: '1 - 一线员工' },
  { value: 2, label: '2 - 班组长' },
  { value: 3, label: '3 - 主管' },
  { value: 4, label: '4 - 经理' },
  { value: 5, label: '5 - 总监' },
  { value: 6, label: '6 - 高级管理' },
  { value: 7, label: '7 - 值班经理' },
  { value: 8, label: '8 - 其他' },
];

function update<K extends keyof UserFormState>(key: K, value: UserFormState[K]): void {
  emit('update:form', { ...props.form, [key]: value });
}

function toggleRole(roleName: string, checked: boolean): void {
  const current = props.form.roles;
  const next = checked
    ? Array.from(new Set([...current, roleName]))
    : current.filter((name) => name !== roleName);
  update('roles', next);
}

function isRoleChecked(roleName: string): boolean {
  return props.form.roles.includes(roleName);
}
</script>

<template>
  <Teleport to="body">
    <div v-if="show" class="modal-overlay" @click.self="emit('close')">
      <div class="modal-content" role="dialog" aria-modal="true" aria-labelledby="user-modal-title">
        <div class="modal-header">
          <h3 id="user-modal-title">
            {{ editing ? '编辑用户' : '创建用户' }}
          </h3>
          <button type="button" class="modal-close" aria-label="关闭" @click="emit('close')">
            ×
          </button>
        </div>
        <div class="modal-body">
          <div class="form-group">
            <label for="user-username">用户名</label>
            <input
              id="user-username"
              type="text"
              :value="form.username"
              :disabled="!!editing"
              placeholder="请输入用户名"
              @input="update('username', ($event.target as HTMLInputElement).value)"
            >
          </div>
          <div class="form-group">
            <label for="user-email">邮箱</label>
            <input
              id="user-email"
              type="email"
              :value="form.email"
              placeholder="请输入邮箱"
              @input="update('email', ($event.target as HTMLInputElement).value)"
            >
          </div>
          <div class="form-group">
            <label for="user-password">密码 {{ editing ? '(留空不修改)' : '' }}</label>
            <div class="password-field">
              <input
                id="user-password"
                :type="showPassword ? 'text' : 'password'"
                :value="form.password"
                :placeholder="editing ? '留空不修改' : '请输入密码'"
                @input="update('password', ($event.target as HTMLInputElement).value)"
              >
              <button
                type="button"
                class="password-toggle"
                :title="showPassword ? '隐藏密码' : '显示密码'"
                @click="showPassword = !showPassword"
              >
                <SvgIcon
                  :src="showPassword
                    ? '/frontend/icons/password_unvisible.svg'
                    : '/frontend/icons/password_visible.svg'"
                  :size="18"
                />
              </button>
            </div>
          </div>

          <div class="form-group">
            <label for="user-department">所属科室</label>
            <input
              id="user-department"
              type="text"
              list="user-department-options"
              :value="form.department"
              placeholder="如：运行控制中心"
              @input="update('department', ($event.target as HTMLInputElement).value)"
            >
            <datalist id="user-department-options">
              <option
                v-for="dept in (departmentSuggestions ?? [])"
                :key="dept"
                :value="dept"
              />
            </datalist>
          </div>

          <div class="form-row-grid">
            <div class="form-group">
              <label for="user-job-level">职级</label>
              <select
                id="user-job-level"
                :value="form.job_level"
                @change="update('job_level', Number(($event.target as HTMLSelectElement).value) || 1)"
              >
                <option
                  v-for="opt in jobLevelOptions"
                  :key="opt.value"
                  :value="opt.value"
                >
                  {{ opt.label }}
                </option>
                <option
                  v-if="form.job_level && !jobLevelOptions.some((o) => o.value === form.job_level)"
                  :value="form.job_level"
                >
                  {{ form.job_level }}
                </option>
              </select>
            </div>
            <div class="form-group">
              <label for="user-job-title">职位名称</label>
              <input
                id="user-job-title"
                type="text"
                :value="form.job_title"
                placeholder="如：值班经理"
                @input="update('job_title', ($event.target as HTMLInputElement).value)"
              >
            </div>
          </div>

          <div class="form-group form-row">
            <label>
              <input
                type="checkbox"
                :checked="form.is_admin"
                @change="update('is_admin', ($event.target as HTMLInputElement).checked)"
              >
              管理员
            </label>
            <label>
              <input
                type="checkbox"
                :checked="form.is_active"
                @change="update('is_active', ($event.target as HTMLInputElement).checked)"
              >
              启用
            </label>
          </div>
          <div class="form-group">
            <label>角色</label>
            <div v-if="roles.length === 0" class="role-empty">
              暂无角色，请先创建角色
            </div>
            <div v-else class="role-checkboxes">
              <label v-for="role in roles" :key="role.id">
                <input
                  type="checkbox"
                  :checked="isRoleChecked(role.name)"
                  @change="toggleRole(role.name, ($event.target as HTMLInputElement).checked)"
                >
                {{ role.name }}
              </label>
            </div>
          </div>
        </div>
        <div class="modal-footer">
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
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
/* 对话框 = 抬起面 +  scrim；控件走标本配方（signal-surface.css token） */
.modal-overlay { position: fixed; inset: 0; background: var(--scrim); display: flex; align-items: center; justify-content: center; z-index: 10000; }
.modal-content { background: var(--face-raised); color: var(--ink); border: 1px solid var(--line); border-radius: var(--r-panel); box-shadow: var(--shadow-md); width: 520px; max-width: 94vw; max-height: 88vh; display: flex; flex-direction: column; }
.modal-header { display: flex; align-items: center; justify-content: space-between; padding: 16px 20px; border-bottom: 1px solid var(--line); }
.modal-header h3 { margin: 0; font-size: var(--fs-title); font-weight: var(--fw-semibold); }
.modal-close { background: none; border: none; font-size: 24px; cursor: pointer; color: var(--ink-subtle); }
.modal-close:hover { color: var(--ink); }
.modal-body { padding: 20px; overflow-y: auto; flex: 1; }
.form-group { margin-bottom: 16px; }
.form-group label { display: block; font-size: var(--fs-label); font-weight: var(--fw-medium); margin-bottom: 6px; color: var(--ink-subtle); }
.form-group input[type="text"],
.form-group input[type="email"],
.form-group input[type="password"],
.form-group select { width: 100%; padding: 7px 12px; border: 1px solid var(--line-strong); border-radius: var(--r-control); font-size: var(--fs-body); box-sizing: border-box; background: var(--face-page); color: var(--ink); font-family: inherit; }
.form-group input:focus-visible,
.form-group select:focus-visible { outline: 2px solid var(--act); outline-offset: 1px; }
.password-field { position: relative; }
.password-field input { padding-right: 40px; }
.password-toggle {
  position: absolute;
  right: 8px;
  top: 50%;
  transform: translateY(-50%);
  background: none;
  border: none;
  cursor: pointer;
  padding: 4px;
  display: flex;
  align-items: center;
  color: var(--ink-subtle);
}
.form-row { display: flex; gap: 24px; }
input[type="checkbox"] { accent-color: var(--act); width: 14px; height: 14px; }
.form-row label { display: flex; align-items: center; gap: 6px; font-size: var(--fs-body); cursor: pointer; }
.form-row-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
.role-checkboxes { display: flex; flex-wrap: wrap; gap: 12px; }
.role-checkboxes label { display: flex; align-items: center; gap: 4px; font-size: var(--fs-body); cursor: pointer; font-weight: normal; }
.role-empty { font-size: var(--fs-body); color: var(--ink-muted); padding: 8px 0; }
.modal-footer { display: flex; justify-content: flex-end; gap: 8px; padding: 16px 20px; border-top: 1px solid var(--line); }
.btn { min-height: var(--h-sm); padding: 0 14px; border-radius: var(--r-control); font-size: var(--fs-label); font-weight: var(--fw-medium); border: 1px solid var(--line-strong); background: transparent; color: var(--ink); cursor: pointer; }
.btn:hover { border-color: var(--act); color: var(--act); }
.btn-primary { background: var(--act); color: var(--act-on); border-color: transparent; }
.btn-primary:hover { background: var(--act); color: var(--act-on); filter: brightness(1.06); }
.btn-primary:disabled { background: color-mix(in srgb, var(--ink) 8%, transparent); color: var(--ink-muted); border-color: transparent; cursor: not-allowed; filter: none; }
.btn-secondary { background: transparent; color: var(--ink-subtle); }
</style>
