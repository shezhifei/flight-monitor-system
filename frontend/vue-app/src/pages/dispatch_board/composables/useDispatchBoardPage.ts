import { computed, reactive, ref } from 'vue';
import {
  useDispatchBoardData,
  loadTerminalInfoList,
  type TerminalInfo,
  type ConflictItem,
  type AnalyticsData,
  type ResourceFocus,
  type ReplanStrategy,
  type DispatchOrderStatus,
  type BatchProcessState,
} from '@/composables/useDispatchBoardData';
import { useDispatchBoardDetail } from '@/composables/useDispatchBoardDetail';
import { useDispatchChat } from '@/composables/useDispatchChat';
import { useDispatchReplan } from '@/composables/useDispatchReplan';

interface GuideSettings {
  autoRefresh: boolean;
  refreshInterval: string;
  showCompleted: boolean;
  timeScale: string;
  conflictNotification: boolean;
  completeNotification: boolean;
  cornerFade: boolean;
}

const GUIDE_SETTINGS_KEY = 'dispatch_board_guide_settings';

const defaultGuideSettings: GuideSettings = {
  autoRefresh: true,
  refreshInterval: '30000',
  showCompleted: true,
  timeScale: '30',
  conflictNotification: true,
  completeNotification: false,
  cornerFade: false,
};

function loadGuideSettings(): GuideSettings {
  try {
    const raw = window.localStorage.getItem(GUIDE_SETTINGS_KEY);
    if (!raw) return { ...defaultGuideSettings };
    const parsed = JSON.parse(raw) as Partial<GuideSettings>;
    return { ...defaultGuideSettings, ...parsed };
  } catch {
    return { ...defaultGuideSettings };
  }
}

export function useDispatchBoardPage() {
  // Drawer / Panel visibility
  const isAiDrawerVisible = ref(false);
  const isStatusPanelVisible = ref(false);
  const isChatDrawerVisible = ref(false);
  const isOpsMenuVisible = ref(false);
  const isGanttLegendPopoverVisible = ref(false);
  const isGuideAndLegendPanelVisible = ref(false);
  const isBatchToolbarVisible = ref(false);

  // AI Drawer tabs
  const activeAiTab = ref<'assistant' | 'conflict'>('assistant');

  // AI Assistant state
  const aiStreamEnabled = ref(true);
  const aiObjective = ref('clear_pending');
  const aiSuggestionList = ref<Array<{ id: string; title: string; description: string; confidence?: number; orderId?: string; orderIds?: string[]; suggestionType?: string }>>([]);
  const aiMetrics = ref({ conflicts: 0, pending: 0, heavy: 0 });

  // Analytics state
  const analyticsMode = ref<'team' | 'employee'>('team');
  const analyticsData = ref<AnalyticsData | null>(null);
  const analyticsMetrics = ref({ conflictRate: '-', replanRate: '-', responseMinutes: '-', balanceScore: '-', idleRate: '-', ontimeRate: '-' });
  const analyticsBreakdownList = ref<Array<{ id: string; label: string; value: string; orderCount?: number; completedOrderCount?: number; occupiedMinutes?: number; teamLabels?: string[]; resourceId?: string; orderIds?: string[]; representativeOrderId?: string }>>([]);

  // Conflict state
  const conflictRawList = ref<ConflictItem[]>([]);
  const conflictList = ref<ConflictItem[]>([]);
  const availableConflictTypes = computed(() => {
    const types = new Set<string>();
    conflictRawList.value.forEach(c => { if (c.conflict_type) types.add(c.conflict_type); });
    return Array.from(types);
  });
  const conflictSeverityFilter = ref('all');
  const conflictTypeFilter = ref('all');
  const conflictQueryInput = ref('');
  const conflictMetrics = ref({ total: 0, high: 0, orders: 0 });

  // Scenario state
  const scenarioEquipment = ref('');
  const scenarioStand = ref('');
  const scenarioDelay = ref('');
  const scenarioFrozen = ref('');
  const scenarioImpactedOrders = ref<Array<{ id: string; title: string; description: string; orderId?: string }>>([]);
  const scenarioProjectedConflicts = ref<Array<{ id: string; title: string; description: string; orderId?: string }>>([]);
  const scenarioRecommendations = ref<Array<{ id: string; title: string; description: string; orderId?: string }>>([]);
  const scenarioMetricsData = ref({ impactedCount: '-', conflictCount: '-', delayedCount: '-', riskLevel: '-', manualConfirmation: '-', changedCount: '-' });

  // Replan state
  const replanMaxSuggestions = ref(20);
  const replanStrategy = ref<ReplanStrategy>('balanced');
  const { mode: replanMode, suggestions: replanSuggestionList, solverMetadata, canApply: replanCanApply, runReplan, applyReplan, clearReplan } = useDispatchReplan();

  const replanStatusLabel = computed(() => {
    switch (replanMode.value) {
      case 'snapshotting': return '获取快照中...';
      case 'solving': return '求解器计算中...';
      // 求解器把"排不上人"当成合法解（空候选槽位按构造钉成 gap），所以求解成功
      // 不等于方案可执行。plan_complete 为假时要单独说出来，不能只报建议条数。
      case 'previewing': {
        if (solverMetadata.value?.plan_complete !== true) {
          return `已预览 ${replanSuggestionList.value.length} 条建议（方案不完整，禁止应用）`;
        }
        if (solverMetadata.value?.lexicographic_degraded === true) {
          const stages = solverMetadata.value.degraded_stages?.join('、') || '未知阶段';
          return `已预览 ${replanSuggestionList.value.length} 条建议（限时近似：${stages}）`;
        }
        return `已预览 ${replanSuggestionList.value.length} 条建议`;
      }
      case 'applying': return '应用方案中...';
      case 'error': return '发生错误';
      default: return '请先点击"预览重排"';
    }
  });

  const categorizedReplanSuggestions = computed(() => {
    type SuggestionItem = typeof replanSuggestionList.value[number];
    const groups: Record<string, { label: string; items: SuggestionItem[] }> = {
      assigned_conflict_resolution: { label: '冲突修复项', items: [] },
      unassigned_new_assignment: { label: '新增派单项', items: [] },
      unassigned_late_assignment: { label: '迟到派单项', items: [] },
      other: { label: '重排建议', items: [] },
    };
    replanSuggestionList.value.forEach(s => {
      const type = String(s.suggestionType || '').trim();
      if (groups[type]) groups[type].items.push(s);
      else groups.other.items.push(s);
    });
    return Object.values(groups).filter(g => g.items.length > 0);
  });

  // Chat state
  const chatInput = ref('');
  const chatInputCount = ref(0);
  const chatAtAll = ref(false);
  const { chatGroups: chatGroupList, chatMessages: chatMessageList, chatSelectedGroupId: chatActiveGroup, chatLoadingMessages, chatMessagesError, chatMessagesHasMore, chatMessagesNextBeforeSeq, chatUnreadTotal, loadChatGroups, loadChatMessages, selectChatGroup, sendChatMessage, initChatSession, destroyChatSession } = useDispatchChat();

  // Settings state
  const guideSettings = reactive<GuideSettings>(loadGuideSettings());
  const settingRefreshInterval = ref(guideSettings.refreshInterval);
  const settingSafetyGateFilter = ref('all');

  // Status panel state
  const selectedStatus = ref<DispatchOrderStatus>('pending');

  // Search state
  const searchQuery = ref('');
  const searchResults = ref<Array<{ id: string; label: string; sub: string }>>([]);
  const searchIndex = ref(0);
  const searchMetaLabel = ref('未搜索');

  // Trend chart
  const trendChartRef = ref<HTMLElement | null>(null);

  // Ops menu
  const activeViewMode = ref<'flight' | 'team' | 'employee' | 'equipment'>('flight');

  // Terminals
  const terminals = ref<TerminalInfo[]>([]);
  const activeTerminal = ref<string>('all');

  // Resource focus
  const resourceFocusText = ref('');
  const resourceFocus = ref<ResourceFocus | null>(null);
  const impactedOrderIds = ref<string[]>([]);

  // Dispatch board data composable
  const { timelineData, windowStartMs, windowEndMs, safetyProgress, safetyGateFilter, setSafetyGateFilter, setTerminal, refreshTimeline, refreshSafetyProgress, setWindow } = useDispatchBoardData();

  // Batch process
  const batchProcess = ref<BatchProcessState>({ isRunning: false, currentIndex: 0, totalItems: 0, successCount: 0, failCount: 0, currentOrderId: null, errors: [], isGuided: false, orderIds: [] });
  const selectedOrderIds = ref<string[]>([]);

  // Detail composable
  const { isVisible: isDetailDrawerVisible, mode: detailMode, title: detailTitle, order: detailOrder, flightSummary: detailFlightSummary, flightOrders: detailFlightOrders, checklist: detailChecklist, checklistLoading: detailChecklistLoading, checklistError: detailChecklistError, gateHint: detailGateHint, submittingKey: detailSubmittingKey, opening: detailOpening, completing: detailCompleting, currentOrderId: detailCurrentOrderId, safetyGateState: detailSafetyGateState, criticalChecklistItems, routineChecklistItems, completionReady: detailCompletionReady, completionButtonText: detailCompletionButtonText, routinePendingCount: detailRoutinePendingCount, canSubmitChecklist: detailCanSubmitChecklist, canCompleteOrder: detailCanCompleteOrder, openOrderDetail, openFlightSummaryDetail, openFlightOrderDetail, closeDetailDrawer, refreshChecklist: refreshDetailChecklist, submitChecklistItem: submitDetailChecklistItem, submitRoutineChecklistBatch: submitDetailRoutineChecklistBatch, completeCurrentOrder: completeCurrentDetailOrder } = useDispatchBoardDetail({ refreshTimeline, refreshSafetyProgress, onFocusChange: ({ focus, text }) => { resourceFocus.value = focus; resourceFocusText.value = text || '未选择聚焦'; } });

  return {
    isAiDrawerVisible, isStatusPanelVisible, isChatDrawerVisible, isOpsMenuVisible, isGanttLegendPopoverVisible, isGuideAndLegendPanelVisible, isBatchToolbarVisible,
    activeAiTab, aiStreamEnabled, aiObjective, aiSuggestionList, aiMetrics,
    analyticsMode, analyticsData, analyticsMetrics, analyticsBreakdownList,
    conflictRawList, conflictList, availableConflictTypes, conflictSeverityFilter, conflictTypeFilter, conflictQueryInput, conflictMetrics,
    scenarioEquipment, scenarioStand, scenarioDelay, scenarioFrozen, scenarioImpactedOrders, scenarioProjectedConflicts, scenarioRecommendations, scenarioMetricsData,
    replanMaxSuggestions, replanStrategy, replanMode, replanSuggestionList, solverMetadata, replanCanApply, replanStatusLabel, categorizedReplanSuggestions, runReplan, applyReplan, clearReplan,
    chatInput, chatInputCount, chatAtAll, chatGroupList, chatMessageList, chatActiveGroup, chatLoadingMessages, chatMessagesError, chatMessagesHasMore, chatMessagesNextBeforeSeq, chatUnreadTotal,
    guideSettings, settingRefreshInterval, settingSafetyGateFilter,
    selectedStatus, selectedOrderIds, batchProcess,
    searchQuery, searchResults, searchIndex, searchMetaLabel,
    trendChartRef,
    activeViewMode, terminals, activeTerminal,
    resourceFocusText, resourceFocus, impactedOrderIds,
    timelineData, windowStartMs, windowEndMs, safetyProgress, safetyGateFilter, detailCurrentOrderId,
    detailOrder, detailMode, detailTitle, detailOpening, detailFlightSummary, detailFlightOrders,
    detailChecklist, detailChecklistLoading, detailChecklistError, detailGateHint, detailSubmittingKey,
    detailCompleting, detailSafetyGateState, criticalChecklistItems, routineChecklistItems,
    detailCompletionReady, detailCompletionButtonText, detailRoutinePendingCount,
    detailCanSubmitChecklist, detailCanCompleteOrder,
    isDetailDrawerVisible,
    loadChatGroups, loadChatMessages, selectChatGroup, sendChatMessage, initChatSession, destroyChatSession,
    loadTerminals: async () => { const loaded = await loadTerminalInfoList(); terminals.value = [{ terminal: 'all', label: '全部', active: true }, ...loaded]; },
    switchTerminal: (terminal: string) => { activeTerminal.value = terminal; terminals.value.forEach(t => { t.active = t.terminal === terminal; }); setTerminal(terminal); refreshTimeline(); },
    refreshTimeline, refreshSafetyProgress, setSafetyGateFilter, setWindow,
    openOrderDetail, openFlightSummaryDetail, openFlightOrderDetail, closeDetailDrawer,
    refreshDetailChecklist, submitDetailChecklistItem, submitDetailRoutineChecklistBatch, completeCurrentDetailOrder,
  };
}
