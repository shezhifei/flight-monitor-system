<script setup lang="ts">
// KpiDashboard Page - KPI diagnostics with API data binding
import { onMounted, ref, computed } from 'vue';
import { useApi } from '@/composables/useApi';
import { useAuth } from '@/composables/useAuth';
import { pageUrl } from '@/shared/page-routes';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import UiBanner from '@/components/ui/UiBanner.vue';
import UiButton from '@/components/ui/UiButton.vue';
import UiFacts from '@/components/ui/UiFacts.vue';
import UiPill from '@/components/ui/UiPill.vue';
import UiPlaceBar from '@/components/ui/UiPlaceBar.vue';
import UiReadout from '@/components/ui/UiReadout.vue';
import UiReadoutStrip from '@/components/ui/UiReadoutStrip.vue';
import UiSelect from '@/components/ui/UiSelect.vue';
import UiStage from '@/components/ui/UiStage.vue';
import UiToolbar from '@/components/ui/UiToolbar.vue';
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

// 地点条：面包屑只报地点；快照时刻是当前谓词下的读出
const crumbs = [
  { label: '工作台', href: pageUrl('dashboard') },
  { label: 'KPI 诊断台' },
];

const timeRangeOptions = [
  { value: 'today', label: '今日' },
  { value: 'this_week', label: '本周' },
  { value: 'this_month', label: '本月' },
  { value: 'custom', label: '自定义' },
];

// Time range state
const timeRange = ref('today');
const startDate = ref('');
const endDate = ref('');
const isCustomRange = computed(() => timeRange.value === 'custom');

// Verdict state
const verdictLead = ref('等待诊断结论...');
const verdictState = ref<'pass' | 'warning' | 'fail' | 'pending'>('pending');
const verdictSupport = ref('更新后会基于目标、阈值和历史生成结论。');
const snapshotTime = ref('—');

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

const verdictTone = computed<'mute' | 'ok' | 'warn' | 'danger'>(() => {
  if (verdictState.value === 'pass') return 'ok';
  if (verdictState.value === 'warning') return 'warn';
  if (verdictState.value === 'fail') return 'danger';
  return 'mute';
});

// 「今天达标没有」的声画在值上；pending 不染声
const attainmentTone = computed<'ok' | 'warn' | 'danger' | undefined>(() => {
  if (verdictState.value === 'pass') return 'ok';
  if (verdictState.value === 'warning') return 'warn';
  if (verdictState.value === 'fail') return 'danger';
  return undefined;
});

const decisionFacts = computed(() => [
  { label: '今天达标没有', value: decisionAttainment.value },
  { label: '差距来自哪里', value: decisionSource.value },
  { label: '下一步查哪层', value: decisionNextStep.value },
]);

// 节点事态：pass/warning/fail 回四声（§5.3 状态章回语义 tone）
function nodeTone(status: ServiceNodeRow['status']): 'ok' | 'warn' | 'danger' {
  if (status === 'pass') return 'ok';
  if (status === 'warning') return 'warn';
  return 'danger';
}

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
    <UiStage label="KPI 诊断台" pad="body" class="kpi-stage">
      <template #place>
        <!-- page-title 类透传到地点条根，e2e 按它定位页题（等价于旧页头） -->
        <UiPlaceBar class="page-title" :crumbs="crumbs" :count-label="snapshotTime">
          <template #meta>
            <nav class="utility-nav" aria-label="分析导航">
              <a aria-current="page" :href="pageUrl('kpi_dashboard')">KPI</a>
              <a :href="pageUrl('operations_review_report')">运行复盘</a>
            </nav>
          </template>
        </UiPlaceBar>
      </template>

      <template #toolbar>
        <UiToolbar seek-label="诊断范围" solve-label="操作">
          <template #seek>
            <UiSelect
              id="timeRange"
              v-model="timeRange"
              :options="timeRangeOptions"
              label="时间范围"
              @update:model-value="handleTimeRangeChange"
            />
            <div id="customRangeGroup" class="kpi-toolbar-group kpi-custom-range" :class="{ 'is-hidden': !isCustomRange }">
              <input
                id="startDate"
                v-model="startDate"
                type="date"
                aria-label="开始日期"
                :disabled="!isCustomRange"
                @change="handleCustomDateChange"
              >
              <input
                id="endDate"
                v-model="endDate"
                type="date"
                aria-label="结束日期"
                :disabled="!isCustomRange"
                @change="handleCustomDateChange"
              >
            </div>
          </template>
          <template #solve>
            <UiButton
              id="refreshBtn"
              variant="primary"
              :disabled="isLoading"
              @click="refreshDashboard"
            >
              {{ isLoading ? '更新中...' : '更新数据' }}
            </UiButton>
            <UiButton id="logoutBtn" variant="quiet" @click="auth.logout()">
              退出
            </UiButton>
          </template>
        </UiToolbar>
      </template>

      <!-- 升：加载失败时才插在工具条与主体之间 -->
      <template v-if="errorMessages.length" #alert>
        <UiBanner
          v-for="message in errorMessages"
          :key="message"
          tone="danger"
          role="alert"
          class="inline-error"
        >
          {{ message }}
        </UiBanner>
      </template>

      <!-- 主体：工作面延续，小节之间只隔一根线 -->
      <section class="kpi-section verdict-section" aria-label="3 秒判断">
        <div class="section-headline">
          <h2 class="section-title">
            3 秒判断
          </h2>
        </div>
        <div class="verdict-head">
          <strong id="verdictLead" class="verdict-lead">{{ verdictLead }}</strong>
          <UiPill id="verdictStateChip" :tone="verdictTone">
            {{ verdictState === 'pending' ? '待更新' : verdictState === 'pass' ? '达标' : verdictState === 'warning' ? '警告' : '未达标' }}
          </UiPill>
        </div>
        <p id="verdictSupport" class="verdict-support">
          {{ verdictSupport }}
        </p>
        <UiFacts class="decision-facts" :columns="3" density="roomy" :items="decisionFacts">
          <template #value="{ fact, text }">
            <strong v-if="fact.label === '今天达标没有'" id="decisionAttainment" class="decision-value" :data-tone="attainmentTone">{{ text }}</strong>
            <strong v-else-if="fact.label === '差距来自哪里'" id="decisionSource" class="decision-value">{{ text }}</strong>
            <strong v-else id="decisionNextStep" class="decision-value">{{ text }}</strong>
          </template>
        </UiFacts>
        <UiReadoutStrip density="roomy" label="核心指标读数" class="scoreboard">
          <div class="kpi-readout">
            <UiReadout id="scoreDepartureValue" label="出港准点率" :value="scoreDepartureValue" />
            <p id="scoreDepartureNote" class="kpi-readout__hint">
              目标 {{ scoreDepartureTarget }}
            </p>
          </div>
          <div class="kpi-readout">
            <UiReadout id="scoreGapValue" label="距目标差" :value="scoreGapValue" />
            <p id="scoreGapNote" class="kpi-readout__hint">
              参照历史末值
            </p>
          </div>
          <div class="kpi-readout">
            <UiReadout id="scoreTurnValue" label="过站尾差" :value="scoreTurnValue" />
            <p id="scoreTurnNote" class="kpi-readout__hint">
              阈值 {{ scoreTurnThreshold }}
            </p>
          </div>
          <div class="kpi-readout">
            <UiReadout id="scoreServiceValue" label="服务稳定度" :value="scoreServiceValue" />
            <p id="scoreServiceNote" class="kpi-readout__hint">
              目标 {{ scoreServiceTarget }}
            </p>
          </div>
        </UiReadoutStrip>
      </section>

      <div id="kpiGrid" class="diagnostic-board">
        <!-- Trend section -->
        <section class="kpi-section trend-stage" aria-label="目标差趋势">
          <div class="section-headline">
            <h2 class="section-title">
              目标差趋势
            </h2>
            <span id="trendMeta" class="section-meta">{{ trendMeta }}</span>
          </div>
          <div class="trend-context">
            <div class="layer-summary">
              <strong id="trendBoardLead">{{ trendBoardLead }}</strong>
              <p id="trendBoardSupport">
                {{ trendBoardSupport }}
              </p>
            </div>
            <UiPill id="trendDeltaBadge" tone="mute">
              {{ trendDeltaBadge }}
            </UiPill>
          </div>
          <div id="trendBars">
            <div v-if="trendData.length > 0" class="trend-bar-list">
              <div v-for="point in trendData" :key="point.label" class="trend-bar-item">
                <span class="trend-bar-label">{{ point.label }}</span>
                <div class="trend-bar-track">
                  <div
                    class="trend-bar-fill"
                    :data-tone="point.value >= 90 ? 'ok' : point.value >= 85 ? 'warn' : 'danger'"
                    :style="{ width: Math.max(0, point.value) + '%' }"
                  />
                </div>
                <span class="trend-bar-value">{{ point.value }}%</span>
              </div>
            </div>
            <div v-else class="chart-placeholder-text">
              暂无趋势数据
            </div>
          </div>
        </section>

        <!-- Time pressure section -->
        <section class="kpi-section time-layer-panel" aria-label="时间压力">
          <div class="section-headline">
            <h2 class="section-title">
              时间压力
            </h2>
            <span id="hourlyMeta" class="section-meta">{{ hourlyMeta }}</span>
          </div>
          <div class="layer-summary">
            <strong id="timePressureLead">{{ timePressureLead }}</strong>
            <p id="timePressureSupport">
              {{ timePressureSupport }}
            </p>
          </div>
          <div id="hourlyBars">
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
        </section>

        <!-- Tail/Distribution section -->
        <section class="kpi-section tail-layer-panel" aria-label="尾部拖累">
          <div class="section-headline">
            <h2 class="section-title">
              尾部拖累
            </h2>
            <span id="distributionMeta" class="section-meta">{{ distributionMeta }}</span>
          </div>
          <div class="layer-summary">
            <strong id="tailPressureLead">{{ tailPressureLead }}</strong>
            <p id="tailPressureSupport">
              {{ tailPressureSupport }}
            </p>
          </div>
          <div id="distributionBars">
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
        </section>

        <!-- Node section -->
        <section class="kpi-section node-layer-panel" aria-label="节点拖累">
          <div class="section-headline">
            <h2 class="section-title">
              节点拖累
            </h2>
            <span id="nodesSummaryText" class="section-meta">{{ nodesSummaryText }}</span>
          </div>
          <UiReadoutStrip density="dense" label="节点信号读数" class="node-signals">
            <div class="kpi-readout">
              <UiReadout id="p90Turnaround" label="P90 过站" :value="p90Turnaround" />
              <p id="turnaroundInsightText" class="kpi-readout__hint">
                {{ turnaroundInsightText }}
              </p>
            </div>
            <div class="kpi-readout">
              <UiReadout id="equipmentRate" label="设备利用率" :value="equipmentRate" />
              <p id="equipmentInsightText" class="kpi-readout__hint">
                {{ equipmentInsightText }}
              </p>
            </div>
            <div class="kpi-readout">
              <UiReadout id="abnormalRatio" label="异常航班占比" :value="abnormalRatio" />
              <p id="abnormalInsightText" class="kpi-readout__hint">
                {{ abnormalInsightText }}
              </p>
            </div>
          </UiReadoutStrip>
          <div class="layer-summary">
            <strong id="nodePressureLead">{{ nodePressureLead }}</strong>
            <p id="nodePressureSupport">
              {{ nodePressureSupport }}
            </p>
          </div>
          <div class="chart-scroll">
            <div id="serviceNodes" class="line-list">
              <template v-if="serviceNodeRows.length > 0">
                <div v-for="node in serviceNodeRows" :key="node.id" class="line-item">
                  <div class="node-header">
                    <span>{{ node.label }}</span>
                    <strong class="node-value" :data-tone="nodeTone(node.status)">{{ node.displayValue }}</strong>
                  </div>
                  <div class="track">
                    <div class="fill" :data-tone="nodeTone(node.status)" :style="{ width: `${Math.min(100, Math.max(0, node.value))}%` }" />
                  </div>
                </div>
              </template>
              <div v-else class="chart-placeholder-text">
                暂无节点明细
              </div>
            </div>
          </div>
        </section>
      </div>
    </UiStage>
    <ThemeToggle />
  </div>
</template>

<style scoped>
/* 仪器占满滚动口外高：地点条/工具条常驻，主体自滚（§3.1） */
.kpi-stage { height: 100%; }

/* 地点条 meta 里的跨页导航：当前位置是持守（aria-current），不是动词 */
.utility-nav { display: flex; gap: var(--s1); }
.utility-nav a { padding: var(--s1) var(--s3); border-radius: var(--r-control); text-decoration: none; color: var(--ink-muted); font-size: var(--fs-label); }
.utility-nav a:hover { background: color-mix(in srgb, var(--ink) 8%, transparent); color: var(--ink); }
.utility-nav a[aria-current='page'] { background: var(--act-soft); color: var(--act); }
.utility-nav a:focus-visible { outline: 2px solid var(--act); outline-offset: 2px; }

/* 自定义时段：与工具条齐高 32 */
.kpi-custom-range { display: inline-flex; align-items: center; gap: var(--s2); }
.kpi-custom-range input { height: var(--h-sm); padding: 0 var(--s2); border: 1px solid var(--line-strong); border-radius: var(--r-control); font-size: var(--fs-label); color: var(--ink); background: var(--face-page); }
.kpi-custom-range input:focus-visible { outline: 2px solid var(--act); outline-offset: 2px; }
.is-hidden { display: none; }

.inline-error + .inline-error { margin-top: var(--s2); }

/* 小节：同一工作面上的分隔只给一根线，不再铺第二张面 */
.kpi-section { padding: var(--s4) 0; }
.kpi-section + .kpi-section { border-top: 1px solid var(--line); }
.section-headline { display: flex; justify-content: space-between; align-items: baseline; gap: var(--s3); margin-bottom: var(--s3); }
.section-title { font-size: var(--fs-title); font-weight: var(--fw-semibold); color: var(--ink); margin: 0; }
.section-meta { font-size: var(--fs-label); color: var(--ink-muted); }

.verdict-head { display: flex; align-items: center; gap: var(--s3); margin-bottom: var(--s2); }
.verdict-lead { font-size: var(--fs-page); font-weight: var(--fw-semibold); color: var(--ink); }
.verdict-support { font-size: var(--fs-body); color: var(--ink-subtle); margin: 0 0 var(--s4); }

/* 三问（事实格）与读数条之间只隔一根线 */
.decision-facts { margin-bottom: var(--s4); padding-bottom: var(--s4); border-bottom: 1px solid var(--line); }
.decision-value { font-weight: var(--fw-semibold); }
.decision-value[data-tone='ok'] { color: var(--ok); }
.decision-value[data-tone='warn'] { color: var(--warn); }
.decision-value[data-tone='danger'] { color: var(--danger); }

/* 读数下的参照系小字（目标/阈值），不是第二张卡 */
.kpi-readout { display: grid; gap: var(--s1); min-width: 0; }
.kpi-readout__hint { margin: 0; font-size: var(--fs-label); color: var(--ink-muted); }

.node-signals { margin-bottom: var(--s3); padding-left: 0; padding-right: 0; }
.scoreboard { padding-left: 0; padding-right: 0; }

.diagnostic-board { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 0 var(--s4); }
.trend-stage { grid-column: span 2; }
.trend-context { display: flex; justify-content: space-between; align-items: flex-start; gap: var(--s3); margin-bottom: var(--s3); }
.layer-summary strong { display: block; font-size: var(--fs-section); font-weight: var(--fw-semibold); color: var(--ink); }
.layer-summary p { font-size: var(--fs-label); color: var(--ink-muted); margin: var(--s1) 0 0; }
.chart-scroll { max-height: 200px; overflow-y: auto; }
.chart-placeholder-text { display: flex; align-items: center; justify-content: center; height: 100%; min-height: 120px; color: var(--ink-muted); font-size: var(--fs-body); }

/* Trend bars */
.trend-bar-list { display: flex; flex-direction: column; gap: var(--s2); }
.trend-bar-item { display: flex; align-items: center; gap: var(--s2); }
.trend-bar-label { width: 40px; font-size: var(--fs-label); color: var(--ink-muted); }
.trend-bar-track { flex: 1; height: 18px; background: color-mix(in srgb, var(--ink) 8%, transparent); border-radius: 4px; overflow: hidden; }
.trend-bar-fill { height: 100%; border-radius: 4px; transition: width var(--t-mid) var(--ease); }
.trend-bar-fill[data-tone='ok'] { background: var(--ok); }
.trend-bar-fill[data-tone='warn'] { background: var(--warn); }
.trend-bar-fill[data-tone='danger'] { background: var(--danger); }
/* 图区底座：宽高走 scoped，不再逐块内联 */
#trendBars { width: 100%; min-height: 260px; }
#hourlyBars, #distributionBars { width: 100%; min-height: 220px; }
.trend-bar-value { width: 45px; font-size: var(--fs-label); font-weight: var(--fw-semibold); text-align: right; }

/* Hourly bars */
.hourly-bar-list { display: flex; flex-direction: column; gap: var(--s2); }
.hourly-bar-item { display: flex; align-items: center; gap: var(--s2); }
.hourly-bar-label { width: 50px; font-size: var(--fs-label); color: var(--ink-muted); }
.hourly-bar-track { flex: 1; height: 14px; background: color-mix(in srgb, var(--ink) 8%, transparent); border-radius: 3px; overflow: hidden; }
.hourly-bar-fill { height: 100%; background: var(--act); border-radius: 3px; transition: width var(--t-mid) var(--ease); }
.hourly-bar-value { width: 40px; font-size: var(--fs-label); font-weight: var(--fw-semibold); text-align: right; }

/* Distribution bars */
.dist-bar-list { display: flex; flex-direction: column; gap: var(--s2); }
.dist-bar-item { display: flex; align-items: center; gap: var(--s2); }
.dist-bar-label { width: 40px; font-size: var(--fs-label); color: var(--ink-muted); }
.dist-bar-track { flex: 1; height: 14px; background: color-mix(in srgb, var(--ink) 8%, transparent); border-radius: 3px; overflow: hidden; }
.dist-bar-fill { height: 100%; background: var(--warn); border-radius: 3px; transition: width var(--t-mid) var(--ease); }
.dist-bar-value { width: 50px; font-size: var(--fs-label); font-weight: var(--fw-semibold); text-align: right; }

/* 节点明细：行间一根线，事态画在值与进度条上 */
.line-list { display: grid; }
.line-item { display: grid; gap: var(--s1); padding: var(--s2) 0; border-bottom: 1px solid var(--line); }
.line-item:last-child { border-bottom: 0; }
.node-header { display: flex; justify-content: space-between; gap: var(--s2); color: var(--ink); font-size: var(--fs-label); }
.node-value { font-weight: var(--fw-semibold); font-family: var(--mono); font-variant-numeric: tabular-nums; }
.node-value[data-tone='ok'] { color: var(--ok); }
.node-value[data-tone='warn'] { color: var(--warn); }
.node-value[data-tone='danger'] { color: var(--danger); }
.track { height: 6px; border-radius: var(--r-pill); background: color-mix(in srgb, var(--ink) 8%, transparent); overflow: hidden; }
.fill { display: block; height: 100%; border-radius: inherit; transition: width var(--t-mid) var(--ease); }
.fill[data-tone='ok'] { background: var(--ok); }
.fill[data-tone='warn'] { background: var(--warn); }
.fill[data-tone='danger'] { background: var(--danger); }

@media (max-width: 1099px) {
  .diagnostic-board { grid-template-columns: minmax(0, 1fr); }
  .trend-stage { grid-column: auto; }
}
</style>
