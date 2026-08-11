<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import AdminMasterDetail from '@/components/admin/AdminMasterDetail.vue';
import AdminOverviewList from '@/components/admin/AdminOverviewList.vue';
import AdminOverviewTools from '@/components/admin/AdminOverviewTools.vue';
import type { AdminOverviewItem } from '@/components/admin/adminOverviewTypes';
import type {
  RequirementVersionResponse,
  TaskTypeResponse,
  TaskTypeCreatePayload,
} from './dispatchRuleWorkbenchApi';

const props = withDefaults(
  defineProps<{
    taskTypes: TaskTypeResponse[];
    requirementVersions: RequirementVersionResponse[];
    selectedTaskTypeCode: string;
    saving: boolean;
    disabled: boolean;
    disabledReason?: string;
    showCreateForm?: boolean;
  }>(),
  {
    disabledReason: undefined,
    showCreateForm: false,
  },
);

const emit = defineEmits<{
  (e: 'select', code: string): void;
  (e: 'create', payload: TaskTypeCreatePayload): void;
  (e: 'delete', code: string): void;
  (e: 'update:showCreateForm', value: boolean): void;
}>();

type Tab = 'rules' | 'requirements' | 'history';

const activeTab = ref<Tab>('rules');
const searchQuery = ref('');

function createDefaultDraft(): TaskTypeCreatePayload {
  return {
    code: '',
    name: '',
    category: '',
    default_duration_minutes: 30,
    trigger_offset_minutes: 30,
    trigger_type: 'before_eta',
    description: '',
  };
}

const draft = ref<TaskTypeCreatePayload>(createDefaultDraft());

watch(
  () => props.showCreateForm,
  (open) => {
    if (!open) draft.value = createDefaultDraft();
  },
);

const filteredTaskTypes = computed(() => {
  const query = searchQuery.value.trim().toLowerCase();
  if (!query) return props.taskTypes;
  return props.taskTypes.filter(
    (t) => t.name.toLowerCase().includes(query) || t.code.toLowerCase().includes(query),
  );
});

const overviewItems = computed<AdminOverviewItem[]>(() =>
  filteredTaskTypes.value.map((t) => ({
    id: t.code,
    title: t.name,
    meta: `${t.code} · ${t.category || '未分类'}`,
    deletable: true,
  })),
);

const selectedTaskType = computed(() =>
  props.taskTypes.find((t) => t.code === props.selectedTaskTypeCode) ?? null,
);

const versionsForSelectedTaskType = computed(() => {
  if (!selectedTaskType.value) return [];
  return props.requirementVersions.filter(
    (v) => v.task_type === selectedTaskType.value!.code,
  );
});

function submitCreate(): void {
  if (!draft.value.code.trim() || !draft.value.name.trim()) return;
  emit('create', { ...draft.value });
}

function confirmDelete(code: string): void {
  if (!window.confirm(`确认删除任务类型 ${code}? 此操作不可恢复。`)) return;
  emit('delete', code);
}

function toggleCreateForm(): void {
  emit('update:showCreateForm', !props.showCreateForm);
}
</script>

<template>
  <div class="task-type-workbench">
    <div class="section-toolbar">
      <div class="filter-group">
        <AdminOverviewTools
          v-model="searchQuery"
          placeholder="搜索任务类型编码或名称…"
          search-aria-label="搜索任务类型"
          :show-create="false"
        />
      </div>
      <button
        type="button"
        class="btn btn-primary btn-sm"
        :disabled="disabled || saving"
        @click="toggleCreateForm"
      >
        {{ props.showCreateForm ? '取消新增' : '新增任务类型' }}
      </button>
    </div>

    <p v-if="disabled && disabledReason" class="disabled-note">
      {{ disabledReason }}
    </p>

    <form v-if="props.showCreateForm" class="panel create-form" @submit.prevent="submitCreate">
      <div class="form-grid">
        <label>
          <span>编码</span>
          <input
            v-model="draft.code"
            type="text"
            required
            maxlength="40"
            placeholder="如 TOWING"
          >
        </label>
        <label>
          <span>名称</span>
          <input
            v-model="draft.name"
            type="text"
            required
            maxlength="80"
            placeholder="如 拖飞机"
          >
        </label>
        <label>
          <span>分类</span>
          <input
            v-model="draft.category"
            type="text"
            maxlength="40"
            placeholder="可选"
          >
        </label>
        <label>
          <span>默认时长 (分)</span>
          <input
            v-model.number="draft.default_duration_minutes"
            type="number"
            min="0"
            max="600"
          >
        </label>
        <label>
          <span>触发偏移 (分)</span>
          <input
            v-model.number="draft.trigger_offset_minutes"
            type="number"
            min="0"
            max="720"
          >
        </label>
        <label>
          <span>触发类型</span>
          <select v-model="draft.trigger_type">
            <option value="before_eta">
              起飞前
            </option>
            <option value="after_etd">
              起飞后
            </option>
            <option value="manual">
              人工触发
            </option>
          </select>
        </label>
      </div>
      <label class="full-row">
        <span>说明</span>
        <textarea
          v-model="draft.description"
          rows="2"
          maxlength="200"
          placeholder="可选说明"
        />
      </label>
      <div class="form-actions">
        <button type="submit" class="btn btn-primary btn-sm" :disabled="saving">
          {{ saving ? '保存中…' : '保存任务类型' }}
        </button>
      </div>
    </form>

    <AdminMasterDetail list-aria-label="任务类型列表" detail-aria-label="任务类型详情">
      <template #list>
        <AdminOverviewList
          :items="overviewItems"
          :selected-id="selectedTaskTypeCode || null"
          empty-text="暂无任务类型"
          aria-label="任务类型列表"
          show-delete
          :delete-disabled="disabled || saving"
          delete-title="删除任务类型"
          @select="emit('select', $event)"
          @delete="confirmDelete"
        />
      </template>

      <template #detail>
        <template v-if="!selectedTaskType">
          <div class="empty-state">
            <div class="empty-state-title">
              请选择任务类型
            </div>
            <p>从左侧列表选择一项，查看规则与资质配置。</p>
          </div>
        </template>
        <template v-else>
          <div class="detail-head">
            <h3>
              {{ selectedTaskType.name }}
              <span class="muted">({{ selectedTaskType.code }})</span>
            </h3>
            <p class="muted">
              默认时长 {{ selectedTaskType.default_duration_minutes ?? '-' }} 分钟
              · 触发 {{ selectedTaskType.trigger_type }}
              · 偏移 {{ selectedTaskType.trigger_offset_minutes }} 分
            </p>
          </div>

          <div class="inner-tabs" role="tablist">
            <button
              v-for="tab in (['rules', 'requirements', 'history'] as Tab[])"
              :key="tab"
              type="button"
              role="tab"
              class="inner-tab"
              :class="{ active: activeTab === tab }"
              :aria-selected="activeTab === tab"
              @click="activeTab = tab"
            >
              {{ tab === 'rules' ? '规则配置' : tab === 'requirements' ? '资质要求' : '历史记录' }}
            </button>
          </div>

          <div v-if="activeTab === 'rules'" class="tab-body">
            <slot name="rules" :task-type="selectedTaskType" />
          </div>
          <div v-else-if="activeTab === 'requirements'" class="tab-body">
            <slot name="requirements" :task-type="selectedTaskType" />
          </div>
          <div v-else class="tab-body">
            <p class="muted">
              资质要求版本历史 — 共 {{ versionsForSelectedTaskType.length }} 条
            </p>
            <div v-if="versionsForSelectedTaskType.length" class="table-container history-table">
              <table>
                <thead>
                  <tr>
                    <th>状态</th>
                    <th>版本</th>
                    <th>时间</th>
                    <th>备注</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="v in versionsForSelectedTaskType" :key="v.id">
                    <td>
                      <span class="badge" :class="v.status === 'published' ? 'badge-success' : 'badge-warning'">
                        {{ v.status }}
                      </span>
                    </td>
                    <td>v{{ v.version_no }}</td>
                    <td>
                      {{
                        v.published_at
                          ? new Date(v.published_at).toLocaleString()
                          : (v.created_at ? new Date(v.created_at).toLocaleString() : '—')
                      }}
                    </td>
                    <td>{{ v.notes || '—' }}</td>
                  </tr>
                </tbody>
              </table>
            </div>
            <div v-else class="empty-state">
              <div class="empty-state-title">
                暂无历史版本
              </div>
            </div>
          </div>
        </template>
      </template>
    </AdminMasterDetail>
  </div>
</template>

<style scoped>
/* 壳层 toolbar / panel / table / btn / master-detail 走 admin-page */

.task-type-workbench {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-height: 0;
}

.disabled-note {
  margin: 0;
  font-size: 12px;
  color: var(--ws-warn);
  background: var(--dh-signal-warn-soft);
  border: 1px solid color-mix(in srgb, var(--ws-warn) 35%, transparent);
  padding: 8px 12px;
  border-radius: 10px;
}

.create-form {
  padding: 16px 18px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.form-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 12px;
}

.form-grid label,
.full-row {
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-size: 12px;
  font-weight: 600;
  color: var(--admin-text-subtle);
}

.form-grid input,
.form-grid select,
.full-row textarea {
  min-height: 36px;
  padding: 6px 10px;
  border: 1px solid var(--admin-border);
  border-radius: 8px;
  font-size: 13px;
  font-weight: 500;
  color: var(--admin-text);
  background: var(--admin-card-bg);
  font-family: inherit;
}

.full-row textarea {
  min-height: 64px;
  resize: vertical;
}

.form-actions {
  display: flex;
  justify-content: flex-end;
}

.muted {
  font-size: 11px;
  color: var(--admin-text-muted);
  font-weight: 500;
}

.detail-head h3 {
  margin: 0;
  font-size: 18px;
  font-weight: 700;
  color: var(--admin-text);
  letter-spacing: -0.02em;
}

.detail-head p {
  margin: 4px 0 0;
}

.inner-tabs {
  display: flex;
  gap: 16px;
  border-bottom: 1px solid var(--admin-border);
  margin: 0 -2px;
}

.inner-tab {
  padding: 10px 2px 12px;
  border: none;
  background: none;
  cursor: pointer;
  font-size: 13px;
  font-weight: 600;
  color: var(--admin-text-subtle);
  border-bottom: 2px solid transparent;
  margin-bottom: -1px;
}

.inner-tab:hover {
  color: var(--admin-text);
}

.inner-tab.active {
  color: var(--ws-primary);
  border-bottom-color: var(--ws-primary);
}

.tab-body {
  min-height: 200px;
  flex: 1;
  min-width: 0;
}

.history-table {
  margin-top: 8px;
}

@media (max-width: 1024px) {
  .form-grid {
    grid-template-columns: 1fr;
  }
}
</style>
