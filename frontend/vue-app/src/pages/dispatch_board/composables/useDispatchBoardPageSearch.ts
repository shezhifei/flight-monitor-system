import { computed } from 'vue';
import type { ComputedRef, Ref } from 'vue';
import {
  countOrdersByStatus,
  filterTimelineBySafetyGate,
  type DispatchOrder,
  type DispatchOrderStatus,
  type SafetyGateFilter,
  type SafetyProgressMap,
  type TerminalInfo,
  type TimelineData,
} from '@/composables/useDispatchBoardData';

export interface UseDispatchBoardPageSearchOptions {
  timelineData: Readonly<Ref<TimelineData | null>>;
  safetyProgress: Readonly<Ref<Readonly<SafetyProgressMap>>>;
  safetyGateFilter: Readonly<Ref<SafetyGateFilter>>;
  guideSettings: { showCompleted: boolean };
  selectedStatus: Ref<DispatchOrderStatus>;
  searchQuery: Ref<string>;
  searchResults: Ref<Array<{ id: string; label: string; sub: string }>>;
  searchIndex: Ref<number>;
  searchMetaLabel: Ref<string>;
  selectedOrderIds: Ref<string[]>;
  activeTerminal: Ref<string>;
  terminals: Ref<TerminalInfo[]>;
  setTerminal: (terminal: string) => void;
  refreshTimeline: () => Promise<void>;
}

export interface UseDispatchBoardPageSearchReturn {
  visibleTimelineItems: ComputedRef<readonly DispatchOrder[]>;
  displayedTimelineData: ComputedRef<TimelineData | null>;
  statusCounts: ComputedRef<Record<DispatchOrderStatus, number>>;
  statusOrderList: ComputedRef<Array<{ id: string; title: string; status: string; start_time: string }>>;
  statusTotalCount: ComputedRef<number>;
  terminalSelectorData: ComputedRef<Array<{ id: string; name: string; count: number }>>;
  currentTerminalId: ComputedRef<string>;
  switchTerminal: (terminal: string) => void;
  handleSearch: () => void;
  handleSearchNext: () => void;
  handleStatusFilterBlocked: () => void;
  handleStatusShowAll: () => void;
  handleStatusSelectAll: () => void;
  handleStatusOrderOpen: (orderId: string) => void;
  toggleOrderSelection: (orderId: string) => void;
}

export function useDispatchBoardPageSearch(options: UseDispatchBoardPageSearchOptions): UseDispatchBoardPageSearchReturn {
  const {
    timelineData,
    safetyProgress,
    safetyGateFilter,
    guideSettings,
    selectedStatus,
    searchQuery,
    searchResults,
    searchIndex,
    searchMetaLabel,
    selectedOrderIds,
    activeTerminal,
    terminals,
    setTerminal,
    refreshTimeline,
  } = options;

  const visibleTimelineItems = computed<readonly DispatchOrder[]>(() => {
    const items = timelineData.value?.items || [];
    return guideSettings.showCompleted
      ? items
      : items.filter((item) => String(item.status || '').trim().toLowerCase() !== 'completed');
  });

  const displayedTimelineData = computed<TimelineData | null>(() => {
    const data = timelineData.value;
    return data ? { ...data, items: visibleTimelineItems.value } : data;
  });

  const statusCounts = computed<Record<DispatchOrderStatus, number>>(() =>
    countOrdersByStatus(visibleTimelineItems.value, safetyProgress.value, safetyGateFilter.value),
  );

  const statusOrderList = computed(() => {
    const filtered = filterTimelineBySafetyGate(visibleTimelineItems.value, safetyProgress.value, safetyGateFilter.value);
    return filtered
      .filter((item) => !item.is_flight_summary && item.status === selectedStatus.value)
      .map((item) => ({
        id: String(item.order_id || ''),
        title: String(item.task_type || item.flight_id || '-'),
        status: String(item.status || 'pending'),
        start_time: String(item.start_time || ''),
      }))
      .sort((a, b) => new Date(a.start_time || 0).getTime() - new Date(b.start_time || 0).getTime());
  });

  const statusTotalCount = computed(() => statusOrderList.value.length);

  const terminalSelectorData = computed(() =>
    terminals.value.filter((t) => t.terminal !== 'all').map((t) => ({ id: t.terminal, name: t.label, count: 0 })),
  );
  const currentTerminalId = computed(() => activeTerminal.value);

  function switchTerminal(t: string) {
    activeTerminal.value = t;
    terminals.value.forEach((term) => {
      term.active = term.terminal === t;
    });
    setTerminal(t);
    refreshTimeline();
  }

  function handleSearch() {
    const q = searchQuery.value.trim();
    if (!q) {
      searchResults.value = [];
      searchMetaLabel.value = '未搜索';
      return;
    }
    const items = visibleTimelineItems.value;
    const matches = items
      .filter(
        (item) =>
          String(item.order_id || '').toLowerCase().includes(q.toLowerCase()) ||
          String(item.flight_id || '').toLowerCase().includes(q.toLowerCase()) ||
          String(item.task_type || '').toLowerCase().includes(q.toLowerCase()),
      )
      .slice(0, 10)
      .map((item, i) => ({
        id: `search-${i}`,
        label: String(item.order_id || item.flight_id || ''),
        sub: String(item.task_type || item.status || ''),
      }));
    searchResults.value = matches;
    searchIndex.value = 0;
    searchMetaLabel.value = matches.length > 0 ? `${matches.length} 条结果` : '无匹配';
  }

  function handleSearchNext() {
    if (searchResults.value.length === 0) return;
    searchIndex.value = (searchIndex.value + 1) % searchResults.value.length;
  }

  function handleStatusFilterBlocked() {
    selectedStatus.value = 'pending';
  }

  function handleStatusShowAll() {
    selectedStatus.value = 'pending';
  }

  function handleStatusSelectAll() {
    const ids = statusOrderList.value.map((o) => o.id);
    selectedOrderIds.value = [...new Set([...selectedOrderIds.value, ...ids])];
  }

  function handleStatusOrderOpen(orderId: string) {
    // The caller is expected to wire this to openOrderDetail.
    // Returning the orderId keeps the API explicit.
    return orderId;
  }

  function toggleOrderSelection(orderId: string) {
    const idx = selectedOrderIds.value.indexOf(orderId);
    if (idx === -1) selectedOrderIds.value.push(orderId);
    else selectedOrderIds.value.splice(idx, 1);
  }

  return {
    visibleTimelineItems,
    displayedTimelineData,
    statusCounts,
    statusOrderList,
    statusTotalCount,
    terminalSelectorData,
    currentTerminalId,
    switchTerminal,
    handleSearch,
    handleSearchNext,
    handleStatusFilterBlocked,
    handleStatusShowAll,
    handleStatusSelectAll,
    handleStatusOrderOpen,
    toggleOrderSelection,
  };
}
