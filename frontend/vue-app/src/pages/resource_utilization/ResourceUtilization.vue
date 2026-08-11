<script setup lang="ts">
// ResourceUtilization Page - Structural shell migrated from resource_utilization.html
// Business logic (ECharts, analytics) deferred to later tasks
import { pageUrl } from '@/shared/page-routes';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import SvgIcon from '@/components/ui/SvgIcon.vue';
import { useResourceUtilization } from '@/composables/useResourceUtilization';
import { useAuth } from '@/composables/useAuth';

const { loading, snapshot, bottlenecks, error, actionSuggestions, reviewCadence, fetchSnapshot } = useResourceUtilization();

const auth = useAuth();
function handleLogout() { auth.logout(); }

function utilizationPercent(value: unknown): number {
  const numeric = typeof value === 'number' && Number.isFinite(value) ? value : 0;
  return Math.abs(numeric) <= 1 ? numeric * 100 : numeric;
}

function formatUtilization(value: unknown): string {
  return `${utilizationPercent(value).toFixed(1)}%`;
}

function utilizationColor(value: unknown): string {
  const percent = utilizationPercent(value);
  if (percent > 85) return '#ef4444';
  if (percent > 60) return '#f59e0b';
  return '#22c55e';
}

function peakUtilization(): number {
  return snapshot.value.length ? Math.max(...snapshot.value.map((row) => utilizationPercent(row.utilization))) : 0;
}
</script>

<template>
  <div class="workspace-page data-hub-page resource-utilization-page">
    <header class="utility-bar dashboard-topbar">
      <div class="utility-main">
        <a :href="pageUrl('dashboard')" class="home-link" title="返回工作台">
          <SvgIcon src="/frontend/icons/home.svg" :size="18" label="返回" />
        </a>
        <div class="dashboard-title-block">
          <span class="page-kicker">Bottleneck Board</span>
          <span class="page-title">资源利用率</span>
          <span class="utility-note">面向资源调度、保障经理与排班负责人的瓶颈判断台</span>
        </div>
        <nav class="utility-nav" aria-label="资源导航">
          <a :href="pageUrl('dispatch_board')">甘特调度</a>
          <a class="active" :href="pageUrl('resource_utilization')">资源利用率</a>
        </nav>
      </div>
      <div class="utility-actions">
        <span id="lastUpdated" class="state-chip">{{ loading ? '刷新中...' : '已更新' }}</span>
        <button
          id="refreshBtn"
          class="btn primary"
          type="button"
          @click="fetchSnapshot()"
        >
          刷新
        </button>
        <button
          id="logoutBtn"
          class="btn"
          type="button"
          @click="handleLogout"
        >
          退出登录
        </button>
      </div>
    </header>

    <section class="panel dashboard-ribbon resource-ribbon resource-bottleneck-banner">
      <div class="dashboard-ribbon-copy">
        <span class="section-eyebrow">资源瓶颈台</span>
        <h2 id="resourceHeadlineLead" class="dashboard-ribbon-title">
          资源瓶颈台
        </h2>
      </div>
      <div class="dashboard-ribbon-actions resource-ribbon-notes">
        <span id="resourceStateChip" class="state-chip accent">{{ loading ? '加载中...' : (snapshot.length ? '已加载' : '等待快照') }}</span>
        <span id="resourceCadenceChip" class="meta-chip">复查节奏 --</span>
      </div>
    </section>

    <section v-if="error" class="inline-error" role="alert">
      {{ error }}
    </section>

    <section class="panel resource-metrics resource-hero-strip">
      <article class="metric-item hero-metric metric-strong bottleneck-hero">
        <span class="metric-label">当前瓶颈</span>
        <span id="metricBottleneckDimension" class="metric-value">{{ bottlenecks.length ? bottlenecks[0]?.name || '-' : '-' }}</span>
        <span id="metricBottleneckSub" class="metric-sub">{{ bottlenecks.length ? `${bottlenecks.length} 个瓶颈对象` : '等待统一判断' }}</span>
      </article>
      <article class="metric-item hero-metric">
        <span class="metric-label">峰值占用</span>
        <span id="metricPeakRate" class="metric-value">{{ snapshot.length ? `${peakUtilization().toFixed(1)}%` : '-' }}</span>
        <span id="metricPeakSub" class="metric-sub">等待峰值对象</span>
      </article>
      <article class="metric-item hero-metric">
        <span class="metric-label">超阈值对象</span>
        <span id="metricOverloaded" class="metric-value">{{ bottlenecks.length || '-' }}</span>
        <span id="metricOverloadedSub" class="metric-sub">80% 以上对象数</span>
      </article>
      <article class="metric-item hero-metric">
        <span class="metric-label">系统余量</span>
        <span id="metricHeadroom" class="metric-value">{{ snapshot.length ? `${Math.max(0, 100 - peakUtilization()).toFixed(1)}%` : '-' }}</span>
        <span id="metricHeadroomSub" class="metric-sub">按主瓶颈平均值估算</span>
      </article>
    </section>

    <section class="resource-board">
      <div class="resource-main-stage">
        <article class="panel bottleneck-ladder-panel spotlight-panel">
          <div class="section-headline">
            <div class="section-heading-block">
              <h2 class="section-title">
                对象排序
              </h2>
            </div>
            <span id="leaderboardMeta" class="section-meta">{{ snapshot.length ? `${snapshot.length} 个对象` : '等待资源快照' }}</span>
          </div>
          <div id="bottleneckLeaderboard" class="bottleneck-ladder">
            <template v-if="snapshot.length">
              <div
                v-for="(item, idx) in snapshot"
                :key="idx"
                class="ladder-row"
                style="display:flex;justify-content:space-between;padding:8px 12px;border-bottom:1px solid #e2e8f0;"
              >
                <span>{{ item.name || item.dimension || `对象 ${idx + 1}` }}</span>
                <span :style="{ color: utilizationColor(item.utilization), fontWeight: 600 }">{{ formatUtilization(item.utilization) }}</span>
              </div>
            </template>
            <div v-else class="empty chart-empty">
              {{ loading ? '加载中...' : '等待加载资源数据...' }}
            </div>
          </div>
        </article>

        <div class="resource-diagnostic-grid">
          <article class="panel dimension-panel">
            <div class="section-headline">
              <div class="section-heading-block">
                <h2 class="section-title">
                  维度对比
                </h2>
              </div>
              <span id="dimensionPanelMeta" class="section-meta">统一口径</span>
            </div>
            <div id="dimensionComparisonList" class="dimension-card-grid">
              <template v-if="snapshot.length">
                <div v-for="(item, idx) in snapshot" :key="idx" style="padding:12px;background:var(--bg-page);border-radius:8px;">
                  <div style="font-size:12px;color:#64748b;">
                    {{ item.dimension || item.name }}
                  </div>
                  <div style="font-size:18px;font-weight:700;color:#0f172a;">
                    {{ formatUtilization(item.utilization) }}
                  </div>
                </div>
              </template>
              <div v-else class="empty chart-empty">
                暂无维度数据
              </div>
            </div>
            <div id="dimensionRadarChart" style="width:100%;min-height:240px" />
          </article>

          <article class="panel pattern-panel">
            <div class="section-headline">
              <div class="section-heading-block">
                <h2 class="section-title">
                  瓶颈形态
                </h2>
              </div>
              <span id="patternPanelMeta" class="section-meta">{{ bottlenecks.length ? `${bottlenecks.length} 个瓶颈` : '等待研判' }}</span>
            </div>
            <div id="pressurePatternList" class="distribution-strip pattern-strip">
              <template v-if="bottlenecks.length">
                <div v-for="(b, idx) in bottlenecks" :key="idx" style="padding:8px 12px;background:#fef2f2;border-radius:8px;border-left:3px solid #ef4444;">
                  <strong style="font-size:13px;color:#0f172a;">{{ b.name || b.dimension }}</strong>
                  <span style="font-size:12px;color:#ef4444;margin-left:8px;">{{ formatUtilization(b.utilization) }}</span>
                </div>
              </template>
              <div v-else class="empty chart-empty">
                {{ loading ? '研判中...' : '等待研判' }}
              </div>
            </div>
            <div id="patternBarChart" style="width:100%;min-height:200px" />
          </article>
        </div>
      </div>

      <aside class="panel insight-panel resource-risk-sidebar">
        <div class="section-headline">
          <div class="section-heading-block">
            <h2 class="section-title">
              判断与动作
            </h2>
          </div>
          <span id="resourceSidebarMeta" class="section-meta">统一判断</span>
        </div>
        <div class="resource-insight-stack">
          <section class="resource-insight-group">
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

          <section class="resource-insight-group">
            <h3 class="resource-insight-title">
              立即动作
            </h3>
            <div id="actionSummaryList" class="distribution-strip">
              <template v-if="actionSuggestions.length">
                <div
                  v-for="action in actionSuggestions"
                  :key="action.id"
                  class="action-suggestion"
                  :class="`severity-${action.severity}`"
                >
                  <strong>{{ action.title }}</strong>
                  <span>{{ action.detail }}</span>
                </div>
              </template>
              <div v-else class="empty chart-empty">
                {{ loading ? '生成建议中...' : '当前无超阈值对象' }}
              </div>
            </div>
          </section>

          <section class="resource-insight-group">
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

          <section class="resource-insight-group">
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
      </aside>
    </section>
    <ThemeToggle />
  </div>
</template>

<style scoped>
/* ResourceUtilization specific styles */
.page {
  min-height: 100vh;
  background: var(--bg-sidebar);
}

.utility-bar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 24px;
  background: var(--bg-card);
  border-bottom: 1px solid var(--border-light);
}

.dashboard-topbar .utility-main {
  display: flex;
  align-items: center;
  gap: 16px;
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
}

.dashboard-title-block {
  display: flex;
  flex-direction: column;
}

.page-kicker {
  font-size: 11px;
  color: var(--system-gray2);
  text-transform: uppercase;
}

.page-title {
  font-size: 20px;
  font-weight: 700;
  color: var(--text-primary);
}

.utility-note {
  font-size: 12px;
  color: var(--system-gray2);
}

.utility-nav {
  display: flex;
  gap: 4px;
}

.utility-nav a {
  padding: 8px 16px;
  border-radius: 8px;
  text-decoration: none;
  color: var(--text-tertiary);
  font-size: 13px;
}

.utility-nav a:hover {
  background: var(--ws-surface-muted);
  color: var(--text-primary);
}

.utility-nav a.active {
  background: var(--system-blue);
  color: var(--text-inverse);
}

.utility-actions {
  display: flex;
  align-items: center;
  gap: 12px;
}

.state-chip {
  padding: 4px 10px;
  border-radius: 12px;
  font-size: 12px;
  background: var(--ws-surface-muted);
  color: var(--text-tertiary);
}

.state-chip.accent {
  background: var(--system-blue-subtle);
  color: var(--system-blue);
}

.btn {
  padding: 8px 16px;
  border-radius: 8px;
  font-size: 13px;
  font-weight: 500;
  border: 1px solid var(--border-light);
  background: var(--bg-card);
  cursor: pointer;
}

.btn.primary {
  background: var(--system-blue);
  color: var(--text-inverse);
  border-color: var(--system-blue);
}

.dashboard-ribbon {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 24px;
  margin: 16px 24px;
  background: var(--bg-card);
  border-radius: 12px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
}

.inline-error {
  margin: 0 24px 16px;
  padding: 12px 14px;
  border: 1px solid #fecaca;
  border-radius: 8px;
  background: var(--dh-signal-critical-soft);
  color: var(--ws-danger);
  font-size: 13px;
  font-weight: 600;
}

.section-eyebrow {
  font-size: 11px;
  color: var(--system-gray2);
  text-transform: uppercase;
}

.dashboard-ribbon-title {
  font-size: 18px;
  font-weight: 600;
  color: var(--text-primary);
  margin: 4px 0 0;
}

.meta-chip {
  padding: 4px 10px;
  background: var(--ws-surface-muted);
  border-radius: 8px;
  font-size: 12px;
  color: var(--text-tertiary);
}

.resource-metrics {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 16px;
  margin: 0 24px 16px;
  padding: 20px;
  background: var(--bg-card);
  border-radius: 12px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
}

.metric-item {
  text-align: center;
  padding: 16px;
  background: var(--bg-page);
  border-radius: 10px;
}

.metric-label {
  display: block;
  font-size: 12px;
  color: var(--text-tertiary);
  margin-bottom: 8px;
}

.metric-value {
  display: block;
  font-size: 28px;
  font-weight: 700;
  color: var(--text-primary);
}

.metric-sub {
  display: block;
  font-size: 11px;
  color: var(--system-gray2);
  margin-top: 6px;
}

.resource-board {
  display: flex;
  gap: 16px;
  padding: 0 24px 24px;
}

.resource-main-stage {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.panel {
  background: var(--bg-card);
  border-radius: 12px;
  padding: 20px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
}

.spotlight-panel {
  border-left: 4px solid var(--system-blue);
}

.section-headline {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 16px;
}

.section-title {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
}

.section-meta {
  font-size: 12px;
  color: var(--system-gray2);
}

.bottleneck-ladder {
  min-height: 200px;
}

.resource-diagnostic-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 16px;
}

.dimension-card-grid,
.distribution-strip {
  min-height: 100px;
}

.insight-panel {
  width: 320px;
  flex-shrink: 0;
}

.resource-insight-group {
  margin-bottom: 20px;
}

.resource-insight-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  margin: 0 0 12px;
}

.insight-callout {
  padding: 16px;
  background: var(--bg-page);
  border-radius: 10px;
}

.insight-callout strong {
  display: block;
  font-size: 14px;
  color: var(--text-primary);
}

.insight-callout p {
  font-size: 12px;
  color: var(--text-tertiary);
  margin: 8px 0 0;
}

.cadence-list,
.threshold-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.cadence-item {
  padding: 10px 12px;
  background: var(--bg-page);
  border-radius: 8px;
}

.cadence-item strong {
  display: block;
  font-size: 13px;
  color: var(--text-primary);
}

.cadence-item span {
  font-size: 11px;
  color: var(--text-tertiary);
}

.action-suggestion {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 10px 12px;
  border-radius: 8px;
  border-left: 3px solid var(--system-orange);
  background: #fffbeb;
}

.action-suggestion.severity-critical {
  border-left-color: var(--system-red);
  background: var(--dh-signal-critical-soft);
}

.action-suggestion strong {
  font-size: 13px;
  color: var(--text-primary);
}

.action-suggestion span {
  font-size: 12px;
  line-height: 1.5;
  color: var(--text-secondary);
}

.threshold-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 0;
}

.threshold-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
}

.threshold-dot.low { background: var(--system-green); }
.threshold-dot.medium { background: var(--system-blue); }
.threshold-dot.high { background: var(--system-orange); }
.threshold-dot.critical { background: var(--system-red); }

.threshold-label {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-primary);
  width: 70px;
}

.threshold-desc {
  font-size: 11px;
  color: var(--text-tertiary);
}

.empty {
  text-align: center;
  padding: 40px;
  color: var(--system-gray2);
}
</style>
