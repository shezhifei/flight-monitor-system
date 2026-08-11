<script setup lang="ts">
import { pageUrl } from '@/shared/page-routes';
import '@/styles/main.css';
import { useOpsReview } from '@/composables/useOpsReview';
import { useAuth } from '@/composables/useAuth';
import { useToast } from '@/composables/useToast';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import SvgIcon from '@/components/ui/SvgIcon.vue';
import { ref, computed } from 'vue';
import { downloadTextFile } from '@/lib/download';

const {
  loading,
  error,
  baselineData,
  trendData,
  kpiComparison,
  aiReport,
  generatingReport,
  replayRunning,
  replayEvents,
  fetchBaselineCompare,
  fetchKpiCompare,
  fetchTrendWithAnomalies,
  generateReport,
  runReplay,
} = useOpsReview();

const auth = useAuth();
const toast = useToast();
function handleLogout() { auth.logout(); }

const baselineDate = ref(new Date().toISOString().slice(0, 10));
const weatherCategory = ref('normal');
const currentStep = ref(0);
const replaySliderValue = ref(0);
const baseStartDate = ref('');
const baseEndDate = ref('');
const compareStartDate = ref('');
const compareEndDate = ref('');

const currentEvent = computed(() => replayEvents.value[currentStep.value] ?? null);

async function handleRefreshAll() {
  await Promise.all([
    fetchBaselineCompare({ date: baselineDate.value, weather: weatherCategory.value }),
    fetchTrendWithAnomalies(),
  ]);
}

async function handleLoadBaseline() {
  await fetchBaselineCompare({ date: baselineDate.value, weather: weatherCategory.value });
}

async function handleRunReplay() {
  const events = await runReplay(baselineDate.value, weatherCategory.value);
  currentStep.value = 0;
  replaySliderValue.value = events.length > 0 ? 0 : 0;
}

async function handleLoadKpiCompare() {
  await fetchKpiCompare({
    baseStartDate: baseStartDate.value,
    baseEndDate: baseEndDate.value,
    compareStartDate: compareStartDate.value,
    compareEndDate: compareEndDate.value,
  });
}

function stepForward() {
  if (currentStep.value < Math.max(0, replayEvents.value.length - 1)) {
    currentStep.value++;
    replaySliderValue.value = currentStep.value;
  }
}

function exportMarkdown() {
  const content = aiReport.value || '';
  try {
    downloadTextFile({ content, filename: 'ops-review-report.md', mimeType: 'text/markdown;charset=utf-8' });
    toast.showToast('success', 'Markdown 复盘报告已导出', { duration: 3200 });
  } catch (error) {
    toast.showToast('error', `导出失败: ${error instanceof Error ? error.message : String(error)}`, { duration: 5000 });
  }
}

function exportJson() {
  const data = { baseline: baselineData.value, kpiComparison: kpiComparison.value, trendData: trendData.value, aiReport: aiReport.value };
  try {
    downloadTextFile({
      content: JSON.stringify(data, null, 2),
      filename: 'ops-review-report.json',
      mimeType: 'application/json;charset=utf-8',
    });
    toast.showToast('success', 'JSON 复盘数据已导出', { duration: 3200 });
  } catch (error) {
    toast.showToast('error', `导出失败: ${error instanceof Error ? error.message : String(error)}`, { duration: 5000 });
  }
}

function onSliderInput(e: Event) {
  const val = Number((e.target as HTMLInputElement).value);
  replaySliderValue.value = val;
  currentStep.value = val;
}
</script>

<template>
  <div class="workspace-page operations-review-page">
    <div class="page">
      <header class="utility-bar">
        <div class="utility-main">
          <a :href="pageUrl('dashboard')" class="home-link" title="返回工作台">
            <SvgIcon src="/frontend/icons/home.svg" :size="18" label="返回" />
          </a>
          <span class="page-title">运行复盘</span>
          <nav class="utility-nav" aria-label="分析导航">
            <a :href="pageUrl('kpi_dashboard')">KPI</a>
            <a class="active" :href="pageUrl('operations_review_report')">复盘</a>
          </nav>
        </div>
        <div class="utility-actions">
          <button
            id="refreshAllBtn"
            class="btn primary"
            :disabled="loading"
            @click="handleRefreshAll"
          >
            {{ loading ? '刷新中...' : '刷新全部' }}
          </button>
          <button id="logoutBtn" class="btn" @click="handleLogout">
            退出登录
          </button>
        </div>
      </header>

      <section v-if="error" class="inline-error" role="alert">
        {{ error }}
      </section>

      <section class="ops-top-grid">
        <article class="panel replay-panel">
          <div class="section-headline">
            <h2 class="section-title">
              回放控制台
            </h2>
            <span id="replayMetaText" class="section-meta">{{ replayRunning ? '回放中...' : (baselineData ? '已加载' : '未加载') }}</span>
          </div>
          <div class="controls-row">
            <button
              id="reloadReplayBtn"
              class="btn primary"
              :disabled="loading || replayRunning"
              @click="handleRunReplay"
            >
              拉取事件
            </button>
            <button
              id="playBtn"
              class="btn"
              :disabled="replayRunning"
              @click="handleRunReplay()"
            >
              {{ replayRunning ? '回放中...' : '播放' }}
            </button>
            <button id="stepBtn" class="btn" @click="stepForward">
              下一步
            </button>
          </div>
          <input
            id="replaySlider"
            class="replay-slider"
            type="range"
            min="0"
            :max="Math.max(0, replayEvents.length - 1)"
            :value="replaySliderValue"
            @input="onSliderInput"
          >
          <div class="summary-grid">
            <div class="summary-item">
              <span class="summary-label">回放事件总数</span><span id="summaryEventTotal" class="summary-value">{{ baselineData?.totalEvents ?? '-' }}</span>
            </div>
            <div class="summary-item">
              <span class="summary-label">航班更新事件</span><span id="summaryFlightEvents" class="summary-value">{{ baselineData?.flightEvents ?? '-' }}</span>
            </div>
            <div class="summary-item">
              <span class="summary-label">异常事件</span><span id="summaryAnomalyEvents" class="summary-value">{{ baselineData?.anomalyEvents ?? '-' }}</span>
            </div>
            <div class="summary-item">
              <span class="summary-label">调度冲突</span><span id="summaryDispatchConflicts" class="summary-value">{{ baselineData?.dispatchConflicts ?? '-' }}</span>
            </div>
          </div>
        </article>

        <article class="panel baseline-panel">
          <div class="section-headline">
            <h2 class="section-title">
              基线偏离诊断
            </h2>
            <span id="baselineCompareMeta" class="section-meta">{{ loading ? '加载中...' : '选择日期与天气类别' }}</span>
          </div>
          <div class="panel-controls baseline-controls">
            <input
              id="baselineDate"
              v-model="baselineDate"
              type="date"
              aria-label="目标日期"
            >
            <select id="weatherCategory" v-model="weatherCategory" aria-label="天气类别">
              <option value="normal">
                晴好
              </option>
              <option value="rain">
                雨天
              </option>
              <option value="storm">
                暴风雨
              </option>
              <option value="snow">
                雪天
              </option>
            </select>
            <button
              id="loadBaselineBtn"
              class="btn primary"
              :disabled="loading"
              @click="handleLoadBaseline"
            >
              加载图表
            </button>
          </div>
          <div id="baselineChart" />
          <div id="baselineAlerts" class="inline-alert" />
        </article>
      </section>

      <section class="ops-main-grid">
        <article class="panel events-panel">
          <div class="section-headline">
            <h2 class="section-title">
              事件链路调查
            </h2>
            <span class="section-meta">列表与详情联动</span>
          </div>
          <div class="event-split-layout">
            <div id="replayEventList" class="list">
              <template v-if="replayEvents.length">
                <div
                  v-for="(evt, idx) in replayEvents"
                  :key="idx"
                  class="event-item"
                  :class="{ active: currentStep === idx }"
                  @click="currentStep = idx; replaySliderValue = idx"
                >
                  {{ evt.title }}
                </div>
              </template>
              <div v-else style="text-align:center;padding:24px;color:#94a3b8;">
                暂无事件数据
              </div>
            </div>
            <div class="detail-box">
              <div class="row-head">
                <strong id="eventDetailTitle" class="detail-title">{{ currentEvent?.title || '当前事件' }}</strong>
                <span id="eventDetailTag" class="chip info">{{ currentEvent?.level || 'INFO' }}</span>
              </div>
              <div id="eventDetailSub" class="status-line detail-sub">
                {{ currentEvent?.subtitle || currentEvent?.description || '-' }}
              </div>
              <pre id="eventDetailJson" class="detail-json">{{ currentEvent ? JSON.stringify(currentEvent, null, 2) : '{}' }}</pre>
            </div>
          </div>
        </article>

        <div class="ops-right-stack">
          <article class="panel compare-panel">
            <div class="section-headline">
              <h2 class="section-title">
                KPI 对比
              </h2>
              <span id="kpiCompareMeta" class="section-meta">{{ kpiComparison.length ? `${kpiComparison.length} 项指标` : '-' }}</span>
            </div>
            <div class="panel-controls panel-controls-grid-two">
              <input
                id="baseStartDate"
                v-model="baseStartDate"
                type="date"
                aria-label="基线开始日期"
                title="基线开始时间"
              >
              <input
                id="baseEndDate"
                v-model="baseEndDate"
                type="date"
                aria-label="基线结束日期"
                title="基线结束时间"
              >
              <input
                id="compareStartDate"
                v-model="compareStartDate"
                type="date"
                aria-label="对比开始日期"
                title="对比开始时间"
              >
              <input
                id="compareEndDate"
                v-model="compareEndDate"
                type="date"
                aria-label="对比结束日期"
                title="对比结束时间"
              >
              <button
                id="loadKpiCompareBtn"
                class="btn primary grid-span-two"
                :disabled="loading"
                @click="handleLoadKpiCompare"
              >
                对比
              </button>
            </div>
            <table class="modern-table">
              <thead>
                <tr>
                  <th>指标</th>
                  <th>基线</th>
                  <th>对比</th>
                  <th>变化</th>
                </tr>
              </thead>
              <tbody id="kpiCompareRows">
                <template v-if="kpiComparison.length">
                  <tr v-for="(kpi, idx) in kpiComparison" :key="idx">
                    <td style="padding:8px 12px;border-bottom:1px solid #f1f5f9;">
                      {{ kpi.metric || kpi.name }}
                    </td>
                    <td style="padding:8px 12px;border-bottom:1px solid #f1f5f9;">
                      {{ kpi.baseline ?? '-' }}
                    </td>
                    <td style="padding:8px 12px;border-bottom:1px solid #f1f5f9;">
                      {{ kpi.compare ?? kpi.current ?? '-' }}
                    </td>
                    <td style="padding:8px 12px;border-bottom:1px solid #f1f5f9;" :style="{ color: (kpi.change ?? 0) > 0 ? '#22c55e' : (kpi.change ?? 0) < 0 ? '#ef4444' : '#64748b', fontWeight: 600 }">
                      {{ (kpi.change ?? 0) > 0 ? '+' : '' }}{{ kpi.change ?? '-' }}
                    </td>
                  </tr>
                </template>
                <tr v-else>
                  <td colspan="4" style="text-align:center;padding:24px;color:#94a3b8;">
                    {{ loading ? '加载中...' : '暂无对比数据' }}
                  </td>
                </tr>
              </tbody>
            </table>
            <div class="section-headline section-subhead">
              <h3 class="section-title">
                趋势与异常叠加
              </h3>
              <span id="trendMeta" class="section-meta">{{ trendData.length ? `${trendData.length} 个数据点` : '-' }}</span>
            </div>
            <div id="trendOverlayList" class="trend-list">
              <template v-if="trendData.length">
                <div v-for="(item, idx) in trendData" :key="idx" style="display:flex;justify-content:space-between;padding:8px 12px;border-bottom:1px solid #f1f5f9;font-size:13px;">
                  <span>{{ item.label || item.metric || `数据点 ${idx + 1}` }}</span>
                  <span :style="{ color: item.anomaly ? '#ef4444' : 'var(--text-primary)', fontWeight: item.anomaly ? 700 : 400 }">
                    {{ item.value ?? '-' }}{{ item.anomaly ? ' ⚠' : '' }}
                  </span>
                </div>
              </template>
              <div v-else style="text-align:center;padding:24px;color:#94a3b8;">
                暂无趋势数据
              </div>
            </div>
          </article>

          <article class="panel report-panel">
            <div class="section-headline">
              <h2 class="section-title">
                复盘报告
              </h2>
              <span id="reportMeta" class="section-meta">{{ generatingReport ? '生成中...' : (aiReport ? '已生成' : '尚未生成') }}</span>
            </div>
            <div class="panel-controls report-controls">
              <button
                id="generateReportBtn"
                class="btn primary"
                :disabled="generatingReport"
                @click="generateReport()"
              >
                {{ generatingReport ? '生成中...' : '生成报告' }}
              </button>
              <button id="exportMarkdownBtn" class="btn" @click="exportMarkdown">
                导出 MD
              </button>
              <button id="exportJsonBtn" class="btn" @click="exportJson">
                导出 JSON
              </button>
            </div>
            <pre id="reportOutput" class="report-output">{{ aiReport || '点击"生成报告"后，将基于当前已加载 KPI 数据输出复盘报告。' }}</pre>
          </article>
        </div>
      </section>
    </div>
    <ThemeToggle />
  </div>
</template>

<style scoped>
.workspace-page.operations-review-page {
  min-height: 100vh;
}

.inline-error {
  margin: 16px 24px 0;
  padding: 12px 14px;
  border: 1px solid #fecaca;
  border-radius: 8px;
  background: var(--dh-signal-critical-soft);
  color: var(--ws-danger);
  font-size: 13px;
  font-weight: 600;
}
</style>
