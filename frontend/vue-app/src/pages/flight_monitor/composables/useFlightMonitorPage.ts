import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import type { ComputedRef, Ref } from 'vue';
import { pageUrl, type PageKey } from '../../../shared/page-routes';
import { useNotification } from '../../../composables/useNotification';
import { useToast } from '../../../composables/useToast';
import {
  DEFAULT_BUSINESS_FILTERS,
  DEFAULT_SEARCH_FIELDS,
  normalizeFlightId as normalizeFlightIdValue,
  type Flight,
} from '../../../composables/useFlightData';
import { useFlightData } from '../../../composables/useFlightData';
import { useFlightStream } from '../../../composables/useFlightStream';
import { useSSE } from '../../../composables/useSSE';
import { useAuth } from '../../../composables/useAuth';
import type { FlightViewMode } from '../../../components/flight-monitor/helpers';
import { useFlightMonitorList } from './useFlightMonitorList';
import { useFlightMonitorModals } from './useFlightMonitorModals';
import { useMilestonePulse } from './useMilestonePulse';
import { useFlightCellSelection } from './useFlightCellSelection';
import { useFlightBatchEdit } from './useFlightBatchEdit';
import { getBatchFieldLabel } from '../flightBatchEditableFields';
import type { ChatNotificationTarget } from '../../../composables/chatTargetFromNotification';

export interface UseFlightMonitorPageReturn {
  // Navigation
  pageUrl: (name: string) => string;

  // Core data
  flightData: ReturnType<typeof useFlightData>;
  flightStream: ReturnType<typeof useFlightStream>;
  notificationData: ReturnType<typeof useNotification>;
  auth: ReturnType<typeof useAuth>;
  toast: ReturnType<typeof useToast>;

  // UI state
  viewMode: Ref<FlightViewMode>;
  searchOptionsExpanded: Ref<boolean>;
  businessFilterExpanded: Ref<boolean>;
  selectedFlightId: Ref<string | null>;
  alertPoolOpen: Ref<boolean>;
  updatePanelOpen: Ref<boolean>;
  ariaAnnouncement: Ref<string>;
  dispatchNotifyOpen: Ref<boolean>;
  dispatchChatOpen: Ref<boolean>;
  chatFocusGroupId: Ref<string | null>;
  flightInsightOpen: Ref<boolean>;
  openChatFromDock: () => void;
  openChatFromNotification: (target: ChatNotificationTarget) => void;
  closeChat: () => void;

  // Computed
  selectedFlight: ComputedRef<Flight | null>;
  isAuthenticated: ComputedRef<boolean>;
  lastUpdatedLabel: ComputedRef<string>;
  realtimeUpdateMessages: ComputedRef<string[]>;
  isInitialLoading: ComputedRef<boolean>;
  isRefreshing: ComputedRef<boolean>;
  initFailed: ComputedRef<boolean>;
  statusBanner: ComputedRef<{ tone: 'info' | 'warning' | 'danger'; title: string; description: string } | null>;

  // List
  list: ReturnType<typeof useFlightMonitorList>;

  // Modals
  modals: ReturnType<typeof useFlightMonitorModals>;

  // Batch cell selection / edit
  cellSelection: ReturnType<typeof useFlightCellSelection>;
  batchEdit: ReturnType<typeof useFlightBatchEdit>;

  // Milestone pulse (EP-04 里程碑强提醒)
  milestonePulse: ReturnType<typeof useMilestonePulse>;
  selectionRevision: Ref<number>;
  orderedFlightIds: ComputedRef<string[]>;
  handleCellSelectStart: (flightId: string, field: string, additive: boolean, shiftKey: boolean) => void;
  handleCellSelectExtend: (flightId: string, field: string) => void;
  handleCellSelectEnd: () => void;
  batchContextFieldLabel: ComputedRef<string>;

  // Lifecycle
  refreshFlights: () => Promise<void>;
  setupPanelResizer: () => void;
}

export function useFlightMonitorPage(): UseFlightMonitorPageReturn {
  const flightData = useFlightData({
    flights: [],
    originalFlights: [],
    airportContext: {
      code: 'CAN',
      display_name: '广州白云',
      name_aliases: ['广州', '白云机场'],
    },
    businessFilters: { ...DEFAULT_BUSINESS_FILTERS, commercialSignedFilter: 'all' },
    searchFields: DEFAULT_SEARCH_FIELDS,
  });

  const notificationData = useNotification();
  const toast = useToast();
  const sse = useSSE();
  const auth = useAuth();

  function announce(message: string): void {
    ariaAnnouncement.value = message;
  }

  const flightStream = useFlightStream({
    flightData,
    announce,
  });

  sse.on('notification_created', (e) => {
    try {
      const data = JSON.parse((e as MessageEvent).data);
      const msg = `收到来自 ${data.payload?.origin_label || '系统'} 的新调度通知`;
      announce(msg);
      toast.showToast('info', msg, { duration: 5000 });
      notificationData.updateUnreadCount();
    } catch (err) {
      // 忽略解析错误
    }
  });

  sse.on('message', (e) => {
    try {
      const data = JSON.parse((e as MessageEvent).data);
      if (data.topic === 'user_notifications' || (data.payload && data.payload.notification_id)) {
        notificationData.updateUnreadCount();
      }
    } catch (err) {
      // 忽略解析错误
    }
  });

  const viewMode = ref<FlightViewMode>('card');
  const searchOptionsExpanded = ref(false);
  const businessFilterExpanded = ref(false);
  const selectedFlightId = ref<string | null>(null);
  const alertPoolOpen = ref(false);
  const updatePanelOpen = ref(false);
  const ariaAnnouncement = ref('');
  const dispatchNotifyOpen = ref(false);
  const dispatchChatOpen = ref(false);
  const chatFocusGroupId = ref<string | null>(null);
  const flightInsightOpen = ref(false);

  const selectedFlight = computed(() => flightData.findFlightById(selectedFlightId.value));
  const isAuthenticated = computed(() => auth.isAuthenticated());
  const lastUpdatedLabel = computed(() => `最后更新: ${flightStream.lastUpdatedAt.value.toLocaleTimeString('zh-CN', { hour12: false })}`);
  const realtimeUpdateMessages = computed(() => Array.from(flightStream.updateMessages.value));

  const isInitialLoading = computed(() => !flightStream.initialized.value);
  const isRefreshing = computed(() => flightStream.isRefreshing.value);
  const initFailed = computed(() => Boolean(flightStream.initializationError.value));
  const hasVisibleFlights = computed(() => flightData.sortedFlights.value.length > 0);

  const statusBanner = computed(() => {
    if (initFailed.value && !hasVisibleFlights.value) {
      return {
        tone: 'danger' as const,
        title: '航班数据加载失败',
        description: flightStream.initializationError.value
          ? `${flightStream.initializationError.value}，请稍后重试。`
          : '实时快照暂不可用，请稍后重试。',
      };
    }
    if (flightStream.connectionStatusKey.value === 'reconnecting') {
      return {
        tone: 'info' as const,
        title: '实时连接重试中',
        description: '页面保留最近一次成功快照，恢复后会自动继续更新。',
      };
    }
    if (flightStream.connectionStatusKey.value === 'offline' && flightStream.initialized.value) {
      return {
        tone: 'danger' as const,
        title: '实时连接已中断',
        description: '当前展示的是最近一次成功快照，自动重连失败时可手动刷新。',
      };
    }
    if (isRefreshing.value && flightStream.initialized.value) {
      return {
        tone: 'info' as const,
        title: '正在刷新航班快照',
        description: '列表和详情将在刷新完成后更新为最新结果。',
      };
    }
    return null;
  });

  watch(
    () => flightData.sortedFlights.value,
    (flights) => {
      if (!flights.length) {
        selectedFlightId.value = null;
        alertPoolOpen.value = false;
        return;
      }
      const selectedVisible = flights.some((flight) => normalizeFlightId(flight.flight_id) === selectedFlightId.value);
      if (!selectedVisible) {
        selectedFlightId.value = normalizeFlightId(flights[0]?.flight_id);
      }
    },
    { immediate: true },
  );

  const cellSelection = useFlightCellSelection();
  const selectionRevision = ref(0);

  function bumpSelectionRevision(): void {
    selectionRevision.value += 1;
  }

  const orderedFlightIds = computed(() =>
    flightData.sortedFlights.value
      .map((flight) => normalizeFlightIdValue(flight.flight_id))
      .filter(Boolean),
  );

  async function refreshFlights(): Promise<void> {
    if (isRefreshing.value) return;
    try {
      await flightStream.refreshFlights();
      alertPoolOpen.value = false;
      cellSelection.clearSelection();
      bumpSelectionRevision();
    } catch (error) {
      const message = error instanceof Error ? error.message : '刷新航班数据失败';
      announce(message);
      toast.showToast('warning', message, { duration: 4000 });
    }
  }

  function setupPanelResizer(): void {
    const resizer = document.getElementById('resizer');
    const panel = document.querySelector('.flight-list-panel') as HTMLElement;
    if (!resizer || !panel) return;

    let isResizing = false;
    let startX = 0;
    let startWidth = 0;

    function onMouseMove(e: MouseEvent) {
      if (!isResizing) return;
      const offset = e.clientX - startX;
      const newWidth = Math.max(300, Math.min(startWidth + offset, window.innerWidth - 300));
      panel.style.width = `${newWidth}px`;
      panel.style.flex = 'none';
    }

    function onMouseUp() {
      if (isResizing) {
        isResizing = false;
        document.body.style.cursor = '';
        localStorage.setItem('flight_monitor_list_width', panel.style.width);
        document.removeEventListener('mousemove', onMouseMove);
        document.removeEventListener('mouseup', onMouseUp);
      }
    }

    resizer.addEventListener('mousedown', (e: MouseEvent) => {
      isResizing = true;
      startX = e.clientX;
      startWidth = panel.getBoundingClientRect().width;
      document.body.style.cursor = 'ew-resize';
      document.addEventListener('mousemove', onMouseMove);
      document.addEventListener('mouseup', onMouseUp);
    });

    const savedWidth = localStorage.getItem('flight_monitor_list_width');
    if (savedWidth) {
      const w = parseInt(savedWidth, 10);
      if (!isNaN(w) && w >= 300) {
        panel.style.width = `${w}px`;
        panel.style.flex = 'none';
      }
    }
  }

  const list = useFlightMonitorList({
    flightData,
    flightStream,
    selectedFlightId,
    viewMode,
    alertPoolOpen,
    searchOptionsExpanded,
    businessFilterExpanded,
    announce,
  });

  function openChatFromDock(): void {
    chatFocusGroupId.value = null;
    dispatchChatOpen.value = true;
  }

  function openChatFromNotification(target: ChatNotificationTarget): void {
    if (target.flightId) {
      list.selectFlight(target.flightId);
    }
    chatFocusGroupId.value = target.groupId;
    dispatchNotifyOpen.value = false;
    dispatchChatOpen.value = true;
  }

  function closeChat(): void {
    dispatchChatOpen.value = false;
    chatFocusGroupId.value = null;
  }

  const modals = useFlightMonitorModals({
    flightData,
    selectedFlightId,
    selectedFlight,
    isAuthenticated,
    refreshFlights,
    announce,
  });

  const batchEdit = useFlightBatchEdit({
    flightData,
    cellSelection,
    announce,
    refreshFlights,
    openSingleFieldEdit: (flightId, field, type, value) => {
      modals.handleEditField(flightId, field, type, value);
    },
    revokeSingleTimeField: async (flightId, field) => {
      // Timeline milestones: delete latest event via dispatch-timeline API.
      await flightData.writeDispatchTimelineField(flightId, field, null);
      await refreshFlights();
    },
  });

  // EP-04: 关键保障节点（清洁结束/允许登机）完成 → 全屏强提醒
  const milestonePulse = useMilestonePulse(flightData.originalFlights);

  function handleCellSelectStart(
    flightId: string,
    field: string,
    additive: boolean,
    shiftKey: boolean,
  ): void {
    if (!batchEdit.canEditField(field)) {
      return;
    }
    const ordered = orderedFlightIds.value;
    if (shiftKey && cellSelection.selectedField.value === field && cellSelection.anchorFlightId.value) {
      const result = cellSelection.selectRange(
        cellSelection.anchorFlightId.value,
        flightId,
        field,
        ordered,
        { additive },
      );
      if (!result.ok && result.message) {
        toast.showToast('warning', result.message, { duration: 3000 });
      }
    } else {
      const result = cellSelection.startDrag(flightId, field, { additive });
      if (!result.ok && result.message) {
        toast.showToast('warning', result.message, { duration: 3000 });
      }
    }
    bumpSelectionRevision();
  }

  function handleCellSelectExtend(flightId: string, field: string): void {
    if (!batchEdit.canEditField(field)) {
      return;
    }
    const result = cellSelection.updateDrag(flightId, field, orderedFlightIds.value);
    if (!result.ok && result.reason === 'cross_column' && result.message) {
      toast.showToast('warning', result.message, { duration: 2500 });
    }
    bumpSelectionRevision();
  }

  function handleCellSelectEnd(): void {
    cellSelection.endDrag();
    bumpSelectionRevision();
  }

  const batchContextFieldLabel = computed(() => {
    const field = cellSelection.selectedField.value || batchEdit.contextMenuState.value.field || '';
    return field ? getBatchFieldLabel(field) : '';
  });

  // Clear cell selection when list identity changes (view/sort/filter/auth).
  watch(viewMode, () => {
    cellSelection.clearSelection();
    bumpSelectionRevision();
  });

  watch(
    () => flightData.sortConfig.value,
    () => {
      cellSelection.clearSelection();
      bumpSelectionRevision();
    },
    { deep: true },
  );

  watch(
    () => [
      flightData.searchQuery.value,
      flightData.businessFilters.value,
      flightData.searchFields.value,
    ],
    () => {
      cellSelection.clearSelection();
      bumpSelectionRevision();
    },
    { deep: true },
  );

  watch(isAuthenticated, () => {
    cellSelection.clearSelection();
    bumpSelectionRevision();
  });

  function handleEscapeKey(event: KeyboardEvent): void {
    if (event.key !== 'Escape') {
      return;
    }
    if (batchEdit.modalState.value.isOpen) {
      batchEdit.closeBatchEdit();
      return;
    }
    if (batchEdit.contextMenuState.value.isOpen) {
      batchEdit.closeCellContextMenu();
      return;
    }
    if (cellSelection.selectedCount.value > 0) {
      cellSelection.clearSelection();
      bumpSelectionRevision();
    }
  }

  function handleShortcutKeys(event: KeyboardEvent): void {
    // '/' 聚焦搜索框（输入控件聚焦时不触发）
    if (event.key === '/' && !event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey) {
      const target = event.target as HTMLElement | null;
      if (target && target.matches('input, textarea, select, [contenteditable="true"]')) {
        return;
      }
      event.preventDefault();
      const searchInput = document.getElementById('searchInput') as HTMLInputElement | null;
      if (searchInput) {
        searchInput.focus();
        searchInput.select();
      }
      return;
    }

    // Ctrl/Cmd+Shift+V 切换卡片/表格视图（同时退出告警池）
    if ((event.ctrlKey || event.metaKey) && event.shiftKey && event.key.toLowerCase() === 'v') {
      event.preventDefault();
      list.setViewMode(viewMode.value === 'card' ? 'table' : 'card');
    }
  }

  function handleDocumentClick(): void {
    batchEdit.closeCellContextMenu();
    modals.closeContextMenu();
  }

  onMounted(() => {
    notificationData.updateUnreadCount();
    document.addEventListener('click', handleDocumentClick);
    document.addEventListener('keydown', handleEscapeKey);
    document.addEventListener('keydown', handleShortcutKeys);

    void flightStream.initialize().catch((error) => {
      const message = error instanceof Error ? error.message : '航班数据初始化失败';
      announce(message);
      toast.showToast('error', message, { duration: 5000 });
    });

    flightData.loadBusinessCaseTypes().then((types) => {
      modals.eventCreationState.value.types = types;
    }).catch((e) => console.warn('Failed to load business types', e));

    setupPanelResizer();
  });

  onUnmounted(() => {
    document.removeEventListener('click', handleDocumentClick);
    document.removeEventListener('keydown', handleEscapeKey);
    document.removeEventListener('keydown', handleShortcutKeys);
  });

  return {
    pageUrl: (name: string) => pageUrl(name as PageKey),
    flightData,
    flightStream,
    notificationData,
    auth,
    toast,
    viewMode,
    searchOptionsExpanded,
    businessFilterExpanded,
    selectedFlightId,
    alertPoolOpen,
    updatePanelOpen,
    ariaAnnouncement,
    dispatchNotifyOpen,
    dispatchChatOpen,
    chatFocusGroupId,
    flightInsightOpen,
    openChatFromDock,
    openChatFromNotification,
    closeChat,
    selectedFlight,
    isAuthenticated,
    lastUpdatedLabel,
    realtimeUpdateMessages,
    isInitialLoading,
    isRefreshing,
    initFailed,
    statusBanner,
    list,
    modals,
    cellSelection,
    batchEdit,
    milestonePulse,
    selectionRevision,
    orderedFlightIds,
    handleCellSelectStart,
    handleCellSelectExtend,
    handleCellSelectEnd,
    batchContextFieldLabel,
    refreshFlights,
    setupPanelResizer,
  };
}

function normalizeFlightId(flightId: unknown): string | null {
  const id = String(flightId ?? '').trim();
  return id || null;
}
