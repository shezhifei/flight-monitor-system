<script setup lang="ts">
import { ref, watch } from 'vue';
import { useLabelManager } from '../composables/useLabelManager';
import { useAuth } from '@/composables/useAuth';
import { useToast } from '@/composables/useToast';
import SvgIcon from '@/components/ui/SvgIcon.vue';
import LabelTable from './LabelTable.vue';
import LabelFormDialog from './LabelFormDialog.vue';
import type { LabelDefinition, CreateLabelRequest, UpdateLabelRequest } from '@/types/backend';

const props = withDefaults(
  defineProps<{
    /** 面板激活时再拉数，避免合页后无谓请求 */
    active?: boolean;
  }>(),
  { active: true },
);

const auth = useAuth();
const toast = useToast();
const isAdmin = () => auth.isAdmin();

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
} = useLabelManager({ autoLoad: false });

const showDialog = ref(false);
const editingLabel = ref<LabelDefinition | null>(null);
const saving = ref(false);
const loadedOnce = ref(false);

watch(
  () => props.active,
  async (active) => {
    if (!active || loadedOnce.value) return;
    loadedOnce.value = true;
    await refreshLabels();
  },
  { immediate: true },
);

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
    toast.showToast('error', `标签保存失败: ${e instanceof Error ? e.message : String(e)}`, {
      duration: 5000,
    });
  } finally {
    saving.value = false;
  }
}

async function handleDelete(label: LabelDefinition) {
  if (!confirm(`确定删除标签「${label.name}」吗?`)) return;
  try {
    await deleteLabel(label.label_id);
    toast.showToast('success', '标签已删除');
  } catch (e) {
    toast.showToast('error', `标签删除失败: ${e instanceof Error ? e.message : String(e)}`, {
      duration: 5000,
    });
  }
}
</script>

<template>
  <div class="label-panel">
    <!-- 搜索 + 筛选 + 操作：单行，少占视高 -->
    <div class="label-toolbar">
      <div class="search-group label-toolbar__search">
        <span class="search-icon" aria-hidden="true">
          <SvgIcon src="/frontend/icons/search.svg" :size="16" />
        </span>
        <input
          v-model="searchQuery"
          type="search"
          class="search-input"
          placeholder="搜索标签代码或名称…"
          aria-label="搜索标签"
          autocomplete="off"
        >
      </div>
      <select v-model="scopeFilter" class="filter-select label-toolbar__select" aria-label="范围筛选">
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
      <select v-model="categoryFilter" class="filter-select label-toolbar__select" aria-label="类型筛选">
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
      <div class="label-toolbar__actions">
        <button type="button" class="btn btn-secondary btn-sm" :disabled="loading" @click="refreshLabels">
          刷新
        </button>
        <button
          v-if="isAdmin()"
          type="button"
          class="btn btn-primary btn-sm"
          @click="openCreateDialog"
        >
          + 新建标签
        </button>
      </div>
    </div>

    <div v-if="error" class="error-banner" role="alert">
      <span>{{ error }}</span>
      <button type="button" class="btn btn-secondary btn-sm" @click="refreshLabels">
        重试
      </button>
    </div>

    <div class="table-container">
      <LabelTable
        :labels="filteredLabels"
        :loading="loading"
        :is-admin="isAdmin()"
        @edit="openEditDialog"
        @delete="handleDelete"
      />
    </div>

    <LabelFormDialog
      :visible="showDialog"
      :label="editingLabel"
      :loading="saving"
      @close="closeDialog"
      @save="handleSave"
    />
  </div>
</template>

<style scoped>
.label-panel {
  display: flex;
  flex-direction: column;
  min-height: 0;
}

/* 强制单行：搜索 | 范围 | 类型 | 按钮 */
.label-toolbar {
  display: flex;
  flex-wrap: nowrap;
  align-items: center;
  gap: 10px;
  margin-bottom: 16px;
  min-width: 0;
}

.label-toolbar__search {
  flex: 1 1 auto;
  width: auto !important;
  min-width: 140px !important;
  max-width: 360px;
}

.label-toolbar__select {
  flex: 0 0 auto;
  width: auto;
  min-width: 112px;
  max-width: 148px;
}

.label-toolbar__actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 0 0 auto;
  margin-left: auto;
}

.error-banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 16px;
  padding: 10px 14px;
  border-radius: 10px;
  font-size: 13px;
  background: var(--error-bg-subtle);
  border: 1px solid var(--error-border-subtle);
  color: var(--ws-danger);
}

@media (max-width: 720px) {
  .label-toolbar {
    flex-wrap: wrap;
  }

  .label-toolbar__search {
    flex: 1 1 100%;
    max-width: none;
  }

  .label-toolbar__actions {
    margin-left: 0;
  }
}
</style>
