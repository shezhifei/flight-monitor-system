<script setup lang="ts">
import type { ConflictItem, AnalyticsData } from '@/composables/useDispatchBoardData';
import type { ReplanSuggestion } from '@/composables/useDispatchReplan';
import type { AiSuggestion } from '../composables/useDispatchBoardPageAiTypes';

type AnalyticsBreakdownItem = {
  id: string;
  label: string;
  value: string;
  orderCount?: number;
  completedOrderCount?: number;
  occupiedMinutes?: number;
  teamLabels?: string[];
  resourceId?: string;
  orderIds?: string[];
  representativeOrderId?: string;
};

type ScenarioItem = { id: string; title: string; description: string; orderId?: string };

defineProps<{
  isAiDrawerVisible: boolean;
  activeAiTab: 'assistant' | 'conflict';
  aiStreamEnabled: boolean;
  aiObjective: string;
  aiMetrics: { conflicts: number; pending: number; heavy: number };
  aiSuggestionList: AiSuggestion[];
  analyticsData: AnalyticsData | null;
  analyticsMode: 'team' | 'employee';
  analyticsMetrics: { conflictRate: string; replanRate: string; responseMinutes: string; balanceScore: string; idleRate: string; ontimeRate: string };
  analyticsBreakdownList: AnalyticsBreakdownItem[];
  conflictList: ConflictItem[];
  conflictMetrics: { total: number; high: number; orders: number };
  conflictSeverityFilter: string;
  conflictTypeFilter: string;
  conflictQueryInput: string;
  availableConflictTypes: string[];
  scenarioEquipment: string;
  scenarioStand: string;
  scenarioDelay: string;
  scenarioFrozen: string;
  scenarioImpactedOrders: ScenarioItem[];
  scenarioProjectedConflicts: ScenarioItem[];
  scenarioRecommendations: ScenarioItem[];
  replanStrategy: 'stability' | 'balanced' | 'efficiency';
  replanMaxSuggestions: number;
  replanMode: string;
  replanSuggestionList: ReplanSuggestion[];
  replanCanApply: boolean;
  replanStatusLabel: string;
  categorizedReplanSuggestions: Array<{ label: string; items: ReplanSuggestion[] }>;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'update:activeAiTab', val: 'assistant' | 'conflict'): void;
  (e: 'update:aiStreamEnabled', val: boolean): void;
  (e: 'update:aiObjective', val: string): void;
  (e: 'update:analyticsMode', val: 'team' | 'employee'): void;
  (e: 'update:conflictSeverityFilter', val: string): void;
  (e: 'update:conflictTypeFilter', val: string): void;
  (e: 'update:conflictQueryInput', val: string): void;
  (e: 'update:scenarioEquipment', val: string): void;
  (e: 'update:scenarioStand', val: string): void;
  (e: 'update:scenarioDelay', val: string): void;
  (e: 'update:scenarioFrozen', val: string): void;
  (e: 'update:replanStrategy', val: 'stability' | 'balanced' | 'efficiency'): void;
  (e: 'update:replanMaxSuggestions', val: number): void;
  (e: 'fetchConflicts'): void;
  (e: 'fetchAnalytics'): void;
  (e: 'previewScenario'): void;
  (e: 'clearScenario'): void;
  (e: 'handleAiGenerate'): void;
  (e: 'handleReplanPreview'): void;
  (e: 'handleReplanApply'): void;
  (e: 'handleReplanClear'): void;
  (e: 'previewAiSuggestion', s: AiSuggestion): void;
  (e: 'applyAiSuggestion', s: AiSuggestion): void;
  (e: 'setTrendChartRef', el: HTMLElement | null): void;
}>();
</script>

<template>
  <aside id="aiDrawer" class="drawer" :class="{ open: isAiDrawerVisible }" :aria-hidden="!isAiDrawerVisible">
    <div class="drawer-header"><h3 class="drawer-title">智能派工与冲突重排</h3><button id="aiCloseBtn" class="panel-close-btn" aria-label="关闭智能派工助手" @click="emit('close')">×</button></div>
    <div class="drawer-body">
      <div id="aiDrawerTabs" class="ai-drawer-tabs">
        <button class="drawer-tab" :class="{ active: activeAiTab === 'assistant' }" data-ai-tab="assistant" @click="emit('update:activeAiTab', 'assistant')">智能建议</button>
        <button class="drawer-tab" :class="{ active: activeAiTab === 'conflict' }" data-ai-tab="conflict" @click="emit('update:activeAiTab', 'conflict')">冲突治理</button>
      </div>

      <section id="assistantPanel" class="ai-panel" :class="{ active: activeAiTab === 'assistant' }" data-ai-panel="assistant">
        <p class="window-label">使用方式</p>
        <p class="window-label">建议仅用于人工决策辅助，不会自动发布。</p>
        <ol class="ai-usage-list"><li>选择目标（清空待派工、消解冲突、均衡负载、预防延误）。</li><li>点击"生成建议"获取可执行方案。</li><li>先"预览定位"确认，再加入执行清单。</li><li>建议默认人工确认，避免误发布。</li></ol>
        <label class="ai-stream-setting" for="aiStreamToggle"><input id="aiStreamToggle" :checked="aiStreamEnabled" class="ai-stream-toggle" type="checkbox" @change="emit('update:aiStreamEnabled', ($event.target as HTMLInputElement).checked)">实时 AI 推送</label>
        <div class="ai-controls">
          <select id="aiObjective" :value="aiObjective" @change="emit('update:aiObjective', ($event.target as HTMLSelectElement).value)"><option value="clear_pending">优先清空待派工</option><option value="resolve_conflicts">优先消解资源冲突</option><option value="balance_load">优先均衡负载</option><option value="delay_prevention">优先预防延误</option></select>
          <button id="aiGenerateBtn" class="action-btn primary" @click="emit('handleAiGenerate')">生成建议</button>
        </div>
        <div id="aiMetrics" class="ai-metrics">
          <div class="metric-card"><p class="metric-title">冲突资源行</p><p id="metricConflicts" class="metric-value">{{ aiMetrics.conflicts }}</p></div>
          <div class="metric-card"><p class="metric-title">待派工任务</p><p id="metricPending" class="metric-value">{{ aiMetrics.pending }}</p></div>
          <div class="metric-card"><p class="metric-title">高负载资源</p><p id="metricHeavy" class="metric-value">{{ aiMetrics.heavy }}</p></div>
        </div>
        <div class="section-title">运营分析</div>
        <div class="analytics-toolbar">
          <p class="window-label conflict-data-hint">{{ analyticsData ? '当前时间窗运营分析已加载' : '当前时间窗运营分析未加载' }}</p>
          <div class="analytics-toolbar-actions">
            <button class="action-btn analytics-mode-btn" :class="{ active: analyticsMode === 'team' }" data-analytics-mode="team" @click="() => { emit('update:analyticsMode', 'team'); emit('fetchAnalytics'); }">班组</button>
            <button class="action-btn analytics-mode-btn" :class="{ active: analyticsMode === 'employee' }" data-analytics-mode="employee" @click="() => { emit('update:analyticsMode', 'employee'); emit('fetchAnalytics'); }">个人</button>
            <button class="action-btn" @click="emit('fetchAnalytics')">刷新分析</button>
          </div>
        </div>
        <div id="analyticsMetrics" class="ai-metrics analytics-metrics">
          <div class="metric-card"><p class="metric-title">冲突率</p><p class="metric-value">{{ analyticsMetrics.conflictRate }}</p></div>
          <div class="metric-card"><p class="metric-title">重排率</p><p class="metric-value">{{ analyticsMetrics.replanRate }}</p></div>
          <div class="metric-card"><p class="metric-title">平均响应</p><p class="metric-value">{{ analyticsMetrics.responseMinutes }}</p></div>
          <div class="metric-card"><p class="metric-title">班组均衡分</p><p class="metric-value">{{ analyticsMetrics.balanceScore }}</p></div>
          <div class="metric-card"><p class="metric-title">设备闲置率</p><p class="metric-value">{{ analyticsMetrics.idleRate }}</p></div>
          <div class="metric-card"><p class="metric-title">关键任务准点率</p><p class="metric-value">{{ analyticsMetrics.ontimeRate }}</p></div>
        </div>
        <div class="analytics-trend-card"><div class="section-title" style="margin-top:0;">趋势概览</div><div id="analyticsTrendChart" :ref="(el) => emit('setTrendChartRef', el as HTMLElement | null)" class="analytics-trend-chart" /><div class="analytics-trend-meta">按当前时间窗小时粒度展示工单、冲突与响应趋势。</div></div>
        <div id="analyticsBreakdownList" class="ai-suggestion-list analytics-breakdown-list">
          <div v-for="item in analyticsBreakdownList" :key="item.id" class="breakdown-item">
            <template v-if="analyticsMode === 'employee'">
              <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:4px;"><h4 style="margin:0;font-size:14px;font-weight:500;">{{ item.label }}</h4><span class="suggestion-chip" style="font-size:12px;background:rgba(33,150,243,0.1);color:#2196f3;padding:2px 6px;border-radius:4px;">{{ item.occupiedMinutes || 0 }} 分钟</span></div>
              <div style="display:flex;gap:8px;font-size:12px;color:var(--text-secondary);"><span>工单 {{ item.orderCount || 0 }}</span><span>已完成 {{ item.completedOrderCount || 0 }}</span><span>{{ ((item.teamLabels ?? []).length > 0) ? (item.teamLabels ?? []).join(' / ') : '未关联班组' }}</span></div>
            </template>
            <template v-else><span class="breakdown-label">{{ item.label }}</span><span class="breakdown-value">{{ item.value }}</span></template>
          </div>
          <div v-if="analyticsBreakdownList.length === 0" class="empty-list-tip">暂无分解数据</div>
        </div>
        <div class="section-title">建议列表</div>
        <div id="aiSuggestionList" class="ai-suggestion-list">
          <div v-for="s in aiSuggestionList" :key="s.id" class="suggestion-item">
            <strong>{{ s.title }}</strong><p>{{ s.description }}</p>
            <span v-if="s.confidence" class="confidence-badge">置信度 {{ s.confidence }}%</span>
            <div style="margin-top:8px;display:flex;gap:8px;">
              <button class="action-btn" style="padding:4px 8px;font-size:12px;" @click="emit('previewAiSuggestion', s)">预览定位</button>
              <button class="action-btn" style="padding:4px 8px;font-size:12px;" @click="emit('applyAiSuggestion', s)">加入执行清单</button>
            </div>
          </div>
          <div v-if="aiSuggestionList.length === 0" class="empty-list-tip">点击"生成建议"获取方案</div>
        </div>
      </section>

      <section id="conflictPanel" class="ai-panel" :class="{ active: activeAiTab === 'conflict' }" data-ai-panel="conflict">
        <p class="window-label">冲突治理说明</p>
        <p class="window-label">使用后端冲突检测与重排预览，默认手动确认后再应用，不会自动执行。</p>
        <div class="ai-metrics"><div class="metric-card"><p class="metric-title">冲突总数</p><p class="metric-value">{{ conflictMetrics.total }}</p></div><div class="metric-card"><p class="metric-title">高优先冲突</p><p class="metric-value">{{ conflictMetrics.high }}</p></div><div class="metric-card"><p class="metric-title">影响工单</p><p class="metric-value">{{ conflictMetrics.orders }}</p></div></div>
        <div class="conflict-toolbar">
          <div class="conflict-filter-row">
            <select :value="conflictSeverityFilter" @change="emit('update:conflictSeverityFilter', ($event.target as HTMLSelectElement).value)"><option value="all">全部级别</option><option value="critical">Critical</option><option value="high">High</option><option value="medium">Medium</option><option value="low">Low</option></select>
            <select :value="conflictTypeFilter" @change="emit('update:conflictTypeFilter', ($event.target as HTMLSelectElement).value)"><option value="all">全部冲突类型</option><option v-for="t in availableConflictTypes" :key="t" :value="t">{{ t }}</option></select>
            <input :value="conflictQueryInput" type="text" placeholder="搜索资源/说明/工单ID" @input="emit('update:conflictQueryInput', ($event.target as HTMLInputElement).value)">
            <button class="action-btn" @click="emit('fetchConflicts')">刷新冲突</button>
          </div>
        </div>
        <p class="window-label conflict-data-hint">当前显示甘特时间窗内冲突 ({{ conflictList.length }} 条)</p>
        <div id="conflictList" class="ai-suggestion-list conflict-list">
          <div v-for="(conflict, idx) in conflictList" :key="`conflict-${idx}`" class="conflict-item clickable">
            <strong>{{ conflict.resource_name || conflict.resource_id || '未命名资源' }}</strong><p>{{ conflict.description || conflict.conflict_type || '未知冲突' }}</p>
            <span class="severity-badge" :class="conflict.severity">{{ conflict.severity }}</span>
          </div>
          <div v-if="conflictList.length === 0" class="empty-list-tip">当前时间窗无冲突</div>
        </div>
        <div class="section-title">场景预览</div>
        <p class="window-label">模拟设备停机、机位关闭和工单延迟，只做预览分析，不会写入正式派工。</p>
        <div class="scenario-form">
          <input :value="scenarioEquipment" type="text" placeholder="停用设备 ID，多个用逗号分隔" @input="emit('update:scenarioEquipment', ($event.target as HTMLInputElement).value)">
          <input :value="scenarioStand" type="text" placeholder="关闭机位 ID，多个用逗号分隔" @input="emit('update:scenarioStand', ($event.target as HTMLInputElement).value)">
          <input :value="scenarioDelay" type="text" placeholder="延迟工单，如 order-1:20, order-2:15" @input="emit('update:scenarioDelay', ($event.target as HTMLInputElement).value)">
          <input :value="scenarioFrozen" type="text" placeholder="冻结工单 ID，多个用逗号分隔" @input="emit('update:scenarioFrozen', ($event.target as HTMLInputElement).value)">
        </div>
        <div class="scenario-toolbar">
          <p class="window-label">{{ (scenarioImpactedOrders.length + scenarioProjectedConflicts.length + scenarioRecommendations.length) > 0 ? `已预览 ${scenarioImpactedOrders.length + scenarioProjectedConflicts.length + scenarioRecommendations.length} 条影响` : '输入场景后点击"预览场景"。' }}</p>
          <div class="scenario-toolbar-actions"><button class="action-btn" @click="emit('previewScenario')">预览场景</button><button class="action-btn" @click="emit('clearScenario')">清空场景</button></div>
        </div>
        <div class="section-title">应急重排</div>
        <div class="replan-controls">
          <select :value="replanStrategy" :disabled="replanMode === 'solving' || replanMode === 'applying'" @change="emit('update:replanStrategy', ($event.target as HTMLSelectElement).value as 'stability' | 'balanced' | 'efficiency')"><option value="stability">稳定优先</option><option value="balanced">平衡优先</option><option value="efficiency">效率优先</option></select>
          <select :value="replanMaxSuggestions" :disabled="replanMode === 'solving' || replanMode === 'applying'" @change="emit('update:replanMaxSuggestions', Number(($event.target as HTMLSelectElement).value))"><option :value="10">建议10条</option><option :value="20">建议20条</option><option :value="50">建议50条</option></select>
          <button class="action-btn" :disabled="replanMode === 'solving' || replanMode === 'applying'" @click="emit('handleReplanPreview')">预览重排</button>
          <button class="action-btn primary" :disabled="!replanCanApply" @click="emit('handleReplanApply')">应用重排</button>
          <button class="action-btn" :disabled="replanMode === 'solving' || replanMode === 'applying'" @click="emit('handleReplanClear')">清空预览</button>
        </div>
        <div class="replan-status-bar"><p class="window-label">{{ replanStatusLabel }}</p></div>
        <div id="replanSuggestionList" class="ai-suggestion-list">
          <template v-if="categorizedReplanSuggestions.length > 0">
            <div v-for="(group, gi) in categorizedReplanSuggestions" :key="gi" class="replan-section" style="margin-bottom:16px;">
              <div class="section-title">{{ group.label }}</div>
              <div v-for="s in group.items" :key="s.id" class="replan-item">
                <strong>{{ s.orderId }}</strong><p>{{ s.description }}</p>
              </div>
            </div>
          </template>
          <div v-if="replanSuggestionList.length === 0 && replanMode === 'idle'" class="empty-list-tip">暂无重排建议</div>
        </div>
      </section>
    </div>
  </aside>
</template>

<style scoped>
.replan-status-bar { display: flex; flex-direction: column; gap: 6px; margin-bottom: 12px; }
.clickable { cursor: pointer; transition: background 0.2s; }
.clickable:hover { background: var(--bg-hover, #F8FAFC); }
.replan-item { cursor: pointer; padding: 10px; border-bottom: 1px solid var(--border-light, #f1f5f9); transition: background 0.2s; }
.replan-item:hover { background: var(--bg-hover, #F8FAFC); }
</style>
