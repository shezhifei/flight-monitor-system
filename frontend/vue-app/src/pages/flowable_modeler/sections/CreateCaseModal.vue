<script setup lang="ts">
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

function onOverlayClick(event: MouseEvent) {
  if ((event.target as HTMLElement | null)?.classList.contains('create-case-modal-overlay')) {
    emit('close');
  }
}

function onSubmit(event: Event) {
  event.preventDefault();
  emit('submit');
}
</script>

<template>
  <div
    class="create-case-modal-overlay"
    :class="{ active: open }"
    role="presentation"
    @click="onOverlayClick"
  >
    <div
      class="create-case-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="createCaseModalTitle"
      @click.stop
    >
      <div class="create-case-modal-header">
        <div id="createCaseModalTitle" class="create-case-modal-title">新建业务事项流程</div>
        <button class="create-case-modal-close" type="button" aria-label="关闭" @click="emit('close')">×</button>
      </div>
      <form id="createCaseForm" @submit="onSubmit">
        <div class="create-case-modal-body">
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
        <div class="create-case-modal-footer">
          <button type="button" class="btn btn-secondary" :disabled="submitting" @click="emit('close')">取消</button>
          <button type="submit" class="btn btn-primary" :disabled="submitting">
            {{ submitting ? '创建中…' : '创建流程' }}
          </button>
        </div>
      </form>
    </div>
  </div>
</template>
