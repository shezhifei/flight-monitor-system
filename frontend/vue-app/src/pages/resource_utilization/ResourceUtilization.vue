<script setup lang="ts">
// ResourceUtilization Page - Structural shell migrated from resource_utilization.html
// Business logic (ECharts, analytics) deferred to later tasks
import { pageUrl } from '@/shared/page-routes';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import UiBanner from '@/components/ui/UiBanner.vue';
import UiButton from '@/components/ui/UiButton.vue';
import UiPill from '@/components/ui/UiPill.vue';
import UiPlaceBar from '@/components/ui/UiPlaceBar.vue';
import UiReadout from '@/components/ui/UiReadout.vue';
import UiReadoutStrip from '@/components/ui/UiReadoutStrip.vue';
import UiStage from '@/components/ui/UiStage.vue';
import { useResourceUtilization } from '@/composables/useResourceUtilization';
import { useAuth } from '@/composables/useAuth';

const { loading, snapshot, bottlenecks, error, actionSuggestions, reviewCadence, fetchSnapshot } = useResourceUtilization();

const auth = useAuth();

// 地点条：面包屑只报地点
const crumbs = [
  { label: '工作台', href: pageUrl('dashboard') },
  { label: '资源利用率' },
];

function utilizationPercent(value: unknown): number {
  const numeric = typeof value === 'number' && Number.isFinite(value) ? value : 0;
  return Math.abs(numeric) <= 1 ? numeric * 100 : numeric;
}

function formatUtilization(value: unknown): string {
  return `${utilizationPercent(value).toFixed(1)}%`;
}

type UtilizationTone = 'ok' | 'warn' | 'danger';

/** 声由 CSS 解析：这里只说这个数「说了什么」，不说它是什么颜色。 */
function utilizationTone(value: unknown): UtilizationTone {
  const percent = utilizationPercent(value);
  if (percent > 85) return 'danger';
  if (percent > 60) return 'warn';
  return 'ok';
}

/** 建议严重度回四声：critical=危，high=警（§5.3 状态章回语义 tone） */
function actionTone(severity: 'critical' | 'high'): 'danger' | 'warn' {
  return severity === 'critical' ? 'danger' : 'warn';
}

function peakUtilization(): number {
  return snapshot.value.length ? Math.max(...snapshot.value.map((row) => utilizationPercent(row.utilization))) : 0;
}
</script>

<template>
  <div class="workspace-page data-hub-page resource-utilization-page">
    <UiStage label="资源利用率" pad="body" aside-width="320px" class="resource-stage">
      <template #place>
        <!-- page-title 类透传到地点条根，e2e 按它定位页题（等价于旧页头） -->
        <UiPlaceBar class="page-title" :crumbs="crumbs">
          <template #meta>
            <nav class="utility-nav" aria-label="资源导航">
              <a :href="pageUrl('dispatch_board')">甘特调度</a>
              <a aria-current="page" :href="pageUrl('resource_utilization')">资源利用率</a>
            </nav>
            <UiPill id="resourceStateChip" :tone="loading ? 'warn' : snapshot.length ? 'ok' : 'mute'">
              {{ loading ? '加载中...' : (snapshot.length ? '已加载' : '等待快照') }}
            </UiPill>
            <UiPill id="lastUpdated" tone="mute">
              {{ loading ? '刷新中...' : '已更新' }}
            </UiPill>
            <UiButton
              id="refreshBtn"
              variant="primary"
              :disabled="loading"
              @click="fetchSnapshot()"
            >
              刷新
            </UiButton>
            <UiButton id="logoutBtn" variant="quiet" @click="auth.logout()">
              退出登录
            </UiButton>
          </template>
        </UiPlaceBar>
      </template>

      <!-- 升：快照失败时才打断 -->
      <template v-if="error" #alert>
        <UiBanner tone="danger" role="alert" class="inline-error">
          {{ error }}
        </UiBanner>
      </template>

      <!-- 主体：工作面延续，小节之间只隔一根线 -->
      <section class="resource-section" aria-label="资源瓶颈台">
        <div class="section-headline">
          <h2 id="resourceHeadlineLead" class="section-title">
            资源瓶颈台
          </h2>
          <UiPill id="resourceCadenceChip" tone="mute">
            复查节奏 --
          </UiPill>
        </div>
        <UiReadoutStrip density="roomy" label="资源瓶颈关键读数" class="resource-hero-strip">
          <div class="resource-readout">
            <UiReadout
              id="metricBottleneckDimension"
              label="当前瓶颈"
              :value="bottlenecks.length ? bottlenecks[0]?.name || '-' : '-'"
              :tone="bottlenecks.length ? 'danger' : 'ink'"
            />
            <p id="metricBottleneckSub" class="resource-readout__hint">
              {{ bottlenecks.length ? `${bottlenecks.length} 个瓶颈对象` : '等待统一判断' }}
            </p>
          </div>
          <div class="resource-readout">
            <UiReadout
              id="metricPeakRate"
              label="峰值占用"
              :value="snapshot.length ? `${peakUtilization().toFixed(1)}%` : '-'"
            />
            <p id="metricPeakSub" class="resource-readout__hint">
              等待峰值对象
            </p>
          </div>
          <div class="resource-readout">
            <UiReadout
              id="metricOverloaded"
              label="超阈值对象"
              :value="bottlenecks.length || '-'"
              tone="warn"
            />
            <p id="metricOverloadedSub" class="resource-readout__hint">
              80% 以上对象数
            </p>
          </div>
          <div class="resource-readout">
            <UiReadout
              id="metricHeadroom"
              label="系统余量"
              :value="snapshot.length ? `${Math.max(0, 100 - peakUtilization()).toFixed(1)}%` : '-'"
            />
            <p id="metricHeadroomSub" class="resource-readout__hint">
              按主瓶颈平均值估算
            </p>
          </div>
        </UiReadoutStrip>
      </section>

      <section class="resource-section bottleneck-ladder-panel" aria-label="对象排序">
        <div class="section-headline">
          <h2 class="section-title">
            对象排序
          </h2>
          <span id="leaderboardMeta" class="section-meta">{{ snapshot.length ? `${snapshot.length} 个对象` : '等待资源快照' }}</span>
        </div>
        <div id="bottleneckLeaderboard" class="bottleneck-ladder">
          <template v-if="snapshot.length">
            <div
              v-for="(item, idx) in snapshot"
              :key="idx"
              class="ladder-row"
            >
              <span>{{ item.name || item.dimension || `对象 ${idx + 1}` }}</span>
              <span class="ladder-value" :data-tone="utilizationTone(item.utilization)">{{ formatUtilization(item.utilization) }}</span>
            </div>
          </template>
          <div v-else class="empty">
            {{ loading ? '加载中...' : '等待加载资源数据...' }}
          </div>
        </div>
      </section>

      <div class="resource-diagnostic-grid">
        <section class="resource-section dimension-panel" aria-label="维度对比">
          <div class="section-headline">
            <h2 class="section-title">
              维度对比
            </h2>
            <span id="dimensionPanelMeta" class="section-meta">统一口径</span>
          </div>
          <div id="dimensionComparisonList" class="dimension-card-grid">
            <template v-if="snapshot.length">
              <UiReadout
                v-for="(item, idx) in snapshot"
                :key="idx"
                :label="item.dimension || item.name || `对象 ${idx + 1}`"
                :value="formatUtilization(item.utilization)"
              />
            </template>
            <div v-else class="empty">
              暂无维度数据
            </div>
          </div>
          <div id="dimensionRadarChart" />
        </section>

        <section class="resource-section pattern-panel" aria-label="瓶颈形态">
          <div class="section-headline">
            <h2 class="section-title">
              瓶颈形态
            </h2>
            <span id="patternPanelMeta" class="section-meta">{{ bottlenecks.length ? `${bottlenecks.length} 个瓶颈` : '等待研判' }}</span>
          </div>
          <div id="pressurePatternList" class="pattern-strip">
            <template v-if="bottlenecks.length">
              <div v-for="(b, idx) in bottlenecks" :key="idx" class="pattern-card">
                <strong class="pattern-card__name">{{ b.name || b.dimension }}</strong>
                <span class="pattern-card__value">{{ formatUtilization(b.utilization) }}</span>
              </div>
            </template>
            <div v-else class="empty">
              {{ loading ? '研判中...' : '等待研判' }}
            </div>
          </div>
          <div id="patternBarChart" />
        </section>
      </div>

      <!-- 旁路：判断与动作降为页底一级（凹面），不再嵌第二张工作面 -->
      <template #aside>
        <div class="resource-insight-stack">
          <section class="resource-insight-group" aria-label="当前最需处理">
            <h3 class="resource-insight-title">
              当前最需处理
            </h3>
            <div class="insight-callout resource-callout">
              <strong id="resourceRiskLead">{{ bottlenecks.length ? `最紧迫: ${bottlenecks[0]?.name || bottlenecks[0]?.dimension} (${((bottlenecks[0]?.utilization || 0) * 100).toFixed(1)}%)` : '等待资源快照...' }}</strong>
              <p id="resourceRiskSupport">
                {{ bottlenecks.length ? `共 ${bottlenecks.length} 个对象超过 85% 阈值，需优先处理。` : '刷新后将根据峰值、超阈值数量和扩散程度给出判断。' }}
              </p>
            </div>
          </section>

          <section class="resource-insight-group" aria-label="立即动作">
            <h3 class="resource-insight-title">
              立即动作
            </h3>
            <div id="actionSummaryList" class="action-list">
              <template v-if="actionSuggestions.length">
                <div
                  v-for="action in actionSuggestions"
                  :key="action.id"
                  class="action-suggestion"
                  :data-tone="actionTone(action.severity)"
                >
                  <strong>{{ action.title }}</strong>
                  <span>{{ action.detail }}</span>
                </div>
              </template>
              <div v-else class="empty">
                {{ loading ? '生成建议中...' : '当前无超阈值对象' }}
              </div>
            </div>
          </section>

          <section class="resource-insight-group" aria-label="复查节奏">
            <h3 class="resource-insight-title">
              复查节奏
            </h3>
            <div id="cadenceList" class="cadence-list">
              <div v-for="item in reviewCadence" :key="item.id" class="cadence-item">
                <strong>{{ item.title }}</strong>
                <span>{{ item.detail }}</span>
              </div>
            </div>
          </section>

          <section class="resource-insight-group" aria-label="负载口径">
            <h3 class="resource-insight-title">
              负载口径
            </h3>
            <div class="threshold-list">
              <div class="threshold-item">
                <span class="threshold-dot low" /><span class="threshold-label">0%-29%</span><span class="threshold-desc">低负荷，可延后关注</span>
              </div>
              <div class="threshold-item">
                <span class="threshold-dot medium" /><span class="threshold-label">30%-59%</span><span class="threshold-desc">可承载，保持巡检</span>
              </div>
              <div class="threshold-item">
                <span class="threshold-dot high" /><span class="threshold-label">60%-79%</span><span class="threshold-desc">进入紧张区，准备调整</span>
              </div>
              <div class="threshold-item">
                <span class="threshold-dot critical" /><span class="threshold-label">80%-100%</span><span class="threshold-desc">拥塞风险，优先处置</span>
              </div>
            </div>
          </section>
        </div>
      </template>
    </UiStage>
    <ThemeToggle />
  </div>
</template>

<style scoped>
/* 图区底座尺寸走 scoped，不再内联 */
#dimensionRadarChart { width: 100%; min-height: 240px; }
#patternBarChart { width: 100%; min-height: 200px; }
/* 信号面 token + UI 库件（UiStage/UiPlaceBar/UiReadoutStrip/UiPill/UiBanner） */

.resource-stage { height: 100%; }

/* 地点条 meta 里的跨页导航：当前位置是持守（aria-current），不是动词 */
.utility-nav { display: flex; gap: var(--s1); }
.utility-nav a { padding: var(--s1) var(--s3); border-radius: var(--r-control); text-decoration: none; color: var(--ink-muted); font-size: var(--fs-label); }
.utility-nav a:hover { background: color-mix(in srgb, var(--ink) 8%, transparent); color: var(--ink); }
.utility-nav a[aria-current='page'] { background: var(--act-soft); color: var(--act); }
.utility-nav a:focus-visible { outline: 2px solid var(--act); outline-offset: 2px; }

/* 小节：同一工作面，分隔只给一根线 */
.resource-section { padding: var(--s4) 0; }
.resource-section + .resource-section,
.resource-diagnostic-grid { border-top: 1px solid var(--line); }

.resource-hero-strip { padding-left: 0; padding-right: 0; }

/* 读数下的参照系小字，不是第二张卡 */
.resource-readout { display: grid; gap: var(--s1); min-width: 0; }
.resource-readout__hint { margin: 0; font-size: var(--fs-label); color: var(--ink-muted); }

.section-headline { display: flex; justify-content: space-between; align-items: baseline; gap: var(--s3); margin-bottom: var(--s3); }
.section-title { font-size: var(--fs-title); font-weight: var(--fw-semibold); color: var(--ink); margin: 0; }
.section-meta { font-size: var(--fs-label); color: var(--ink-muted); }

/* 对象排序：行间一根线，声画在数上 */
.bottleneck-ladder { min-height: 200px; }
.ladder-row { display: flex; justify-content: space-between; align-items: baseline; gap: var(--s3); padding: var(--s2) 0; border-bottom: 1px solid var(--line); }
.ladder-row:last-child { border-bottom: 0; }

/* 数比名重一档（§3.2）；声由 data-tone 出，JS 不碰颜色。 */
.ladder-value { font-weight: var(--fw-semibold); font-family: var(--mono); font-variant-numeric: tabular-nums; }
.ladder-value[data-tone='ok'] { color: var(--ok); }
.ladder-value[data-tone='warn'] { color: var(--warn); }
.ladder-value[data-tone='danger'] { color: var(--danger); }

.resource-diagnostic-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 0 var(--s4); }
.resource-diagnostic-grid .resource-section { border-top: 0; }
.resource-diagnostic-grid .resource-section + .resource-section { border-left: 1px solid var(--line); padding-left: var(--s4); }

/* 维度读数：读数自己不描边不换圆角（UiReadout 的约定），排布走网格 */
.dimension-card-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--s3) var(--s4); min-height: 100px; }

/* 瓶颈形态：声画在对象上（左边一条危声 + 淡衬），不是 chrome 着色 */
.pattern-strip { display: grid; gap: var(--s2); min-height: 100px; }
.pattern-card { padding: var(--s2) var(--s3); background: var(--danger-soft); border-radius: var(--r-control); border-left: 3px solid var(--danger); }
.pattern-card__name { font-size: var(--fs-body); color: var(--ink); }
.pattern-card__value { margin-left: var(--s2); font-size: var(--fs-label); color: var(--danger); font-variant-numeric: tabular-nums; }

/* 旁路（判断与动作）：UiStage aside 已降为页底，组与组一根线 */
.resource-insight-stack { display: grid; padding: var(--s3) var(--s4); }
.resource-insight-group { padding: var(--s3) 0; }
.resource-insight-group + .resource-insight-group { border-top: 1px solid var(--line); }
.resource-insight-title { margin: 0 0 var(--s2); font-size: var(--fs-label); font-weight: var(--fw-semibold); color: var(--ink); }

.insight-callout strong { display: block; font-size: var(--fs-section); color: var(--ink); }
.insight-callout p { font-size: var(--fs-label); color: var(--ink-muted); margin: var(--s2) 0 0; }

.action-list,
.cadence-list,
.threshold-list { display: grid; gap: var(--s2); }

.action-suggestion { display: flex; flex-direction: column; gap: var(--s1); padding: var(--s2) 0; border-bottom: 1px solid var(--line); }
.action-suggestion:last-child { border-bottom: 0; }
.action-suggestion strong { font-size: var(--fs-body); color: var(--ink); }
.action-suggestion[data-tone='warn'] strong { color: var(--warn); }
.action-suggestion[data-tone='danger'] strong { color: var(--danger); }
.action-suggestion span { font-size: var(--fs-label); line-height: 1.5; color: var(--ink-subtle); }

.cadence-item { padding: var(--s2) 0; border-bottom: 1px solid var(--line); }
.cadence-item:last-child { border-bottom: 0; }
.cadence-item strong { display: block; font-size: var(--fs-body); color: var(--ink); }
.cadence-item span { font-size: var(--fs-label); color: var(--ink-muted); }

/* 负载口径图例：域内四声点，不升进根 token */
.threshold-item { display: grid; grid-template-columns: auto auto minmax(0, 1fr); gap: var(--s2); align-items: center; padding: var(--s2) 0; }
.threshold-dot { width: 8px; height: 8px; border-radius: var(--r-pill); }
.threshold-dot.low { background: var(--ok); }
.threshold-dot.medium { background: var(--act); }
.threshold-dot.high { background: var(--warn); }
.threshold-dot.critical { background: var(--danger); }
.threshold-label { font-size: var(--fs-label); font-weight: var(--fw-semibold); color: var(--ink); }
.threshold-desc { font-size: var(--fs-label); color: var(--ink-muted); }

.empty { text-align: center; padding: var(--s5); color: var(--ink-muted); font-size: var(--fs-body); }

@media (max-width: 1099px) {
  .resource-diagnostic-grid { grid-template-columns: minmax(0, 1fr); }
  .resource-diagnostic-grid .resource-section + .resource-section { border-left: 0; padding-left: 0; border-top: 1px solid var(--line); }
}
</style>
