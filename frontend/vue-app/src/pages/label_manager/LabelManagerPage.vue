<script setup lang="ts">
import { ref, computed } from 'vue';
import { useLabelManager } from './composables/useLabelManager';
import { useAuth } from '@/composables/useAuth';
import { useToast } from '@/composables/useToast';
import LabelTable from './components/LabelTable.vue';
import LabelFormDialog from './components/LabelFormDialog.vue';
import type { LabelDefinition, CreateLabelRequest, UpdateLabelRequest } from '../../types/backend';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import UiButton from '@/components/ui/UiButton.vue';
import UiSelect from '@/components/ui/UiSelect.vue';

const scopeOptions = [
  { value: '', label: '全部范围' },
  { value: 'flight', label: '航班级' },
  { value: 'leg', label: '航段级' },
  { value: 'both', label: '两者' },
];

const categoryOptions = [
  { value: '', label: '全部类型' },
  { value: 'system', label: '系统' },
  { value: 'custom', label: '自定义' },
];

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
        <UiButton variant="primary" @click="openCreateDialog">
          <span class="btn-icon">+</span>
          新建标签
        </UiButton>
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
        <UiSelect
          v-model="scopeFilter"
          :options="scopeOptions"
          label="按适用范围筛选"
        />
        <UiSelect
          v-model="categoryFilter"
          :options="categoryOptions"
          label="按标签类型筛选"
        />
      </div>
    </div>

    <div v-if="error" class="error-message">
      {{ error }}
      <UiButton variant="danger" size="sm" @click="refreshLabels">
        重试
      </UiButton>
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
/* 信号面：admin 内容页，页面标题用展示级 24px，其余走梯子 */
.label-manager-page {
  padding: var(--s4);
  max-width: 1200px;
  margin: 0 auto;
}

.page-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  margin-bottom: var(--s5);
  padding-bottom: var(--s4);
  border-bottom: 1px solid var(--line);
}

.header-content {
  flex: 1;
}

.page-title {
  margin: 0 0 var(--s2) 0;
  font-size: 24px;
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.page-subtitle {
  margin: 0;
  font-size: var(--fs-section);
  color: var(--ink-subtle);
}

.header-actions {
  flex-shrink: 0;
}

.toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--s3);
  margin-bottom: var(--s3);
  flex-wrap: wrap;
}

.search-group {
  display: flex;
  align-items: center;
  gap: var(--s2);
  background: var(--face-work);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-control);
  padding: 0 var(--s3);
  height: var(--h-sm);
  flex: 1;
  max-width: 400px;
  transition: border-color var(--t-fast) var(--ease);
}

.search-group:focus-within {
  border-color: var(--act);
}

.search-icon {
  color: var(--ink-muted);
  display: flex;
  align-items: center;
}

.search-input {
  border: none;
  outline: none;
  flex: 1;
  min-width: 0;
  font-size: var(--fs-section);
  font-family: inherit;
  color: var(--ink);
  background: transparent;
}

.search-input::placeholder {
  color: var(--ink-muted);
}

.filter-group {
  display: flex;
  gap: var(--s3);
}

.error-message {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s3);
  padding: var(--s3) var(--s4);
  background: var(--danger-soft);
  border: 1px solid color-mix(in srgb, var(--danger) 35%, transparent);
  border-radius: var(--r-control);
  color: var(--danger);
  font-size: var(--fs-section);
  margin-bottom: var(--s3);
}

.btn-icon {
  font-size: var(--fs-title);
  line-height: 1;
}
</style>
