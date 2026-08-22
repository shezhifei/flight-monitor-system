<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { pageUrl } from '@/shared/page-routes';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import SvgIcon from '@/components/ui/SvgIcon.vue';
import UiButton from '@/components/ui/UiButton.vue';
import UiDrawer from '@/components/ui/UiDrawer.vue';
import UiPill from '@/components/ui/UiPill.vue';
import UiSwitch from '@/components/ui/UiSwitch.vue';
import { useApi } from '@/composables/useApi';
import { useAuth } from '@/composables/useAuth';
import { useToast } from '@/composables/useToast';
import {
  createLlmEvalApi,
  type EvalJobDetail,
  type EvalJobSummary,
} from '@/lib/ai/api';
import '@/styles/main.css';

// Frozen G1 fixtures (docs/fixtures); custom paths are allowed.
const DATASET_OPTIONS = [
  { label: 'query_ops 基线 (agent_query_ops_eval.jsonl)', value: 'docs/fixtures/agent_query_ops_eval.jsonl' },
  { label: 'dispatch_ops 基线 (agent_dispatch_ops_eval.jsonl)', value: 'docs/fixtures/agent_dispatch_ops_eval.jsonl' },
];

const ACTIVE_STATUSES = new Set(['pending', 'running']);
const POLL_INTERVAL_MS = 4000;

const toast = useToast();
const auth = useAuth();
const evalApi = createLlmEvalApi(useApi());

const jobs = ref<EvalJobSummary[]>([]);
const currentJob = ref<EvalJobDetail | null>(null);
const creating = ref(false);
const loadingJobs = ref(false);
const drawerOpen = ref(false);

const createForm = ref({
  name: 'agent eval',
  dataset_path: DATASET_OPTIONS[0]?.value ?? '',
  description: '',
  run: true,
});

const sidebarUser = computed(() => {
  const user = auth.getUser();
  const name = user?.username || 'Admin';
  const role = user?.is_admin ? '系统管理员' : (user?.role || '普通用户');
  const avatar = name.trim().charAt(0).toUpperCase() || 'A';
  return { name, role, avatar };
});

const gates = computed(() => currentJob.value?.gates ?? []);

type PillTone = 'act' | 'ok' | 'warn' | 'danger' | 'mute';

function statusBadgeTone(status: string): PillTone {
  switch (status) {
    case 'completed': return 'ok';
    case 'running': return 'act';
    case 'failed': return 'danger';
    default: return 'mute';
  }
}

function gateBadgeTone(status: string): PillTone {
  if (status === 'pass') return 'ok';
  if (status === 'fail') return 'danger';
  return 'mute';
}

function isActive(status: string | undefined): boolean {
  return ACTIVE_STATUSES.has(String(status || ''));
}

async function refreshJobs(): Promise<EvalJobSummary[]> {
  loadingJobs.value = true;
  try {
    const rows = await evalApi.listEvalJobs(30);
    jobs.value = rows;
    return rows;
  } catch (error) {
    toast.showToast('error', error instanceof Error ? error.message : '加载评测任务失败');
    return [];
  } finally {
    loadingJobs.value = false;
  }
}

async function refreshCurrentJob(jobId: string): Promise<void> {
  try {
    currentJob.value = await evalApi.getEvalJob(jobId);
  } catch (error) {
    toast.showToast('error', error instanceof Error ? error.message : '加载详情失败');
  }
}

// 仅在选中任务变化时重启轮询；React 版按 currentJob 引用重启会在打开详情时退化成无间隔轮询
let pollTimer: number | null = null;
let disposed = false;

async function tick(): Promise<void> {
  const rows = await refreshJobs();
  if (disposed) return;
  const job = currentJob.value;
  const hasActive = rows.some((row) => isActive(row.status)) || (job !== null && isActive(job.status));
  if (!hasActive) return;
  if (job !== null && isActive(job.status)) {
    await refreshCurrentJob(String(job.job_id));
  }
  if (disposed) return;
  pollTimer = window.setTimeout(() => void tick(), POLL_INTERVAL_MS);
}

function restartPolling(): void {
  if (pollTimer !== null) {
    window.clearTimeout(pollTimer);
    pollTimer = null;
  }
  void tick();
}

onMounted(restartPolling);
watch(() => currentJob.value?.job_id, () => {
  if (!disposed) restartPolling();
});
onBeforeUnmount(() => {
  disposed = true;
  if (pollTimer !== null) window.clearTimeout(pollTimer);
});

async function handleCancelJob(job: EvalJobSummary): Promise<void> {
  const jobId = String(job.job_id || '');
  try {
    await evalApi.cancelEvalJob(jobId);
    toast.showToast('success', '评测任务已取消');
    await refreshJobs();
  } catch (error) {
    toast.showToast('error', error instanceof Error ? error.message : '取消失败');
  }
}

async function handleCreate(): Promise<void> {
  creating.value = true;
  try {
    const created = await evalApi.createEvalJob({
      name: createForm.value.name || 'agent eval',
      dataset_path: createForm.value.dataset_path || '',
      description: createForm.value.description || '',
      run: Boolean(createForm.value.run),
    });
    toast.showToast('success', `评测任务已创建: ${created.job_id}`);
    drawerOpen.value = false;
    await refreshJobs();
    if (created.job_id) {
      await refreshCurrentJob(created.job_id);
    }
  } catch (error) {
    toast.showToast('error', error instanceof Error ? error.message : '创建评测失败');
  } finally {
    creating.value = false;
  }
}

function handleLogout(): void {
  auth.logout();
}
</script>

<template>
  <div class="admin-container eval-lab-page">
    <aside class="admin-sidebar">
      <div class="sidebar-header">
        <div class="sidebar-logo">
          <SvgIcon src="/frontend/icons/ai.svg" :size="20" />
          <span>LLM 评测</span>
        </div>
      </div>

      <nav class="sidebar-nav">
        <div class="nav-section">
          <div class="nav-section-title">
            AI 工作台
          </div>
          <a class="nav-item" :href="pageUrl('ai_config_center')">
            <span class="nav-item-icon"><SvgIcon src="/frontend/icons/settings.svg" /></span>
            <span>实体配置</span>
          </a>
          <a class="nav-item" :href="pageUrl('ai_monitor')">
            <span class="nav-item-icon"><SvgIcon src="/frontend/icons/activity.svg" /></span>
            <span>实时监控</span>
          </a>
          <a class="nav-item active" aria-current="page" :href="pageUrl('llm_eval_lab')">
            <span class="nav-item-icon"><SvgIcon src="/frontend/icons/target.svg" /></span>
            <span>LLM 评测</span>
          </a>
          <a class="nav-item" :href="pageUrl('nl_query')">
            <span class="nav-item-icon"><SvgIcon src="/frontend/icons/messages.svg" /></span>
            <span>自然语言查询</span>
          </a>
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
            LLM 评测
          </div>
          <div class="content-subtitle">
            创建评测任务并查看证据覆盖与工具策略门禁
          </div>
        </div>
        <div class="header-actions">
          <UiButton variant="primary" @click="drawerOpen = true">
            创建任务
          </UiButton>
          <UiButton :disabled="loadingJobs" @click="refreshJobs">
            {{ loadingJobs ? '加载中…' : '刷新' }}
          </UiButton>
        </div>
      </header>

      <div class="content-body">
        <section class="panel eval-panel">
          <div class="eval-panel-title">
            评测任务列表
          </div>
          <div class="table-container">
            <table>
              <thead>
                <tr>
                  <th>Job ID</th>
                  <th>名称</th>
                  <th>数据集</th>
                  <th>状态</th>
                  <th>进度</th>
                  <th>创建时间</th>
                  <th>操作</th>
                </tr>
              </thead>
              <tbody>
                <tr v-if="!jobs.length && !loadingJobs">
                  <td colspan="7" class="eval-empty">
                    暂无评测任务
                  </td>
                </tr>
                <tr v-for="(job, idx) in jobs" :key="String(job.job_id || idx)">
                  <td class="mono">
                    {{ job.job_id }}
                  </td>
                  <td>{{ job.name || '-' }}</td>
                  <td class="mono muted">
                    {{ job.dataset_path || '-' }}
                  </td>
                  <td>
                    <UiPill :tone="statusBadgeTone(job.status)">
                      {{ job.status || 'unknown' }}
                    </UiPill>
                  </td>
                  <td class="mono muted">
                    {{ job.completed_runs ?? 0 }}/{{ job.total_runs ?? 0 }}
                  </td>
                  <td class="muted">
                    {{ job.created_at || '-' }}
                  </td>
                  <td>
                    <div class="eval-row-actions">
                      <UiButton @click="refreshCurrentJob(String(job.job_id || ''))">
                        查看
                      </UiButton>
                      <UiButton
                        variant="danger"
                        :disabled="!isActive(job.status)"
                        @click="handleCancelJob(job)"
                      >
                        取消
                      </UiButton>
                    </div>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </section>

        <section class="panel eval-panel">
          <template v-if="currentJob">
            <div class="eval-gates-head">
              <div class="eval-gates-title">
                <span class="eval-panel-title">门禁结果</span>
                <UiPill :tone="statusBadgeTone(String(currentJob.status || ''))">
                  {{ String(currentJob.status || '') }}
                </UiPill>
              </div>
              <span class="mono muted">{{ String(currentJob.job_id || '') }}</span>
            </div>

            <p v-if="currentJob.error_message" class="eval-error-banner">
              {{ currentJob.error_message }}
            </p>

            <div v-if="gates.length" class="table-container">
              <table>
                <thead>
                  <tr>
                    <th>门禁</th>
                    <th>实测值</th>
                    <th>阈值</th>
                    <th>结果</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="(gate, idx) in gates" :key="`${gate.metric_name}-${idx}`">
                    <td class="mono">
                      {{ gate.metric_name }}
                    </td>
                    <td class="mono">
                      {{ Number(gate.value).toFixed(3) }}
                    </td>
                    <td class="mono muted">
                      {{ Number(gate.threshold).toFixed(2) }}
                    </td>
                    <td>
                      <UiPill :tone="gateBadgeTone(gate.status)">
                        {{ gate.status }}
                      </UiPill>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
            <p v-else class="eval-hint">
              尚无门禁记录——任务完成（或失败）后这里显示证据覆盖与工具策略门禁。
            </p>
          </template>
          <div v-else class="eval-empty-detail">
            从上方列表中选择一个任务查看门禁结果
          </div>
        </section>
      </div>
    </main>

    <UiDrawer
      :open="drawerOpen"
      title="创建评测任务"
      :width="460"
      @close="drawerOpen = false"
    >
      <form class="eval-form" @submit.prevent="handleCreate">
        <label class="eval-field">
          <span class="eval-field-label">任务名</span>
          <input
            v-model="createForm.name"
            type="text"
            class="eval-input"
            required
          >
        </label>
        <label class="eval-field">
          <span class="eval-field-label">评测数据集 (JSONL)</span>
          <input
            v-model="createForm.dataset_path"
            type="text"
            class="eval-input"
            list="eval-dataset-options"
            placeholder="选择或输入数据集路径"
            required
          >
          <datalist id="eval-dataset-options">
            <option
              v-for="option in DATASET_OPTIONS"
              :key="option.value"
              :value="option.value"
            >
              {{ option.label }}
            </option>
          </datalist>
        </label>
        <label class="eval-field">
          <span class="eval-field-label">描述（可选）</span>
          <textarea v-model="createForm.description" class="eval-input eval-textarea" rows="2" />
        </label>
        <label class="eval-field eval-switch-row">
          <span class="eval-field-label">创建后立即执行</span>
          <UiSwitch v-model:checked="createForm.run" label="创建后立即执行" />
        </label>
        <UiButton
          class="eval-submit"
          native-type="submit"
          variant="primary"
          size="md"
          :disabled="creating"
        >
          {{ creating ? '创建中…' : '创建任务' }}
        </UiButton>
      </form>
    </UiDrawer>
  </div>
</template>

<style scoped>
.eval-panel {
  padding: var(--s3) var(--s4);
  margin-bottom: var(--s4);
}

.eval-panel-title {
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  color: var(--ink-muted);
  margin-bottom: var(--s3);
}

.eval-panel .table-container {
  border: none;
  border-radius: var(--r-cell);
}

.mono {
  font-family: var(--mono);
  font-size: var(--fs-label);
}

.muted {
  color: var(--ink-muted);
}

.eval-empty {
  text-align: center !important;
  color: var(--ink-muted);
  padding: var(--s4) var(--s3) !important;
}

.eval-row-actions {
  display: flex;
  gap: var(--s2);
  justify-content: flex-end;
}

.eval-gates-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--s3);
  margin-bottom: var(--s3);
}

.eval-gates-title {
  display: flex;
  align-items: center;
  gap: var(--s2);
}

.eval-gates-title .eval-panel-title {
  margin-bottom: 0;
}

.eval-error-banner {
  padding: var(--s2) var(--s3);
  border-radius: var(--r-control);
  font-size: var(--fs-label);
  margin: 0 0 var(--s3);
  background: var(--danger-soft);
  border: 1px solid color-mix(in srgb, var(--danger) 32%, transparent);
  color: var(--danger);
}

.eval-hint {
  margin: 0;
  font-size: var(--fs-body);
  color: var(--ink-subtle);
}

.eval-empty-detail {
  padding: var(--s4);
  text-align: center;
  font-size: var(--fs-body);
  color: var(--ink-muted);
}

.eval-form {
  display: flex;
  flex-direction: column;
  gap: var(--s3);
}

.eval-field {
  display: flex;
  flex-direction: column;
  gap: var(--s1);
}

.eval-field-label {
  font-size: var(--fs-label);
  color: var(--ink-subtle);
}

.eval-input {
  box-sizing: border-box;
  width: 100%;
  min-height: var(--h-sm);
  padding: 0 var(--s2);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-control);
  font-size: var(--fs-label);
  background: var(--face-page);
  color: var(--ink);
  transition: border-color var(--t-fast) var(--ease);
}

.eval-input:hover {
  border-color: var(--act);
}

.eval-input:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
  border-color: var(--act);
}

.eval-textarea {
  padding: var(--s2);
  resize: vertical;
  font-family: inherit;
}

.eval-switch-row {
  flex-direction: row;
  align-items: center;
  justify-content: space-between;
}

/* 提交钮归 UiButton，页内只定满宽 */
.eval-submit {
  width: 100%;
}
</style>
