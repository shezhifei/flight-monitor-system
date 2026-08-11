<script setup lang="ts">
import type { LabelDefinition } from '../../../types/backend';

interface Props {
  labels: LabelDefinition[];
  loading?: boolean;
  isAdmin?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  loading: false,
  isAdmin: false,
});

const emit = defineEmits<{
  (e: 'edit', label: LabelDefinition): void;
  (e: 'delete', label: LabelDefinition): void;
}>();

const scopeLabels: Record<string, string> = {
  flight: '航班级',
  leg: '航段级',
  both: '两者',
};
</script>

<template>
  <table>
    <thead>
      <tr>
        <th>颜色</th>
        <th>代码</th>
        <th>名称</th>
        <th>范围</th>
        <th>类型</th>
        <th>状态</th>
        <th>操作</th>
      </tr>
    </thead>
    <tbody>
      <tr v-if="props.loading && props.labels.length === 0">
        <td colspan="7" class="empty-placeholder">
          加载中...
        </td>
      </tr>
      <tr v-else-if="props.labels.length === 0">
        <td colspan="7" class="empty-placeholder">
          暂无标签
        </td>
      </tr>
      <tr v-for="label in props.labels" :key="label.label_id">
        <td>
          <span class="color-dot" :style="{ background: label.color }" />
        </td>
        <td>
          <code class="code-badge">{{ label.code }}</code>
        </td>
        <td>
          <span class="label-name">
            <span v-if="label.icon" class="label-icon">{{ label.icon }}</span>
            {{ label.name }}
          </span>
        </td>
        <td>{{ scopeLabels[label.scope] || label.scope }}</td>
        <td>
          <span
            class="badge"
            :class="label.category === 'system' ? 'badge-info' : 'badge-warning'"
          >
            {{ label.category === 'system' ? '系统' : '自定义' }}
          </span>
        </td>
        <td>
          <span
            class="badge"
            :class="label.is_active ? 'badge-success' : 'badge-muted'"
          >
            {{ label.is_active ? '启用' : '停用' }}
          </span>
        </td>
        <td>
          <button
            type="button"
            class="btn btn-secondary btn-sm"
            :disabled="!props.isAdmin"
            @click="emit('edit', label)"
          >
            编辑
          </button>
          <button
            v-if="label.category !== 'system'"
            type="button"
            class="btn btn-danger btn-sm"
            :disabled="!props.isAdmin"
            @click="emit('delete', label)"
          >
            删除
          </button>
        </td>
      </tr>
    </tbody>
  </table>
</template>

<style scoped>
/* 表格壳层 / 按钮 / badge 用 admin-page；仅保留单元格辅助 */

.color-dot {
  display: inline-block;
  width: 20px;
  height: 20px;
  border-radius: 6px;
  border: 1px solid var(--admin-border);
  vertical-align: middle;
}

.code-badge {
  display: inline-block;
  padding: 2px 8px;
  border-radius: 6px;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 12px;
  background: var(--ws-surface-muted);
  color: var(--admin-text);
  border: 1px solid var(--admin-border);
}

.label-name {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.label-icon {
  font-size: 15px;
}

.badge-muted {
  background: var(--ws-surface-muted);
  color: var(--admin-text-muted);
}

.empty-placeholder {
  text-align: center;
  padding: 40px 16px !important;
  color: var(--admin-text-muted);
}

.btn + .btn {
  margin-left: 6px;
}
</style>
