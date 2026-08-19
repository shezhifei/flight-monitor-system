<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { pageUrl } from '@/shared/page-routes';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import SvgIcon from '@/components/ui/SvgIcon.vue';
import { useAuth } from '@/composables/useAuth';
import { useToast } from '@/composables/useToast';
import LabelManagerPanel from '@/pages/label_manager/components/LabelManagerPanel.vue';
import DepartmentSelector from './DepartmentSelector.vue';
import TaskTypePanel from './TaskTypePanel.vue';
import GenerationRulePanel from './GenerationRulePanel.vue';
import AdjustmentRulePanel from './AdjustmentRulePanel.vue';
import RequirementVersionPanel from './RequirementVersionPanel.vue';
import RulePreviewPanel from './RulePreviewPanel.vue';
import ManualOrderPanel from './ManualOrderPanel.vue';
import RuleExportDrawer from './RuleExportDrawer.vue';
import {
  useDispatchRuleWorkbench,
  type WorkbenchPanel,
} from './useDispatchRuleWorkbench';
import type {
  FlightGenerationRulePayload,
  GenerationAdjustmentRulePayload,
  RequirementDraftPayload,
  RequirementPublishPayload,
  TaskTypeCreatePayload,
} from './dispatchRuleWorkbenchApi';

type PageSection = 'rules' | 'labels';

const toast = useToast();
const auth = useAuth();

const {
  departments,
  taskTypes,
  equipmentTypes,
  bundle,
  selectedDepartmentId,
  selectedDepartment,
  selectedTaskTypeCode,
  activePanel,
  loading,
  saving,
  bootstrapped,
  error,
  dirtyState,
  dirtyCount,
  previewResult,
  manualOrderDraft,
  lastCreatedOrder,
  lastSnapshotAt,
  isAggregateView,
  bootstrap,
  selectDepartment,
  refreshDepartmentBundle,
  createTaskType,
  deleteTaskType,
  saveGenerationRule,
  deleteGenerationRule,
  saveAdjustmentRule,
  saveRequirementDraft,
  publishRequirementDraft,
  runPreview,
  createManualOrder,
  exportSnapshotJson,
  copySnapshotJson,
  markDirty,
  selectTaskType,
  setActivePanel,
} = useDispatchRuleWorkbench();

const drawerOpen = ref(false);
const exporting = ref(false);
const showTaskTypeCreateForm = ref(false);
const activeSection = ref<PageSection>('rules');

const sidebarUser = computed(() => {
  const user = auth.getUser();
  const name = user?.username || 'Admin';
  const role = user?.is_admin ? '系统管理员' : (user?.role || '普通用户');
  const avatar = name.trim().charAt(0).toUpperCase() || 'A';
  return { name, role, avatar };
});

function readSectionFromUrl(): PageSection {
  try {
    const params = new URLSearchParams(window.location.search);
    const raw = (params.get('section') || params.get('tab') || '').toLowerCase();
    if (raw === 'labels' || raw === 'label' || raw === 'tags') return 'labels';
  } catch {
    // ignore
  }
  return 'rules';
}

function syncSectionToUrl(section: PageSection): void {
  try {
    const url = new URL(window.location.href);
    if (section === 'rules') {
      url.searchParams.delete('section');
      url.searchParams.delete('tab');
    } else {
      url.searchParams.set('section', section);
    }
    window.history.replaceState({}, '', url.toString());
  } catch {
    // ignore
  }
}

function switchSection(section: PageSection): void {
  activeSection.value = section;
  syncSectionToUrl(section);
}

function handleLogout(): void {
  auth.logout();
}

onMounted(async () => {
  activeSection.value = readSectionFromUrl();
  try {
    await bootstrap();
  } catch (e) {
    toast.showToast('error', e instanceof Error ? e.message : '初始化工作台失败');
  }
});

watch(activeSection, (section) => {
  syncSectionToUrl(section);
});

async function handleSelectDepartment(id: string): Promise<void> {
  try {
    await selectDepartment(id);
  } catch (e) {
    toast.showToast('error', e instanceof Error ? e.message : '加载科室失败');
  }
}

async function handleRefresh(): Promise<void> {
  try {
    await refreshDepartmentBundle();
    toast.showToast('success', '科室规则已刷新');
  } catch (e) {
    toast.showToast('error', e instanceof Error ? e.message : '刷新失败');
  }
}

async function handleCreateTaskType(payload: TaskTypeCreatePayload): Promise<void> {
  try {
    await createTaskType(payload);
    showTaskTypeCreateForm.value = false;
    toast.showToast('success', '已新增任务类型');
  } catch (e) {
    toast.showToast('error', e instanceof Error ? e.message : '新增任务类型失败');
  }
}

async function handleDeleteTaskType(code: string): Promise<void> {
  try {
    await deleteTaskType(code);
    toast.showToast('success', '已删除任务类型');
  } catch (e) {
    toast.showToast('error', e instanceof Error ? e.message : '删除任务类型失败');
  }
}

async function handleSaveGenerationRule(payload: FlightGenerationRulePayload): Promise<void> {
  try {
    await saveGenerationRule(payload);
    toast.showToast('success', '航班生成规则已保存');
  } catch (e) {
    toast.showToast('error', e instanceof Error ? e.message : '保存失败');
  }
}

async function handleDeleteGenerationRule(ruleId: string): Promise<void> {
  try {
    await deleteGenerationRule(ruleId);
    toast.showToast('success', '已删除生成规则');
  } catch (e) {
    toast.showToast('error', e instanceof Error ? e.message : '删除失败');
  }
}

async function handleSaveAdjustmentRule(payload: GenerationAdjustmentRulePayload): Promise<void> {
  try {
    await saveAdjustmentRule(payload);
    toast.showToast('success', '调整规则已保存');
  } catch (e) {
    toast.showToast('error', e instanceof Error ? e.message : '保存失败');
  }
}

async function handleSaveRequirementDraft(payload: RequirementDraftPayload): Promise<void> {
  try {
    await saveRequirementDraft(payload);
    toast.showToast('success', '资质要求草稿已保存');
  } catch (e) {
    toast.showToast('error', e instanceof Error ? e.message : '保存失败');
  }
}

async function handlePublishRequirement(payload: RequirementPublishPayload): Promise<void> {
  try {
    await publishRequirementDraft(payload);
    toast.showToast('success', '资质要求已发布');
  } catch (e) {
    toast.showToast('error', e instanceof Error ? e.message : '发布失败');
  }
}

async function handlePreview(payload: {
  flight_id?: string | null;
  sample_flight: Record<string, unknown>;
}): Promise<void> {
  try {
    await runPreview(payload);
    toast.showToast('success', '规则预览完成');
  } catch (e) {
    toast.showToast('error', e instanceof Error ? e.message : '预览失败');
  }
}

async function handleCreateManualOrder(): Promise<void> {
  try {
    const result = await createManualOrder();
    toast.showToast('success', `派工单已创建 ${result.id ?? ''}`);
  } catch (e) {
    toast.showToast('error', e instanceof Error ? e.message : '创建派工单失败');
  }
}

async function handleCopySnapshot(): Promise<string> {
  exporting.value = true;
  try {
    return await copySnapshotJson();
  } finally {
    exporting.value = false;
  }
}

async function handleDownloadSnapshot(): Promise<string> {
  exporting.value = true;
  try {
    return await exportSnapshotJson();
  } finally {
    exporting.value = false;
  }
}

function buildSnapshotForPreview() {
  return {
    department_id: selectedDepartment.value?.id ?? null,
    department_name: selectedDepartment.value?.name ?? null,
    task_types_count: taskTypes.value.length,
    equipment_types_count: equipmentTypes.value.length,
    bundle: bundle.value,
  };
}

const dirtyMessages = computed(() => {
  const items: string[] = [];
  if (dirtyState.generationDraft) items.push('航班生成规则草稿');
  if (dirtyState.adjustmentDraft) items.push('调整规则草稿');
  if (dirtyState.requirementDraft) items.push('资质要求草稿');
  if (dirtyState.manualOrderDraft) items.push('人工派工草稿');
  return items;
});

const panels: { id: WorkbenchPanel; label: string }[] = [
  { id: 'generation', label: '生成规则' },
  { id: 'adjustment', label: '调整规则' },
  { id: 'requirement', label: '资质要求' },
  { id: 'preview', label: '规则预览' },
  { id: 'manualOrder', label: '人工派工' },
];

const lastOrderId = computed(() => lastCreatedOrder.value?.id ?? null);

const pageTitle = computed(() =>
  activeSection.value === 'labels' ? '标签定义' : '派工规则',
);

const pageSubtitle = computed(() => {
  if (activeSection.value === 'labels') {
    return '维护航班/航段标签模板（系统与自定义）';
  }
  const snap = lastSnapshotAt.value
    ? ` · 上次导出 ${new Date(lastSnapshotAt.value).toLocaleString()}`
    : '';
  return `按科室管理任务类型、生成规则、调整规则、资质要求与人工派工${snap}`;
});
</script>

<template>
  <div class="admin-container dispatch-rule-page">
    <aside class="admin-sidebar">
      <div class="sidebar-header">
        <div class="sidebar-logo">
          <SvgIcon src="/frontend/icons/settings.svg" :size="20" />
          <span>规则与标签</span>
        </div>
      </div>

      <nav class="sidebar-nav">
        <div class="nav-section">
          <div class="nav-section-title">
            派工
          </div>
          <button
            type="button"
            class="nav-item"
            :class="{ active: activeSection === 'rules' }"
            @click="switchSection('rules')"
          >
            <span class="nav-item-icon"><SvgIcon src="/frontend/icons/settings.svg" /></span>
            <span>派工规则</span>
          </button>
        </div>

        <div class="nav-section">
          <div class="nav-section-title">
            业务标签
          </div>
          <button
            type="button"
            class="nav-item"
            :class="{ active: activeSection === 'labels' }"
            @click="switchSection('labels')"
          >
            <span class="nav-item-icon"><SvgIcon src="/frontend/icons/detail.svg" /></span>
            <span>标签定义</span>
          </button>
        </div>
      </nav>

      <div class="sidebar-footer">
        <div class="user-info">
          <div class="user-avatar">
            {{ sidebarUser.avatar }}
          </div>
          <div class="user-details">
            <div class="user-name">
              {{ sidebarUser.name }}
            </div>
            <div class="user-role">
              {{ sidebarUser.role }}
            </div>
          </div>
        </div>
        <div class="sidebar-footer-actions">
          <ThemeToggle />
          <button
            type="button"
            class="logout-btn"
            title="退出登录"
            @click="handleLogout"
          >
            <SvgIcon src="/frontend/icons/logout.svg" :size="14" />
          </button>
          <a :href="pageUrl('dashboard')" class="nav-item sidebar-home-link">
            <span class="nav-item-icon"><SvgIcon src="/frontend/icons/home.svg" /></span>
            <span>返回工作台</span>
          </a>
        </div>
      </div>
    </aside>

    <main class="main-content">
      <header class="content-header">
        <div class="content-heading">
          <div class="content-title">
            {{ pageTitle }}
          </div>
          <div class="content-subtitle">
            {{ pageSubtitle }}
          </div>
        </div>
        <div v-if="activeSection === 'rules'" class="header-actions">
          <DepartmentSelector
            v-model="selectedDepartmentId"
            :departments="departments"
            :disabled="!bootstrapped || loading"
            @update:model-value="handleSelectDepartment"
          />
          <button
            type="button"
            class="btn btn-secondary btn-sm"
            :disabled="loading || isAggregateView"
            @click="handleRefresh"
          >
            {{ loading ? '加载中…' : '刷新' }}
          </button>
          <button type="button" class="btn btn-secondary btn-sm" @click="drawerOpen = true">
            导出
          </button>
          <span class="status-pill" :data-status="dirtyCount > 0 ? 'warn' : 'neutral'">
            未保存 {{ dirtyCount }}
          </span>
        </div>
      </header>

      <div class="content-body">
        <!-- 派工规则 -->
        <section v-show="activeSection === 'rules'" class="section-pane">
          <p v-if="dirtyMessages.length" class="dirty-banner">
            存在未保存草稿: {{ dirtyMessages.join('，') }}。请在对应面板内点击保存。
          </p>
          <p v-if="error" class="error-banner">
            {{ error }}
          </p>
          <p v-if="isAggregateView" class="aggregate-note">
            当前选择「全部科室」聚合视图，禁用所有写操作。请选择具体科室后再进行编辑。
          </p>

          <TaskTypePanel
            v-model:show-create-form="showTaskTypeCreateForm"
            :task-types="taskTypes"
            :requirement-versions="bundle.requirementVersions"
            :selected-task-type-code="selectedTaskTypeCode"
            :saving="saving"
            :disabled="isAggregateView || loading"
            :disabled-reason="isAggregateView ? '请先选择具体科室才能新增/删除任务类型。' : undefined"
            @select="selectTaskType"
            @create="handleCreateTaskType"
            @delete="handleDeleteTaskType"
          >
            <template #rules="{ taskType }">
              <div class="rules-tabs">
                <div class="inner-tabs" role="tablist">
                  <button
                    v-for="panel in panels"
                    :key="panel.id"
                    type="button"
                    role="tab"
                    class="inner-tab"
                    :class="{ active: activePanel === panel.id }"
                    :aria-selected="activePanel === panel.id"
                    @click="setActivePanel(panel.id)"
                  >
                    {{ panel.label }}
                  </button>
                </div>
                <div class="tab-body">
                  <GenerationRulePanel
                    v-if="activePanel === 'generation'"
                    :rules="bundle.flightGenerationRules"
                    :task-type-code="taskType.code"
                    :saving="saving"
                    :disabled="isAggregateView"
                    @save="handleSaveGenerationRule"
                    @delete="handleDeleteGenerationRule"
                    @dirty="(v) => markDirty('generationDraft', v)"
                  />
                  <AdjustmentRulePanel
                    v-else-if="activePanel === 'adjustment'"
                    :rules="bundle.generationAdjustmentRules"
                    :task-type-code="taskType.code"
                    :saving="saving"
                    :disabled="isAggregateView"
                    @save="handleSaveAdjustmentRule"
                    @dirty="(v) => markDirty('adjustmentDraft', v)"
                  />
                  <RequirementVersionPanel
                    v-else-if="activePanel === 'requirement'"
                    :task-type-code="taskType.code"
                    :versions="bundle.requirementVersions"
                    :equipment-types="equipmentTypes"
                    :saving="saving"
                    :disabled="isAggregateView"
                    @save-draft="handleSaveRequirementDraft"
                    @publish="handlePublishRequirement"
                    @dirty="(v) => markDirty('requirementDraft', v)"
                  />
                  <RulePreviewPanel
                    v-else-if="activePanel === 'preview'"
                    :task-types="taskTypes"
                    :result="previewResult"
                    :saving="saving"
                    @preview="handlePreview"
                  />
                  <ManualOrderPanel
                    v-else-if="activePanel === 'manualOrder'"
                    :draft="manualOrderDraft"
                    :departments="departments"
                    :task-types="taskTypes"
                    :saving="saving"
                    :disabled="isAggregateView"
                    :last-created-order-id="lastOrderId"
                    @update:draft="(v) => (manualOrderDraft = v)"
                    @submit="handleCreateManualOrder"
                    @dirty="(v) => markDirty('manualOrderDraft', v)"
                  />
                </div>
              </div>
            </template>
            <template #requirements="{ taskType }">
              <RequirementVersionPanel
                :task-type-code="taskType.code"
                :versions="bundle.requirementVersions"
                :equipment-types="equipmentTypes"
                :saving="saving"
                :disabled="isAggregateView"
                @save-draft="handleSaveRequirementDraft"
                @publish="handlePublishRequirement"
                @dirty="(v) => markDirty('requirementDraft', v)"
              />
            </template>
          </TaskTypePanel>
        </section>

        <!-- 标签定义 -->
        <section v-show="activeSection === 'labels'" class="section-pane">
          <LabelManagerPanel :active="activeSection === 'labels'" />
        </section>
      </div>
    </main>

    <RuleExportDrawer
      :open="drawerOpen"
      :exporting="exporting"
      :build-snapshot="buildSnapshotForPreview"
      :on-copy="handleCopySnapshot"
      :on-download="handleDownloadSnapshot"
      @close="drawerOpen = false"
    />
  </div>
</template>

<style scoped>
/* 侧栏 button.nav-item 清默认 button 样式；右侧壳层走 admin-layout / admin-page */
.dispatch-rule-page :deep(.admin-sidebar button.nav-item) {
  width: 100%;
  border: none;
  background: transparent;
  font: inherit;
  text-align: left;
  color: inherit;
}

.status-pill {
  font-size: 11px;
  padding: 4px 10px;
  border-radius: 999px;
  background: color-mix(in srgb, var(--ink) 8%, transparent);
  color: var(--ink-muted);
  font-weight: 600;
  white-space: nowrap;
}

.status-pill[data-status='warn'] {
  background: var(--warn-soft);
  color: var(--warn);
}

.dirty-banner,
.error-banner,
.aggregate-note {
  padding: 8px 12px;
  border-radius: 10px;
  font-size: 12px;
  margin: 0 0 16px;
}

.dirty-banner {
  background: var(--warn-soft);
  border: 1px solid color-mix(in srgb, var(--warn) 40%, transparent);
  color: var(--warn);
}

.error-banner {
  background: var(--danger-soft);
  border: 1px solid color-mix(in srgb, var(--danger) 32%, transparent);
  color: var(--danger);
}

.aggregate-note {
  background: var(--act-soft);
  border: 1px solid color-mix(in srgb, var(--act) 40%, transparent);
  color: var(--act);
}

.section-pane {
  min-width: 0;
}

.rules-tabs {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.inner-tabs {
  display: flex;
  gap: 16px;
  border-bottom: 1px solid var(--admin-border);
  flex-wrap: wrap;
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
  color: var(--act);
  border-bottom-color: var(--act);
}

.tab-body {
  min-height: 240px;
}
</style>
