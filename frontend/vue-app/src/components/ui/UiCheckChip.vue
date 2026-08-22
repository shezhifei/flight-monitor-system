<script setup lang="ts">
/**
 * 勾选芯片（信号面 §2.5 持守 / §5.2）：可叠加的多选谓词。
 * - 关：线 + 次墨；开：行动描边 + 行动衬 + 行动字，手离开还在。
 * - 状态绑原生 `input:checked`（读屏拿到 checkbox 语义），CSS 用 :has 跟着走。
 * - 与开关式按钮的分工：一簇同维度的多选（字段、标签）用芯片；
 *   工具条上单独一条布尔过滤用 UiButton 的 aria-pressed。
 */
defineProps<{
  id: string;
  label: string;
  checked: boolean;
  ariaLabel?: string;
  disabled?: boolean;
}>();

const emit = defineEmits<{
  (e: 'update:checked', value: boolean): void;
}>();

function onChange(event: Event): void {
  emit('update:checked', (event.target as HTMLInputElement).checked);
}
</script>

<template>
  <label class="ui-chip" :for="id">
    <input
      :id="id"
      type="checkbox"
      :checked="checked"
      :disabled="disabled"
      :aria-label="ariaLabel ?? label"
      @change="onChange"
    >
    <span>{{ label }}</span>
  </label>
</template>

<style scoped>
.ui-chip {
  display: inline-flex;
  align-items: center;
  gap: var(--s1);
  height: 26px;
  padding: 0 10px;
  border: 1px solid var(--line);
  border-radius: var(--r-pill);
  background: var(--face-work);
  color: var(--ink-subtle);
  font-size: var(--fs-label);
  cursor: pointer;
  user-select: none;
  transition: border-color var(--t-fast) var(--ease), color var(--t-fast) var(--ease);
}

.ui-chip:hover {
  border-color: var(--line-strong);
  color: var(--ink);
}

.ui-chip:has(input:checked) {
  color: var(--act);
  border-color: color-mix(in srgb, var(--act) 45%, var(--line));
  background: var(--act-soft);
}

.ui-chip:has(input:disabled) {
  cursor: not-allowed;
  color: var(--ink-muted);
}

/* 自绘复选：剥掉系统蓝方勾，未选空心、选中行动实底白勾 */
.ui-chip input {
  appearance: none;
  -webkit-appearance: none;
  width: 14px;
  height: 14px;
  margin: 0;
  flex: none;
  border: 1px solid var(--line-strong);
  border-radius: 4px;
  background: transparent;
  cursor: inherit;
  transition: background-color var(--t-fast) var(--ease), border-color var(--t-fast) var(--ease);
}

.ui-chip input:checked {
  background-color: var(--act);
  border-color: var(--act);
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 12 12'%3E%3Cpath d='M2.5 6.2 5 8.7l4.5-5' fill='none' stroke='%23fff' stroke-width='1.8' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E");
  background-size: 10px 10px;
  background-position: center;
  background-repeat: no-repeat;
}

.ui-chip input:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}
</style>
