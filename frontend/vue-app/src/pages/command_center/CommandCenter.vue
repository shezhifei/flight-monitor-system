<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue';
import { useCommandCenter } from '@/composables/useCommandCenter';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';

// Command Center Page - Migrated and connected with real-time logic
const {
  loading,
  error,
  kpis,
  verdict,
  events,
  priorityQueue,
  windowPressure,
  heatmapData,
  dispatchLoad,
  terminalLoad,
  systemHealth,
  fetchSnapshot,
  startAutoRefresh,
  stopAutoRefresh
} = useCommandCenter();

function formatEventTime(ts: string | number | null | undefined): string {
  return new Date(ts ?? Date.now()).toLocaleTimeString();
}

const windowHours = ref('6');
const refreshInterval = ref('30000');
const isPaused = ref(false);
const lastRefreshTime = ref('');
const isFullscreen = ref(false);
const isTogglingFullscreen = ref(false);
const fullscreenError = ref('');

async function refreshSnapshot(windowHours?: number): Promise<void> {
  await fetchSnapshot(windowHours);
  lastRefreshTime.value = new Date().toLocaleTimeString();
}

watch(refreshInterval, (newVal) => {
  if (!isPaused.value) {
    startAutoRefresh(Number(newVal));
  }
});

watch(windowHours, (newVal) => {
  refreshSnapshot(Number(newVal));
});

function handleRefresh(): void {
  refreshSnapshot(Number(windowHours.value));
}

function clearEvidenceFeed(): void {
  if (events.value.length === 0) {
    return;
  }
  if (window.confirm(`确定清空当前视图中的 ${events.value.length} 条证据流吗？`)) {
    events.value = [];
  }
}

function toggleRefresh(): void {
  isPaused.value = !isPaused.value;
  if (isPaused.value) {
    stopAutoRefresh();
  } else {
    startAutoRefresh(Number(refreshInterval.value));
  }
}

function syncFullscreenState(): void {
  isFullscreen.value = Boolean(document.fullscreenElement);
}

async function toggleFullscreen(): Promise<void> {
  if (isTogglingFullscreen.value) {
    return;
  }
  fullscreenError.value = '';
  isTogglingFullscreen.value = true;
  try {
    if (!document.fullscreenElement) {
      await document.documentElement.requestFullscreen();
    } else {
      await document.exitFullscreen();
    }
    syncFullscreenState();
  } catch (error) {
    fullscreenError.value = error instanceof Error ? error.message : '浏览器拒绝切换全屏';
  } finally {
    isTogglingFullscreen.value = false;
  }
}

onMounted(() => {
  syncFullscreenState();
  document.addEventListener('fullscreenchange', syncFullscreenState);
});

onUnmounted(() => {
  document.removeEventListener('fullscreenchange', syncFullscreenState);
});
</script>

<template>
  <div class="workspace-page data-hub-page command-center-page">
    <div id="header-host" />
    <div id="breadcrumb-host" />

    <section class="panel dashboard-ribbon command-ops-bar command-judgement-bar">
      <div class="dashboard-ribbon-copy command-ribbon-copy">
        <span class="section-eyebrow">实时决策</span>
        <h2 class="dashboard-ribbon-title">
          先判断是否失稳，再锁定未来 60 分钟动作对象
        </h2>
      </div>
      <div class="dashboard-ribbon-actions metric-controls">
        <select id="windowHoursSelect" v-model="windowHours" aria-label="窗口时长">
          <option value="2">
            未来 2h
          </option>
          <option value="6" selected>
            未来 6h
          </option>
          <option value="12">
            未来 12h
          </option>
        </select>
        <select id="refreshIntervalSelect" v-model="refreshInterval" aria-label="刷新间隔">
          <option value="15000">
            15s 刷新
          </option>
          <option value="30000" selected>
            30s 刷新
          </option>
          <option value="60000">
            60s 刷新
          </option>
        </select>
        <button
          id="refreshNowBtn"
          class="btn primary"
          :disabled="loading"
          @click="handleRefresh"
        >
          {{ loading ? '刷新中...' : '刷新' }}
        </button>
        <button id="toggleRefreshBtn" class="btn" @click="toggleRefresh">
          {{ isPaused ? '继续' : '暂停' }}
        </button>
        <button id="clearEventsBtn" class="btn" @click="clearEvidenceFeed">
          清空当前视图
        </button>
        <button
          id="fullScreenBtn"
          class="btn"
          :disabled="isTogglingFullscreen"
          @click="toggleFullscreen"
        >
          {{ isFullscreen ? '退出全屏' : '全屏' }}
        </button>
      </div>
      <p v-if="fullscreenError" class="command-inline-error">
        {{ fullscreenError }}
      </p>
      <p v-if="error" class="command-inline-error">
        {{ error }}
      </p>
      <div class="command-verdict-strip">
        <span id="opsVerdictChip" :class="['decision-severity-chip', 'is-' + verdict.severity]">
          {{ verdict.title }}
        </span>
        <div class="command-verdict-copy">
          <strong id="opsVerdictTitle" class="command-verdict-title">{{ verdict.title }}</strong>
          <p id="opsVerdictDetail" class="command-verdict-detail">
            {{ verdict.detail }}
          </p>
        </div>
        <div class="command-verdict-side">
          <span id="opsVerdictMeta" class="command-verdict-window">{{ verdict.window }}</span>
        </div>
      </div>
    </section>

    <section class="metric-grid command-kpi-grid command-stat-strip">
      <article class="metric-card hero-metric stat-critical">
        <div class="metric-label">
          待决策对象
        </div>
        <div id="metricDecisionCount" class="metric-value">
          {{ kpis.decisionCount }}
        </div>
        <div id="metricDecisionCountSub" class="metric-sub">
          P1 / P2 任务数量
        </div>
      </article>
      <article class="metric-card hero-metric">
        <div class="metric-label">
          60 分钟高风险离港
        </div>
        <div id="metricRiskFlights" class="metric-value">
          {{ kpis.riskFlights }}
        </div>
        <div id="metricRiskFlightsSub" class="metric-sub">
          延误超过30分钟航班
        </div>
      </article>
      <article class="metric-card hero-metric">
        <div class="metric-label">
          未闭环异常
        </div>
        <div id="metricOpenAnomalies" class="metric-value">
          {{ kpis.openAnomalies }}
        </div>
        <div id="metricOpenAnomaliesSub" class="metric-sub">
          待处理异常报警
        </div>
      </article>
      <article class="metric-card hero-metric">
        <div class="metric-label">
          调度阻塞
        </div>
        <div id="metricDispatchBlockers" class="metric-value">
          {{ kpis.dispatchBlockers }}
        </div>
        <div id="metricDispatchBlockersSub" class="metric-sub">
          派工受阻/存在冲突
        </div>
      </article>
      <article class="metric-card hero-metric">
        <div class="metric-label">
          延误压力
        </div>
        <div id="metricDelayPressure" class="metric-value">
          {{ kpis.delayPressure }}m
        </div>
        <div id="metricDelayPressureSub" class="metric-sub">
          平均延误时间
        </div>
      </article>
    </section>

    <section class="command-board">
      <article class="panel compact-panel command-action-panel spotlight-panel">
        <div class="section-headline">
          <div class="section-heading-block">
            <h3 class="section-title">
              行动优先队列
            </h3>
          </div>
          <span id="decisionQueueMeta" class="section-meta">按行动紧迫度排序</span>
        </div>
        <div id="decisionQueueList" class="priority-list decision-queue-list">
          <div v-if="priorityQueue.length === 0" class="empty-placeholder">
            暂无决策优先任务
          </div>
          <div
            v-for="item in priorityQueue"
            v-else
            :key="item.id"
            class="priority-item"
            :class="`is-${item.severity}`"
          >
            <div>
              <strong>{{ item.name }}</strong>
              <span>{{ item.meta }}</span>
            </div>
          </div>
        </div>
      </article>

      <aside class="command-context-column">
        <article class="panel compact-panel command-window-panel hero-panel">
          <div class="section-headline">
            <div class="section-heading-block">
              <h3 class="section-title">
                未来窗口离港压力
              </h3>
            </div>
            <span id="windowPressureMeta" class="section-meta">未来窗口</span>
          </div>
          <div id="windowPressureList" class="distribution-strip" style="min-height:200px">
            <div v-if="windowPressure.length === 0" class="empty-placeholder">
              暂无离港压力数据
            </div>
            <div
              v-for="item in windowPressure"
              v-else
              :key="item.id"
              class="distribution-row"
              :class="`is-${item.severity}`"
            >
              <span>{{ item.label }}</span>
              <strong>{{ item.value }}</strong>
              <small>{{ item.detail }}</small>
            </div>
          </div>
        </article>

        <article class="panel compact-panel heatmap-panel">
          <div class="section-headline">
            <div class="section-heading-block">
              <h3 class="section-title">
                异常热区
              </h3>
            </div>
            <span id="riskHeatmapMeta" class="section-meta">按机位区间统计</span>
          </div>
          <div id="standHeatmapChart" class="heatmap-box" style="min-height:180px">
            <div v-if="heatmapData.length === 0" class="empty-placeholder">
              暂无热区数据
            </div>
            <div
              v-for="item in heatmapData"
              v-else
              :key="item.id"
              class="distribution-row"
              :class="`is-${item.severity}`"
            >
              <span>{{ item.label }}</span>
              <strong>{{ item.value }}</strong>
              <small>{{ item.detail }}</small>
            </div>
          </div>
        </article>

        <article class="panel compact-panel dispatch-panel">
          <div class="section-headline">
            <div class="section-heading-block">
              <h3 class="section-title">
                调度负载断面
              </h3>
            </div>
            <span id="dispatchWindowMeta" class="section-meta">调度窗口</span>
          </div>
          <div id="dispatchSummaryList" class="distribution-strip" style="min-height:180px">
            <div v-if="dispatchLoad.length === 0" class="empty-placeholder">
              暂无负载数据
            </div>
            <div
              v-for="item in dispatchLoad"
              v-else
              :key="item.id"
              class="distribution-row"
              :class="`is-${item.severity}`"
            >
              <span>{{ item.label }}</span>
              <strong>{{ item.value }}</strong>
              <small>{{ item.detail }}</small>
            </div>
          </div>
        </article>

        <article class="panel compact-panel terminal-panel">
          <div class="section-headline">
            <div class="section-heading-block">
              <h3 class="section-title">
                航站楼压力
              </h3>
            </div>
            <span id="terminalLoadMeta" class="section-meta">按航班数统计</span>
          </div>
          <div id="terminalLoadList" class="distribution-strip" style="min-height:180px">
            <div v-if="terminalLoad.length === 0" class="empty-placeholder">
              暂无航站楼压力数据
            </div>
            <div
              v-for="item in terminalLoad"
              v-else
              :key="item.id"
              class="distribution-row"
              :class="`is-${item.severity}`"
            >
              <span>{{ item.label }}</span>
              <strong>{{ item.value }}</strong>
              <small>{{ item.detail }}</small>
            </div>
          </div>
        </article>
      </aside>
    </section>

    <section class="panel evidence-panel command-evidence-panel">
      <div class="section-headline">
        <div class="section-heading-block">
          <h3 class="section-title">
            实时证据流
          </h3>
        </div>
        <span id="eventFeedMeta" class="section-meta">最多保留 100 条</span>
      </div>
      <div id="eventFeedList" class="event-timeline">
        <div v-if="events.length === 0" class="empty-placeholder">
          等待实时证据数据流...
        </div>
        <div
          v-for="(evt, idx) in events"
          v-else
          :key="idx"
          class="event-item"
          style="padding: 6px 12px; border-bottom: 1px solid var(--ws-border); font-size: 13px;"
        >
          <span class="event-time" style="color: var(--ws-text-muted); margin-right: 8px;">[{{ formatEventTime(evt.timestamp) }}]</span>
          <span class="event-text" style="color: var(--ws-text);">{{ evt.message || evt.description || JSON.stringify(evt) }}</span>
        </div>
      </div>
    </section>

    <footer class="footer command-footer">
      <span id="footerWindowText">窗口: 未来 {{ windowHours }} 小时</span>
      <span id="metricLastRefresh">{{ lastRefreshTime || '未更新' }}</span>
      <span id="metricAutoRefreshSub">{{ isPaused ? '自动刷新暂停' : '自动刷新开启' }}</span>
      <span id="metricSystemHealth">健康度 {{ systemHealth.score }}%</span>
      <span id="metricSystemHealthSub">{{ systemHealth.label }}</span>
      <span id="footerNoticeText">说明: 当前版本使用系统内置接口，已实现双主题融合。</span>
    </footer>
    <ThemeToggle />
  </div>
</template>

<style scoped>
/* Command Center specific styles - extracted from command_center.html */
.screen.data-hub-shell {
  min-height: 100vh;
}

.panel {
  background: var(--bg-card);
  border: 1px solid var(--border-light);
  border-radius: 8px;
}

.dashboard-ribbon {
  padding: 20px 24px;
  margin: 16px;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  border-radius: 12px;
  color: var(--text-inverse);
}

.dashboard-ribbon-copy {
  margin-bottom: 16px;
}

.section-eyebrow {
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.1em;
  opacity: 0.8;
}

.dashboard-ribbon-title {
  font-size: 18px;
  font-weight: 600;
  margin: 4px 0 0;
}

.dashboard-ribbon-actions {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
  align-items: center;
}

.command-inline-error {
  color: #fff1f2;
  font-size: 12px;
  line-height: 1.4;
  margin: 10px 0 0;
  padding: 8px 10px;
  background: rgba(127, 29, 29, 0.32);
  border: 1px solid rgba(254, 202, 202, 0.35);
  border-radius: 6px;
}

.metric-controls select,
.metric-controls .btn {
  padding: 8px 16px;
  border-radius: 6px;
  font-size: 13px;
}

.metric-controls select {
  border: 1px solid rgba(255,255,255,0.3);
  background: rgba(255,255,255,0.15);
  color: var(--text-inverse);
}

.metric-controls .btn {
  border: none;
  background: rgba(255,255,255,0.2);
  color: var(--text-inverse);
  cursor: pointer;
}

.metric-controls .btn.primary {
  background: var(--bg-card);
  color: var(--secondary-color);
}

.command-verdict-strip {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-top: 20px;
  padding: 16px;
  background: rgba(0,0,0,0.2);
  border-radius: 8px;
}

.decision-severity-chip {
  padding: 6px 12px;
  border-radius: 20px;
  font-size: 12px;
  font-weight: 600;
}

.decision-severity-chip.is-info {
  background: var(--system-blue-subtle);
}

.decision-severity-chip.is-ok {
  background: var(--success-bg-subtle);
}

.decision-severity-chip.is-warn {
  background: var(--dh-signal-warn-soft);
}

.decision-severity-chip.is-critical {
  background: var(--error-bg-subtle);
}

.command-verdict-copy {
  flex: 1;
}

.command-verdict-title {
  display: block;
  font-size: 14px;
}

.command-verdict-detail {
  font-size: 12px;
  opacity: 0.8;
  margin: 4px 0 0;
}

.metric-grid {
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  gap: 16px;
  margin: 0 16px;
}

.metric-card {
  background: var(--bg-card);
  border: 1px solid var(--border-light);
  border-radius: 8px;
  padding: 16px;
}

.metric-card.hero-metric {
  padding: 20px;
}

.metric-label {
  font-size: 11px;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.metric-value {
  font-size: 32px;
  font-weight: 700;
  color: var(--text-primary);
  margin: 8px 0;
}

.metric-sub {
  font-size: 12px;
  color: var(--text-tertiary);
}

.command-board {
  display: grid;
  grid-template-columns: 1fr 400px;
  gap: 16px;
  margin: 16px;
}

.command-action-panel {
  min-height: 400px;
}

.command-context-column {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.compact-panel {
  padding: 16px;
}

.section-headline {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 16px;
}

.section-title {
  font-size: 14px;
  font-weight: 600;
  margin: 0;
}

.section-meta {
  font-size: 11px;
  color: var(--text-tertiary);
}

.empty-placeholder {
  padding: 20px;
  text-align: center;
  color: var(--text-tertiary);
  font-size: 13px;
}

.priority-list,
.distribution-strip,
.heatmap-box {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.priority-item,
.distribution-row {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 6px 12px;
  align-items: center;
  border: 1px solid var(--border-light);
  border-left-width: 4px;
  border-radius: 8px;
  padding: 10px 12px;
  background: var(--bg-card);
}

.priority-item.is-ok,
.distribution-row.is-ok {
  border-left-color: var(--system-green);
}

.priority-item.is-warn,
.distribution-row.is-warn {
  border-left-color: var(--system-orange);
}

.priority-item.is-critical,
.distribution-row.is-critical {
  border-left-color: var(--system-red);
}

.priority-item strong,
.distribution-row span {
  color: var(--text-primary);
  font-size: 13px;
  font-weight: 650;
}

.priority-item span,
.distribution-row small {
  color: var(--text-tertiary);
  font-size: 12px;
}

.distribution-row strong {
  color: var(--text-primary);
  font-size: 20px;
}

.evidence-panel {
  margin: 16px;
  padding: 16px;
}

.event-timeline {
  max-height: 300px;
  overflow-y: auto;
}

.footer {
  display: flex;
  gap: 24px;
  padding: 12px 24px;
  background: var(--bg-sidebar);
  border-top: 1px solid var(--border-light);
  font-size: 12px;
  color: var(--text-tertiary);
}

.command-footer {
  margin: 0 16px 16px;
  border-radius: 8px;
}

.btn {
  padding: 8px 16px;
  border-radius: 6px;
  font-size: 13px;
  cursor: pointer;
  border: 1px solid var(--border-light);
  background: var(--bg-card);
}
</style>
