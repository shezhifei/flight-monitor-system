<script setup lang="ts">
import { computed, ref } from 'vue';
import type {
  ManagedUser,
  QualificationCatalogOption,
  QualificationGrant,
  QualificationGrantFormState,
  QualificationLevelOption,
  UserFormState,
  UserRole,
} from '@/composables/useUserManager';
import SvgIcon from '@/components/ui/SvgIcon.vue';
import UiButton from '@/components/ui/UiButton.vue';
import UiModal from '@/components/ui/UiModal.vue';
import QualificationGrantPanel from './QualificationGrantPanel.vue';

const props = defineProps<{
  show: boolean;
  editing: ManagedUser | null;
  form: UserFormState;
  roles: UserRole[];
  departmentSuggestions?: string[];
  saving?: boolean;
  qualificationHint?: string;
  qualificationGrants?: QualificationGrant[];
  qualificationCatalogs?: QualificationCatalogOption[];
  qualificationLevels?: QualificationLevelOption[];
  qualificationGrantForm?: QualificationGrantFormState;
  savingGrant?: boolean;
}>();
const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'save'): void;
  (e: 'update:form', value: UserFormState): void;
  (e: 'update:qualificationGrantForm', value: QualificationGrantFormState): void;
  (e: 'grant'): void;
  (e: 'revoke', grant: QualificationGrant): void;
}>();

const showPassword = ref(false);

const emailPattern = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
const canSave = computed(() => {
  if (props.saving) return false;
  const username = props.form.username.trim();
  const email = props.form.email.trim();
  const password = props.form.password.trim();
  if (!username || !email || !emailPattern.test(email)) return false;
  if (!props.editing && props.form.account_type !== 'position' && !password) return false;
  if (props.form.account_type === 'position' && !props.form.department.trim()) return false;
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
  <!-- 壳层归 UiModal；底部按钮归 UiButton；表单控件走标本配方 -->
  <UiModal
    :open="show"
    :title="editing ? '编辑用户' : '创建用户'"
    :width="520"
    @close="emit('close')"
  >
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
          :placeholder="editing ? '留空不修改' : (form.account_type === 'position' ? '岗位账号无需登录密码（可填占位）' : '请输入密码')"
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
      <label for="user-account-type">账号类型</label>
      <select
        id="user-account-type"
        :value="form.account_type"
        :disabled="!!editing"
        @change="update('account_type', (($event.target as HTMLSelectElement).value === 'position' ? 'position' : 'personal'))"
      >
        <option value="personal">个人（可登录）</option>
        <option value="position">岗位（席 / 流程收件，不能登录）</option>
      </select>
      <p v-if="form.account_type === 'position'" class="form-hint">
        岗位不是人：禁登录、禁管理员，必须挂科室。创建后不可改类型。
      </p>
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
          :disabled="form.account_type === 'position'"
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

    <QualificationGrantPanel
      :visible="Boolean(editing) && form.account_type === 'personal'"
      :hint="qualificationHint ?? ''"
      :grants="qualificationGrants ?? []"
      :catalogs="qualificationCatalogs ?? []"
      :levels="qualificationLevels ?? []"
      :form="qualificationGrantForm ?? { qualification_code: '', level_code: '' }"
      :saving="savingGrant"
      @update:form="emit('update:qualificationGrantForm', $event)"
      @grant="emit('grant')"
      @revoke="emit('revoke', $event)"
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
/* 弹窗壳层与底栏走 UiModal；此处只管表单格与控件 */
.form-group {
  margin-bottom: var(--s3);
}

.form-group label {
  display: block;
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  margin-bottom: var(--s2);
  color: var(--ink-subtle);
}

.form-hint {
  margin: 6px 0 0;
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

.form-group input[type="text"],
.form-group input[type="email"],
.form-group input[type="password"],
.form-group select {
  width: 100%;
  height: var(--h-sm);
  padding: 0 var(--s3);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-control);
  font-size: var(--fs-body);
  box-sizing: border-box;
  background: var(--face-page);
  color: var(--ink);
  font-family: inherit;
}

.form-group input:focus-visible,
.form-group select:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}

.password-field {
  position: relative;
}

.password-field input {
  padding-right: var(--s5);
}

.password-toggle {
  position: absolute;
  right: var(--s2);
  top: 50%;
  transform: translateY(-50%);
  background: none;
  border: none;
  cursor: pointer;
  padding: var(--s1);
  display: flex;
  align-items: center;
  color: var(--ink-subtle);
}

.form-row {
  display: flex;
  gap: var(--s5);
}

input[type="checkbox"] {
  accent-color: var(--act);
  width: 14px;
  height: 14px;
}

.form-row label {
  display: flex;
  align-items: center;
  gap: var(--s2);
  font-size: var(--fs-body);
  cursor: pointer;
}

.form-row-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--s3);
}

.role-checkboxes {
  display: flex;
  flex-wrap: wrap;
  gap: var(--s3);
}

.role-checkboxes label {
  display: flex;
  align-items: center;
  gap: var(--s1);
  font-size: var(--fs-body);
  cursor: pointer;
  font-weight: normal;
}

.role-empty {
  font-size: var(--fs-body);
  color: var(--ink-muted);
  padding: var(--s2) 0;
}
</style>
