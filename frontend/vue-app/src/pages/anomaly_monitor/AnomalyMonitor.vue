<script setup lang="ts">
// AnomalyMonitor Page - Structural shell migrated and connected with business logic
import { pageUrl } from '@/shared/page-routes';
import { useAnomalyMonitor } from '@/composables/useAnomalyMonitor';
import { computed } from 'vue';
import { hasUserPermission, useAuth } from '@/composables/useAuth';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import SvgIcon from '@/components/ui/SvgIcon.vue';
import UiButton from '@/components/ui/UiButton.vue';
import UiPill from '@/components/ui/UiPill.vue';
import UiSelect from '@/components/ui/UiSelect.vue';

const {
  loading,
  records,
  filters,
  stats,
  streamState,
  error,
  actionError,
  lastUpdatedAt,
  fetchRecords,
  acknowledge,
  resolve,
  isActionBusy,
} = useAnomalyMonitor();

const auth = useAuth();
const canManageAnomalies = computed(() => hasUserPermission(auth.getUser(), 'anomaly:write'));

async function handleAcknowledge(id: string): Promise<void> {
  try {
    await acknowledge(id);
  } catch {
    // The composable exposes the error for inline display.
  }
}

async function handleResolve(id: string): Promise<void> {
  try {
    await resolve(id);
  } catch {
    // The composable exposes the error for inline display.
  }
}

const typeLabels: Record<string, string> = {
  service_node_timeout: '节点超时',
  gate_stand_conflict: '机位冲突',
  kpi_degradation: 'KPI退化',
  ai_risk: 'AI风险',
  dispatch_issue: '调度异常',
};

const severityLabels: Record<string, string> = {
  critical: '紧急',
  high: '高',
  medium: '中',
  low: '低',
};

const statusLabels: Record<string, string> = {
  open: '待处理',
  acknowledged: '已确认',
  resolved: '已解决',
};

const statusOptions = [
  { value: '', label: '全部状态' },
  { value: 'open', label: '待处理' },
  { value: 'acknowledged', label: '已确认' },
  { value: 'resolved', label: '已解决' },
];

const typeOptions = [
  { value: '', label: '全部类型' },
  { value: 'service_node_timeout', label: '节点超时' },
  { value: 'gate_stand_conflict', label: '机位冲突' },
  { value: 'kpi_degradation', label: 'KPI退化' },
  { value: 'ai_risk', label: 'AI风险' },
  { value: 'dispatch_issue', label: '调度异常' },
];

const limitOptions = [
  { value: '50', label: '50 条' },
  { value: '100', label: '100 条' },
  { value: '200', label: '200 条' },
];

const limitValue = computed<string>({
  get: () => String(filters.value.limit),
  set: (value) => {
    filters.value.limit = Number(value);
  },
});

type PillTone = 'act' | 'ok' | 'warn' | 'danger' | 'mute';

function severityTone(severity: string): PillTone {
  if (severity === 'critical') return 'danger';
  if (severity === 'high') return 'warn';
  if (severity === 'medium') return 'act';
  return 'mute';
}

function statusTone(status: string): PillTone {
  if (status === 'open') return 'danger';
  if (status === 'acknowledged') return 'warn';
  if (status === 'resolved') return 'ok';
  return 'mute';
}

const streamTone = computed<PillTone>(() => {
  if (streamState.value === 'online') return 'ok';
  if (streamState.value === 'connecting') return 'warn';
  return 'danger';
});
</script>

<template>
  <div class="workspace-page anomaly-monitor-page">
    <header class="utility-bar">
      <div class="utility-main">
        <a :href="pageUrl('dashboard')" class="home-link" title="返回工作台">
          <SvgIcon src="/frontend/icons/home.svg" :size="18" label="返回" />
        </a>
        <span class="page-title">异常监控中心</span>
        <span class="utility-note">自动检测与闭环处置</span>
      </div>
      <div class="utility-actions">
        <UiPill id="streamState" :tone="streamTone">
          流状态: {{ streamState === 'online' ? '在线' : streamState === 'connecting' ? '连接中' : '离线' }}
        </UiPill>
        <UiButton
          id="refreshButton"
          variant="primary"
          :disabled="loading"
          @click="fetchRecords"
        >
          刷新
        </UiButton>
        <UiButton id="logoutButton" @click="auth.logout()">
          退出
        </UiButton>
      </div>
    </header>

    <section class="anomaly-grid">
      <aside class="panel filter-panel">
        <h2 class="section-title">
          筛选条件
        </h2>
        <div class="filter-form">
          <div class="filter-field">
            <span class="field-label">状态</span>
            <UiSelect
              id="statusFilter"
              v-model="filters.status"
              :options="statusOptions"
              label="状态筛选"
              min-width="100%"
            />
          </div>
          <div class="filter-field">
            <span class="field-label">类型</span>
            <UiSelect
              id="typeFilter"
              v-model="filters.type"
              :options="typeOptions"
              label="类型筛选"
              min-width="100%"
            />
          </div>
          <div class="filter-field">
            <span class="field-label">数量</span>
            <UiSelect
              id="limitFilter"
              v-model="limitValue"
              :options="limitOptions"
              label="数量筛选"
              min-width="100%"
            />
          </div>
        </div>
        <div id="updatedAt" class="meta-line">
          {{ lastUpdatedAt ? `最后更新: ${new Date(lastUpdatedAt).toLocaleString()}` : '已开启实时监控流' }}
        </div>
      </aside>

      <section class="panel records-panel">
        <div id="statsGrid" class="metrics-strip">
          <div class="metric-item">
            <span class="metric-label">总数</span><span id="stat-total" class="metric-value">{{ stats.total }}</span>
          </div>
          <div class="metric-item">
            <span class="metric-label">Open</span><span id="stat-open" class="metric-value">{{ stats.open }}</span>
          </div>
          <div class="metric-item">
            <span class="metric-label">已确认</span><span id="stat-ack" class="metric-value">{{ stats.acknowledged }}</span>
          </div>
          <div class="metric-item">
            <span class="metric-label">已解决</span><span id="stat-resolved" class="metric-value">{{ stats.resolved }}</span>
          </div>
          <div class="metric-item">
            <span class="metric-label">严重级别</span><span id="stat-critical" class="metric-value">{{ stats.critical }}</span>
          </div>
          <div class="metric-item">
            <span class="metric-label">已升级</span><span id="stat-escalated" class="metric-value">{{ stats.escalated }}</span>
          </div>
        </div>

        <div class="section-headline">
          <h2 class="section-title">
            异常队列
          </h2>
          <span class="section-meta">按检测时间倒序，支持状态闭环处理</span>
        </div>
        <div v-if="error || actionError" class="inline-error" role="alert">
          {{ actionError || error }}
        </div>

        <div class="records-table-wrap">
          <table>
            <thead>
              <tr>
                <th>检测时间</th>
                <th>航班</th>
                <th>类型</th>
                <th>严重级别</th>
                <th>状态</th>
                <th>升级等级</th>
                <th>标题</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody id="tableBody">
              <tr v-if="loading" key="loading">
                <td colspan="8" class="empty-placeholder">
                  数据加载中...
                </td>
              </tr>
              <tr v-else-if="records.length === 0" key="empty">
                <td colspan="8" class="empty-placeholder">
                  未查询到异常记录
                </td>
              </tr>
              <tr v-for="record in records" v-else :key="record.anomaly_id">
                <td>{{ record.detected_at ? new Date(record.detected_at).toLocaleString() : '-' }}</td>
                <td>{{ record.flight_id }}</td>
                <td>{{ typeLabels[record.anomaly_type] || record.anomaly_type }}</td>
                <td>
                  <UiPill :tone="severityTone(record.severity)">
                    {{ severityLabels[record.severity] || record.severity }}
                  </UiPill>
                </td>
                <td>
                  <UiPill :tone="statusTone(record.status)">
                    {{ statusLabels[record.status] || record.status }}
                  </UiPill>
                </td>
                <td>{{ record.escalation_level }}</td>
                <td>{{ record.title }}</td>
                <td class="actions">
                  <UiPill v-if="!canManageAnomalies">
                    只读
                  </UiPill>
                  <template v-else>
                    <UiButton
                      v-if="record.status === 'open'"
                      size="sm"
                      :disabled="isActionBusy(record.anomaly_id)"
                      @click="handleAcknowledge(record.anomaly_id)"
                    >
                      {{ isActionBusy(record.anomaly_id) ? '处理中...' : '确认' }}
                    </UiButton>
                    <UiButton
                      v-if="record.status !== 'resolved'"
                      variant="primary"
                      size="sm"
                      :disabled="isActionBusy(record.anomaly_id)"
                      @click="handleResolve(record.anomaly_id)"
                    >
                      {{ isActionBusy(record.anomaly_id) ? '处理中...' : '解决' }}
                    </UiButton>
                    <span v-else class="resolved-note">已处理</span>
                  </template>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>
    </section>
    <ThemeToggle />
  </div>
</template>

<style scoped>
/* 信号面 token + UI 库件（UiButton / UiPill / UiSelect） */
.anomaly-monitor-page {
  display: flex;
  flex-direction: column;
}

.home-link:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
}

.anomaly-grid {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(268px, 292px) minmax(0, 1fr);
  gap: var(--s4);
  padding: var(--s4) var(--s5);
  overflow: auto;
}

.filter-panel.panel {
  /* UiSelect 下拉列表需要溢出面板 */
  overflow: visible;
  height: fit-content;
  padding: var(--s4);
  gap: var(--s4);
}

.filter-form {
  display: grid;
  gap: var(--s3);
}

.filter-field {
  display: grid;
  gap: var(--s1);
}

.field-label {
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  color: var(--ink-subtle);
  letter-spacing: 0.03em;
}

.meta-line {
  margin-top: auto;
  padding-top: var(--s2);
  border-top: 1px dashed var(--line);
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

.records-panel {
  min-height: 0;
}

.metrics-strip {
  display: grid;
  grid-template-columns: repeat(6, minmax(0, 1fr));
  gap: var(--s3);
  padding: var(--s3) var(--s4);
  border-bottom: 1px solid var(--line);
  background: color-mix(in srgb, var(--ink) 3%, transparent);
}

.metric-item {
  display: flex;
  flex-direction: column;
  gap: var(--s1);
  text-align: center;
}

.metric-label {
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

.metric-value {
  /* 展示级统计数字，不入字阶梯子 */
  font-size: 20px;
  font-weight: var(--fw-semibold);
  color: var(--ink);
  font-variant-numeric: tabular-nums;
}

.inline-error {
  margin: var(--s3) var(--s4) 0;
  padding: var(--s2) var(--s3);
  border: 1px solid var(--danger);
  border-radius: var(--r-cell);
  background: var(--danger-soft);
  color: var(--danger);
  font-size: var(--fs-body);
}

.records-table-wrap {
  flex: 1;
  overflow-x: auto;
  padding: 0 var(--s2) var(--s2);
}

.records-table-wrap table {
  width: 100%;
  min-width: 980px;
  border-collapse: collapse;
}

.records-table-wrap th,
.records-table-wrap td {
  padding: var(--s3) var(--s4);
  text-align: left;
  border-bottom: 1px solid var(--line);
}

.records-table-wrap th {
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  color: var(--ink-muted);
  background: color-mix(in srgb, var(--ink) 3%, transparent);
  white-space: nowrap;
}

.records-table-wrap td {
  font-size: var(--fs-body);
  color: var(--ink-subtle);
}

.actions {
  display: inline-flex;
  align-items: center;
  gap: var(--s2);
}

.resolved-note {
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

.empty-placeholder {
  padding: var(--s5);
  text-align: center;
  color: var(--ink-muted);
}

@media (min-width: 1720px) {
  .anomaly-grid {
    grid-template-columns: minmax(286px, 320px) minmax(0, 1fr);
  }
}

@media (min-width: 1100px) and (max-width: 1439px) {
  .anomaly-grid {
    grid-template-columns: minmax(248px, 272px) minmax(0, 1fr);
  }
}

@media (max-width: 1099px) {
  .anomaly-grid {
    grid-template-columns: minmax(0, 1fr);
  }

  .metrics-strip {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .records-table-wrap table {
    min-width: 760px;
  }
}
</style>
