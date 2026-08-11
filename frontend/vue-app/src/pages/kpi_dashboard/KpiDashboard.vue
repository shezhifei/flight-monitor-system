<script setup lang="ts">
// KpiDashboard Page - KPI diagnostics with API data binding
import { onMounted, ref, computed } from 'vue';
import { useApi } from '@/composables/useApi';
import { useAuth } from '@/composables/useAuth';
import { pageUrl } from '@/shared/page-routes';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import SvgIcon from '@/components/ui/SvgIcon.vue';
import {
  mapKpiSnapshot,
  mapKpiTrend,
  mapServiceNodes,
  summarizeServiceNodes,
  type KpiDashboardState,
  type ServiceNodeRow,
} from './kpiDashboardModel';


const api = useApi();
const auth = useAuth();
function handleLogout() { auth.logout(); }

// Time range state
const timeRange = ref('today');
const startDate = ref('');
const endDate = ref('');
const isCustomRange = computed(() => timeRange.value === 'custom');

// Verdict state
const verdictLead = ref('等待诊断结论...');
const verdictState = ref<'pass' | 'warning' | 'fail' | 'pending'>('pending');
const verdictSupport = ref('更新后会基于目标、阈值和历史生成结论。');
const snapshotTime = ref('--');

// Scorecard state
const scoreDepartureValue = ref('-');
const scoreGapValue = ref('-');
const scoreTurnValue = ref('-');
const scoreServiceValue = ref('-');
const scoreDepartureTarget = '90%';
const scoreTurnThreshold = '15 分';
const scoreServiceTarget = '95%';

// Decision strip
const decisionAttainment = ref('等待更新');
const decisionSource = ref('等待诊断');
const decisionNextStep = ref('等待建议');

// Trend state
const trendData = ref<Array<{ label: string; value: number; targetGap: number }>>([]);
const trendBoardLead = ref('等待趋势数据...');
const trendBoardSupport = ref('近 7 日会被转换为"离目标还有多远"的信号。');
const trendDeltaBadge = ref('等待更新');
const trendMeta = ref('目标 90%');

// Time pressure state
const hourlyData = ref<Array<{ hour: string; value: number }>>([]);
const timePressureLead = ref('等待时段诊断...');
const timePressureSupport = ref('更新后定位最该回看的时段。');
const hourlyMeta = ref('等待更新');

// Distribution/Tail state
const distributionData = ref<Array<{ label: string; value: number }>>([]);
const tailPressureLead = ref('等待尾部诊断...');
const tailPressureSupport = ref('更新后判断尾差是否超过阈值。');
const distributionMeta = ref('等待更新');

// Node signals
const p90Turnaround = ref('-');
const equipmentRate = ref('-');
const abnormalRatio = ref('-');
const turnaroundInsightText = ref('等待快照');
const equipmentInsightText = ref('等待快照');
const abnormalInsightText = ref('等待快照');
const nodesSummaryText = ref('等待节点数据');
const nodePressureLead = ref('等待节点诊断...');
const nodePressureSupport = ref('更新后定位最弱节点及其与目标的偏差。');
const serviceNodeRows = ref<ServiceNodeRow[]>([]);

const isLoading = ref(false);
const snapshotError = ref('');
const trendError = ref('');
const serviceNodeError = ref('');
const errorMessages = computed(() => [snapshotError.value, trendError.value, serviceNodeError.value].filter(Boolean));

function applyDashboardState(state: KpiDashboardState) {
  verdictLead.value = state.verdictLead;
  verdictState.value = state.verdictState;
  verdictSupport.value = state.verdictSupport;
  snapshotTime.value = state.snapshotTime;
  scoreDepartureValue.value = state.scoreDepartureValue;
  scoreGapValue.value = state.scoreGapValue;
  scoreTurnValue.value = state.scoreTurnValue;
  scoreServiceValue.value = state.scoreServiceValue;
  decisionAttainment.value = state.decisionAttainment;
  decisionSource.value = state.decisionSource;
  decisionNextStep.value = state.decisionNextStep;
  trendData.value = state.trendData;
  trendBoardLead.value = state.trendBoardLead;
  trendBoardSupport.value = state.trendBoardSupport;
  trendDeltaBadge.value = state.trendDeltaBadge;
  trendMeta.value = state.trendMeta;
  hourlyData.value = state.hourlyData;
  timePressureLead.value = state.timePressureLead;
  timePressureSupport.value = state.timePressureSupport;
  hourlyMeta.value = state.hourlyMeta;
  distributionData.value = state.distributionData;
  tailPressureLead.value = state.tailPressureLead;
  tailPressureSupport.value = state.tailPressureSupport;
  distributionMeta.value = state.distributionMeta;
  p90Turnaround.value = state.p90Turnaround;
  equipmentRate.value = state.equipmentRate;
  abnormalRatio.value = state.abnormalRatio;
  turnaroundInsightText.value = state.turnaroundInsightText;
  equipmentInsightText.value = state.equipmentInsightText;
  abnormalInsightText.value = state.abnormalInsightText;
  nodesSummaryText.value = state.nodesSummaryText;
  nodePressureLead.value = state.nodePressureLead;
  nodePressureSupport.value = state.nodePressureSupport;
}

function applyServiceNodeRows(rows: ServiceNodeRow[]) {
  serviceNodeRows.value = rows;
  const summary = summarizeServiceNodes(rows);
  nodesSummaryText.value = summary.nodesSummaryText;
  nodePressureLead.value = summary.nodePressureLead;
  nodePressureSupport.value = summary.nodePressureSupport;
}

function readErrorMessage(payload: unknown, fallback: string): string {
  if (typeof payload === 'string' && payload.trim()) {
    return payload;
  }
  if (payload && typeof payload === 'object') {
    const record = payload as Record<string, unknown>;
    const detail = record.detail;
    if (typeof detail === 'string' && detail.trim()) {
      return detail;
    }
    const message = record.message ?? record.error;
    if (typeof message === 'string' && message.trim()) {
      return message;
    }
  }
  return fallback;
}

// ============================================================
// Fetch KPI data
// ============================================================
function todayDateString(): string {
  return new Date().toISOString().slice(0, 10);
}

function dateDiffDays(start: string, end: string): number {
  const startMs = new Date(`${start}T00:00:00`).getTime();
  const endMs = new Date(`${end}T00:00:00`).getTime();
  if (!Number.isFinite(startMs) || !Number.isFinite(endMs) || endMs < startMs) {
    return 7;
  }
  return Math.min(90, Math.max(1, Math.round((endMs - startMs) / 86_400_000) + 1));
}

function trendDaysForRange(): number {
  if (timeRange.value === 'today') return 1;
  if (timeRange.value === 'this_week') return 7;
  if (timeRange.value === 'this_month') return 30;
  if (startDate.value && endDate.value) return dateDiffDays(startDate.value, endDate.value);
  return 7;
}

function serviceNodeDateForRange(): string {
  if (timeRange.value === 'custom') {
    return endDate.value || startDate.value || todayDateString();
  }
  return todayDateString();
}

function snapshotQueryString(): string {
  const params = new URLSearchParams();
  if (timeRange.value !== 'custom') {
    params.set('time_range', timeRange.value);
  } else {
    params.set('time_range', 'custom');
    if (startDate.value) params.set('start_date', startDate.value);
    if (endDate.value) params.set('end_date', endDate.value);
  }
  return params.toString();
}

async function fetchSnapshot() {
  snapshotError.value = '';
  try {
    const query = snapshotQueryString();
    const res = await api.get<Record<string, unknown>>(`/api/v2/kpi/snapshot${query ? `?${query}` : ''}`);
    if (!res.ok || !res.data) {
      throw new Error(readErrorMessage(res.data, `KPI 快照加载失败: HTTP ${res.status}`));
    }

    applyDashboardState(mapKpiSnapshot(res.data));

  } catch (e) {
    console.warn('Failed to fetch KPI snapshot:', e);
    snapshotError.value = e instanceof Error ? e.message : 'KPI 快照加载失败';
  }
}

async function fetchTrend() {
  trendError.value = '';
  try {
    const params = new URLSearchParams({
      metric: 'on_time_rate',
      days: String(trendDaysForRange()),
    });
    const res = await api.get<unknown>(`/api/v2/kpi/trend?${params.toString()}`);
    if (!res.ok || !res.data) {
      throw new Error(readErrorMessage(res.data, `趋势数据加载失败: HTTP ${res.status}`));
    }
    trendData.value = mapKpiTrend(res.data);
  } catch (e) {
    console.warn('Failed to fetch KPI trend:', e);
    trendError.value = e instanceof Error ? e.message : '趋势数据加载失败';
  }
}

async function fetchServiceNodes() {
  serviceNodeError.value = '';
  try {
    const res = await api.get<unknown>(`/api/v2/kpi/service-nodes?date=${encodeURIComponent(serviceNodeDateForRange())}`);
    if (!res.ok || !res.data) {
      throw new Error(readErrorMessage(res.data, `服务节点加载失败: HTTP ${res.status}`));
    }
    applyServiceNodeRows(mapServiceNodes(res.data));
  } catch (e) {
    console.warn('Failed to fetch service nodes:', e);
    serviceNodeError.value = e instanceof Error ? e.message : '服务节点加载失败';
  }
}

async function refreshDashboard() {
  if (isLoading.value) return;
  isLoading.value = true;
  try {
    await Promise.all([fetchSnapshot(), fetchTrend(), fetchServiceNodes()]);
  } finally {
    isLoading.value = false;
  }
}

// ============================================================
// Time range change handler
// ============================================================
function handleTimeRangeChange() {
  if (!isCustomRange.value) {
    startDate.value = '';
    endDate.value = '';
  }
  void refreshDashboard();
}

function handleCustomDateChange() {
  if (isCustomRange.value && startDate.value && endDate.value) {
    void refreshDashboard();
  }
}

// ============================================================
// Lifecycle
// ============================================================
onMounted(async () => {
  await refreshDashboard();
});
</script>

<template>
  <div class="workspace-page data-hub-page kpi-dashboard-page">
    <header class="utility-bar dashboard-topbar">
      <div class="utility-main">
        <a :href="pageUrl('dashboard')" class="home-link" title="返回工作台">
          <SvgIcon src="/frontend/icons/home.svg" :size="18" label="返回" />
        </a>
        <div class="dashboard-title-block">
          <span class="page-kicker">Performance Analytics</span>
          <span class="page-title">KPI 诊断台</span>
          <span class="utility-note">所有核心数字都必须带参照系，而不是裸指标</span>
        </div>
        <nav class="utility-nav" aria-label="分析导航">
          <a class="active" :href="pageUrl('kpi_dashboard')">KPI</a>
          <a :href="pageUrl('operations_review_report')">运行复盘</a>
        </nav>
      </div>
    </header>

    <section class="panel dashboard-ribbon kpi-toolbar-panel">
      <div class="dashboard-ribbon-copy">
        <span class="section-eyebrow">诊断模式</span>
        <h2 class="dashboard-ribbon-title">
          KPI 诊断台
        </h2>
      </div>
      <div class="dashboard-ribbon-actions kpi-toolbar-actions">
        <div class="kpi-toolbar-group kpi-range-group">
          <select
            id="timeRange"
            v-model="timeRange"
            aria-label="时间范围"
            @change="handleTimeRangeChange"
          >
            <option value="today">
              今日
            </option>
            <option value="this_week">
              本周
            </option>
            <option value="this_month">
              本月
            </option>
            <option value="custom">
              自定义
            </option>
          </select>
        </div>
        <div id="customRangeGroup" class="kpi-toolbar-group kpi-custom-range" :class="{ 'is-hidden': !isCustomRange }">
          <input
            id="startDate"
            v-model="startDate"
            type="date"
            :disabled="!isCustomRange"
            @change="handleCustomDateChange"
          >
          <input
            id="endDate"
            v-model="endDate"
            type="date"
            :disabled="!isCustomRange"
            @change="handleCustomDateChange"
          >
        </div>
        <div class="kpi-toolbar-group kpi-action-group">
          <button
            id="refreshBtn"
            class="btn primary"
            :disabled="isLoading"
            @click="refreshDashboard"
          >
            {{ isLoading ? '更新中...' : '更新数据' }}
          </button>
          <button id="logoutBtn" class="btn" @click="handleLogout">
            退出
          </button>
        </div>
      </div>
    </section>

    <section class="panel verdict-panel">
      <div v-if="errorMessages.length" class="inline-error" role="alert">
        <div v-for="message in errorMessages" :key="message">
          {{ message }}
        </div>
      </div>
      <div class="section-headline">
        <div class="section-heading-block">
          <h2 class="section-title">
            3 秒判断
          </h2>
        </div>
        <span id="snapshotTime" class="section-meta">{{ snapshotTime }}</span>
      </div>
      <div class="decision-strip">
        <article class="decision-pill">
          <span class="decision-label">今天达标没有</span>
          <strong id="decisionAttainment" :class="verdictState">{{ decisionAttainment }}</strong>
        </article>
        <article class="decision-pill">
          <span class="decision-label">差距来自哪里</span>
          <strong id="decisionSource">{{ decisionSource }}</strong>
        </article>
        <article class="decision-pill">
          <span class="decision-label">下一步查哪层</span>
          <strong id="decisionNextStep">{{ decisionNextStep }}</strong>
        </article>
      </div>
      <div class="verdict-layout">
        <div class="verdict-copy">
          <span class="verdict-kicker">Today's Verdict</span>
          <div class="verdict-head">
            <strong id="verdictLead" class="verdict-lead">{{ verdictLead }}</strong>
            <span id="verdictStateChip" class="verdict-chip" :class="`is-${verdictState}`">
              {{ verdictState === 'pending' ? '待更新' : verdictState === 'pass' ? '达标' : verdictState === 'warning' ? '警告' : '未达标' }}
            </span>
          </div>
          <p id="verdictSupport" class="verdict-support">
            {{ verdictSupport }}
          </p>
        </div>
        <div class="scoreboard">
          <article class="score-cell">
            <span class="score-label">出港准点率</span>
            <strong id="scoreDepartureValue">{{ scoreDepartureValue }}</strong>
            <span id="scoreDepartureNote">目标 {{ scoreDepartureTarget }}</span>
          </article>
          <article class="score-cell">
            <span class="score-label">距目标差</span>
            <strong id="scoreGapValue">{{ scoreGapValue }}</strong>
            <span id="scoreGapNote">参照历史末值</span>
          </article>
          <article class="score-cell">
            <span class="score-label">过站尾差</span>
            <strong id="scoreTurnValue">{{ scoreTurnValue }}</strong>
            <span id="scoreTurnNote">阈值 {{ scoreTurnThreshold }}</span>
          </article>
          <article class="score-cell">
            <span class="score-label">服务稳定度</span>
            <strong id="scoreServiceValue">{{ scoreServiceValue }}</strong>
            <span id="scoreServiceNote">目标 {{ scoreServiceTarget }}</span>
          </article>
        </div>
      </div>
    </section>

    <section id="kpiGrid" class="diagnostic-board">
      <!-- Trend panel -->
      <article class="panel trend-stage spotlight-panel">
        <div class="section-headline">
          <div class="section-heading-block">
            <h2 class="section-title">
              目标差趋势
            </h2>
          </div>
          <span id="trendMeta" class="section-meta">{{ trendMeta }}</span>
        </div>
        <div class="trend-context">
          <div class="layer-summary">
            <strong id="trendBoardLead">{{ trendBoardLead }}</strong>
            <p id="trendBoardSupport">
              {{ trendBoardSupport }}
            </p>
          </div>
          <span id="trendDeltaBadge" class="delta-badge">{{ trendDeltaBadge }}</span>
        </div>
        <div class="chart-shell chart-shell-trend">
          <div id="trendBars" style="width:100%;min-height:260px">
            <div v-if="trendData.length > 0" class="trend-bar-list">
              <div v-for="point in trendData" :key="point.label" class="trend-bar-item">
                <span class="trend-bar-label">{{ point.label }}</span>
                <div class="trend-bar-track">
                  <div
                    class="trend-bar-fill"
                    :style="{ width: Math.max(0, point.value) + '%', background: point.value >= 90 ? '#34c759' : point.value >= 85 ? '#ff9500' : '#d64545' }"
                  />
                </div>
                <span class="trend-bar-value">{{ point.value }}%</span>
              </div>
            </div>
            <div v-else class="chart-placeholder-text">
              暂无趋势数据
            </div>
          </div>
        </div>
      </article>

      <!-- Time pressure panel -->
      <article class="panel time-layer-panel">
        <div class="section-headline">
          <div class="section-heading-block">
            <h2 class="section-title">
              时间压力
            </h2>
          </div>
          <span id="hourlyMeta" class="section-meta">{{ hourlyMeta }}</span>
        </div>
        <div class="layer-summary">
          <strong id="timePressureLead">{{ timePressureLead }}</strong>
          <p id="timePressureSupport">
            {{ timePressureSupport }}
          </p>
        </div>
        <div class="chart-shell">
          <div id="hourlyBars" style="width:100%;min-height:220px">
            <div v-if="hourlyData.length > 0" class="hourly-bar-list">
              <div v-for="h in hourlyData" :key="h.hour" class="hourly-bar-item">
                <span class="hourly-bar-label">{{ h.hour }}</span>
                <div class="hourly-bar-track">
                  <div class="hourly-bar-fill" :style="{ width: h.value + '%' }" />
                </div>
                <span class="hourly-bar-value">{{ h.value }}%</span>
              </div>
            </div>
            <div v-else class="chart-placeholder-text">
              暂无时段数据
            </div>
          </div>
        </div>
      </article>

      <!-- Tail/Distribution panel -->
      <article class="panel tail-layer-panel">
        <div class="section-headline">
          <div class="section-heading-block">
            <h2 class="section-title">
              尾部拖累
            </h2>
          </div>
          <span id="distributionMeta" class="section-meta">{{ distributionMeta }}</span>
        </div>
        <div class="layer-summary">
          <strong id="tailPressureLead">{{ tailPressureLead }}</strong>
          <p id="tailPressureSupport">
            {{ tailPressureSupport }}
          </p>
        </div>
        <div class="chart-shell">
          <div id="distributionBars" style="width:100%;min-height:220px">
            <div v-if="distributionData.length > 0" class="dist-bar-list">
              <div v-for="d in distributionData" :key="d.label" class="dist-bar-item">
                <span class="dist-bar-label">{{ d.label }}</span>
                <div class="dist-bar-track">
                  <div class="dist-bar-fill" :style="{ width: Math.min(100, d.value) + '%' }" />
                </div>
                <span class="dist-bar-value">{{ d.value }} 分</span>
              </div>
            </div>
            <div v-else class="chart-placeholder-text">
              暂无尾部数据
            </div>
          </div>
        </div>
      </article>

      <!-- Node panel -->
      <article class="panel node-layer-panel">
        <div class="section-headline">
          <div class="section-heading-block">
            <h2 class="section-title">
              节点拖累
            </h2>
          </div>
          <span id="nodesSummaryText" class="section-meta">{{ nodesSummaryText }}</span>
        </div>
        <div class="signal-grid">
          <article class="signal-card">
            <span>P90 过站</span>
            <strong id="p90Turnaround">{{ p90Turnaround }}</strong>
            <span id="turnaroundInsightText">{{ turnaroundInsightText }}</span>
          </article>
          <article class="signal-card">
            <span>设备利用率</span>
            <strong id="equipmentRate">{{ equipmentRate }}</strong>
            <span id="equipmentInsightText">{{ equipmentInsightText }}</span>
          </article>
          <article class="signal-card">
            <span>异常航班占比</span>
            <strong id="abnormalRatio">{{ abnormalRatio }}</strong>
            <span id="abnormalInsightText">{{ abnormalInsightText }}</span>
          </article>
        </div>
        <div class="layer-summary">
          <strong id="nodePressureLead">{{ nodePressureLead }}</strong>
          <p id="nodePressureSupport">
            {{ nodePressureSupport }}
          </p>
        </div>
        <div class="chart-shell chart-shell-nodes">
          <div class="chart-scroll">
            <div id="serviceNodes" class="line-list">
              <div v-if="serviceNodeRows.length > 0">
                <div v-for="node in serviceNodeRows" :key="node.id" class="line-item">
                  <div class="node-header">
                    <span>{{ node.label }}</span>
                    <strong class="node-value" :class="`is-${node.status}`">{{ node.displayValue }}</strong>
                  </div>
                  <div class="track">
                    <div class="fill" :class="`is-${node.status}`" :style="{ width: `${Math.min(100, Math.max(0, node.value))}%` }" />
                  </div>
                </div>
              </div>
              <div v-else class="chart-placeholder-text">
                暂无节点明细
              </div>
            </div>
          </div>
        </div>
      </article>
    </section>
    <ThemeToggle />
  </div>
</template>

<style scoped>
.page { min-height: 100vh; background: var(--bg-sidebar); }
.utility-bar { display: flex; align-items: center; padding: 12px 24px; background: var(--bg-card); border-bottom: 1px solid var(--border-light); }
.dashboard-topbar .utility-main { display: flex; align-items: center; gap: 16px; }
.home-link { display: flex; align-items: center; justify-content: center; width: 32px; height: 32px; border-radius: 8px; background: var(--ws-surface-muted); color: var(--text-tertiary); }
.dashboard-title-block { display: flex; flex-direction: column; }
.page-kicker { font-size: 11px; color: var(--system-gray2); text-transform: uppercase; letter-spacing: 0.5px; }
.page-title { font-size: 20px; font-weight: 700; color: var(--text-primary); }
.utility-note { font-size: 12px; color: var(--system-gray2); }
.utility-nav { display: flex; gap: 4px; }
.utility-nav a { padding: 8px 16px; border-radius: 8px; text-decoration: none; color: var(--text-tertiary); font-size: 13px; transition: all 0.2s; }
.utility-nav a:hover { background: var(--ws-surface-muted); color: var(--text-primary); }
.utility-nav a.active { background: var(--system-blue); color: var(--text-inverse); }
.dashboard-ribbon { display: flex; justify-content: space-between; align-items: center; padding: 16px 24px; margin: 16px 24px; background: var(--bg-card); border-radius: 12px; box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05); }
.section-eyebrow { font-size: 11px; color: var(--system-gray2); text-transform: uppercase; }
.dashboard-ribbon-title { font-size: 18px; font-weight: 600; color: var(--text-primary); margin: 4px 0 0; }
.kpi-toolbar-actions { display: flex; gap: 12px; align-items: center; }
.kpi-toolbar-group select { padding: 8px 12px; border: 1px solid var(--border-light); border-radius: 8px; font-size: 13px; }
.btn { padding: 8px 16px; border-radius: 8px; font-size: 13px; font-weight: 500; border: 1px solid var(--border-light); background: var(--bg-card); cursor: pointer; }
.btn.primary { background: var(--system-blue); color: var(--text-inverse); border-color: var(--system-blue); }
.btn:disabled { opacity: 0.6; cursor: not-allowed; }
.is-hidden { display: none; }
.verdict-panel { margin: 0 24px 16px; padding: 20px 24px; background: var(--bg-card); border-radius: 12px; box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05); }
.inline-error { margin-bottom: 14px; padding: 10px 12px; border: 1px solid #fecaca; border-radius: 8px; background: var(--dh-signal-critical-soft); color: var(--ws-danger); font-size: 13px; }
.section-headline { display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px; }
.section-title { font-size: 16px; font-weight: 600; color: var(--text-primary); }
.section-meta { font-size: 12px; color: var(--system-gray2); }
.decision-strip { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; margin-bottom: 20px; }
.decision-pill { padding: 16px; background: var(--bg-page); border-radius: 10px; }
.decision-label { display: block; font-size: 12px; color: var(--text-tertiary); margin-bottom: 6px; }
.decision-pill strong { display: block; font-size: 15px; color: var(--text-primary); }
.decision-pill strong.pass { color: var(--system-green); }
.decision-pill strong.warning { color: var(--system-orange); }
.decision-pill strong.fail { color: var(--system-red); }
.verdict-layout { display: flex; gap: 24px; }
.verdict-copy { flex: 1; }
.verdict-kicker { font-size: 11px; color: var(--system-gray2); text-transform: uppercase; }
.verdict-head { display: flex; align-items: center; gap: 12px; margin: 8px 0; }
.verdict-lead { font-size: 18px; color: var(--text-primary); }
.verdict-chip { padding: 4px 10px; border-radius: 12px; font-size: 11px; font-weight: 600; }
.verdict-chip.is-pending { background: var(--ws-surface-muted); color: var(--text-tertiary); }
.verdict-chip.is-pass { background: var(--success-bg-subtle); color: var(--system-green); }
.verdict-chip.is-warning { background: rgba(255, 149, 0, 0.1); color: var(--system-orange); }
.verdict-chip.is-fail { background: rgba(214, 69, 69, 0.1); color: #d64545; }
.verdict-support { font-size: 13px; color: var(--text-tertiary); margin: 0; }
.scoreboard { display: grid; grid-template-columns: repeat(2, 1fr); gap: 12px; width: 280px; }
.score-cell { padding: 12px; background: var(--bg-page); border-radius: 8px; }
.score-label { display: block; font-size: 11px; color: var(--text-tertiary); }
.score-cell strong { display: block; font-size: 20px; color: var(--text-primary); margin: 4px 0; }
.score-cell span { font-size: 11px; color: var(--system-gray2); }
.diagnostic-board { display: grid; grid-template-columns: repeat(2, 1fr); gap: 16px; padding: 0 24px 24px; }
.panel { background: var(--bg-card); border-radius: 12px; padding: 20px; box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05); }
.trend-stage { grid-column: span 2; }
.spotlight-panel { border-left: 4px solid var(--system-blue); }
.trend-context { display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px; }
.layer-summary strong { display: block; font-size: 14px; color: var(--text-primary); }
.layer-summary p { font-size: 12px; color: var(--text-tertiary); margin: 4px 0 0; }
.delta-badge { padding: 4px 10px; background: var(--ws-surface-muted); border-radius: 8px; font-size: 12px; color: var(--text-tertiary); }
.chart-shell { background: var(--ws-surface-muted); border-radius: 8px; min-height: 200px; padding: 12px; }
.signal-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; margin-bottom: 16px; }
.signal-card { padding: 12px; background: var(--bg-page); border-radius: 8px; text-align: center; }
.signal-card span { display: block; font-size: 11px; color: var(--text-tertiary); }
.signal-card strong { display: block; font-size: 18px; color: var(--text-primary); margin: 4px 0; }
.chart-scroll { max-height: 200px; overflow-y: auto; }
.chart-placeholder-text { display: flex; align-items: center; justify-content: center; height: 100%; min-height: 120px; color: var(--system-gray2); font-size: 13px; }

/* Trend bars */
.trend-bar-list { display: flex; flex-direction: column; gap: 8px; }
.trend-bar-item { display: flex; align-items: center; gap: 8px; }
.trend-bar-label { width: 40px; font-size: 12px; color: var(--text-tertiary); }
.trend-bar-track { flex: 1; height: 18px; background: var(--bg-input); border-radius: 4px; overflow: hidden; }
.trend-bar-fill { height: 100%; border-radius: 4px; transition: width 0.3s; }
.trend-bar-value { width: 45px; font-size: 12px; font-weight: 600; text-align: right; }

/* Hourly bars */
.hourly-bar-list { display: flex; flex-direction: column; gap: 6px; }
.hourly-bar-item { display: flex; align-items: center; gap: 8px; }
.hourly-bar-label { width: 50px; font-size: 11px; color: var(--text-tertiary); }
.hourly-bar-track { flex: 1; height: 14px; background: var(--bg-input); border-radius: 3px; overflow: hidden; }
.hourly-bar-fill { height: 100%; background: var(--system-blue); border-radius: 3px; transition: width 0.3s; }
.hourly-bar-value { width: 40px; font-size: 11px; font-weight: 600; text-align: right; }

/* Distribution bars */
.dist-bar-list { display: flex; flex-direction: column; gap: 6px; }
.dist-bar-item { display: flex; align-items: center; gap: 8px; }
.dist-bar-label { width: 40px; font-size: 12px; color: var(--text-tertiary); }
.dist-bar-track { flex: 1; height: 14px; background: var(--bg-input); border-radius: 3px; overflow: hidden; }
.dist-bar-fill { height: 100%; background: var(--system-orange); border-radius: 3px; transition: width 0.3s; }
.dist-bar-value { width: 50px; font-size: 11px; font-weight: 600; text-align: right; }

.svg-icon { width: 32px; height: 32px; vertical-align: middle; }
</style>
