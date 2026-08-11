<script setup lang="ts">
import { ref, computed } from 'vue';
import { useLabelManager } from './composables/useLabelManager';
import { useAuth } from '@/composables/useAuth';
import { useToast } from '@/composables/useToast';
import LabelTable from './components/LabelTable.vue';
import LabelFormDialog from './components/LabelFormDialog.vue';
import type { LabelDefinition, CreateLabelRequest, UpdateLabelRequest } from '../../types/backend';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';

const auth = useAuth();
const toast = useToast();
const isAdmin = computed(() => auth.isAdmin());

const {
  filteredLabels,
  loading,
  error,
  searchQuery,
  scopeFilter,
  categoryFilter,
  createLabel,
  updateLabel,
  deleteLabel,
  refreshLabels,
} = useLabelManager();

const showDialog = ref(false);
const editingLabel = ref<LabelDefinition | null>(null);
const saving = ref(false);

function openCreateDialog() {
  editingLabel.value = null;
  showDialog.value = true;
}

function openEditDialog(label: LabelDefinition) {
  editingLabel.value = label;
  showDialog.value = true;
}

function closeDialog() {
  showDialog.value = false;
  editingLabel.value = null;
}

async function handleSave(data: CreateLabelRequest | UpdateLabelRequest) {
  saving.value = true;
  try {
    if (editingLabel.value) {
      await updateLabel(editingLabel.value.label_id, data as UpdateLabelRequest);
      toast.showToast('success', '标签已更新');
    } else {
      await createLabel(data as CreateLabelRequest);
      toast.showToast('success', '标签已创建');
    }
    closeDialog();
  } catch (e) {
    console.error('Failed to save label:', e);
    toast.showToast('error', `标签保存失败: ${e instanceof Error ? e.message : String(e)}`, { duration: 5000 });
  } finally {
    saving.value = false;
  }
}

async function handleDelete(label: LabelDefinition) {
  if (!confirm(`确定删除标签"${label.name}"吗?`)) {
    return;
  }
  try {
    await deleteLabel(label.label_id);
    toast.showToast('success', '标签已删除');
  } catch (e) {
    console.error('Failed to delete label:', e);
    toast.showToast('error', `标签删除失败: ${e instanceof Error ? e.message : String(e)}`, { duration: 5000 });
  }
}
</script>

<template>
  <div class="label-manager-page">
    <div class="page-header">
      <div class="header-content">
        <h1 class="page-title">
          标签定义管理
        </h1>
        <p class="page-subtitle">
          维护航班标签模板，支持航班级和航段级标签
        </p>
      </div>
      <div v-if="isAdmin" class="header-actions">
        <button class="btn btn-primary" @click="openCreateDialog">
          <span class="btn-icon">+</span>
          新建标签
        </button>
      </div>
    </div>

    <div class="toolbar">
      <div class="search-group">
        <span class="search-icon">
          <svg
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <circle cx="11" cy="11" r="8" />
            <path d="m21 21-4.3-4.3" />
          </svg>
        </span>
        <input
          v-model="searchQuery"
          type="text"
          class="search-input"
          placeholder="搜索标签代码或名称..."
        >
      </div>
      <div class="filter-group">
        <select v-model="scopeFilter" class="filter-select">
          <option value="">
            全部范围
          </option>
          <option value="flight">
            航班级
          </option>
          <option value="leg">
            航段级
          </option>
          <option value="both">
            两者
          </option>
        </select>
        <select v-model="categoryFilter" class="filter-select">
          <option value="">
            全部类型
          </option>
          <option value="system">
            系统
          </option>
          <option value="custom">
            自定义
          </option>
        </select>
      </div>
    </div>

    <div v-if="error" class="error-message">
      {{ error }}
      <button class="retry-btn" @click="refreshLabels">
        重试
      </button>
    </div>

    <LabelTable
      :labels="filteredLabels"
      :loading="loading"
      :is-admin="isAdmin"
      @edit="openEditDialog"
      @delete="handleDelete"
    />

    <LabelFormDialog
      :visible="showDialog"
      :label="editingLabel"
      :loading="saving"
      @close="closeDialog"
      @save="handleSave"
    />
    <ThemeToggle />
  </div>
</template>

<style scoped>
.label-manager-page {
  padding: 24px;
  max-width: 1200px;
  margin: 0 auto;
}

.page-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  margin-bottom: 32px;
  padding-bottom: 24px;
  border-bottom: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
}

.header-content {
  flex: 1;
}

.page-title {
  margin: 0 0 8px 0;
  font-size: 24px;
  font-weight: 600;
  color: var(--text-primary, #111827);
}

.page-subtitle {
  margin: 0;
  font-size: 14px;
  color: var(--text-secondary, #546E7A);
}

.header-actions {
  flex-shrink: 0;
}

.toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
  margin-bottom: 16px;
  flex-wrap: wrap;
}

.search-group {
  display: flex;
  align-items: center;
  gap: 8px;
  background: var(--bg-primary, #fff);
  border: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
  border-radius: 8px;
  padding: 8px 12px;
  flex: 1;
  max-width: 400px;
}

.search-icon {
  color: var(--text-secondary, #9ca3af);
  display: flex;
  align-items: center;
}

.search-input {
  border: none;
  outline: none;
  flex: 1;
  font-size: 14px;
  background: transparent;
}

.filter-group {
  display: flex;
  gap: 12px;
}

.filter-select {
  padding: 8px 12px;
  border: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
  border-radius: 8px;
  font-size: 14px;
  background: var(--bg-primary, #fff);
  color: var(--text-primary, #374151);
  cursor: pointer;
  min-width: 120px;
}

.filter-select:focus {
  outline: none;
  border-color: var(--primary-color, #3b82f6);
}

.error-message {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  background: var(--error-bg-subtle);
  border: 1px solid var(--error-border-subtle);
  border-radius: 8px;
  color: var(--system-red);
  font-size: 14px;
  margin-bottom: 16px;
}

.retry-btn {
  padding: 4px 12px;
  background: var(--system-red);
  color: var(--text-inverse);
  border: none;
  border-radius: 4px;
  font-size: 12px;
  cursor: pointer;
}

.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 10px 20px;
  border-radius: 8px;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  border: none;
  transition: all 0.2s;
}

.btn-primary {
  background: var(--primary-color, #3b82f6);
  color: var(--text-inverse);
}

.btn-primary:hover {
  background: var(--primary-hover, #2563eb);
}

.btn-icon {
  margin-right: 6px;
  font-size: 16px;
}
</style>
