import { computed, readonly, ref } from 'vue';
import type { ComputedRef, Ref } from 'vue';
import { useToast } from '@/composables/useToast';
import {
  DEFAULT_FUTURE_MINUTES,
  DEFAULT_PAST_MINUTES,
  DEFAULT_REFRESH_INTERVAL_MS,
  fetchAnalytics,
  fetchTimeline,
  fetchTimelineSafetyProgress,
  loadConfiguredTerminals,
} from './useDispatchBoardGantt';
import type { AnalyticsBreakdownMode, AnalyticsData, SafetyGateFilter, TimelineData, ViewMode } from './useDispatchBoardGantt';
import {
  countOrdersByStatus,
  filterTimelineBySafetyGate,
  normalizeSearchQuery,
  searchTimelineItems,
} from './useDispatchBoardSearch';
import type { SearchMatch } from './useDispatchBoardSearch';
import type {
  BatchProcessState,
  DispatchOrder,
  DispatchOrderStatus,
} from './useDispatchBoardOrders';
import type { ResourceFocus } from './useDispatchBoardResources';
import type { SafetyProgressMap } from './useDispatchBoardChecklist';

export * from './useDispatchBoardApi';
export * from './useDispatchBoardOrders';
export * from './useDispatchBoardChecklist';
export * from './useDispatchBoardSearch';
export * from './useDispatchBoardResources';
export * from './useDispatchBoardGantt';

// ---------------------------------------------------------------------------
// Composable
// ---------------------------------------------------------------------------

export interface UseDispatchBoardDataOptions {
  initialViewMode?: ViewMode;
  initialTerminal?: string;
  initialSafetyGateFilter?: SafetyGateFilter;
  initialRefreshIntervalMs?: number;
  pastMinutes?: number;
  futureMinutes?: number;
}

export interface UseDispatchBoardDataReturn {
  // Reactive state
  viewMode: Readonly<Ref<ViewMode>>;
  terminal: Readonly<Ref<string>>;
  terminals: Readonly<Ref<readonly string[]>>;
  windowStartMs: Readonly<Ref<number>>;
  windowEndMs: Readonly<Ref<number>>;
  timelineData: Readonly<Ref<TimelineData | null>>;
  safetyProgress: Readonly<Ref<Readonly<SafetyProgressMap>>>;
  resourceFocus: Ref<ResourceFocus | null>;
  searchQuery: Readonly<Ref<string>>;
  searchMatches: Readonly<Ref<readonly SearchMatch[]>>;
  searchMatchIndex: Readonly<Ref<number>>;
  safetyGateFilter: Readonly<Ref<SafetyGateFilter>>;
  refreshIntervalMs: Readonly<Ref<number>>;
  loading: Readonly<Ref<boolean>>;
  error: Readonly<Ref<string | null>>;
  analyticsData: Readonly<Ref<AnalyticsData>>;
  analyticsBreakdownMode: Readonly<Ref<AnalyticsBreakdownMode>>;
  statusCounts: ComputedRef<Record<DispatchOrderStatus, number>>;
  filteredTimelineItems: ComputedRef<readonly DispatchOrder[]>;
  batchState: Readonly<Ref<BatchProcessState>>;

  // Actions
  setViewMode: (mode: ViewMode) => void;
  setTerminal: (terminal: string) => void;
  setWindow: (startMs: number, endMs: number) => void;
  resetWindowToNow: () => void;
  setSafetyGateFilter: (filter: SafetyGateFilter) => void;
  setResourceFocus: (focus: ResourceFocus | null) => void;
  setSearchQuery: (query: string) => void;
  setSearchMatchIndex: (index: number) => void;
  setRefreshIntervalMs: (ms: number) => void;
  setAnalyticsBreakdownMode: (mode: AnalyticsBreakdownMode) => void;
  moveBatchIndex: (step: number) => void;
  setBatchState: (state: Partial<BatchProcessState>) => void;

  // Fetch actions
  refreshTimeline: () => Promise<void>;
  refreshSafetyProgress: () => Promise<void>;
  refreshAnalytics: () => Promise<void>;
  performSearch: () => void;
}

export function useDispatchBoardData(options: UseDispatchBoardDataOptions = {}): UseDispatchBoardDataReturn {
  const toast = useToast();
  const viewMode = ref<ViewMode>(options.initialViewMode ?? 'flight');
  const terminal = ref<string>(options.initialTerminal ?? 'all');
  const terminals = ref<string[]>(['all']);
  const pastMinutes = options.pastMinutes ?? DEFAULT_PAST_MINUTES;
  const futureMinutes = options.futureMinutes ?? DEFAULT_FUTURE_MINUTES;

  const now = Date.now();
  const windowStartMs = ref<number>(now - pastMinutes * 60_000);
  const windowEndMs = ref<number>(now + futureMinutes * 60_000);

  const timelineData = ref<TimelineData | null>(null);
  const safetyProgress = ref<SafetyProgressMap>({});
  const resourceFocus = ref<ResourceFocus | null>(null);
  const searchQuery = ref<string>('');
  const searchMatches = ref<SearchMatch[]>([]);
  const searchMatchIndex = ref<number>(-1);
  const safetyGateFilter = ref<SafetyGateFilter>(options.initialSafetyGateFilter ?? 'all');
  const refreshIntervalMs = ref<number>(options.initialRefreshIntervalMs ?? DEFAULT_REFRESH_INTERVAL_MS);
  const loading = ref<boolean>(false);
  const error = ref<string | null>(null);
  const analyticsData = ref<AnalyticsData>({ summary: null, breakdown: [], trend: [] });
  const analyticsBreakdownMode = ref<AnalyticsBreakdownMode>('team');

  const batchState = ref<BatchProcessState>({
    isRunning: false,
    currentIndex: 0,
    totalItems: 0,
    successCount: 0,
    failCount: 0,
    currentOrderId: null,
    errors: [],
    isGuided: false,
    orderIds: [],
  });

  const statusCounts = computed<Record<DispatchOrderStatus, number>>(() => {
    const items = timelineData.value?.items ?? [];
    return countOrdersByStatus(items, safetyProgress.value, safetyGateFilter.value);
  });

  const filteredTimelineItems = computed(() => {
    const items = timelineData.value?.items ?? [];
    return filterTimelineBySafetyGate(items, safetyProgress.value, safetyGateFilter.value);
  }) as ComputedRef<readonly DispatchOrder[]>;

  function setViewMode(mode: ViewMode) {
    viewMode.value = mode;
  }

  function setTerminal(t: string) {
    terminal.value = t;
  }

  function setWindow(startMs: number, endMs: number) {
    windowStartMs.value = startMs;
    windowEndMs.value = endMs;
  }

  function resetWindowToNow() {
    const nowMs = Date.now();
    windowStartMs.value = nowMs - pastMinutes * 60_000;
    windowEndMs.value = nowMs + futureMinutes * 60_000;
  }

  function setSafetyGateFilter(filter: SafetyGateFilter) {
    safetyGateFilter.value = filter;
  }

  function setResourceFocus(focus: ResourceFocus | null) {
    resourceFocus.value = focus;
  }

  function setSearchQuery(query: string) {
    searchQuery.value = normalizeSearchQuery(query);
  }

  function setSearchMatchIndex(index: number) {
    searchMatchIndex.value = index;
  }

  function setRefreshIntervalMs(ms: number) {
    refreshIntervalMs.value = ms;
  }

  function setAnalyticsBreakdownMode(mode: AnalyticsBreakdownMode) {
    analyticsBreakdownMode.value = mode;
  }

  function moveBatchIndex(step: number) {
    const next = batchState.value.currentIndex + step;
    const len = batchState.value.orderIds.length;
    if (len === 0) return;

    if (next >= 1 && next <= len) {
      batchState.value.currentIndex = next;
      batchState.value.currentOrderId = batchState.value.orderIds[next - 1];
    }
  }

  function setBatchState(newState: Partial<BatchProcessState>) {
    batchState.value = { ...batchState.value, ...newState };
  }

  async function refreshTimeline() {
    loading.value = true;
    error.value = null;
    try {
      const data = await fetchTimeline({
        viewMode: viewMode.value,
        windowStartMs: windowStartMs.value,
        windowEndMs: windowEndMs.value,
        terminal: terminal.value,
      });
      timelineData.value = data;
      try {
        safetyProgress.value = await fetchTimelineSafetyProgress(data);
      } catch (e) {
        safetyProgress.value = {};
        error.value = e instanceof Error ? `安全进度加载失败: ${e.message}` : '安全进度加载失败';
      }

      // Auto-load terminals if not yet populated
      if (terminals.value.length <= 1) {
        terminals.value = ['all', ...await loadConfiguredTerminals(data)];
      }
    } catch (e) {
      error.value = e instanceof Error ? e.message : '刷新时间线失败';
      toast.showToast('error', error.value);
    } finally {
      loading.value = false;
    }
  }

  async function refreshSafetyProgress() {
    try {
      safetyProgress.value = await fetchTimelineSafetyProgress(timelineData.value);
    } catch (e) {
      error.value = e instanceof Error ? `安全进度刷新失败: ${e.message}` : '安全进度刷新失败';
    }
  }

  async function refreshAnalytics() {
    try {
      const data = await fetchAnalytics({
        windowStartMs: windowStartMs.value,
        windowEndMs: windowEndMs.value,
      });
      analyticsData.value = {
        summary: data.summary,
        breakdown: [...data.breakdown],
        trend: [...data.trend],
      };
    } catch (e) {
      error.value = e instanceof Error ? `分析数据刷新失败: ${e.message}` : '分析数据刷新失败';
      toast.showToast('error', error.value);
    }
  }

  function performSearch() {
    const items = timelineData.value?.items ?? [];
    searchMatches.value = [...searchTimelineItems(items, searchQuery.value)] as SearchMatch[];
    searchMatchIndex.value = searchMatches.value.length > 0 ? 0 : -1;
  }

  return {
    // Reactive state
    viewMode: readonly(viewMode),
    terminal: readonly(terminal),
    terminals: readonly(terminals),
    windowStartMs: readonly(windowStartMs),
    windowEndMs: readonly(windowEndMs),
    timelineData: readonly(timelineData),
    safetyProgress: readonly(safetyProgress) as Readonly<Ref<Readonly<SafetyProgressMap>>>,
    resourceFocus,
    searchQuery: readonly(searchQuery),
    searchMatches: readonly(searchMatches),
    searchMatchIndex: readonly(searchMatchIndex),
    safetyGateFilter: readonly(safetyGateFilter),
    refreshIntervalMs: readonly(refreshIntervalMs),
    loading: readonly(loading),
    error: readonly(error),
    analyticsData: readonly(analyticsData),
    analyticsBreakdownMode: readonly(analyticsBreakdownMode),
    statusCounts,
    filteredTimelineItems,
    batchState,

    // Actions
    setViewMode,
    setTerminal,
    setWindow,
    resetWindowToNow,
    setSafetyGateFilter,
    setResourceFocus,
    setSearchQuery,
    setSearchMatchIndex,
    setRefreshIntervalMs,
    setAnalyticsBreakdownMode,
    moveBatchIndex,
    setBatchState,

    // Fetch actions
    refreshTimeline,
    refreshSafetyProgress,
    refreshAnalytics,
    performSearch,
  };
}

