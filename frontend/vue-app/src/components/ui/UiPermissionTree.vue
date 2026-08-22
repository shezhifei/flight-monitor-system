<script setup lang="ts">
import { computed, ref } from 'vue';
import UiSearch from './UiSearch.vue';

/**
 * 权限树（信号面 §3.8 弹窗内的一件器）：
 * 一行一个权限，一楼一组，组头是全选/半选；搜是有值的寻簇。
 * 选中态只有复选一格，不铺行动衬。
 */
export interface PermissionItem {
  /** 权限标识（唯一键） */
  key: string;
  /** 描述，可有可无 */
  description?: string;
}

interface PermissionGroup {
  prefix: string;
  items: PermissionItem[];
}

const props = withDefaults(defineProps<{
  modelValue: string[];
  items: PermissionItem[];
  /** 头部左边的名字（如「权限分配」） */
  label: string;
  emptyText?: string;
}>(), {
  emptyText: '暂无可分配权限',
});

const emit = defineEmits<{
  (e: 'update:modelValue', value: string[]): void;
}>();

const searchText = ref('');

function prefixOf(name: string): string {
  const match = name.match(/^([^:._]+)/);
  return match ? match[1] : name;
}

const filtered = computed(() => {
  const q = searchText.value.trim().toLowerCase();
  if (!q) return props.items;
  return props.items.filter(
    (item) =>
      item.key.toLowerCase().includes(q)
      || (item.description ?? '').toLowerCase().includes(q),
  );
});

const grouped = computed<PermissionGroup[]>(() => {
  const groups = new Map<string, PermissionItem[]>();
  for (const item of filtered.value) {
    const key = prefixOf(item.key);
    const list = groups.get(key) ?? [];
    list.push(item);
    groups.set(key, list);
  }
  return Array.from(groups.entries())
    .map(([prefix, groupItems]) => ({
      prefix,
      items: groupItems.slice().sort((a, b) => a.key.localeCompare(b.key)),
    }))
    .sort((a, b) => a.prefix.localeCompare(b.prefix));
});

function isChecked(key: string): boolean {
  return props.modelValue.includes(key);
}

function groupChecked(group: PermissionGroup): boolean {
  return group.items.every((item) => props.modelValue.includes(item.key));
}

function groupIndeterminate(group: PermissionGroup): boolean {
  const some = group.items.some((item) => props.modelValue.includes(item.key));
  return some && !groupChecked(group);
}

function toggleItem(key: string, checked: boolean): void {
  const next = checked
    ? Array.from(new Set([...props.modelValue, key]))
    : props.modelValue.filter((k) => k !== key);
  emit('update:modelValue', next);
}

function toggleGroup(group: PermissionGroup, checked: boolean): void {
  const keys = group.items.map((item) => item.key);
  const next = checked
    ? Array.from(new Set([...props.modelValue, ...keys]))
    : props.modelValue.filter((k) => !keys.includes(k));
  emit('update:modelValue', next);
}
</script>

<template>
  <div class="ui-perm-tree">
    <div class="ui-perm-tree__head">
      <span class="ui-perm-tree__label">{{ label }}</span>
      <span class="ui-perm-tree__count">已选 {{ modelValue.length }} 项</span>
    </div>
    <UiSearch
      v-model="searchText"
      :label="`搜索${label}`"
      placeholder="搜索名称或描述..."
    />
    <div v-if="grouped.length === 0" class="ui-perm-tree__empty">
      {{ emptyText }}
    </div>
    <div v-else class="ui-perm-tree__groups">
      <div v-for="group in grouped" :key="group.prefix" class="ui-perm-tree__group">
        <label class="ui-perm-tree__group-head">
          <input
            type="checkbox"
            :checked="groupChecked(group)"
            :indeterminate.prop="groupIndeterminate(group)"
            @change="toggleGroup(group, ($event.target as HTMLInputElement).checked)"
          >
          <span class="ui-perm-tree__group-title">{{ group.prefix }}</span>
          <span class="ui-perm-tree__group-meta">{{ group.items.length }} 项</span>
        </label>
        <div class="ui-perm-tree__items">
          <label
            v-for="item in group.items"
            :key="item.key"
            class="ui-perm-tree__item"
          >
            <input
              type="checkbox"
              :checked="isChecked(item.key)"
              @change="toggleItem(item.key, ($event.target as HTMLInputElement).checked)"
            >
            <span class="ui-perm-tree__code">{{ item.key }}</span>
            <span v-if="item.description" class="ui-perm-tree__desc">{{ item.description }}</span>
          </label>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.ui-perm-tree__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: var(--s2);
}

.ui-perm-tree__label {
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  color: var(--ink-subtle);
}

.ui-perm-tree__count {
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

.ui-perm-tree__empty {
  margin-top: var(--s2);
  padding: var(--s5);
  text-align: center;
  color: var(--ink-muted);
  font-size: var(--fs-body);
  border: 1px dashed var(--line-strong);
  border-radius: var(--r-control);
}

.ui-perm-tree__groups {
  margin-top: var(--s2);
  border: 1px solid var(--line);
  border-radius: var(--r-control);
  max-height: 320px;
  overflow-y: auto;
}

.ui-perm-tree__group {
  border-bottom: 1px solid var(--line);
}

.ui-perm-tree__group:last-child {
  border-bottom: none;
}

.ui-perm-tree__group-head {
  display: flex;
  align-items: center;
  gap: var(--s2);
  padding: var(--s2) var(--s3);
  background: var(--face-page);
  font-weight: var(--fw-semibold);
  font-size: var(--fs-body);
  cursor: pointer;
  margin: 0;
}

.ui-perm-tree__group-title {
  flex: 1;
}

.ui-perm-tree__group-meta {
  font-size: var(--fs-label);
  color: var(--ink-muted);
  font-weight: normal;
}

.ui-perm-tree__items {
  /* 左缩进与组头复选框后的标题对齐：组头内距 + 复选位宽 */
  padding: var(--s2) var(--s3) var(--s2) calc(var(--s3) + 20px);
  display: flex;
  flex-direction: column;
  gap: var(--s1);
}

.ui-perm-tree__item {
  display: flex;
  align-items: center;
  gap: var(--s2);
  font-size: var(--fs-body);
  cursor: pointer;
  margin: 0;
  font-weight: normal;
}

.ui-perm-tree__code {
  font-family: var(--mono);
  font-size: var(--fs-label);
  color: var(--ink);
}

.ui-perm-tree__desc {
  color: var(--ink-muted);
  font-size: var(--fs-label);
}

.ui-perm-tree input[type="checkbox"] {
  accent-color: var(--act);
}
</style>
