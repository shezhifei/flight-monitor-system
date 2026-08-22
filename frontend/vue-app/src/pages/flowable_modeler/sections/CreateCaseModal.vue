<script setup lang="ts">
import UiButton from '@/components/ui/UiButton.vue';
import UiModal from '@/components/ui/UiModal.vue';

defineProps<{
  open: boolean;
  code: string;
  name: string;
  scope: 'DEPARTMENT' | 'COMMON';
  departmentLabel: string;
  hasDepartmentScope: boolean;
  scopeHint: string;
  error: string;
  submitting: boolean;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'submit'): void;
  (e: 'update:code', value: string): void;
  (e: 'update:name', value: string): void;
  (e: 'update:scope', value: 'DEPARTMENT' | 'COMMON'): void;
}>();

function onSubmit(event: Event) {
  event.preventDefault();
  emit('submit');
}
</script>

<template>
  <UiModal
    :open="open"
    title="新建业务事项流程"
    :width="460"
    @close="emit('close')"
  >
    <form id="createCaseForm" @submit="onSubmit">
      <div class="create-case-body">
        <div class="create-case-field">
          <label for="createCaseCode">业务事项代码</label>
          <input
            id="createCaseCode"
            :value="code"
            name="code"
            type="text"
            maxlength="64"
            placeholder="例如: custom_case_abc"
            autocomplete="off"
            :disabled="submitting"
            @input="emit('update:code', ($event.target as HTMLInputElement).value)"
          >
        </div>
        <div class="create-case-field">
          <label for="createCaseName">业务事项名称</label>
          <input
            id="createCaseName"
            :value="name"
            name="name"
            type="text"
            maxlength="100"
            placeholder="例如: 自定义业务事项ABC"
            autocomplete="off"
            :disabled="submitting"
            @input="emit('update:name', ($event.target as HTMLInputElement).value)"
          >
        </div>
        <div class="create-case-field">
          <label for="createCaseScope">归属范围</label>
          <select
            id="createCaseScope"
            :value="scope"
            name="visibility_scope"
            :disabled="submitting"
            @change="emit('update:scope', ($event.target as HTMLSelectElement).value as 'DEPARTMENT' | 'COMMON')"
          >
            <option value="DEPARTMENT" :disabled="!hasDepartmentScope">当前部门</option>
            <option value="COMMON">通用</option>
          </select>
        </div>
        <div class="create-case-field">
          <label>当前部门</label>
          <div class="create-case-static">{{ departmentLabel || '未配置部门' }}</div>
        </div>
        <div class="create-case-hint">{{ scopeHint }}</div>
        <div class="create-case-hint">代码仅支持字母、数字和下划线，且需在流程列表中唯一。</div>
        <div v-if="error" class="create-case-error" role="alert">{{ error }}</div>
      </div>
    </form>
    <template #footer>
      <UiButton variant="ghost" native-type="button" :disabled="submitting" @click="emit('close')">
        取消
      </UiButton>
      <UiButton variant="primary" native-type="submit" form="createCaseForm" :disabled="submitting">
        {{ submitting ? '创建中…' : '创建流程' }}
      </UiButton>
    </template>
  </UiModal>
</template>

<style scoped>
/* 壳（幕/帽/脚）归 UiModal；页里只留表单字段的形 */
.create-case-body {
  display: flex;
  flex-direction: column;
  gap: var(--s3);
}

.create-case-field {
  display: flex;
  flex-direction: column;
  gap: var(--s2);
}

.create-case-field label {
  font-size: var(--fs-body);
  font-weight: var(--fw-medium);
  color: var(--ink-subtle);
}

.create-case-field input,
.create-case-field select {
  width: 100%;
  box-sizing: border-box;
  border: 1px solid var(--line);
  border-radius: var(--r-control);
  padding: 9px 11px;
  font-size: var(--fs-section);
  color: var(--ink);
  background: var(--face-page);
}

.create-case-field input:focus-visible,
.create-case-field select:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
  background: var(--face-work);
}

.create-case-static {
  border: 1px solid var(--line);
  border-radius: var(--r-control);
  padding: 9px 11px;
  font-size: var(--fs-body);
  color: var(--ink-subtle);
  background: color-mix(in srgb, var(--ink) 6%, transparent);
}

.create-case-hint {
  font-size: var(--fs-label);
  color: var(--ink-subtle);
  line-height: 1.5;
}

.create-case-error {
  font-size: var(--fs-label);
  color: var(--danger);
  min-height: 18px;
}
</style>
