<script setup lang="ts">
import { computed, ref } from 'vue';
import SvgIcon from '@/components/ui/SvgIcon.vue';
import type { DepartmentResponse } from './dispatchRuleWorkbenchApi';
import { ALL_DEPARTMENTS_AGGREGATE } from './useDispatchRuleWorkbench';

const props = defineProps<{
  departments: DepartmentResponse[];
  modelValue: string;
  disabled?: boolean;
}>();

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
}>();

const searchQuery = ref('');
const open = ref(false);

const filteredDepartments = computed(() => {
  const query = searchQuery.value.trim().toLowerCase();
  if (!query) return props.departments;
  return props.departments.filter((d) =>
    d.name.toLowerCase().includes(query) ||
    (d.code ?? '').toLowerCase().includes(query),
  );
});

const selectedLabel = computed(() => {
  if (props.modelValue === ALL_DEPARTMENTS_AGGREGATE) return '全部科室 (只读)';
  const match = props.departments.find((d) => d.id === props.modelValue);
  return match ? match.name : '请选择科室';
});

function selectDept(id: string): void {
  emit('update:modelValue', id);
  open.value = false;
  searchQuery.value = '';
}
</script>

<template>
  <div class="department-selector">
    <label class="selector-label">科室</label>
    <button
      type="button"
      class="selector-button"
      :disabled="disabled"
      :aria-expanded="open"
      @click="open = !open"
    >
      <span>{{ selectedLabel }}</span>
      <span class="selector-chevron">▾</span>
    </button>
    <div v-if="open" class="selector-popover" role="listbox">
      <!-- 与 admin-page 统一：search-group + search.svg 16px -->
      <div class="search-group selector-search-wrap">
        <span class="search-icon" aria-hidden="true">
          <SvgIcon src="/frontend/icons/search.svg" :size="16" />
        </span>
        <input
          v-model="searchQuery"
          type="search"
          class="search-input"
          placeholder="搜索科室名称或编码"
          aria-label="搜索科室"
          autocomplete="off"
        >
      </div>
      <button
        type="button"
        role="option"
        class="selector-option"
        :class="{ active: modelValue === ALL_DEPARTMENTS_AGGREGATE }"
        @click="selectDept(ALL_DEPARTMENTS_AGGREGATE)"
      >
        全部科室 <span class="meta">只读聚合视图</span>
      </button>
      <button
        v-for="dept in filteredDepartments"
        :key="dept.id"
        type="button"
        role="option"
        class="selector-option"
        :class="{ active: modelValue === dept.id }"
        @click="selectDept(dept.id)"
      >
        <strong>{{ dept.name }}</strong>
        <span class="meta">{{ dept.code || '—' }}</span>
      </button>
      <div v-if="filteredDepartments.length === 0" class="selector-empty">
        无匹配科室
      </div>
    </div>
  </div>
</template>

<style scoped>
.department-selector {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 8px;
}

.selector-label {
  font-size: 12px;
  color: var(--admin-text-muted, var(--text-tertiary));
  font-weight: 600;
}

.selector-button {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  box-sizing: border-box;
  min-width: 200px;
  height: 40px;
  min-height: 40px;
  padding: 0 12px;
  border: 1px solid var(--admin-border, var(--border-light));
  border-radius: 10px;
  background: var(--admin-card-bg, var(--bg-card));
  color: var(--admin-text, var(--text-primary));
  font-size: 13px;
  font-weight: 500;
  font-family: inherit;
  cursor: pointer;
  box-shadow: 0 6px 16px rgba(15, 23, 42, 0.03);
}

.selector-button:disabled {
  cursor: not-allowed;
  opacity: 0.6;
}

.selector-chevron {
  margin-left: auto;
  color: var(--admin-text-muted, var(--text-tertiary));
  font-size: 12px;
}

.selector-popover {
  position: absolute;
  top: calc(100% + 4px);
  left: 56px;
  width: 300px;
  max-height: 320px;
  overflow-y: auto;
  background: var(--admin-card-bg, var(--bg-card));
  border: 1px solid var(--admin-border, var(--border-light));
  border-radius: 12px;
  box-shadow: var(--ws-shadow-md, 0 8px 24px rgba(0, 0, 0, 0.2));
  z-index: 50;
  padding: 10px;
  color: var(--admin-text, var(--text-primary));
}

/* 弹层内搜索：满宽，仍用全局 search-group / search-input 原语 */
.selector-search-wrap {
  width: 100% !important;
  min-width: 0 !important;
  margin-bottom: 8px;
}

.selector-option {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 2px;
  text-align: left;
  padding: 8px 10px;
  border: none;
  background: transparent;
  border-radius: 8px;
  cursor: pointer;
  color: var(--admin-text, var(--text-primary));
  font-size: 13px;
  font-family: inherit;
}

.selector-option:hover {
  background: var(--ws-surface-muted, var(--bg-sidebar));
}

.selector-option.active {
  background: var(--system-blue-subtle);
  color: var(--ws-primary, var(--system-blue));
}

.selector-option .meta {
  font-size: 11px;
  color: var(--admin-text-muted, var(--text-tertiary));
  font-weight: 500;
}

.selector-empty {
  padding: 12px;
  text-align: center;
  font-size: 12px;
  color: var(--admin-text-muted, var(--text-tertiary));
}
</style>
