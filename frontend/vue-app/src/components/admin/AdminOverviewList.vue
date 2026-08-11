<script setup lang="ts">
import SvgIcon from '@/components/ui/SvgIcon.vue';
import type { AdminOverviewItem } from './adminOverviewTypes';

const props = withDefaults(
  defineProps<{
    items: AdminOverviewItem[];
    selectedId?: string | null;
    emptyText?: string;
    errorText?: string;
    ariaLabel?: string;
    /**
     * remove  — hard delete icon (派工任务类型)
     * deprecate — legacy flowable: × 弃用 / 刷新恢复
     */
    actionMode?: 'remove' | 'deprecate';
    showDelete?: boolean;
    deleteDisabled?: boolean;
    deleteTitle?: string;
    restoreTitle?: string;
    density?: 'default' | 'compact';
  }>(),
  {
    selectedId: null,
    emptyText: '暂无数据',
    errorText: '',
    ariaLabel: '概览列表',
    actionMode: 'remove',
    showDelete: false,
    deleteDisabled: false,
    deleteTitle: '删除',
    restoreTitle: '恢复使用',
    density: 'default',
  },
);

const emit = defineEmits<{
  (e: 'select', id: string): void;
  (e: 'delete', id: string): void;
  (e: 'restore', id: string): void;
}>();

function isActive(id: string): boolean {
  return props.selectedId != null && props.selectedId === id;
}

function canAct(item: AdminOverviewItem): boolean {
  return props.showDelete && item.deletable !== false;
}

function onSelect(id: string): void {
  emit('select', id);
}

function onDelete(event: Event, id: string): void {
  event.stopPropagation();
  emit('delete', id);
}

function onRestore(event: Event, id: string): void {
  event.stopPropagation();
  emit('restore', id);
}

function actionTitle(item: AdminOverviewItem): string {
  if (props.actionMode === 'deprecate' && item.deprecated) return props.restoreTitle;
  if (props.actionMode === 'deprecate') return props.deleteTitle || '弃用该类型';
  return props.deleteTitle;
}
</script>

<template>
  <div
    class="admin-overview-list"
    :class="{ 'is-compact': density === 'compact' }"
    role="listbox"
    :aria-label="ariaLabel"
  >
    <div v-if="errorText" class="admin-overview-empty is-error" role="alert">
      {{ errorText }}
    </div>
    <div v-else-if="items.length === 0" class="admin-overview-empty">
      <slot name="empty">
        {{ emptyText }}
      </slot>
    </div>
    <template v-else>
      <div
        v-for="item in items"
        :key="item.id"
        class="admin-overview-item"
        role="option"
        :aria-selected="isActive(item.id)"
        :class="{
          active: isActive(item.id),
          deprecated: !!item.deprecated,
          'has-actions': canAct(item) || !!$slots.actions,
        }"
      >
        <button
          type="button"
          class="admin-overview-item__body"
          @click="onSelect(item.id)"
        >
          <span class="admin-overview-item__title-row">
            <strong class="admin-overview-item__title">{{ item.title }}</strong>
            <span v-if="item.deprecated" class="admin-overview-item__badge">已弃用</span>
          </span>
          <span v-if="item.meta" class="admin-overview-item__meta">{{ item.meta }}</span>
          <span v-if="item.description" class="admin-overview-item__desc">{{ item.description }}</span>
        </button>
        <div v-if="canAct(item) || $slots.actions" class="admin-overview-item__actions">
          <slot name="actions" :item="item" />
          <button
            v-if="canAct(item) && actionMode === 'deprecate' && item.deprecated"
            type="button"
            class="admin-overview-item__icon-btn admin-overview-item__restore"
            :title="actionTitle(item)"
            :aria-label="actionTitle(item)"
            :disabled="deleteDisabled"
            @click="onRestore($event, item.id)"
          >
            <SvgIcon src="/frontend/icons/refresh.svg" :size="14" />
          </button>
          <button
            v-else-if="canAct(item) && actionMode === 'deprecate'"
            type="button"
            class="admin-overview-item__icon-btn admin-overview-item__delete"
            :title="actionTitle(item)"
            :aria-label="actionTitle(item)"
            :disabled="deleteDisabled"
            @click="onDelete($event, item.id)"
          >
            <span class="admin-overview-item__times" aria-hidden="true">&times;</span>
          </button>
          <button
            v-else-if="canAct(item)"
            type="button"
            class="admin-overview-item__icon-btn admin-overview-item__delete"
            :title="actionTitle(item)"
            :aria-label="actionTitle(item)"
            :disabled="deleteDisabled"
            @click="onDelete($event, item.id)"
          >
            <SvgIcon src="/frontend/icons/delete.svg" :size="14" />
          </button>
        </div>
      </div>
    </template>
  </div>
</template>
