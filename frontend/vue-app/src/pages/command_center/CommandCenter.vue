<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { useCommandCenter } from '@/composables/useCommandCenter';
import { pageUrl } from '@/shared/page-routes';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import UiBanner from '@/components/ui/UiBanner.vue';
import UiButton from '@/components/ui/UiButton.vue';
import UiPill from '@/components/ui/UiPill.vue';
import UiPlaceBar from '@/components/ui/UiPlaceBar.vue';
import UiReadout from '@/components/ui/UiReadout.vue';
import UiReadoutStrip from '@/components/ui/UiReadoutStrip.vue';
import UiSelect from '@/components/ui/UiSelect.vue';
import UiStage from '@/components/ui/UiStage.vue';
import UiToolbar from '@/components/ui/UiToolbar.vue';

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

const crumbs = [
  { label: '工作台', href: pageUrl('dashboard') },
  { label: '指挥中心' },
];

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

const windowHoursOptions = [
  { value: '2', label: '未来 2h' },
  { value: '6', label: '未来 6h' },
  { value: '12', label: '未来 12h' },
];

const refreshIntervalOptions = [
  { value: '15000', label: '15s 刷新' },
  { value: '30000', label: '30s 刷新' },
  { value: '60000', label: '60s 刷新' },
];

const verdictTone = computed<'act' | 'ok' | 'warn' | 'danger'>(() => {
  if (verdict.value.severity === 'ok') return 'ok';
  if (verdict.value.severity === 'warn') return 'warn';
  if (verdict.value.severity === 'critical') return 'danger';
  return 'act';
});

// 列表行的事态声：critical/warn/ok 回四声，其余收回墨（§5.3 状态章回语义 tone）
function severityTone(severity: string): 'ok' | 'warn' | 'danger' | undefined {
  if (severity === 'critical') return 'danger';
  if (severity === 'warn') return 'warn';
  if (severity === 'ok') return 'ok';
  return undefined;
}

const healthTone = computed<'ok' | 'warn' | 'danger'>(() => {
  const score = Number(systemHealth.value.score);
  if (!Number.isFinite(score)) return 'warn';
  if (score >= 90) return 'ok';
  if (score >= 60) return 'warn';
  return 'danger';
});

const alertMessages = computed(() => [fullscreenError.value, error.value].filter(Boolean));

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
    <UiStage label="指挥中心" pad="body" class="command-stage">
      <template #place>
        <UiPlaceBar :crumbs="crumbs" :count-label="`窗口: 未来 ${windowHours}h`">
          <template #meta>
            <UiPill id="metricSystemHealth" :tone="healthTone">
              健康度 {{ systemHealth.score }}%
            </UiPill>
            <span id="metricLastRefresh" class="command-place-meta">{{ lastRefreshTime || '未更新' }}</span>
            <span id="metricAutoRefreshSub" class="command-place-meta">{{ isPaused ? '自动刷新暂停' : '自动刷新开启' }}</span>
          </template>
        </UiPlaceBar>
      </template>

      <template #toolbar>
        <UiToolbar seek-label="观察窗口" solve-label="操作">
          <template #seek>
            <UiSelect
              id="windowHoursSelect"
              v-model="windowHours"
              :options="windowHoursOptions"
              label="窗口时长"
            />
            <UiSelect
              id="refreshIntervalSelect"
              v-model="refreshInterval"
              :options="refreshIntervalOptions"
              label="刷新间隔"
            />
          </template>
          <template #solve>
            <UiButton
              id="refreshNowBtn"
              variant="primary"
              :disabled="loading"
              @click="handleRefresh"
            >
              {{ loading ? '刷新中...' : '刷新' }}
            </UiButton>
            <UiButton id="toggleRefreshBtn" :pressed="isPaused" @click="toggleRefresh">
              {{ isPaused ? '继续' : '暂停' }}
            </UiButton>
            <UiButton id="clearEventsBtn" @click="clearEvidenceFeed">
              清空当前视图
            </UiButton>
            <UiButton
              id="fullScreenBtn"
              variant="quiet"
              :disabled="isTogglingFullscreen"
              @click="toggleFullscreen"
            >
              {{ isFullscreen ? '退出全屏' : '全屏' }}
            </UiButton>
          </template>
        </UiToolbar>
      </template>

      <!-- 升：取数/全屏失败时才打断 -->
      <template v-if="alertMessages.length" #alert>
        <UiBanner
          v-for="message in alertMessages"
          :key="message"
          tone="danger"
          role="alert"
          class="command-inline-error"
        >
          {{ message }}
        </UiBanner>
      </template>

      <!-- 态势结论：持久状态，不是升 -->
      <section class="command-section command-verdict-strip" aria-label="运行态势">
        <div class="command-verdict-copy">
          <strong id="opsVerdictTitle" class="command-verdict-title" :data-tone="verdictTone">{{ verdict.title }}</strong>
          <p id="opsVerdictDetail" class="command-verdict-detail">
            {{ verdict.detail }}
          </p>
        </div>
        <span id="opsVerdictMeta" class="command-verdict-window">{{ verdict.window }}</span>
      </section>

      <UiReadoutStrip
        class="command-stat-readouts"
        density="roomy"
        label="指挥中心关键读数"
      >
        <div class="command-stat-readout">
          <UiReadout
            id="metricDecisionCount"
            label="待决策对象"
            :value="kpis.decisionCount"
            tone="danger"
          />
          <p id="metricDecisionCountSub" class="command-stat-readout__hint">
            P1 / P2 任务数量
          </p>
        </div>
        <div class="command-stat-readout">
          <UiReadout
            id="metricRiskFlights"
            label="60 分钟高风险离港"
            :value="kpis.riskFlights"
          />
          <p id="metricRiskFlightsSub" class="command-stat-readout__hint">
            延误超过30分钟航班
          </p>
        </div>
        <div class="command-stat-readout">
          <UiReadout
            id="metricOpenAnomalies"
            label="未闭环异常"
            :value="kpis.openAnomalies"
          />
          <p id="metricOpenAnomaliesSub" class="command-stat-readout__hint">
            待处理异常报警
          </p>
        </div>
        <div class="command-stat-readout">
          <UiReadout
            id="metricDispatchBlockers"
            label="调度阻塞"
            :value="kpis.dispatchBlockers"
          />
          <p id="metricDispatchBlockersSub" class="command-stat-readout__hint">
            派工受阻/存在冲突
          </p>
        </div>
        <div class="command-stat-readout">
          <UiReadout
            id="metricDelayPressure"
            label="延误压力"
            :value="kpis.delayPressure"
            unit="m"
          />
          <p id="metricDelayPressureSub" class="command-stat-readout__hint">
            平均延误时间
          </p>
        </div>
      </UiReadoutStrip>

      <div class="command-board">
        <section class="command-section command-action-panel" aria-label="行动优先队列">
          <div class="section-headline">
            <h3 class="section-title">
              行动优先队列
            </h3>
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
              :data-tone="severityTone(item.severity)"
            >
              <strong>{{ item.name }}</strong>
              <span>{{ item.meta }}</span>
            </div>
          </div>
        </section>

        <div class="command-context-column">
          <section class="command-section command-window-panel" aria-label="未来窗口离港压力">
            <div class="section-headline">
              <h3 class="section-title">
                未来窗口离港压力
              </h3>
              <span id="windowPressureMeta" class="section-meta">未来窗口</span>
            </div>
            <div id="windowPressureList" class="distribution-strip">
              <div v-if="windowPressure.length === 0" class="empty-placeholder">
                暂无离港压力数据
              </div>
              <div
                v-for="item in windowPressure"
                v-else
                :key="item.id"
                class="distribution-row"
                :data-tone="severityTone(item.severity)"
              >
                <span>{{ item.label }}</span>
                <strong>{{ item.value }}</strong>
                <small>{{ item.detail }}</small>
              </div>
            </div>
          </section>

          <section class="command-section heatmap-panel" aria-label="异常热区">
            <div class="section-headline">
              <h3 class="section-title">
                异常热区
              </h3>
              <span id="riskHeatmapMeta" class="section-meta">按机位区间统计</span>
            </div>
            <div id="standHeatmapChart" class="heatmap-box">
              <div v-if="heatmapData.length === 0" class="empty-placeholder">
                暂无热区数据
              </div>
              <div
                v-for="item in heatmapData"
                v-else
                :key="item.id"
                class="distribution-row"
                :data-tone="severityTone(item.severity)"
              >
                <span>{{ item.label }}</span>
                <strong>{{ item.value }}</strong>
                <small>{{ item.detail }}</small>
              </div>
            </div>
          </section>

          <section class="command-section dispatch-panel" aria-label="调度负载断面">
            <div class="section-headline">
              <h3 class="section-title">
                调度负载断面
              </h3>
              <span id="dispatchWindowMeta" class="section-meta">调度窗口</span>
            </div>
            <div id="dispatchSummaryList" class="distribution-strip">
              <div v-if="dispatchLoad.length === 0" class="empty-placeholder">
                暂无负载数据
              </div>
              <div
                v-for="item in dispatchLoad"
                v-else
                :key="item.id"
                class="distribution-row"
                :data-tone="severityTone(item.severity)"
              >
                <span>{{ item.label }}</span>
                <strong>{{ item.value }}</strong>
                <small>{{ item.detail }}</small>
              </div>
            </div>
          </section>

          <section class="command-section terminal-panel" aria-label="航站楼压力">
            <div class="section-headline">
              <h3 class="section-title">
                航站楼压力
              </h3>
              <span id="terminalLoadMeta" class="section-meta">按航班数统计</span>
            </div>
            <div id="terminalLoadList" class="distribution-strip">
              <div v-if="terminalLoad.length === 0" class="empty-placeholder">
                暂无航站楼压力数据
              </div>
              <div
                v-for="item in terminalLoad"
                v-else
                :key="item.id"
                class="distribution-row"
                :data-tone="severityTone(item.severity)"
              >
                <span>{{ item.label }}</span>
                <strong>{{ item.value }}</strong>
                <small>{{ item.detail }}</small>
              </div>
            </div>
          </section>
        </div>
      </div>

      <section class="command-section command-evidence-panel" aria-label="实时证据流">
        <div class="section-headline">
          <h3 class="section-title">
            实时证据流
          </h3>
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
          >
            <span class="event-time">[{{ formatEventTime(evt.timestamp) }}]</span>
            <span class="event-text">{{ evt.message || evt.description || JSON.stringify(evt) }}</span>
          </div>
        </div>
      </section>
    </UiStage>
    <ThemeToggle />
  </div>
</template>

<style scoped>
/* 信号面 token + UI 库件（UiStage/UiPlaceBar/UiToolbar/UiReadoutStrip/UiBanner） */
.command-stage { height: 100%; }

.command-place-meta { font-size: var(--fs-label); color: var(--ink-muted); font-variant-numeric: tabular-nums; }

/* 小节：同一工作面，分隔只给一根线，不再铺第二张面 */
.command-section { padding: var(--s4) 0; }
.command-stat-readouts { padding-left: 0; padding-right: 0; border-top: 1px solid var(--line); }

.command-verdict-strip {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--s4);
}

.command-verdict-copy { flex: 1; min-width: 0; }

.command-verdict-title {
  display: block;
  font-size: var(--fs-page);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.command-verdict-title[data-tone='ok'] { color: var(--ok); }
.command-verdict-title[data-tone='warn'] { color: var(--warn); }
.command-verdict-title[data-tone='danger'] { color: var(--danger); }
.command-verdict-title[data-tone='act'] { color: var(--act); }

.command-verdict-detail {
  font-size: var(--fs-label);
  color: var(--ink-subtle);
  margin: var(--s1) 0 0;
}

.command-verdict-window { font-size: var(--fs-label); color: var(--ink-muted); }

.command-stat-readout { display: grid; gap: var(--s2); min-width: 0; }

.command-stat-readout__hint {
  margin: 0;
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

.command-board {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(340px, 400px);
  gap: 0 var(--s4);
  border-top: 1px solid var(--line);
}

.command-board .command-section { border-top: 0; }
.command-context-column .command-section + .command-section { border-top: 1px solid var(--line); }

.command-context-column {
  display: flex;
  flex-direction: column;
  border-left: 1px solid var(--line);
  padding-left: var(--s4);
}

.section-headline {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: var(--s3);
  margin-bottom: var(--s3);
}

.section-title {
  font-size: var(--fs-section);
  font-weight: var(--fw-semibold);
  color: var(--ink);
  margin: 0;
}

.section-meta { font-size: var(--fs-label); color: var(--ink-muted); }

.empty-placeholder {
  padding: var(--s5);
  text-align: center;
  color: var(--ink-muted);
  font-size: var(--fs-body);
}

.priority-list,
.distribution-strip,
.heatmap-box {
  display: flex;
  flex-direction: column;
}

/* 图区底座最小高度走 scoped，不再逐块内联 */
#windowPressureList { min-height: 200px; }
#standHeatmapChart,
#dispatchSummaryList,
#terminalLoadList { min-height: 180px; }

/* 事态行：不描边不洗底，行与行一根线，声画在数上（§3.2/§4.21） */
.priority-item,
.distribution-row {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: var(--s1) var(--s3);
  align-items: baseline;
  padding: var(--s2) 0;
  border-bottom: 1px solid var(--line);
}

.priority-item:last-child,
.distribution-row:last-child { border-bottom: 0; }

.priority-item strong,
.distribution-row span {
  color: var(--ink);
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
}

.priority-item span,
.distribution-row small {
  color: var(--ink-muted);
  font-size: var(--fs-label);
}

.distribution-row strong {
  color: var(--ink);
  font-size: var(--fs-title);
  font-family: var(--mono);
  font-variant-numeric: tabular-nums;
}

.priority-item[data-tone='ok'] strong,
.distribution-row[data-tone='ok'] strong { color: var(--ok); }
.priority-item[data-tone='warn'] strong,
.distribution-row[data-tone='warn'] strong { color: var(--warn); }
.priority-item[data-tone='danger'] strong,
.distribution-row[data-tone='danger'] strong { color: var(--danger); }

.command-evidence-panel { border-top: 1px solid var(--line); }

.event-timeline {
  max-height: 300px;
  overflow-y: auto;
}

.event-item {
  padding: var(--s2) 0;
  border-bottom: 1px solid var(--line);
  font-size: var(--fs-body);
}

.event-item:last-child { border-bottom: 0; }

.event-time {
  color: var(--ink-muted);
  margin-right: var(--s2);
  font-family: var(--mono);
  font-variant-numeric: tabular-nums;
}

.event-text { color: var(--ink); }

@media (max-width: 1439px) {
  .command-board { grid-template-columns: minmax(0, 1fr); }
  .command-context-column { border-left: 0; padding-left: 0; }
}

@media (max-width: 1099px) {
  .command-verdict-strip { flex-direction: column; align-items: flex-start; gap: var(--s2); }
}
</style>
