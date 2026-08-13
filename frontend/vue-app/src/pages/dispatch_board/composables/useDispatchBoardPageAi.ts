import { computed, ref } from 'vue';
import type { ComputedRef, Ref } from 'vue';
import { useApi } from '@/composables/useApi';
import { useToast } from '@/composables/useToast';
import { useDispatchReplan, type ReplanSuggestion } from '@/composables/useDispatchReplan';
import { buildResourceFocus, type DispatchOrder, type ResourceFocus, type TimelineData } from '@/composables/useDispatchBoardData';
import {
  type AiSuggestion,
  type EmployeeAnalyticsItem,
  toTimestamp,
  unwrapEnvelope,
} from './useDispatchBoardPageAiTypes';
import { useDispatchBoardPageAiAnalytics } from './useDispatchBoardPageAiAnalytics';
import { useDispatchBoardPageAiScenario } from './useDispatchBoardPageAiScenario';

export interface UseDispatchBoardPageAiOptions {
  timelineData: Readonly<Ref<TimelineData | null>>;
  windowStartMs: Readonly<Ref<number>>;
  windowEndMs: Readonly<Ref<number>>;
  visibleTimelineItems: ComputedRef<readonly DispatchOrder[]>;
  isStatusPanelVisible: Ref<boolean>;
  activeViewMode: Ref<'flight' | 'team' | 'employee' | 'equipment'>;
  analyticsMode: Ref<'team' | 'employee'>;
  resourceFocus: Ref<ResourceFocus | null>;
  resourceFocusText: Ref<string>;
  impactedOrderIds: Ref<string[]>;
  setWindow: (startMs: number, endMs: number) => void;
  refreshTimeline: () => Promise<void>;
}

export interface UseDispatchBoardPageAiReturn {
  activeAiTab: Ref<'assistant' | 'conflict'>;
  aiStreamEnabled: Ref<boolean>;
  aiObjective: Ref<string>;
  aiSuggestionList: Ref<AiSuggestion[]>;
  aiMetrics: Ref<{ conflicts: number; pending: number; heavy: number }>;
  analyticsMode: Ref<'team' | 'employee'>;
  analyticsData: Ref<unknown | null>;
  analyticsMetrics: Ref<{ conflictRate: string; replanRate: string; responseMinutes: string; balanceScore: string; idleRate: string; ontimeRate: string }>;
  analyticsBreakdownList: Ref<Array<{ id: string; label: string; value: string; orderCount?: number; completedOrderCount?: number; occupiedMinutes?: number; teamLabels?: string[]; resourceId?: string; orderIds?: string[]; representativeOrderId?: string }>>;
  conflictRawList: Ref<import('@/composables/useDispatchBoardData').ConflictItem[]>;
  conflictList: Ref<import('@/composables/useDispatchBoardData').ConflictItem[]>;
  conflictSeverityFilter: Ref<string>;
  conflictTypeFilter: Ref<string>;
  conflictMetrics: Ref<{ total: number; high: number; orders: number }>;
  availableConflictTypes: ComputedRef<string[]>;
  scenarioEquipment: Ref<string>;
  scenarioStand: Ref<string>;
  scenarioDelay: Ref<string>;
  scenarioFrozen: Ref<string>;
  scenarioImpactedOrders: Ref<Array<{ id: string; title: string; description: string; orderId?: string }>>;
  scenarioProjectedConflicts: Ref<Array<{ id: string; title: string; description: string; orderId?: string }>>;
  scenarioRecommendations: Ref<Array<{ id: string; title: string; description: string; orderId?: string }>>;
  scenarioMetricsData: Ref<{ impactedCount: string; conflictCount: string; delayedCount: string; riskLevel: string; manualConfirmation: string; changedCount: string }>;
  replanMaxSuggestions: Ref<number>;
  replanStrategy: Ref<'stability' | 'balanced' | 'efficiency'>;
  replanMode: Ref<string>;
  replanSuggestionList: Ref<ReplanSuggestion[]>;
  solverMetadata: Ref<unknown>;
  replanCanApply: Readonly<Ref<boolean>>;
  replanStatusLabel: ComputedRef<string>;
  categorizedReplanSuggestions: ComputedRef<Array<{ label: string; items: ReplanSuggestion[] }>>;
  trendChartRef: Ref<HTMLElement | null>;
  fetchConflicts: () => Promise<void>;
  updateConflictList: () => void;
  fetchAnalytics: () => Promise<void>;
  previewScenario: () => Promise<void>;
  clearScenario: () => void;
  fetchAiSuggestions: () => Promise<void>;
  handleAiGenerate: () => Promise<void>;
  handleReplanPreview: () => Promise<void>;
  handleReplanApply: () => Promise<void>;
  handleReplanClear: () => void;
  previewAiSuggestion: (s: AiSuggestion) => void;
  applyAiSuggestion: (s: AiSuggestion) => void;
  setImpactedOrders: (orderIds: ReadonlyArray<unknown>) => void;
  panTimelineToOrder: (orderId: string) => void;
}

export function useDispatchBoardPageAi(options: UseDispatchBoardPageAiOptions): UseDispatchBoardPageAiReturn {
  const {
    timelineData,
    windowStartMs,
    windowEndMs,
    visibleTimelineItems,
    isStatusPanelVisible,
    activeViewMode,
    analyticsMode,
    resourceFocus,
    resourceFocusText,
    impactedOrderIds,
    setWindow,
    refreshTimeline,
  } = options;

  const api = useApi();
  const toast = useToast();

  const activeAiTab = ref<'assistant' | 'conflict'>('assistant');
  const aiStreamEnabled = ref(true);
  const aiObjective = ref('clear_pending');
  const aiSuggestionList = ref<AiSuggestion[]>([]);
  const aiMetrics = ref({ conflicts: 0, pending: 0, heavy: 0 });

  const analytics = useDispatchBoardPageAiAnalytics({
    timelineData,
    windowStartMs,
    windowEndMs,
    visibleTimelineItems,
    analyticsMode,
    impactedOrderIds,
  });

  const scenario = useDispatchBoardPageAiScenario({
    windowStartMs,
    windowEndMs,
    impactedOrderIds,
  });

  const replanMaxSuggestions = ref(20);
  const replanStrategy = ref<'stability' | 'balanced' | 'efficiency'>('balanced');
  const {
    mode: replanMode,
    suggestions: replanSuggestionList,
    solverMetadata,
    canApply: replanCanApply,
    runReplan,
    applyReplan,
    clearReplan,
  } = useDispatchReplan();

  const replanStatusLabel = computed(() => {
    switch (replanMode.value) {
      case 'snapshotting':
        return '获取快照中...';
      case 'solving':
        return '求解器计算中...';
      case 'previewing':
        // 见 useDispatchBoardPage 的同名 computed：feasible 与 plan_complete 是
        // 两件事，槽位没排满时必须显式提示，否则会被当成干净成功。
        if (solverMetadata.value?.plan_complete !== true) {
          return `已预览 ${replanSuggestionList.value.length} 条建议（方案不完整，禁止应用）`;
        }
        if (solverMetadata.value?.lexicographic_degraded === true) {
          const stages = solverMetadata.value.degraded_stages?.join('、') || '未知阶段';
          return `已预览 ${replanSuggestionList.value.length} 条建议（限时近似：${stages}）`;
        }
        return `已预览 ${replanSuggestionList.value.length} 条建议`;
      case 'applying':
        return '应用方案中...';
      case 'error':
        return '发生错误';
      default:
        return '请先点击"预览重排"';
    }
  });

  const categorizedReplanSuggestions = computed(() => {
    const groups: Record<string, { label: string; items: ReplanSuggestion[] }> = {
      assigned_conflict_resolution: { label: '冲突修复项', items: [] },
      unassigned_new_assignment: { label: '新增派单项', items: [] },
      unassigned_late_assignment: { label: '迟到派单项', items: [] },
      other: { label: '重排建议', items: [] },
    };
    replanSuggestionList.value.forEach((s) => {
      const type = String(s.suggestionType || '').trim();
      if (groups[type]) groups[type].items.push(s);
      else groups.other.items.push(s);
    });
    return Object.values(groups).filter((g) => g.items.length > 0);
  });

  function setImpactedOrders(orderIds: ReadonlyArray<unknown>): void {
    impactedOrderIds.value = Array.from(new Set(orderIds.map((id: unknown) => String(id || '').trim()).filter(Boolean)));
  }

  function panTimelineToOrder(orderId: string) {
    const target = timelineData.value?.items?.find((item: DispatchOrder) => item.order_id === orderId || item.id === orderId);
    if (!target) return;
    const startMs = Date.parse(String(target.start_time || target.planned_start_time));
    const endMs = Date.parse(String(target.end_time || target.planned_end_time || target.effective_end_time));
    if (startMs && endMs) {
      setWindow(startMs - 20 * 60 * 1000, endMs + 60 * 60 * 1000);
      refreshTimeline();
    }
  }

  function previewAiSuggestion(s: AiSuggestion) {
    const ids = s.orderIds && s.orderIds.length > 0 ? s.orderIds : s.orderId ? [s.orderId] : [];
    if (ids.length > 0) {
      setImpactedOrders(ids);
      const focus = buildResourceFocus({
        resource_type: 'employee',
        resource_id: ids[0],
        related_order_ids: ids,
        source_panel: 'ai_assistant',
        source_key: s.id,
      });
      resourceFocus.value = focus;
      resourceFocusText.value = `建议定位: ${s.title}`;
      panTimelineToOrder(ids[0]);
      toast.show('success', '已定位建议对应任务');
    } else {
      toast.show('warning', '当前建议暂无可定位目标');
    }
  }

  function applyAiSuggestion(s: AiSuggestion) {
    if (!s) return;
    const ids = s.orderIds && s.orderIds.length > 0 ? s.orderIds : s.orderId ? [s.orderId] : [];
    if (s.suggestionType === 'conflict-priority' || s.suggestionType === 'backend_conflict') {
      isStatusPanelVisible.value = true;
      if (activeViewMode.value !== 'employee') activeViewMode.value = 'employee';
      if (ids.length > 0) analytics.conflictQueryInput.value = ids[0];
      toast.show('success', '已切换到冲突治理面板');
    } else if (s.suggestionType === 'pending-priority') {
      isStatusPanelVisible.value = false;
      toast.show('success', '已开启待派工单详情以供处理');
    } else if (s.suggestionType === 'load-balance') {
      if (activeViewMode.value !== 'employee') activeViewMode.value = 'employee';
      isStatusPanelVisible.value = true;
      analyticsMode.value = 'employee';
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

  async function handleReplanPreview() {
    await runReplan(replanStrategy.value, replanMaxSuggestions.value, {
      windowStartMs: windowStartMs.value,
      windowEndMs: windowEndMs.value,
    });
    impactedOrderIds.value = replanSuggestionList.value.map((item) => item.orderId || '');
  }

  async function handleReplanApply() {
    const success = await applyReplan(replanStrategy.value);
    if (success) {
      impactedOrderIds.value = [];
      await Promise.all([refreshTimeline(), analytics.fetchConflicts(), analytics.fetchAnalytics()]);
    }
  }

  function handleReplanClear() {
    clearReplan();
    impactedOrderIds.value = [];
  }

  async function fetchAiSuggestions() {
    const timelineItems = (timelineData.value?.items || []).filter((item: DispatchOrder) => !item.is_flight_summary);
    const pendingOrders = timelineItems.filter((item: DispatchOrder) => String(item.status || '').trim() === 'pending');
    const inProgressOrders = timelineItems.filter((item: DispatchOrder) => String(item.status || '').trim() === 'in_progress');
    const heavyEmployees = analytics.buildEmployeeAnalyticsBreakdown(timelineItems).filter((item: EmployeeAnalyticsItem) => item.orderCount >= 2);
    if (aiObjective.value === 'resolve_conflicts' && analytics.conflictList.value.length === 0) await analytics.fetchConflicts();
    const suggestions: AiSuggestion[] = [];
    try {
      const urgency = aiObjective.value === 'resolve_conflicts' || analytics.conflictList.value.length > 0 ? '高' : '中';
      const prompt = `当前系统有 ${pendingOrders.length} 个待派工单, ${analytics.conflictList.value.length} 个冲突。当前目标：${aiObjective.value}。请提供调度建议。`;
      const res = await api.post<Record<string, unknown>>('/api/v2/ai/tools/execute', {
        tool_name: 'get_handling_recommendation',
        tool_args: { incident_description: prompt, urgency },
      });
      if (res.ok) {
        const payload = unwrapEnvelope<Record<string, unknown>>(res.data);
        const payloadRec = payload as Record<string, unknown> | null;
        const resultData = (payloadRec?.result_data as Record<string, unknown> | undefined) || {};
        const text = String(resultData.output || resultData.summary || '').trim();
        if (text)
          suggestions.push({
            id: 'advisor-priority',
            title: 'AI 调度建议',
            description: text,
            confidence: 95,
            orderId: pendingOrders[0]?.order_id,
            suggestionType: 'backend_conflict',
          });
      }
    } catch (e) {
      console.warn('Failed to fetch AI advisor suggestion:', e);
    }
    if (pendingOrders.length > 0) {
      const earliest = [...pendingOrders].sort(
        (a: DispatchOrder, b: DispatchOrder) =>
          toTimestamp(a.planned_start_time || a.start_time) - toTimestamp(b.planned_start_time || b.start_time),
      )[0];
      suggestions.push({
        id: 'pending-priority',
        title: `优先处理待派工 (${pendingOrders.length} 项)`,
        description: `建议优先处理 ${earliest?.task_type || earliest?.order_id || '最早待派工任务'}`,
        confidence: 86,
        orderId: earliest?.order_id,
        suggestionType: 'pending-priority',
      });
    }
    if (analytics.conflictList.value.length > 0) {
      const primary = analytics.conflictList.value[0];
      const ids = Array.isArray(primary.related_dispatch_order_ids) ? primary.related_dispatch_order_ids : [];
      suggestions.push({
        id: 'conflict-priority',
        title: `优先消解冲突 (${analytics.conflictList.value.length} 项)`,
        description: `${primary.resource_name || primary.resource_id || '资源'} 存在 ${primary.conflict_type || '冲突'}`,
        confidence: 91,
        orderId: ids[0] ? String(ids[0]) : undefined,
        orderIds: ids.map(String),
        suggestionType: 'conflict-priority',
      });
    }
    if (heavyEmployees.length > 0) {
      const top = heavyEmployees[0];
      suggestions.push({
        id: 'load-balance',
        title: `资源负载偏高 (${heavyEmployees.length} 条)`,
        description: `${top.label} 当前负载较高`,
        confidence: 78,
        orderIds: top.orderIds,
        orderId: top.representativeOrderId,
        suggestionType: 'load-balance',
      });
    }
    if (aiObjective.value === 'delay_prevention' && inProgressOrders.length > 0) {
      suggestions.push({
        id: 'delay-prevention',
        title: '关注进行中工单的延误传导',
        description: `当前有 ${inProgressOrders.length} 条进行中工单`,
        confidence: 74,
        orderId: inProgressOrders[0]?.order_id,
        suggestionType: 'delay-prevention',
      });
    }
    aiSuggestionList.value = suggestions;
    aiMetrics.value = { conflicts: analytics.conflictList.value.length, pending: pendingOrders.length, heavy: heavyEmployees.length };
  }

  async function handleAiGenerate() {
    await Promise.all([fetchAiSuggestions(), analytics.fetchAnalytics()]);
  }

  return {
    activeAiTab,
    aiStreamEnabled,
    aiObjective,
    aiSuggestionList,
    aiMetrics,
    analyticsMode,
    ...analytics,
    ...scenario,
    replanMaxSuggestions,
    replanStrategy,
    replanMode,
    replanSuggestionList,
    solverMetadata,
    replanCanApply,
    replanStatusLabel,
    categorizedReplanSuggestions,
    fetchConflicts: analytics.fetchConflicts,
    updateConflictList: analytics.updateConflictList,
    fetchAnalytics: analytics.fetchAnalytics,
    previewScenario: scenario.previewScenario,
    clearScenario: scenario.clearScenario,
    fetchAiSuggestions,
    handleAiGenerate,
    handleReplanPreview,
    handleReplanApply,
    handleReplanClear,
    previewAiSuggestion,
    applyAiSuggestion,
    setImpactedOrders,
    panTimelineToOrder,
  };
}
