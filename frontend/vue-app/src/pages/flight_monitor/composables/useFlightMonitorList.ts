import { computed, nextTick, ref } from 'vue';
import type { Ref, ComputedRef } from 'vue';
import {
  DEFAULT_BUSINESS_FILTERS,
  DEFAULT_SEARCH_FIELDS,
  getAnomalyCountForFlight,
  hasActiveBusinessFilters,
  hasVipMarker,
  isDelayedFlight,
  normalizeFlightId,
  type BusinessFilters,
  type Flight,
} from '../../../composables/useFlightData';
import { BASE_COLUMNS, DEFAULT_VISIBLE_COLUMN_KEYS } from '../../../components/flight-monitor/FlightList.vue';
import type { FlightViewMode } from '../../../components/flight-monitor/helpers';
import { useFlightStream } from '../../../composables/useFlightStream';
import type { UseFlightDataReturn } from '../../../composables/useFlightData';

export interface UseFlightMonitorListOptions {
  flightData: UseFlightDataReturn;
  flightStream: ReturnType<typeof useFlightStream>;
  selectedFlightId: Ref<string | null>;
  viewMode: Ref<FlightViewMode>;
  alertPoolOpen: Ref<boolean>;
  searchOptionsExpanded: Ref<boolean>;
  businessFilterExpanded: Ref<boolean>;
  announce: (message: string) => void;
}

export interface UseFlightMonitorListReturn {
  originalFlights: ComputedRef<Flight[]>;
  totalFlights: ComputedRef<number>;
  visibleFlights: ComputedRef<Flight[]>;
  visibleCount: ComputedRef<number>;
  hasActiveFilters: ComputedRef<boolean>;
  filterCounts: ComputedRef<{ anomaly: number; delay: number; vip: number; quickTurn: number }>;
  visibleAnomalyFlights: ComputedRef<Flight[]>;
  anomalySeverity: ComputedRef<'high' | 'medium' | 'low'>;
  hasVisibleFlights: ComputedRef<boolean>;
  showFilteredEmptyState: ComputedRef<boolean>;
  showDatasetEmptyState: ComputedRef<boolean>;
  showFlightList: ComputedRef<boolean>;
  columnConfigState: Ref<{
    isOpen: boolean;
    items: string[];
    visibleColumns: Record<string, boolean>;
  }>;
  visibleColumns: ComputedRef<string[]>;
  handleSort: (field: string) => void;
  selectFlight: (flightId: string) => void;
  focusSelectedFlight: () => Promise<void>;
  submitSearch: () => Promise<void>;
  toggleAlertPool: () => void;
  closeAlertPool: () => void;
  setViewMode: (nextMode: FlightViewMode) => void;
  handleSearchFieldChange: (key: keyof typeof DEFAULT_SEARCH_FIELDS, checked: boolean) => void;
  handleBusinessFilterChange: (key: keyof BusinessFilters, value: BusinessFilters[keyof BusinessFilters]) => void;
  resetBusinessFilters: () => void;
  clearAllFilters: () => void;
  handleColumnSave: () => void;
  closeColumnModal: () => void;
  getColumnLabel: (key: string) => string;
  resetColumnConfig: () => void;
  reorderColumnItems: (fromKey: string, toKey: string) => void;
}

const defaultBusinessFilters: BusinessFilters = { ...DEFAULT_BUSINESS_FILTERS, commercialSignedFilter: 'all' };

const defaultVisibleColumnKeys = new Set(DEFAULT_VISIBLE_COLUMN_KEYS);

/**
 * 列可见性：默认取当前默认可见集合；localStorage key `flight_monitor_columns`
 * 保存每列显式布尔值；存档中未记录的列（例如后来新增的列）按当前默认集合补齐。
 */
function loadColumnVisibility(): Record<string, boolean> {
  const visibility = BASE_COLUMNS.reduce(
    (acc, col) => ({ ...acc, [col.key]: defaultVisibleColumnKeys.has(col.key) }),
    {} as Record<string, boolean>,
  );
  try {
    const raw = localStorage.getItem('flight_monitor_columns');
    if (raw) {
      const saved = JSON.parse(raw) as Record<string, unknown>;
      for (const col of BASE_COLUMNS) {
        if (typeof saved[col.key] === 'boolean') visibility[col.key] = saved[col.key] as boolean;
      }
    }
  } catch {
    // 存档损坏时按默认可见集合回退
  }
  return visibility;
}

/** 列顺序：保留已保存顺序，并把新增列追加到末尾 */
function loadColumnOrder(): string[] {
  const baseKeys = BASE_COLUMNS.map((c) => c.key);
  try {
    const raw = localStorage.getItem('flight_monitor_columns_order');
    if (raw) {
      const saved = JSON.parse(raw) as unknown;
      if (Array.isArray(saved)) {
        const known = new Set(baseKeys);
        const ordered: string[] = [];
        for (const key of saved) {
          if (typeof key === 'string' && known.has(key) && !ordered.includes(key)) {
            ordered.push(key);
          }
        }
        for (const key of baseKeys) {
          if (!ordered.includes(key)) ordered.push(key);
        }
        return ordered;
      }
    }
  } catch {
    // ignore
  }
  return baseKeys;
}

export function useFlightMonitorList(options: UseFlightMonitorListOptions): UseFlightMonitorListReturn {
  const { flightData, flightStream, selectedFlightId, viewMode, alertPoolOpen, searchOptionsExpanded, businessFilterExpanded, announce } = options;

  const originalFlights = computed<Flight[]>(() => Array.from(flightData.originalFlights.value) as Flight[]);
  const totalFlights = computed(() => originalFlights.value.length);
  const visibleFlights = computed<Flight[]>(() => Array.from(flightData.sortedFlights.value) as Flight[]);
  const visibleCount = computed(() => visibleFlights.value.length);

  const hasActiveFilters = computed(() => {
    return Boolean(flightData.searchQuery.value.trim())
      || hasActiveBusinessFilters(flightData.businessFilters.value, defaultBusinessFilters);
  });

  const filterCounts = computed(() => ({
    anomaly: originalFlights.value.filter((flight) => getAnomalyCountForFlight(flight) > 0).length,
    delay: originalFlights.value.filter((flight) => isDelayedFlight(flight)).length,
    vip: originalFlights.value.filter((flight) => hasVipMarker(flight)).length,
    quickTurn: originalFlights.value.filter((flight) => Boolean(flight?.is_quick_turnaround)).length,
  }));

  const visibleAnomalyFlights = computed(() => visibleFlights.value.filter((flight) => getAnomalyCountForFlight(flight) > 0));
  const anomalySeverity = computed<'high' | 'medium' | 'low'>(() => {
    const maxAnomalyCount = Math.max(0, ...visibleAnomalyFlights.value.map((flight) => getAnomalyCountForFlight(flight)));
    if (maxAnomalyCount >= 2) return 'high';
    if (maxAnomalyCount === 1) return 'medium';
    return 'low';
  });

  const hasVisibleFlights = computed(() => visibleFlights.value.length > 0);
  const isInitialLoading = computed(() => !flightStream.initialized.value);

  const showFilteredEmptyState = computed(() => !isInitialLoading.value && hasActiveFilters.value && !hasVisibleFlights.value);
  const showDatasetEmptyState = computed(() => !isInitialLoading.value && !hasActiveFilters.value && !hasVisibleFlights.value);
  const showFlightList = computed(() => !isInitialLoading.value && hasVisibleFlights.value);

  const columnConfigState = ref({
    isOpen: false,
    items: loadColumnOrder(),
    visibleColumns: loadColumnVisibility(),
  });

  const visibleColumns = computed(() =>
    columnConfigState.value.items.filter((key) => columnConfigState.value.visibleColumns[key] !== false),
  );

  function handleSort(field: string): void {
    const current = flightData.sortConfig.value;
    if (current.field === field) {
      flightData.setSortConfig({ field, direction: current.direction === 'asc' ? 'desc' : 'asc' });
    } else {
      flightData.setSortConfig({ field, direction: 'asc' });
    }
  }

  function selectFlight(flightId: string): void {
    selectedFlightId.value = flightId;
    announce(`已选中航班 ${flightData.findFlightById(flightId)?.flight_number ?? flightId}`);
  }

  async function focusSelectedFlight(): Promise<void> {
    if (!selectedFlightId.value) return;
    await nextTick();
    const target = document.querySelector<HTMLElement>(`[data-flight-id="${selectedFlightId.value}"]`);
    target?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    target?.focus();
  }

  async function submitSearch(): Promise<void> {
    if (!visibleFlights.value.length) {
      announce('当前没有匹配的航班');
      return;
    }
    if (!selectedFlightId.value || !visibleFlights.value.some((flight) => normalizeFlightId(flight.flight_id) === selectedFlightId.value)) {
      selectedFlightId.value = normalizeFlightId(visibleFlights.value[0]?.flight_id);
    }
    await focusSelectedFlight();
    announce(`搜索完成，当前显示 ${visibleCount.value} 个航班`);
  }

  function setViewMode(nextMode: FlightViewMode): void {
    viewMode.value = nextMode;
    alertPoolOpen.value = false;
    announce(nextMode === 'card' ? '已切换到卡片视图' : '已切换到表格视图');
  }

  // 进入告警池前的卡片/表格视图，「返回航班列表」时恢复。
  const lastNonAlertViewMode = ref<FlightViewMode>(viewMode.value);

  function toggleAlertPool(): void {
    if (!visibleAnomalyFlights.value.length) return;
    if (!alertPoolOpen.value) {
      lastNonAlertViewMode.value = viewMode.value;
    }
    viewMode.value = 'card';
    alertPoolOpen.value = !alertPoolOpen.value;
    announce(alertPoolOpen.value ? '已打开异常告警池' : '已关闭异常告警池');
  }

  function closeAlertPool(): void {
    if (!alertPoolOpen.value) return;
    alertPoolOpen.value = false;
    viewMode.value = lastNonAlertViewMode.value;
    announce('已返回航班列表');
  }

  function handleSearchFieldChange(key: keyof typeof DEFAULT_SEARCH_FIELDS, checked: boolean): void {
    flightData.setSearchFields({
      ...flightData.searchFields.value,
      [key]: checked,
    });
  }

  function handleBusinessFilterChange(key: keyof BusinessFilters, value: BusinessFilters[keyof BusinessFilters]): void {
    flightData.setBusinessFilters({
      ...flightData.businessFilters.value,
      [key]: value,
    });
  }

  function resetBusinessFilters(): void {
    flightData.setBusinessFilters(defaultBusinessFilters);
    announce('已恢复默认业务筛选');
  }

  function clearAllFilters(): void {
    flightData.setSearchQuery('');
    flightData.setSearchFields(DEFAULT_SEARCH_FIELDS);
    flightData.setBusinessFilters(defaultBusinessFilters);
    searchOptionsExpanded.value = false;
    businessFilterExpanded.value = false;
    alertPoolOpen.value = false;
    announce('已清空全部筛选条件');
  }

  function handleColumnSave(): void {
    columnConfigState.value.isOpen = false;
    localStorage.setItem('flight_monitor_columns', JSON.stringify(columnConfigState.value.visibleColumns));
    localStorage.setItem('flight_monitor_columns_order', JSON.stringify(columnConfigState.value.items));
    announce('表头配置已保存');
  }

  function closeColumnModal(): void {
    columnConfigState.value.isOpen = false;
  }

  function getColumnLabel(key: string): string {
    return BASE_COLUMNS.find((c) => c.key === key)?.label ?? key;
  }

  function resetColumnConfig(): void {
    columnConfigState.value.visibleColumns = BASE_COLUMNS.reduce(
      (acc, col) => ({ ...acc, [col.key]: defaultVisibleColumnKeys.has(col.key) }),
      {} as Record<string, boolean>,
    );
    columnConfigState.value.items = BASE_COLUMNS.map((c) => c.key);
  }

  /** 配置列弹窗内拖拽重排 */
  function reorderColumnItems(fromKey: string, toKey: string): void {
    if (!fromKey || !toKey || fromKey === toKey) return;
    const items = [...columnConfigState.value.items];
    const fromIdx = items.indexOf(fromKey);
    const toIdx = items.indexOf(toKey);
    if (fromIdx < 0 || toIdx < 0) return;
    items.splice(fromIdx, 1);
    // 源在目标后 → after(target)；源在目标前 → before(target)
    // 等价于：移除后按目标当前下标插入（from < to 时目标下标会 -1，再 +1 即 after）
    const insertAt = fromIdx < toIdx ? toIdx : toIdx;
    items.splice(insertAt, 0, fromKey);
    columnConfigState.value.items = items;
  }

  return {
    originalFlights,
    totalFlights,
    visibleFlights,
    visibleCount,
    hasActiveFilters,
    filterCounts,
    visibleAnomalyFlights,
    anomalySeverity,
    hasVisibleFlights,
    showFilteredEmptyState,
    showDatasetEmptyState,
    showFlightList,
    columnConfigState,
    visibleColumns,
    handleSort,
    selectFlight,
    focusSelectedFlight,
    submitSearch,
    toggleAlertPool,
    closeAlertPool,
    setViewMode,
    handleSearchFieldChange,
    handleBusinessFilterChange,
    resetBusinessFilters,
    clearAllFilters,
    handleColumnSave,
    closeColumnModal,
    getColumnLabel,
    resetColumnConfig,
    reorderColumnItems,
  };
}
