<script setup lang="ts">
// AnomalyMonitor Page - Structural shell migrated and connected with business logic
import { pageUrl } from '@/shared/page-routes';
import { useAnomalyMonitor } from '@/composables/useAnomalyMonitor';
import { computed } from 'vue';
import { hasUserPermission, useAuth } from '@/composables/useAuth';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import SvgIcon from '@/components/ui/SvgIcon.vue';

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
const canManageAnomalies = computed(() => hasUserPermission(auth.getUser(), 'flight:manage'));
function handleLogout() { auth.logout(); }

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
        <span id="streamState" :class="['stream-chip', streamState]">
          流状态: {{ streamState === 'online' ? '在线' : streamState === 'connecting' ? '连接中' : '离线' }}
        </span>
        <button
          id="refreshButton"
          class="btn primary"
          :disabled="loading"
          @click="fetchRecords"
        >
          刷新
        </button>
        <button id="logoutButton" class="btn" @click="handleLogout">
          退出
        </button>
      </div>
    </header>

    <section class="anomaly-grid">
      <aside class="panel filter-panel">
        <h2 class="section-title">
          筛选条件
        </h2>
        <div class="filter-form">
          <label class="filter-field">
            <span class="field-label">状态</span>
            <select id="statusFilter" v-model="filters.status">
              <option value="">全部状态</option>
              <option value="open">待处理</option>
              <option value="acknowledged">已确认</option>
              <option value="resolved">已解决</option>
            </select>
          </label>
          <label class="filter-field">
            <span class="field-label">类型</span>
            <select id="typeFilter" v-model="filters.type">
              <option value="">全部类型</option>
              <option value="service_node_timeout">节点超时</option>
              <option value="gate_stand_conflict">机位冲突</option>
              <option value="kpi_degradation">KPI退化</option>
              <option value="ai_risk">AI风险</option>
              <option value="dispatch_issue">调度异常</option>
            </select>
          </label>
          <label class="filter-field">
            <span class="field-label">数量</span>
            <select id="limitFilter" v-model="filters.limit">
              <option value="50">50 条</option>
              <option value="100">100 条</option>
              <option value="200">200 条</option>
            </select>
          </label>
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
                  <span :class="['badge', 'severity-' + record.severity]">{{ severityLabels[record.severity] || record.severity }}</span>
                </td>
                <td>
                  <span :class="['badge', 'status-' + record.status]">{{ statusLabels[record.status] || record.status }}</span>
                </td>
                <td>{{ record.escalation_level }}</td>
                <td>{{ record.title }}</td>
                <td class="actions">
                  <span v-if="!canManageAnomalies" class="readonly-badge">只读</span>
                  <template v-else>
                    <button
                      v-if="record.status === 'open'"
                      class="btn btn-sm"
                      :disabled="isActionBusy(record.anomaly_id)"
                      @click="handleAcknowledge(record.anomaly_id)"
                    >
                      {{ isActionBusy(record.anomaly_id) ? '处理中...' : '确认' }}
                    </button>
                    <button
                      v-if="record.status !== 'resolved'"
                      class="btn btn-sm primary"
                      :disabled="isActionBusy(record.anomaly_id)"
                      @click="handleResolve(record.anomaly_id)"
                    >
                      {{ isActionBusy(record.anomaly_id) ? '处理中...' : '解决' }}
                    </button>
                    <span v-else style="color: var(--ws-text-muted);">已处理</span>
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
/* AnomalyMonitor specific styles */
.page {
  min-height: 100vh;
  background: var(--bg-app, #F5F5F7);
}

.utility-bar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 24px;
  background: var(--bg-card);
  border-bottom: 1px solid var(--border-light);
}

.utility-main {
  display: flex;
  align-items: center;
  gap: 12px;
}

.home-link {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border-radius: 8px;
  background: var(--ws-surface-muted);
  color: var(--text-tertiary);
  text-decoration: none;
  transition: all 0.2s;
}

.home-link:hover {
  background: var(--bg-input);
  color: var(--text-primary);
}

.page-title {
  font-size: 18px;
  font-weight: 600;
  color: var(--text-primary);
}

.utility-note {
  color: var(--system-gray2);
  font-size: 13px;
}

.utility-actions {
  display: flex;
  align-items: center;
  gap: 12px;
}

.stream-chip {
  padding: 4px 10px;
  border-radius: 12px;
  font-size: 12px;
  font-weight: 500;
}

.stream-chip.connecting {
  background: var(--dh-signal-warn-soft);
  color: var(--system-orange);
}

.btn {
  padding: 8px 16px;
  border-radius: 8px;
  font-size: 13px;
  font-weight: 500;
  border: 1px solid var(--border-light);
  background: var(--bg-card);
  color: var(--text-tertiary);
  cursor: pointer;
  transition: all 0.2s;
}

.btn:hover {
  background: var(--bg-page);
  border-color: var(--border-light);
}

.btn.primary {
  background: var(--system-blue);
  color: var(--text-inverse);
  border-color: var(--system-blue);
}

.btn.primary:hover {
  background: var(--system-blue);
}

.btn:disabled {
  cursor: not-allowed;
  opacity: 0.6;
}

.readonly-badge {
  display: inline-flex;
  align-items: center;
  min-height: 28px;
  padding: 0 10px;
  border-radius: 999px;
  background: var(--ws-surface-muted);
  color: var(--text-tertiary);
  font-size: 12px;
  font-weight: 600;
}

.anomaly-grid {
  display: grid;
  grid-template-columns: 280px 1fr;
  gap: 20px;
  padding: 20px 24px;
}

.panel {
  background: var(--bg-card);
  border-radius: 12px;
  padding: 20px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
}

.filter-panel {
  height: fit-content;
}

.section-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary);
  margin: 0 0 16px;
}

.filter-form {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.filter-field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.field-label {
  font-size: 12px;
  color: var(--text-tertiary);
}

.filter-field select {
  padding: 8px 12px;
  border: 1px solid var(--border-light);
  border-radius: 8px;
  font-size: 13px;
  background: var(--bg-card);
}

.filter-field select:focus {
  outline: none;
  border-color: var(--system-blue);
  box-shadow: 0 0 0 3px var(--focus-ring-blue);
}

.meta-line {
  margin-top: 16px;
  font-size: 12px;
  color: var(--system-gray2);
}

.records-panel {
  min-height: 500px;
}

.metrics-strip {
  display: grid;
  grid-template-columns: repeat(6, 1fr);
  gap: 12px;
  padding: 16px;
  background: var(--bg-page);
  border-radius: 10px;
  margin-bottom: 20px;
}

.metric-item {
  text-align: center;
}

.metric-label {
  display: block;
  font-size: 11px;
  color: var(--system-gray2);
  margin-bottom: 4px;
}

.metric-value {
  display: block;
  font-size: 20px;
  font-weight: 700;
  color: var(--text-primary);
}

.section-headline {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 16px;
}

.section-meta {
  font-size: 12px;
  color: var(--system-gray2);
}

.inline-error {
  margin-bottom: 12px;
  padding: 10px 12px;
  border: 1px solid #fecaca;
  border-radius: 8px;
  background: var(--dh-signal-critical-soft);
  color: var(--ws-danger);
  font-size: 13px;
}

.records-table-wrap {
  overflow-x: auto;
}

.records-table-wrap table {
  width: 100%;
  border-collapse: collapse;
}

.records-table-wrap th,
.records-table-wrap td {
  padding: 12px 16px;
  text-align: left;
  border-bottom: 1px solid #f1f5f9;
}

.records-table-wrap th {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-tertiary);
  background: var(--bg-page);
}

.records-table-wrap td {
  font-size: 13px;
  color: var(--text-secondary);
}

.empty-placeholder {
  text-align: center;
  padding: 40px;
  color: var(--system-gray2);
}

.empty {
  text-align: center;
  padding: 40px;
  color: var(--system-gray2);
}
</style>
