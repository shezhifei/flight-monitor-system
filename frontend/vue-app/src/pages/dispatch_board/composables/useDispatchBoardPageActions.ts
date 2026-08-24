import { computed, onMounted, onUnmounted, watch } from 'vue';
import type { ComputedRef } from 'vue';
import { useApi } from '@/composables/useApi';
import { useToast } from '@/composables/useToast';
import { useDispatchOverrunWarnings } from '@/composables/useDispatchOverrunWarnings';
import { useDispatchBoardPage } from './useDispatchBoardPage';
import {
  fetchAnalytics as fetchDispatchAnalytics,
  buildResourceFocus,
  type ConflictItem,
  type DispatchOrder,
  type TimelineMember,
  type DispatchQualificationGap,
  type AnalyticsTrendPoint,
  type AnalyticsBreakdownItem,
  batchCompleteOrders,
  countOrdersByStatus,
  type SafetyGateFilter,
} from '@/composables/useDispatchBoardData';
import { formatDetailDateTime } from '@/composables/useDispatchBoardDetail';
import { unwrapApiData } from '@/shared/apiEnvelope';
import {
  type EmployeeAnalyticsBucket,
  type EmployeeAnalyticsItem,
  type AiSuggestion,
  toTimestamp,
  normalizeOrderIds,
  splitCommaSeparatedIds,
  parseScenarioDelayInput,
} from './useDispatchBoardPageAiTypes';
import { renderTrendChartInto } from './useTrendChart';
import { buildDispatchReplanProposalPayload, isDirectReplanApplyEnabled } from './dispatchReplanProposal';

interface GuideSettings {
  autoRefresh: boolean;
  refreshInterval: string;
  showCompleted: boolean;
  timeScale: string;
  conflictNotification: boolean;
  completeNotification: boolean;
  cornerFade: boolean;
}

interface GanttChartParams {
  data?: {
    raw?: DispatchOrder;
  };
}

interface UseDispatchBoardPageActionsOptions {
  p: ReturnType<typeof useDispatchBoardPage>;
  visibleTimelineItems: ComputedRef<readonly DispatchOrder[]>;
  statusOrderList: ComputedRef<Array<{ id: string; title: string; status: string; start_time: string }>>;
}

export function useDispatchBoardPageActions(options: UseDispatchBoardPageActionsOptions) {
  const { p, visibleTimelineItems, statusOrderList } = options;
  const api = useApi();
  const toast = useToast();
  const overrunWarnings = useDispatchOverrunWarnings();
  // Task I4: direct replan-apply stays behind an explicit feature flag; the
  // default path routes through the proposal approval flow.
  const replanDirectApplyEnabled = isDirectReplanApplyEnabled();

  let notificationSnapshotReady = false;
  watch(
    () => ({ conflicts: p.conflictList.value.length, completed: countOrdersByStatus(p.timelineData.value?.items || [], p.safetyProgress.value, p.safetyGateFilter.value).completed || 0 }),
    (next, previous) => {
      if (!notificationSnapshotReady) { notificationSnapshotReady = true; return; }
      if (p.guideSettings.conflictNotification && next.conflicts > previous.conflicts) toast.show('warning', `新增 ${next.conflicts - previous.conflicts} 项调度冲突`);
      if (p.guideSettings.completeNotification && next.completed > previous.completed) toast.show('success', `新增完成 ${next.completed - previous.completed} 条工单`);
    },
  );

  function switchTerminal(t: string) { p.switchTerminal(t); }

  function handleSearch() {
    const q = p.searchQuery.value.trim();
    if (!q) { p.searchResults.value = []; p.searchMetaLabel.value = '未搜索'; return; }
    const items = visibleTimelineItems.value;
    const matches = items.filter(item => String(item.order_id || '').toLowerCase().includes(q.toLowerCase()) || String(item.flight_id || '').toLowerCase().includes(q.toLowerCase()) || String(item.task_type || '').toLowerCase().includes(q.toLowerCase())).slice(0, 10).map((item, i) => ({ id: `search-${i}`, label: String(item.order_id || item.flight_id || ''), sub: String(item.task_type || item.status || '') }));
    p.searchResults.value = matches;
    p.searchIndex.value = 0;
    p.searchMetaLabel.value = matches.length > 0 ? `${matches.length} 条结果` : '无匹配';
  }
  function handleSearchNext() {
    if (p.searchResults.value.length === 0) return;
    p.searchIndex.value = (p.searchIndex.value + 1) % p.searchResults.value.length;
  }

  function toggleAiDrawer() { p.isAiDrawerVisible.value = !p.isAiDrawerVisible.value; }
  function closeAiDrawer() { p.isAiDrawerVisible.value = false; }

  /**
   * Task I4: the assistant lives in the shared dispatch_ops React shell
   * (`window.DISPATCH_AI_BRIDGE`); this Vue drawer delegates to it instead
   * of running its own AI loop.
   */
  function openAssistantShell() {
    const bridge = (window as unknown as {
      DISPATCH_AI_BRIDGE?: {
        openDrawer?: (tab: 'assistant' | 'conflict', options?: { refresh?: boolean; context?: Record<string, unknown> }) => void;
      };
    }).DISPATCH_AI_BRIDGE;
    if (!bridge?.openDrawer) {
      toast.show('warning', '派工 AI 助手尚未就绪，请稍后重试');
      return;
    }
    closeAiDrawer();
    bridge.openDrawer('assistant', {
      context: {
        source_page: 'dispatch_board',
        window_start: new Date(p.windowStartMs.value).toISOString(),
        window_end: new Date(p.windowEndMs.value).toISOString(),
      },
    });
  }
  function toggleStatusPanel() { p.isStatusPanelVisible.value = !p.isStatusPanelVisible.value; }
  function closeStatusPanel() { p.isStatusPanelVisible.value = false; }
  function toggleChatDrawer() {
    p.isChatDrawerVisible.value = !p.isChatDrawerVisible.value;
    p.setChatPanelVisible(p.isChatDrawerVisible.value);
    if (p.isChatDrawerVisible.value) {
      void p.loadChatGroups({ silent: true });
    }
  }
  function closeChatDrawer() {
    p.isChatDrawerVisible.value = false;
    p.setChatPanelVisible(false);
  }

  async function sendChatFromDrawer(payload?: { mentionUserIds?: string[]; atAll?: boolean }) {
    const content = p.chatInput.value;
    const result = await p.sendChatMessage(
      content,
      Boolean(payload?.atAll),
      payload?.mentionUserIds ?? [],
    );
    if (result.ok) {
      p.chatInput.value = '';
    }
  }
  function toggleOpsMenu() { p.isOpsMenuVisible.value = !p.isOpsMenuVisible.value; }
  function closeOpsMenu() { p.isOpsMenuVisible.value = false; }
  function toggleGanttLegend() { p.isGanttLegendPopoverVisible.value = !p.isGanttLegendPopoverVisible.value; }
  function toggleGuideAndLegendPanel() { p.isGuideAndLegendPanelVisible.value = !p.isGuideAndLegendPanelVisible.value; }
  function closeGuideAndLegendPanel() { p.isGuideAndLegendPanelVisible.value = false; }
  function toggleBatchToolbar() { p.isBatchToolbarVisible.value = !p.isBatchToolbarVisible.value; }
  function handleViewTabChange(tab: 'flight' | 'team' | 'employee' | 'equipment') { p.activeViewMode.value = tab; closeOpsMenu(); }

  function resetWindowToNow() {
    const now = Date.now();
    p.setWindow(now - 2 * 60 * 60 * 1000, now + 4 * 60 * 60 * 1000);
    p.refreshTimeline();
    closeOpsMenu();
  }

  function handleSettingsApply() {
    p.setSafetyGateFilter(p.settingSafetyGateFilter.value as SafetyGateFilter);
    startAutoRefresh();
    closeOpsMenu();
    p.refreshTimeline();
  }

  function handleGuideSettingsChange(next: Partial<GuideSettings>) {
    const prevScale = p.guideSettings.timeScale;
    Object.assign(p.guideSettings, next);
    p.settingRefreshInterval.value = p.guideSettings.refreshInterval;
    if (prevScale !== p.guideSettings.timeScale) {
      const minutes = Number(p.guideSettings.timeScale);
      if (Number.isFinite(minutes) && minutes > 0) {
        const now = Date.now();
        p.setWindow(now - Math.max(30, minutes * 4) * 60 * 1000, now + Math.max(60, minutes * 8) * 60 * 1000);
        p.refreshTimeline();
      }
    }
    startAutoRefresh();
  }

  function handleStatusFilterBlocked() { p.setSafetyGateFilter('blocked'); }
  function handleStatusShowAll() { p.setSafetyGateFilter('all'); }
  function handleStatusSelectAll() {
    const ids = statusOrderList.value.map(o => o.id);
    p.selectedOrderIds.value = [...new Set([...p.selectedOrderIds.value, ...ids])];
  }
  function handleStatusOrderOpen(orderId: string) { p.openOrderDetail(orderId, 'status_panel'); }
  function toggleOrderSelection(orderId: string) {
    const idx = p.selectedOrderIds.value.indexOf(orderId);
    if (idx === -1) p.selectedOrderIds.value.push(orderId);
    else p.selectedOrderIds.value.splice(idx, 1);
  }

  async function handleReplanPreview() {
    await p.runReplan(p.replanStrategy.value, p.replanMaxSuggestions.value, { windowStartMs: p.windowStartMs.value, windowEndMs: p.windowEndMs.value });
    p.impactedOrderIds.value = p.replanSuggestionList.value.map(item => item.orderId);
  }
  async function handleReplanApply() {
    if (replanDirectApplyEnabled) {
      // Escape hatch (Task I4): direct apply only for human operators behind
      // the explicit feature flag — never the agent path.
      const success = await p.applyReplan(p.replanStrategy.value);
      if (success) {
        p.impactedOrderIds.value = [];
        await Promise.all([p.refreshTimeline(), fetchConflicts(), fetchAnalytics()]);
      }
      return;
    }
    if (!p.replanCanApply.value || p.replanSuggestionList.value.length === 0) {
      toast.show('warning', '请先预览重排并确认方案完整');
      return;
    }
    try {
      const payload = buildDispatchReplanProposalPayload(p.replanStrategy.value, p.replanSuggestionList.value);
      const res = await api.post<Record<string, unknown>>('/api/v2/ai/proposals', payload);
      if (res.ok) {
        toast.show('success', '重排提案已提交，待审批后执行');
        p.impactedOrderIds.value = [];
      } else {
        toast.show('error', `重排提案提交失败 (HTTP ${res.status})`);
      }
    } catch (e) {
      toast.show('error', `重排提案提交失败: ${e instanceof Error ? e.message : String(e)}`);
    }
  }
  function handleReplanClear() { p.clearReplan(); p.impactedOrderIds.value = []; }

  async function fetchConflicts() {
    try {
      const params = new URLSearchParams();
      params.set('window_start', new Date(p.windowStartMs.value).toISOString());
      params.set('window_end', new Date(p.windowEndMs.value).toISOString());
      params.set('limit', '200');
      const res = await api.get<unknown>(`/api/v2/dispatch-orders/conflicts?${params.toString()}`);
      const payload = unwrapApiData<{ conflicts?: ConflictItem[] }>(res.data);
      if (res.ok && payload) {
        p.conflictRawList.value = (payload.conflicts || []).map(c => ({ ...c, description: String(c.description || c.message || c.conflict_type || '').trim() }));
        updateConflictList();
      }
    } catch (e) { console.warn('Failed to fetch conflicts:', e); }
  }

  function updateConflictList() {
    const query = p.conflictQueryInput.value.trim().toLowerCase();
    p.conflictList.value = p.conflictRawList.value.filter(c => {
      if (p.conflictSeverityFilter.value !== 'all' && String(c.severity || '').trim().toLowerCase() !== p.conflictSeverityFilter.value) return false;
      if (p.conflictTypeFilter.value !== 'all' && String(c.conflict_type || '').trim() !== p.conflictTypeFilter.value) return false;
      if (query) {
        const searchText = [c.resource_name, c.resource_id, c.conflict_type, c.description, c.message, ...(Array.isArray(c.related_dispatch_order_ids) ? c.related_dispatch_order_ids : [])].map(v => String(v || '').trim().toLowerCase()).filter(Boolean).join(' ');
        if (!searchText.includes(query)) return false;
      }
      return true;
    });
    const related = new Set<string>();
    p.conflictList.value.forEach(c => (c.related_dispatch_order_ids || []).forEach(id => { const n = String(id || '').trim(); if (n) related.add(n); }));
    p.conflictMetrics.value = { total: p.conflictList.value.length, high: p.conflictList.value.filter(c => ['critical', 'high'].includes(String(c.severity || '').toLowerCase())).length, orders: related.size };
  }

  watch([() => p.conflictSeverityFilter.value, () => p.conflictTypeFilter.value, () => p.conflictQueryInput.value], updateConflictList);

  async function fetchAnalytics() {
    try {
      const res = await fetchDispatchAnalytics({ windowStartMs: p.windowStartMs.value, windowEndMs: p.windowEndMs.value });
      p.analyticsData.value = res;
      if (res) {
        const summary = res.summary || {};
        p.analyticsMetrics.value = { conflictRate: String(summary.conflict_rate ?? '-'), replanRate: String(summary.replan_rate ?? '-'), responseMinutes: String(summary.avg_dispatch_response_minutes ?? '-'), balanceScore: String(summary.team_load_balance_score ?? '-'), idleRate: String(summary.equipment_idle_rate ?? '-'), ontimeRate: String(summary.key_order_ontime_rate ?? '-') };
        p.analyticsBreakdownList.value = p.analyticsMode.value === 'employee' ? buildEmployeeAnalyticsBreakdown(visibleTimelineItems.value) : (res.breakdown || []).map((b: AnalyticsBreakdownItem, i: number) => ({ id: `breakdown-${i}`, label: String(b.group_label ?? b.label ?? b.name ?? b.group_key ?? `项 ${i + 1}`), value: String(b.order_count ?? b.value ?? b.score ?? '') }));
        if (Array.isArray(res.trend) && res.trend.length > 0) renderTrendChart(res.trend);
      }
    } catch (e) { console.warn('Failed to fetch analytics:', e); }
  }

  function renderTrendChart(data: ReadonlyArray<AnalyticsTrendPoint>) {
    const trendChartRef = p.trendChartRef.value;
    if (!trendChartRef || !data || data.length === 0) return;
    renderTrendChartInto(trendChartRef, {
      tooltip: { trigger: 'axis' }, grid: { left: '3%', right: '4%', bottom: '3%', containLabel: true },
      xAxis: { type: 'category', data: data.map(d => { const date = new Date(String(d.timestamp || d.time || d.date || '')); return date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' }); }) },
      yAxis: { type: 'value' },
      series: [{ data: data.map(d => Number(d.value ?? d.count ?? d.amount ?? 0)), type: 'line', smooth: true, areaStyle: { opacity: 0.3 } }],
    });
  }

  function buildEmployeeAnalyticsBreakdown(items: readonly DispatchOrder[]): EmployeeAnalyticsItem[] {
    const grouped = new Map<string, EmployeeAnalyticsBucket>();
    for (const item of items) {
      if (!item || item.is_flight_summary) continue;
      const members = Array.isArray(item.members) ? item.members : [];
      const primary = members[0] || null;
      const id = String(item.individual_user_id || item.focus_user_id || primary?.user_id || '').trim();
      const label = String(item.individual_username || item.focus_user_name || primary?.username || primary?.user_display_name || primary?.name || '').trim();
      if (!id && !label) continue;
      const key = id || label;
      const bucket = grouped.get(key) || { label: label || id, orderCount: 0, completedOrderCount: 0, occupiedMinutes: 0, teamLabels: new Set<string>(), resourceId: id, orderIds: new Set<string>(), representativeOrderId: '' };
      bucket.orderCount += 1;
      if (String(item.status || '').trim() === 'completed') bucket.completedOrderCount += 1;
      if (item.order_id) { bucket.orderIds.add(item.order_id); if (!bucket.representativeOrderId) bucket.representativeOrderId = item.order_id; }
      const start = toTimestamp(item.actual_start_time || item.planned_start_time || item.start_time);
      const end = toTimestamp(item.actual_end_time || item.effective_end_time || item.planned_end_time || item.end_time);
      if (start > 0 && end > start) { bucket.occupiedMinutes += Math.round((end - start) / 60000); bucket.representativeOrderId = item.order_id || ''; }
      const team = String(item.team_name || '').trim();
      if (team) bucket.teamLabels.add(team);
      grouped.set(key, bucket);
    }
    return Array.from(grouped.entries()).map(([k, v]) => ({ id: `employee-${k}`, label: v.label, value: `${v.orderCount} 单 / ${v.occupiedMinutes} 分钟`, orderCount: v.orderCount, completedOrderCount: v.completedOrderCount, occupiedMinutes: v.occupiedMinutes, teamLabels: Array.from(v.teamLabels), resourceId: v.resourceId, orderIds: Array.from(v.orderIds), representativeOrderId: v.representativeOrderId })).sort((a, b) => b.orderCount - a.orderCount || b.occupiedMinutes - a.occupiedMinutes || a.label.localeCompare(b.label, 'zh-CN'));
  }

  function setImpactedOrders(orderIds: ReadonlyArray<unknown>): void {
    p.impactedOrderIds.value = normalizeOrderIds(orderIds);
  }

  function panTimelineToOrder(orderId: string) {
    const target = p.timelineData.value?.items?.find((item: DispatchOrder) => item.order_id === orderId || item.id === orderId);
    if (!target) return;
    const startMs = Date.parse(String(target.start_time || target.planned_start_time));
    const endMs = Date.parse(String(target.end_time || target.planned_end_time || target.effective_end_time));
    if (startMs && endMs) {
      p.setWindow(startMs - 20 * 60 * 1000, endMs + 60 * 60 * 1000);
      p.refreshTimeline();
    }
  }

  /** Jump to an overrun-related order; non-blocking — never disables publish/replan. */
  function handleOverrunJumpOrder(orderId: string) {
    const id = String(orderId || '').trim();
    if (!id) return;
    panTimelineToOrder(id);
    p.openOrderDetail(id, 'overrun_warning');
  }

  function handleOverrunJumpOrders(payload: {
    currentOrderId?: string | null;
    nextOrderId?: string | null;
  }) {
    const preferred = String(payload.currentOrderId || payload.nextOrderId || '').trim();
    if (preferred) {
      handleOverrunJumpOrder(preferred);
    }
  }

  async function handleOverrunAcknowledge(id: string) {
    try {
      await overrunWarnings.acknowledge(id);
      toast.show('success', '已确认预警（仅表示已看过）');
    } catch {
      // toast already shown by composable
    }
  }

  async function handleOverrunResolve(id: string) {
    try {
      await overrunWarnings.resolve(id);
      toast.show('success', '已关闭预警');
    } catch {
      // toast already shown by composable
    }
  }

  function previewAiSuggestion(s: AiSuggestion) {
    const ids = s.orderIds && s.orderIds.length > 0 ? s.orderIds : (s.orderId ? [s.orderId] : []);
    if (ids.length > 0) {
      setImpactedOrders(ids);
      const focus = buildResourceFocus({
        resource_type: 'employee',
        resource_id: ids[0],
        related_order_ids: ids,
        source_panel: 'ai_assistant',
        source_key: s.id
      });
      p.resourceFocus.value = focus;
      p.resourceFocusText.value = `建议定位: ${s.title}`;
      panTimelineToOrder(ids[0]);
      toast.show('success', '已定位建议对应任务');
    } else {
      toast.show('warning', '当前建议暂无可定位目标');
    }
  }

  function applyAiSuggestion(s: AiSuggestion) {
    if (!s) return;
    const ids = s.orderIds && s.orderIds.length > 0 ? s.orderIds : (s.orderId ? [s.orderId] : []);
    if (s.suggestionType === 'conflict-priority' || s.suggestionType === 'backend_conflict') {
      p.isStatusPanelVisible.value = true;
      if (p.activeViewMode.value !== 'employee') handleViewTabChange('employee');
      if (ids.length > 0) p.conflictQueryInput.value = ids[0];
      toast.show('success', '已切换到冲突治理面板');
    } else if (s.suggestionType === 'pending-priority') {
      p.isStatusPanelVisible.value = false;
      if (ids.length > 0) p.openOrderDetail(ids[0], 'ai_assistant');
      toast.show('success', '已开启待派工单详情以供处理');
    } else if (s.suggestionType === 'load-balance') {
      if (p.activeViewMode.value !== 'employee') handleViewTabChange('employee');
      p.isStatusPanelVisible.value = true;
      p.analyticsMode.value = 'employee';
      toast.show('success', '已切换到员工视角及资源分析');
    } else {
      if (ids.length > 0) {
        previewAiSuggestion(s);
        toast.show('info', '请在定位结果中确认处理方式');
      } else {
        toast.show('warning', '该建议缺少可执行目标，请刷新建议后重试');
      }
    }
  }

  async function previewScenario() {
    try {
      const delayedOrdersResult = parseScenarioDelayInput(p.scenarioDelay.value);
      if (delayedOrdersResult.error) { toast.show('error', delayedOrdersResult.error); return; }
      const res = await api.post<unknown>('/api/v2/dispatch/scenarios/preview', {
        window_start: new Date(p.windowStartMs.value).toISOString(),
        window_end: new Date(p.windowEndMs.value).toISOString(),
        equipment_unavailable_ids: splitCommaSeparatedIds(p.scenarioEquipment.value),
        closed_stand_ids: splitCommaSeparatedIds(p.scenarioStand.value),
        delayed_orders: delayedOrdersResult.items,
        frozen_order_ids: splitCommaSeparatedIds(p.scenarioFrozen.value),
      });
      const payload = unwrapApiData<Record<string, unknown>>(res.data);
      if (!res.ok) { toast.show('error', `场景预览失败: HTTP ${res.status}`); return; }
      if (res.ok && payload) {
        const recommendations = Array.isArray(payload.recommendations) ? payload.recommendations : [];
        const impactedOrders = Array.isArray(payload.impacted_orders) ? payload.impacted_orders : [];
        const projectedConflicts = Array.isArray(payload.projected_conflicts) ? payload.projected_conflicts : [];
        p.scenarioImpactedOrders.value = impactedOrders.map((r: Record<string, unknown>, i: number) => ({
          id: `impacted-${i}`, title: String(r.impact_type || r.dispatch_order_id || `影响 ${i + 1}`), description: String(r.reason || r.message || ''), orderId: String(r.dispatch_order_id || '').trim() || undefined,
        }));
        p.scenarioProjectedConflicts.value = projectedConflicts.map((c: Record<string, unknown>, i: number) => ({
          id: `conflict-${i}`, title: String(c.conflict_type || `冲突 ${i + 1}`), description: String(c.message || c.description || ''), orderId: (Array.isArray(c.related_dispatch_order_ids) && c.related_dispatch_order_ids.length > 0) ? String(c.related_dispatch_order_ids[0]) : undefined,
        }));
        p.scenarioRecommendations.value = recommendations.map((r: Record<string, unknown>, i: number) => ({
          id: `rec-${i}`, title: String(r.action || r.type || `建议 ${i + 1}`), description: String(r.reason || r.description || ''), orderId: String(r.dispatch_order_id || r.target_order_id || '').trim() || undefined,
        }));
        const impactedOrderIds = normalizeOrderIds([
          ...(Array.isArray(payload.changed_orders) ? payload.changed_orders : []),
          ...impactedOrders.map((item: Record<string, unknown>) => item.dispatch_order_id),
        ]);
        setImpactedOrders(impactedOrderIds);
        const impactSummary = (payload.impact_summary && typeof payload.impact_summary === 'object') ? payload.impact_summary as Record<string, unknown> : {};
        p.scenarioMetricsData.value = {
          impactedCount: String(impactSummary.impacted_orders ?? '-'),
          conflictCount: String(impactSummary.projected_conflicts ?? '-'),
          delayedCount: String(impactSummary.delayed_orders ?? '-'),
          riskLevel: String(payload.risk_level ?? '-'),
          manualConfirmation: String(payload.requires_manual_confirmation ?? '-'),
          changedCount: String(Array.isArray(payload.changed_orders) ? payload.changed_orders.length : '-'),
        };
      }
    } catch (e) { console.warn('Failed to preview scenario:', e); toast.show('error', `场景预览失败: ${e instanceof Error ? e.message : String(e)}`); }
  }

  function clearScenario() {
    p.scenarioEquipment.value = '';
    p.scenarioStand.value = '';
    p.scenarioDelay.value = '';
    p.scenarioFrozen.value = '';
    p.scenarioImpactedOrders.value = [];
    p.scenarioProjectedConflicts.value = [];
    p.scenarioRecommendations.value = [];
    setImpactedOrders([]);
    p.scenarioMetricsData.value = { impactedCount: '-', conflictCount: '-', delayedCount: '-', riskLevel: '-', manualConfirmation: '-', changedCount: '-' };
  }

  async function fetchAiSuggestions() {
    const timelineItems = (p.timelineData.value?.items || []).filter((item: DispatchOrder) => !item.is_flight_summary);
    const pendingOrders = timelineItems.filter((item: DispatchOrder) => String(item.status || '').trim() === 'pending');
    const inProgressOrders = timelineItems.filter((item: DispatchOrder) => String(item.status || '').trim() === 'in_progress');
    const heavyEmployees = buildEmployeeAnalyticsBreakdown(timelineItems).filter((item: EmployeeAnalyticsItem) => item.orderCount >= 2);
    if (p.aiObjective.value === 'resolve_conflicts' && p.conflictList.value.length === 0) await fetchConflicts();
    const suggestions: AiSuggestion[] = [];
    // Task I4: the board no longer runs its own AI loop (`tools/execute`
    // get_handling_recommendation); conversational advice lives in the
    // shared dispatch_ops assistant shell. Suggestions below stay local
    // heuristics derived from the timeline and conflict data.
    if (pendingOrders.length > 0) {
      const earliest = [...pendingOrders].sort((a: DispatchOrder, b: DispatchOrder) => toTimestamp(a.planned_start_time || a.start_time) - toTimestamp(b.planned_start_time || b.start_time))[0];
      suggestions.push({ id: 'pending-priority', title: `优先处理待派工 (${pendingOrders.length} 项)`, description: `建议优先处理 ${earliest?.task_type || earliest?.order_id || '最早待派工任务'}`, confidence: 86, orderId: earliest?.order_id, suggestionType: 'pending-priority' });
    }
    if (p.conflictList.value.length > 0) {
      const primary = p.conflictList.value[0];
      const ids = Array.isArray(primary.related_dispatch_order_ids) ? primary.related_dispatch_order_ids : [];
      suggestions.push({ id: 'conflict-priority', title: `优先消解冲突 (${p.conflictList.value.length} 项)`, description: `${primary.resource_name || primary.resource_id || '资源'} 存在 ${primary.conflict_type || '冲突'}`, confidence: 91, orderId: ids[0] ? String(ids[0]) : undefined, orderIds: ids.map(String), suggestionType: 'conflict-priority' });
    }
    if (heavyEmployees.length > 0) {
      const top = heavyEmployees[0];
      suggestions.push({ id: 'load-balance', title: `资源负载偏高 (${heavyEmployees.length} 条)`, description: `${top.label} 当前负载较高`, confidence: 78, orderIds: top.orderIds, orderId: top.representativeOrderId, suggestionType: 'load-balance' });
    }
    if (p.aiObjective.value === 'delay_prevention' && inProgressOrders.length > 0) {
      suggestions.push({ id: 'delay-prevention', title: '关注进行中工单的延误传导', description: `当前有 ${inProgressOrders.length} 条进行中工单`, confidence: 74, orderId: inProgressOrders[0]?.order_id, suggestionType: 'delay-prevention' });
    }
    p.aiSuggestionList.value = suggestions;
    p.aiMetrics.value = { conflicts: p.conflictList.value.length, pending: pendingOrders.length, heavy: heavyEmployees.length };
  }

  async function handleAiGenerate() { await Promise.all([fetchAiSuggestions(), fetchAnalytics()]); }

  let autoRefreshTimer: ReturnType<typeof setInterval> | null = null;
  function startAutoRefresh() {
    stopAutoRefresh();
    if (!p.guideSettings.autoRefresh) return;
    const ms = Number(p.settingRefreshInterval.value);
    if (ms > 0) autoRefreshTimer = setInterval(() => p.refreshTimeline(), ms);
  }
  function stopAutoRefresh() { if (autoRefreshTimer !== null) { clearInterval(autoRefreshTimer); autoRefreshTimer = null; } }

  watch(() => p.settingRefreshInterval.value, () => { p.guideSettings.refreshInterval = p.settingRefreshInterval.value; startAutoRefresh(); });
  watch(p.guideSettings, (s) => { window.localStorage.setItem('dispatch_board_guide_settings', JSON.stringify(s)); }, { deep: true });

  function handleBackdropClick() { closeAiDrawer(); closeStatusPanel(); p.closeDetailDrawer(); closeChatDrawer(); closeOpsMenu(); closeGuideAndLegendPanel(); }

  function handleGanttChartDoubleClick(params: GanttChartParams) {
    const raw = params.data?.raw;
    if (!raw) return;
    if (raw.is_flight_summary) { p.openFlightSummaryDetail(raw, 'gantt'); return; }
    const id = String(raw.order_id ?? raw.id ?? '').trim();
    if (id) p.openOrderDetail(id, 'gantt');
  }

  function handleGanttChartClick(params: GanttChartParams) {
    const raw = params.data?.raw;
    if (!raw || raw.is_flight_summary) return;
    const isDraft = String(raw.publication_state || '').trim().toLowerCase() === 'prepublished' && String(raw.status || '').trim().toLowerCase() === 'pending';
    if (isDraft) { const orderId = String(raw.order_id ?? raw.id ?? '').trim(); if (orderId) toggleOrderSelection(orderId); }
  }

  async function handleBatchComplete() {
    if (p.selectedOrderIds.value.length === 0) { toast.show('warning', '请先选择要处理的工单'); return; }
    const eligible = p.selectedOrderIds.value.filter(id => { const item = visibleTimelineItems.value.find((it: DispatchOrder) => String(it.order_id ?? it.id ?? '') === id); return item && String(item.status ?? '').trim() === 'in_progress'; });
    if (eligible.length === 0) { toast.show('warning', '选中的工单中没有进行中的工单可完工'); return; }
    p.batchProcess.value = { ...p.batchProcess.value, isRunning: true, currentIndex: 0, totalItems: eligible.length, successCount: 0, failCount: 0, currentOrderId: null, errors: [], orderIds: [...eligible] };
    try {
      const result = await batchCompleteOrders(eligible, (current: number, _total: number, orderId: string) => { p.batchProcess.value.currentIndex = current; p.batchProcess.value.currentOrderId = orderId; });
      p.batchProcess.value.successCount = result.success; p.batchProcess.value.failCount = result.failed; p.batchProcess.value.errors = result.errors;
      if (result.success > 0) { toast.show('success', `批量完成：成功 ${result.success} 项`); await p.refreshTimeline(); }
      if (result.failed > 0) toast.show('error', `批量完成：失败 ${result.failed} 项`);
    } catch (error) {
      p.batchProcess.value.errors = [...p.batchProcess.value.errors, { orderId: p.batchProcess.value.currentOrderId || 'batch', error: String(error) }];
      toast.show('error', `批量完成失败: ${String(error)}`);
    } finally { p.batchProcess.value.isRunning = false; }
  }

  async function handleBatchPublish() {
    if (p.selectedOrderIds.value.length === 0) { toast.show('warning', '请先选择要发布的工单'); return; }
    try {
      const res = await api.post('/api/v2/dispatch-orders/batch-publish-drafts', { order_ids: p.selectedOrderIds.value });
      const payload = unwrapApiData<{ published?: number; failed?: number }>(res.data);
      if (res.ok && payload) {
        if (payload.published) toast.show('success', `已发布 ${payload.published} 条工单`);
        if (payload.failed) toast.show('error', `发布失败 ${payload.failed} 条`);
        p.selectedOrderIds.value = []; await p.refreshTimeline();
      }
    } catch (e) { console.warn('Failed to batch publish:', e); }
  }
  function handleBatchClear() { p.selectedOrderIds.value = []; }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === '?' && !e.ctrlKey && !e.metaKey) toggleGuideAndLegendPanel();
    if (e.ctrlKey && e.key === 'a') { e.preventDefault(); handleStatusSelectAll(); toast.show('info', `已全选 ${p.selectedOrderIds.value.length} 条工单`); }
    if (e.key === 'Escape') handleBackdropClick();
    if (e.ctrlKey && e.key === 'b') { e.preventDefault(); toggleBatchToolbar(); }
  }

  onMounted(async () => {
    await p.loadTerminals();
    await Promise.all([p.refreshTimeline(), p.refreshSafetyProgress()]);
    startAutoRefresh();
    // Non-blocking overrun warnings: fetch unresolved + SSE topic dispatch_alerts
    void overrunWarnings.start();
    p.initChatSession();
    void p.loadChatGroups({ silent: true });
    window.addEventListener('keydown', handleKeydown);
  });

  onUnmounted(() => {
    stopAutoRefresh();
    overrunWarnings.stop();
    p.destroyChatSession();
    window.removeEventListener('keydown', handleKeydown);
  });

  const detailCrewMembers = computed(() => {
    const order = p.detailOrder.value;
    if (!order) return [];
    const members = (Array.isArray(order.task_crew?.members) && order.task_crew.members.length ? order.task_crew.members : (Array.isArray(order.members) ? order.members : []));
    return members.map((m: TimelineMember) => { const u = String(m?.username || m?.user_display_name || m?.user_id || '').trim(); const s = String(m?.slot_code || '').trim(); const l = String(m?.qualification_level_code || '').trim(); return [u || '-', [s, l].filter(Boolean).join(' / ')].filter(Boolean).join(' '); }).filter(Boolean);
  });
  const detailQualificationGaps = computed(() => (Array.isArray(p.detailOrder.value?.qualification_gap) ? p.detailOrder.value.qualification_gap : []).map((g: DispatchQualificationGap) => [String(g?.slot_code || '').trim(), String(g?.qualification_code || '').trim(), String(g?.min_level_code || '').trim()].filter(Boolean).join(' / ')).filter(Boolean));
  const detailEquipmentCodes = computed(() => (Array.isArray(p.detailOrder.value?.equipment_codes) ? p.detailOrder.value.equipment_codes : []).map((c: string | null | undefined) => String(c ?? '').trim()).filter(Boolean));

  const detailTaskInfoRows = computed(() => {
    const o = p.detailOrder.value;
    if (!o) return [];
    return [{ label: '工单 ID', value: p.detailCurrentOrderId.value || '-' }, { label: '航班', value: String(o.flight_id || '') }, { label: '作业类型', value: String(o.task_type_name || o.task_type || '') }, { label: '机位', value: String(o.stand_code || o.stand_id || '') }, { label: '登机口', value: String(o.gate || '') }, { label: '状态', value: String(o.status || '') }, { label: '来源', value: String(o.origin_label || o.source || '') }, { label: '派工方式', value: String(o.dispatch_type || '').trim().toLowerCase() === 'auto' ? '自动' : '手动' }, { label: '门禁状态', value: p.detailSafetyGateState.value || 'unknown' }];
  });
  const detailTimeInfoRows = computed(() => {
    const o = p.detailOrder.value;
    if (!o) return [];
    return [{ label: '计划开始', value: formatDetailDateTime(o.planned_start_time || o.start_time) }, { label: '计划结束', value: formatDetailDateTime(o.planned_end_time || o.end_time) }, { label: '实际开始', value: formatDetailDateTime(o.actual_start_time) }, { label: '实际结束', value: formatDetailDateTime(o.actual_end_time) }, { label: '预计完成', value: formatDetailDateTime(o.estimated_completion_time) }, { label: '有效结束', value: formatDetailDateTime(o.effective_end_time || o.actual_end_time || o.planned_end_time || o.end_time) }];
  });
  const detailResourceInfoRows = computed(() => {
    const o = p.detailOrder.value;
    if (!o) return [];
    return [{ label: '归属班组', value: String(o.team_name || '') }, { label: '负责人', value: String(o.individual_username || o.focus_user_name || '') }, { label: '执行编组', value: detailCrewMembers.value.length > 0 ? detailCrewMembers.value.join(' / ') : '-' }, { label: '资质缺口', value: detailQualificationGaps.value.length > 0 ? detailQualificationGaps.value.join(' ; ') : '-' }, { label: '设备', value: detailEquipmentCodes.value.length > 0 ? detailEquipmentCodes.value.join(' / ') : '-' }];
  });
  const detailFlightStatusSummary = computed(() => {
    const counts = new Map<string, number>();
    for (const order of p.detailFlightOrders.value) { const s = String(order.status || ''); counts.set(s, (counts.get(s) || 0) + 1); }
    return Array.from(counts.entries()).map(([label, value]) => ({ label, value }));
  });

  return {
    switchTerminal,
    handleSearch, handleSearchNext,
    toggleAiDrawer, closeAiDrawer, openAssistantShell, replanDirectApplyEnabled, toggleStatusPanel, closeStatusPanel, toggleChatDrawer, closeChatDrawer, sendChatFromDrawer, toggleOpsMenu, closeOpsMenu, toggleGanttLegend, toggleGuideAndLegendPanel, closeGuideAndLegendPanel, toggleBatchToolbar,
    handleViewTabChange, resetWindowToNow, handleSettingsApply, handleGuideSettingsChange,
    handleStatusFilterBlocked, handleStatusShowAll, handleStatusSelectAll, handleStatusOrderOpen, toggleOrderSelection,
    handleReplanPreview, handleReplanApply, handleReplanClear,
    fetchConflicts, fetchAnalytics,
    previewAiSuggestion, applyAiSuggestion, handleAiGenerate,
    previewScenario, clearScenario,
    handleBatchComplete, handleBatchPublish, handleBatchClear,
    handleKeydown, handleBackdropClick, handleGanttChartDoubleClick, handleGanttChartClick,
    detailCrewMembers, detailQualificationGaps, detailEquipmentCodes,
    detailTaskInfoRows, detailTimeInfoRows, detailResourceInfoRows, detailFlightStatusSummary,
    // Non-blocking overrun warning surface (does not gate publish/replan/dispatch)
    overrunWarnings: overrunWarnings.warnings,
    overrunWarningBusyIds: overrunWarnings.actionBusyIds,
    handleOverrunAcknowledge,
    handleOverrunResolve,
    handleOverrunJumpOrder,
    handleOverrunJumpOrders,
  };
}
