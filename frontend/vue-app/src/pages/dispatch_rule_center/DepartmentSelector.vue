<script setup lang="ts">
import { computed, ref } from 'vue';
import UiSearch from '@/components/ui/UiSearch.vue';
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
    <div class="selector-box">
      <button
        type="button"
        class="selector-button"
        :disabled="disabled"
        :aria-expanded="open"
        @click="open = !open"
      >
        <span>{{ selectedLabel }}</span>
        <span class="selector-chevron" aria-hidden="true">▾</span>
      </button>
      <div v-if="open" class="selector-popover" role="listbox">
        <UiSearch
          v-model="searchQuery"
          class="selector-search"
          label="搜索科室"
          placeholder="搜索科室名称或编码"
          :grow="false"
        />
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
  </div>
</template>

<style scoped>
.department-selector {
  display: inline-flex;
  align-items: center;
  gap: var(--s2);
}

.selector-label {
  font-size: var(--fs-label);
  color: var(--ink-muted);
  font-weight: var(--fw-semibold);
}

.selector-box {
  position: relative;
}

.selector-button {
  display: inline-flex;
  align-items: center;
  gap: var(--s2);
  box-sizing: border-box;
  min-width: 200px;
  height: var(--h-lg);
  padding: 0 var(--s3);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-panel);
  background: var(--face-work);
  color: var(--ink);
  font-size: var(--fs-body);
  font-weight: var(--fw-medium);
  font-family: inherit;
  cursor: pointer;
  box-shadow: var(--shadow-sm);
}

.selector-button:disabled {
  cursor: not-allowed;
  opacity: 0.6;
}

.selector-chevron {
  margin-left: auto;
  color: var(--ink-muted);
  font-size: var(--fs-label);
}

.selector-popover {
  position: absolute;
  top: calc(100% + var(--s1));
  left: 0;
  width: 300px;
  max-height: 320px;
  overflow-y: auto;
  background: var(--face-work);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-panel);
  box-shadow: var(--shadow-md);
  z-index: var(--z-float);
  padding: var(--s2);
  color: var(--ink);
}

/* 弹层内搜索：满宽，复用库件 UiSearch */
.selector-search {
  width: 100%;
  margin-bottom: var(--s2);
}

.selector-option {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 2px;
  text-align: left;
  padding: var(--s2) var(--s3);
  border: none;
  background: transparent;
  border-radius: var(--r-control);
  cursor: pointer;
  color: var(--ink);
  font-size: var(--fs-body);
  font-family: inherit;
}

.selector-option:hover {
  background: color-mix(in srgb, var(--ink) 6%, transparent);
}

.selector-option.active {
  background: var(--act-soft);
  color: var(--act);
}

.selector-option .meta {
  font-size: var(--fs-label);
  color: var(--ink-muted);
  font-weight: var(--fw-medium);
}

.selector-empty {
  padding: var(--s3);
  text-align: center;
  font-size: var(--fs-label);
  color: var(--ink-muted);
}
</style>
