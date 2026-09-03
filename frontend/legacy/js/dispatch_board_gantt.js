(function () {
    'use strict';

    // Archive shim: gantt used bare showToast while toast component only exports window.Toast.show.
    function showToast(message, type, options) {
        if (window.Toast && typeof window.Toast.show === 'function') {
            return window.Toast.show(type || 'info', message, options || {});
        }
        if (typeof window.showToast === 'function' && window.showToast !== showToast) {
            return window.showToast(message, type, options);
        }
        return null;
    }

    const STATUS_LABELS = {
        pending: '待派工',
        assigned: '已分配',
        in_progress: '进行中',
        completed: '已完成',
        cancelled: '已取消'
    };

    const STATUS_SYMBOLS = {
        pending: '○',
        assigned: '●',
        in_progress: '▶',
        completed: '✓',
        cancelled: '×'
    };

    const STATUS_ORDER = ['pending', 'assigned', 'in_progress', 'completed', 'cancelled'];
    const CONFLICT_SEVERITY_ORDER = ['critical', 'high', 'medium', 'low'];
    const CONFLICT_TYPE_LABELS = {
        team_overlap: '班组冲突',
        individual_overlap: '人员冲突',
        stand_overlap: '机位冲突',
        equipment_overlap: '设备冲突'
    };
    const REPLAN_REASON_LABELS = {
        resource_time_overlap: '资源时间重叠',
        assigned_conflict_repair: '冲突修复',
        unassigned_assignment: '未指派排班'
    };
    const STATUS_COLORS = {
        pending: '#D97706',
        assigned: '#2563EB',
        in_progress: '#0F9D8A',
        completed: '#2F9E44',
        cancelled: '#94A3B8'
    };
    const SEMANTIC_COLORS = {
        alert: '#D64545',
        lock: '#475569',
        summaryText: '#203246',
        metaText: '#5f7082'
    };
    const SAFETY_GATE_COLORS = {
        ready: '#34C759',
        pending: '#FF9500',
        blocked: SEMANTIC_COLORS.alert
    };

    const CHART_THEME = {
        emptyText: '#6a7788',
        axisLine: '#8a97a8',
        axisLabel: '#5f7082',
        laneLabel: '#33485f',
        splitLine: 'rgba(15, 23, 42, 0.08)',
        zoomBorder: 'rgba(15, 23, 42, 0.12)',
        zoomBg: 'rgba(255,255,255,0.74)',
        zoomFiller: 'rgba(11,119,227,0.2)',
        tooltipBorder: 'rgba(15, 23, 42, 0.1)',
        nowLine: '#FF3B30',
        nowLabelText: '#FF3B30',
        nowLabelBg: 'rgba(255, 59, 48, 0.12)',
        nowLabelBorder: 'rgba(255, 59, 48, 0.35)',
        itemHighlightStroke: '#12293f',
        itemConflictStroke: SEMANTIC_COLORS.alert,
        itemSummaryStroke: '#1D4ED8',
        itemStroke: 'rgba(15, 23, 42, 0.22)',
        laneFocusFill: 'rgba(0, 122, 255, 0.08)',
        laneFocusStroke: 'rgba(0, 122, 255, 0.22)',
        laneSecondaryFocusFill: 'rgba(0, 122, 255, 0.04)',
        laneSecondaryFocusStroke: 'rgba(0, 122, 255, 0.12)',
        laneFocusLabelText: '#0f3e73',
        laneFocusLabelBg: 'rgba(0, 122, 255, 0.14)',
        laneSecondaryFocusLabelText: '#29547f',
        laneSecondaryFocusLabelBg: 'rgba(0, 122, 255, 0.08)',
        itemText: '#ffffff',
        detailSubText: SEMANTIC_COLORS.metaText
    };

    const VIEW_LABELS = {
        flight: '航班视角',
        team: '班组视角',
        employee: '员工视角',
        equipment: '设备视角'
    };

    const DEFAULT_REFRESH_INTERVAL_MS = 15000;
    const RESOURCE_FOCUS_PANEL_LABELS = {
        analytics: '运营分析',
        conflict: '冲突治理',
        scenario: '场景预览',
        replan: '冲突重排'
    };
    const NOW_TICK_INTERVAL_MS = 1000;
    const DEFAULT_PAST_MINUTES = 60;
    const DEFAULT_FUTURE_MINUTES = 360;
    const STATUS_LIST_LIMIT = 200;
    const CONFLICT_LIST_LIMIT = 180;
    const CONFLICT_REFRESH_MIN_INTERVAL_MS = 8000;
    const ANALYTICS_REFRESH_MIN_INTERVAL_MS = 30000;
    const SEARCH_RESULT_RENDER_LIMIT = 10;
    const GUIDE_STORAGE_KEY = 'dispatch_gantt_guide_v1_dismissed';
    const AI_STREAM_ENABLED_KEY = 'dispatch_gantt_ai_stream_enabled';
    const FRONTEND_REPLAN_WORKER_URL = '/frontend/js/workers/dispatch_replan_worker.js';
    const DISPATCH_BOARD_DATA_SCRIPT_URL = '/frontend/js/dispatch_board/data.js?v=1';
    const DISPATCH_BOARD_WORKER_POOL_SCRIPT_URL = '/frontend/js/dispatch_board/worker_pool.js?v=1';
    const FEEDBACK_COMPONENT_URLS = Object.freeze({
        toast: '/frontend/js/components/toast.js',
        loading: '/frontend/js/components/loading.js',
        emptyError: '/frontend/js/components/empty_error.js',
    });

    const state = {
        chart: null,
        user: null,
        isAdmin: false,
        viewMode: 'flight',
        terminal: 'all',
        terminals: ['all'],
        windowStartMs: 0,
        windowEndMs: 0,
        timelineData: null,
        selectedStatus: 'pending',
        statusPanelSelectedOrderIds: new Set(),
        statusPanelBatchOrderIds: [],
        statusPanelBatchIndex: -1,
        highlightedItemId: null,
        highlightedOrderId: null,
        resourceFocus: null,
        detailMode: null,
        detailOrder: null,
        detailFlightSummary: null,
        detailFlightOrders: [],
        detailSafetyChecklist: null,
        detailSafetyLoading: false,
        detailSafetyError: '',
        detailSafetyGateHint: null,
        detailSafetySubmittingKey: '',
        cornerInfoAutoFade: false,
        cornerInfoFadeTimer: null,
        cornerInfoRafId: null,
        cornerInfoPointerX: 0,
        cornerInfoPointerY: 0,
        legendDensity: 'full',
        legendPopoverOpen: false,
        legendPopoverCloseTimer: null,
        opsMenuOpen: false,
        aiDrawerTab: 'assistant',
        aiSuggestions: [],
        aiAssistantWidget: null,
        aiStreamEnabled: true,
        analyticsSummary: null,
        analyticsBreakdown: [],
        analyticsTrend: [],
        analyticsTrendChart: null,
        analyticsBreakdownMode: 'team',
        analyticsLoading: false,
        analyticsError: '',
        analyticsLastUpdatedAt: 0,
        analyticsRequestSeq: 0,
        chatEnabled: true,
        chatGroups: [],
        chatUnreadTotal: 0,
        chatSelectedGroupId: '',
        chatMessages: [],
        chatMessagesHasMore: false,
        chatMessagesNextBeforeSeq: null,
        chatLoadingGroups: false,
        chatLoadingMessages: false,
        chatSending: false,
        chatStream: null,
        chatReconnectTimer: null,
        chatReconnectDelayMs: 3000,
        chatInputDraft: '',
        chatAtAll: false,
        conflictsRaw: [],
        conflictsFiltered: [],
        conflictLoading: false,
        conflictError: '',
        conflictLastUpdatedAt: 0,
        conflictSeverity: 'all',
        conflictType: 'all',
        conflictQuery: '',
        conflictSelectedOrderId: null,
        replanStrategy: 'balanced',
        replanMaxSuggestions: 20,
        replanPreview: [],
        replanSnapshot: null,
        replanSnapshotId: '',
        replanSolverVersion: '',
        replanSolverMode: 'idle',
        replanSolverMetadata: {},
        replanSolverResult: null,
        replanError: '',
        replanLoading: false,
        replanApplying: false,
        scenarioPreview: null,
        scenarioLoading: false,
        scenarioError: '',
        scenarioEquipmentInput: '',
        scenarioStandInput: '',
        scenarioDelayInput: '',
        scenarioFrozenInput: '',
        impactedOrderIds: new Set(),
        impactedItemIds: new Set(),
        conflictRequestSeq: 0,
        replanRequestSeq: 0,
        timelineSafetyProgressByOrder: {},
        safetyGateFilter: 'all',
        searchQuery: '',
        searchMatches: [],
        searchMatchIndex: -1,
        searchResultOpen: false,
        refreshIntervalMs: DEFAULT_REFRESH_INTERVAL_MS,
        refreshTimer: null,
        nowTimer: null,
        loading: false,
        draftSelectedOrderIds: new Set(),
        selectedOrderIds: new Set(),
        draftOptimizing: false,
        draftOptimizeError: '',
        feedbackComponentsPromise: null,
    };

    function getDispatchAiBridge() {
        if (!window.DISPATCH_AI_BRIDGE || typeof window.DISPATCH_AI_BRIDGE !== 'object') {
            return null;
        }
        return window.DISPATCH_AI_BRIDGE;
    }

    function getDispatchBoardData() {
        if (!window.DispatchBoardData || typeof window.DispatchBoardData !== 'object') {
            return null;
        }
        return window.DispatchBoardData;
    }

    function getDispatchBoardWorkerPool() {
        if (!window.DispatchBoardWorkerPool || typeof window.DispatchBoardWorkerPool !== 'object') {
            return null;
        }
        return window.DispatchBoardWorkerPool;
    }

    function showButtonLoading(target, label) {
        if (!target || !(target instanceof HTMLElement)) {
            return;
        }
        if (window.Loading && typeof window.Loading.show === 'function') {
            window.Loading.show({
                mode: 'button',
                target,
                label: label || '处理中...',
            });
            return;
        }
        target.disabled = true;
    }

    function hideButtonLoading(target) {
        if (!target || !(target instanceof HTMLElement)) {
            return;
        }
        if (window.Loading && typeof window.Loading.hide === 'function') {
            window.Loading.hide(target);
            return;
        }
        target.disabled = false;
    }

    function showContainerLoading(container, message, options = {}) {
        if (!container) {
            return false;
        }
        if (window.Loading && typeof window.Loading.show === 'function') {
            if (options.preserveChildren !== true) {
                container.replaceChildren();
            }
            window.Loading.show({
                mode: 'skeleton',
                target: container,
                message: message || '正在加载内容',
                lines: options.lines || 3,
                minHeight: options.minHeight || '120px',
            });
            return true;
        }
        return false;
    }

    function hideContainerLoading(container) {
        if (!container) {
            return;
        }
        if (window.Loading && typeof window.Loading.hide === 'function') {
            window.Loading.hide(container);
        }
    }

    function renderUnifiedState(container, type, message, onRetry) {
        if (!container) {
            return false;
        }
        hideContainerLoading(container);
        if (window.EmptyError && typeof window.EmptyError.show === 'function') {
            window.EmptyError.show(container, type, message, onRetry);
            return true;
        }
        return false;
    }

    function ensureDispatchBoardDataModule() {
        const existing = getDispatchBoardData();
        if (existing) {
            return Promise.resolve(existing);
        }

        const existingScript = document.querySelector('script[data-dispatch-board-data="true"]');
        if (existingScript) {
            return new Promise((resolve, reject) => {
                existingScript.addEventListener('load', () => {
                    const loaded = getDispatchBoardData();
                    if (loaded) {
                        resolve(loaded);
                        return;
                    }
                    reject(new Error('派工调度数据模块初始化失败'));
                }, { once: true });
                existingScript.addEventListener('error', () => {
                    reject(new Error('派工调度数据模块加载失败'));
                }, { once: true });
            });
        }

        return new Promise((resolve, reject) => {
            const script = document.createElement('script');
            script.src = DISPATCH_BOARD_DATA_SCRIPT_URL;
            script.async = false;
            script.dataset.dispatchBoardData = 'true';
            script.addEventListener('load', () => {
                const loaded = getDispatchBoardData();
                if (loaded) {
                    resolve(loaded);
                    return;
                }
                reject(new Error('派工调度数据模块初始化失败'));
            }, { once: true });
            script.addEventListener('error', () => {
                reject(new Error('派工调度数据模块加载失败'));
            }, { once: true });
            document.head.appendChild(script);
        });
    }

    function ensureDispatchBoardWorkerPoolModule() {
        const existing = getDispatchBoardWorkerPool();
        if (existing) {
            return Promise.resolve(existing);
        }

        const existingScript = document.querySelector('script[data-dispatch-board-worker-pool="true"]');
        if (existingScript) {
            return new Promise((resolve, reject) => {
                existingScript.addEventListener('load', () => {
                    const loaded = getDispatchBoardWorkerPool();
                    if (loaded) {
                        resolve(loaded);
                        return;
                    }
                    reject(new Error('派工调度 Worker 池初始化失败'));
                }, { once: true });
                existingScript.addEventListener('error', () => {
                    reject(new Error('派工调度 Worker 池加载失败'));
                }, { once: true });
            });
        }

        return new Promise((resolve, reject) => {
            const script = document.createElement('script');
            script.src = DISPATCH_BOARD_WORKER_POOL_SCRIPT_URL;
            script.async = false;
            script.dataset.dispatchBoardWorkerPool = 'true';
            script.addEventListener('load', () => {
                const loaded = getDispatchBoardWorkerPool();
                if (loaded) {
                    resolve(loaded);
                    return;
                }
                reject(new Error('派工调度 Worker 池初始化失败'));
            }, { once: true });
            script.addEventListener('error', () => {
                reject(new Error('派工调度 Worker 池加载失败'));
            }, { once: true });
            document.head.appendChild(script);
        });
    }

    document.addEventListener('DOMContentLoaded', async () => {
        try {
            await ensureDispatchBoardDataModule();
            await bootstrap();
        } catch (error) {
            console.error('初始化派工调度台失败:', error);
            showToast(error.message || '初始化派工调度台失败');
        }
    });

    async function bootstrap() {
        const user = await checkAuth();
        if (!user) {
            window.location.href = '/frontend/html/login.html';
            return;
        }

        applyThemeTokens();

        state.user = user;
        state.isAdmin = Boolean(user.is_admin || (window.Auth && window.Auth.isAdmin && window.Auth.isAdmin()));
        state.aiStreamEnabled = readBoolStorage(AI_STREAM_ENABLED_KEY, true);

        const userInfo = document.getElementById('userInfo');
        if (userInfo) {
            userInfo.textContent = user.username || '用户';
        }

        // Render unified header
        if (window.Header) {
            window.Header.render('#header-host', {
                title: '派工中心',
                showBack: true,
                backHref: '/frontend/html/dashboard.html',
                user: user,
                extraLeft: '<div class="page-tabs">' +
                    '<a class="tab active" href="/frontend/html/dispatch_board.html">甘特调度台</a>' +
                    '<a class="tab" href="/frontend/html/resource_utilization.html">资源利用率</a>' +
                    '</div>',
                actions: [
                    { label: '规则中心', onClick: function() { window.location.href = '/frontend/html/dispatch_rule_center.html'; } }
                ],
                onLogout: function() {
                    if (typeof window.logout === 'function') window.logout();
                }
            });
        }

        if (window.Breadcrumb && typeof window.Breadcrumb.render === 'function') {
            window.Breadcrumb.render('#breadcrumb-host', [
                { label: '工作台', href: '/frontend/html/dashboard.html' },
                { label: '派工中心', current: true }
            ]);
        }

        const ruleCenterBtn = document.getElementById('openRuleCenterBtn');
        if (ruleCenterBtn) {
            ruleCenterBtn.addEventListener('click', () => {
                window.location.href = '/frontend/html/dispatch_rule_center.html';
            });
        }

        if (!applyInitialWindowParamsFromLocation()) {
            resetWindowToNow();
        }
        bindControls();
        initializeDispatchChatUI();
        initChart();
        initializeOpsMenu();
        initializeCornerInfoOverlay();
        initializeGanttLegendOverlay();
        initializeGuide();
        initializeAIAssistantWidget();
        renderTerminalTabs();
        await loadConfiguredTerminals();
        await refreshTimeline();
        await initializeDispatchChatFeature();
        await handleInitialChatOpenFromLocation();
        renderViewModeHint();
        updateSearchMeta();
        await switchAiDrawerTab(state.aiDrawerTab, { refresh: false });
        renderConflictGovernance();
        renderReplanHint();
        startTimers();
    }

    function initializeAIAssistantWidget() {
        if (state.aiAssistantWidget) {
            return;
        }
        state.aiAssistantWidget = {
            enableSSE: Boolean(state.aiStreamEnabled),
            toggle: null,
            setOpen(open) {
                const bridge = getDispatchAiBridge();
                if (!bridge) {
                    return;
                }
                if (!open) {
                    if (typeof bridge.closeDrawer === 'function') {
                        bridge.closeDrawer();
                    }
                    return;
                }
                if (typeof bridge.openDrawer === 'function') {
                    bridge.openDrawer('assistant', { refresh: false });
                }
            },
            persistSSEPreference() { },
            connectStream() { },
            disconnectStream() { },
        };
    }

    function bindControls() {
        const viewTabGroup = document.getElementById('viewTabGroup');
        if (viewTabGroup) {
            viewTabGroup.addEventListener('click', async (event) => {
                const button = event.target.closest('.chip-btn[data-view]');
                if (!button) {
                    return;
                }
                const nextView = button.dataset.view;
                if (!nextView || nextView === state.viewMode) {
                    return;
                }
                const activeResourceFocus = getActiveResourceFocus();
                if (activeResourceFocus && nextView !== activeResourceFocus.target_view_mode) {
                    clearResourceFocus({ render: false, silent: true });
                }
                state.viewMode = nextView;
                state.highlightedItemId = null;
                syncActiveButtons(viewTabGroup, '.chip-btn[data-view]', 'view', nextView);
                await refreshTimeline();
            });
        }

        const terminalGroup = document.getElementById('terminalGroup');
        if (terminalGroup) {
            terminalGroup.addEventListener('click', async (event) => {
                const button = event.target.closest('.chip-btn[data-terminal]');
                if (!button) {
                    return;
                }
                const nextTerminal = button.dataset.terminal;
                if (!nextTerminal || nextTerminal === state.terminal) {
                    return;
                }
                state.terminal = nextTerminal;
                syncActiveButtons(terminalGroup, '.chip-btn[data-terminal]', 'terminal', nextTerminal);
                await refreshTimeline();
            });
        }

        const refreshBtn = document.getElementById('refreshBtn');
        if (refreshBtn) {
            refreshBtn.addEventListener('click', async () => {
                showButtonLoading(refreshBtn, '刷新中...');
                try {
                    await loadConfiguredTerminals();
                    await refreshTimeline();
                } finally {
                    hideButtonLoading(refreshBtn);
                    closeOpsMenu();
                }
            });
        }

        const backToNowBtn = document.getElementById('backToNowBtn');
        if (backToNowBtn) {
            backToNowBtn.addEventListener('click', async () => {
                resetWindowToNow();
                await refreshTimeline();
                closeOpsMenu();
            });
        }

        const backToNowFloatingBtn = document.getElementById('backToNowFloatingBtn');
        if (backToNowFloatingBtn) {
            backToNowFloatingBtn.addEventListener('click', async () => {
                resetWindowToNow();
                await refreshTimeline();
            });
        }

        const statusPanelClose = document.getElementById('statusPanelClose');
        if (statusPanelClose) {
            statusPanelClose.addEventListener('click', () => closeStatusPanel());
        }

        const statusCounts = document.getElementById('statusCounts');
        if (statusCounts) {
            statusCounts.addEventListener('click', (event) => {
                const card = event.target.closest('.status-count-card[data-status]');
                if (!card) {
                    return;
                }
                const nextStatus = card.dataset.status;
                if (!nextStatus) {
                    return;
                }
                state.selectedStatus = nextStatus;
                renderStatusCounts();
                renderStatusOrderList();
                renderStatusToolbar();
            });
        }

        const statusFilterBlockedBtn = document.getElementById('statusFilterBlockedBtn');
        if (statusFilterBlockedBtn) {
            statusFilterBlockedBtn.addEventListener('click', () => {
                applyQuickSafetyGateFilter('blocked');
            });
        }

        const statusShowAllBtn = document.getElementById('statusShowAllBtn');
        if (statusShowAllBtn) {
            statusShowAllBtn.addEventListener('click', () => {
                applyQuickSafetyGateFilter('all');
            });
        }

        const statusSelectAllBtn = document.getElementById('statusSelectAllBtn');
        if (statusSelectAllBtn) {
            statusSelectAllBtn.addEventListener('click', () => {
                toggleSelectAllCurrentStatusOrders();
            });
        }

        const statusBatchOpenBtn = document.getElementById('statusBatchOpenBtn');
        if (statusBatchOpenBtn) {
            statusBatchOpenBtn.addEventListener('click', async () => {
                await startStatusPanelBatchProcess();
            });
        }

        const statusOrderList = document.getElementById('statusOrderList');
        if (statusOrderList) {
            statusOrderList.addEventListener('click', async (event) => {
                const selectBtn = event.target.closest('.status-order-select[data-order-id]');
                if (selectBtn) {
                    const orderId = String(selectBtn.dataset.orderId || '').trim();
                    if (orderId) {
                        toggleStatusOrderSelection(orderId);
                        renderStatusOrderList();
                        renderStatusToolbar();
                    }
                    return;
                }

                const item = event.target.closest('.status-order-item[data-focus-item-id]');
                if (!item) {
                    return;
                }

                const focusItemId = item.dataset.focusItemId;
                const orderId = item.dataset.orderId;
                await focusTimelineItem(focusItemId, orderId);
            });
        }

        const detailCloseBtn = document.getElementById('detailCloseBtn');
        if (detailCloseBtn) {
            detailCloseBtn.addEventListener('click', closeDetailDrawer);
        }

        const detailActions = document.getElementById('detailActions');
        if (detailActions) {
            detailActions.addEventListener('click', async (event) => {
                const actionBtn = event.target.closest('[data-action]');
                if (!actionBtn) {
                    return;
                }

                const action = actionBtn.dataset.action;
                if (action === 'close') {
                    closeDetailDrawer();
                } else if (action === 'batch-prev') {
                    await moveStatusBatchOrder(-1);
                } else if (action === 'batch-next') {
                    await moveStatusBatchOrder(1);
                } else if (action === 'batch-stop') {
                    stopStatusPanelBatchProcess();
                } else if (action === 'publish-order' && state.detailOrder) {
                    await publishDispatchOrder(state.detailOrder.id);
                } else if (action === 'cancel' && state.detailOrder) {
                    await cancelDispatchOrder(state.detailOrder.id);
                } else if (action === 'refresh-safety-checklist' && state.detailOrder) {
                    await loadOrderSafetyChecklist(state.detailOrder.id);
                } else if (action === 'complete-order' && state.detailOrder) {
                    await completeDispatchOrder(state.detailOrder.id);
                } else if (action === 'report-issue' && state.detailOrder) {
                    await openQuickIssueReport(state.detailOrder.id);
                } else if (action === 'eta-report' && state.detailOrder) {
                    await reportEstimatedCompletion(state.detailOrder.id);
                } else if (action === 'reassign' && state.detailOrder) {
                    window.location.href = `/frontend/html/resource_manager.html?dispatch_order_id=${encodeURIComponent(state.detailOrder.id)}`;
                } else if (action === 'open-flight-chat') {
                    const flightId = state.detailOrder
                        ? state.detailOrder.flight_id
                        : (state.detailFlightSummary ? state.detailFlightSummary.flight_id : '');
                    if (flightId) {
                        await openChatDrawer({ flightId });
                    }
                } else if (action === 'locate' && state.detailOrder) {
                    await focusOrder(state.detailOrder.id);
                } else if (action === 'govern-conflict' && state.detailOrder) {
                    setImpactedOrders([state.detailOrder.id], { render: true });
                    state.conflictSelectedOrderId = state.detailOrder.id;
                    await openAiDrawer('conflict', { refresh: true });
                }
            });
        }

        const detailContent = document.getElementById('detailContent');
        if (detailContent) {
            detailContent.addEventListener('click', async (event) => {
                const checklistAction = event.target.closest('.safety-item-action[data-item-code][data-result]');
                if (checklistAction && state.detailOrder) {
                    const itemCode = checklistAction.dataset.itemCode;
                    const result = checklistAction.dataset.result;
                    if (itemCode && result) {
                        await submitSafetyChecklistItem(state.detailOrder.id, itemCode, result);
                    }
                    return;
                }

                const refreshAction = event.target.closest('[data-action="refresh-safety-checklist"]');
                if (refreshAction && state.detailOrder) {
                    await loadOrderSafetyChecklist(state.detailOrder.id);
                    return;
                }

                const routineBatchAction = event.target.closest('[data-action="submit-routine-batch"]');
                if (routineBatchAction && state.detailOrder) {
                    await submitRoutineChecklistBatch(state.detailOrder.id);
                    return;
                }

                const orderItem = event.target.closest('.detail-order-item[data-order-id]');
                if (!orderItem) {
                    return;
                }

                const orderId = orderItem.dataset.orderId;
                if (orderId) {
                    await openOrderDetail(orderId);
                    await focusOrder(orderId);
                }
            });
        }

        const openAiFloatingBtn = document.getElementById('openAiFloatingBtn');
        if (openAiFloatingBtn) {
            openAiFloatingBtn.addEventListener('click', async () => {
                await openAiDrawer('assistant', { refresh: false });
                closeOpsMenu();
            });
        }

        const openStatusFloatingBtn = document.getElementById('openStatusFloatingBtn');
        if (openStatusFloatingBtn) {
            openStatusFloatingBtn.addEventListener('click', () => {
                toggleStatusPanel();
                closeOpsMenu();
            });
        }

        const openChatFloatingBtn = document.getElementById('openChatFloatingBtn');
        if (openChatFloatingBtn) {
            openChatFloatingBtn.addEventListener('click', async () => {
                await openChatDrawer();
                closeOpsMenu();
            });
        }

        const openChatCornerBadgeBtn = document.getElementById('openChatCornerBadgeBtn');
        if (openChatCornerBadgeBtn) {
            openChatCornerBadgeBtn.addEventListener('click', async () => {
                await openChatDrawer();
            });
        }

        const openGuideBtn = document.getElementById('openGuideBtn');
        if (openGuideBtn) {
            openGuideBtn.addEventListener('click', () => {
                openGuideModal();
            });
        }

        const timelineSearchBtn = document.getElementById('timelineSearchBtn');
        if (timelineSearchBtn) {
            timelineSearchBtn.addEventListener('click', async () => {
                await performTimelineSearch();
            });
        }

        const timelineSearchNextBtn = document.getElementById('timelineSearchNextBtn');
        if (timelineSearchNextBtn) {
            timelineSearchNextBtn.addEventListener('click', async () => {
                await jumpToNextSearchMatch();
            });
        }

        const timelineSearchInput = document.getElementById('timelineSearchInput');
        if (timelineSearchInput) {
            timelineSearchInput.addEventListener('keydown', async (event) => {
                if (event.isComposing) {
                    return;
                }

                if (event.key === 'Enter') {
                    event.preventDefault();
                    await performTimelineSearch();
                    return;
                }

                if (event.key === 'ArrowDown') {
                    event.preventDefault();
                    await moveSearchSelection(1);
                    return;
                }

                if (event.key === 'ArrowUp') {
                    event.preventDefault();
                    await moveSearchSelection(-1);
                    return;
                }

                if (event.key === 'Escape') {
                    closeSearchResultPanel();
                }
            });

            timelineSearchInput.addEventListener('input', () => {
                const query = normalizeSearchQuery(timelineSearchInput.value);
                state.searchQuery = query;
                if (!query) {
                    syncSearchWithTimeline();
                    closeSearchResultPanel();
                    return;
                }

                state.searchMatchIndex = -1;
                syncSearchWithTimeline();
                openSearchResultPanel();
            });

            timelineSearchInput.addEventListener('focus', () => {
                if (state.searchQuery) {
                    openSearchResultPanel();
                }
            });
        }

        const timelineSearchResults = document.getElementById('timelineSearchResults');
        if (timelineSearchResults) {
            timelineSearchResults.addEventListener('click', async (event) => {
                const button = event.target.closest('.search-result-item[data-match-index]');
                if (!button) {
                    return;
                }
                const index = Number(button.dataset.matchIndex);
                if (!Number.isFinite(index)) {
                    return;
                }
                state.searchMatchIndex = index;
                await locateSearchMatch(index);
            });
        }

        const aiCloseBtn = document.getElementById('aiCloseBtn');
        if (aiCloseBtn) {
            aiCloseBtn.addEventListener('click', closeAiDrawer);
        }

        const chatCloseBtn = document.getElementById('chatCloseBtn');
        if (chatCloseBtn) {
            chatCloseBtn.addEventListener('click', closeChatDrawer);
        }

        const chatGroupList = document.getElementById('chatGroupList');
        if (chatGroupList) {
            chatGroupList.addEventListener('click', async (event) => {
                const target = event.target.closest('.chat-group-item[data-group-id]');
                if (!target) {
                    return;
                }
                const groupId = String(target.dataset.groupId || '').trim();
                if (!groupId) {
                    return;
                }
                await selectChatGroup(groupId, { refreshMessages: true, markRead: true });
            });
        }

        const chatMessageList = document.getElementById('chatMessageList');
        if (chatMessageList) {
            chatMessageList.addEventListener('scroll', async () => {
                if (chatMessageList.scrollTop > 80) {
                    return;
                }
                if (!state.chatMessagesHasMore || state.chatLoadingMessages || !state.chatSelectedGroupId) {
                    return;
                }
                await loadChatMessages(state.chatSelectedGroupId, {
                    beforeSeq: state.chatMessagesNextBeforeSeq,
                    prepend: true,
                });
            });
        }

        const chatSendBtn = document.getElementById('chatSendBtn');
        if (chatSendBtn) {
            chatSendBtn.addEventListener('click', async () => {
                await sendChatMessage();
            });
        }

        const chatInput = document.getElementById('chatInput');
        if (chatInput) {
            chatInput.addEventListener('input', () => {
                state.chatInputDraft = String(chatInput.value || '');
                renderChatComposer();
            });

            chatInput.addEventListener('keydown', async (event) => {
                if (event.isComposing) {
                    return;
                }

                if (event.key === 'Enter' && event.ctrlKey) {
                    return;
                }

                if (event.key === 'Enter') {
                    event.preventDefault();
                    await sendChatMessage();
                }
            });
        }

        const chatAtAllToggle = document.getElementById('chatAtAllToggle');
        if (chatAtAllToggle) {
            chatAtAllToggle.addEventListener('change', () => {
                state.chatAtAll = Boolean(chatAtAllToggle.checked);
            });
        }

        const settingsApplyBtn = document.getElementById('settingsApplyBtn');
        if (settingsApplyBtn) {
            settingsApplyBtn.addEventListener('click', applySettings);
        }

        const aiGenerateBtn = document.getElementById('aiGenerateBtn');
        if (aiGenerateBtn) {
            aiGenerateBtn.addEventListener('click', async () => {
                await generateAiSuggestions();
            });
        }

        const aiStreamToggle = document.getElementById('aiStreamToggle');
        if (aiStreamToggle) {
            aiStreamToggle.checked = Boolean(state.aiStreamEnabled);
            aiStreamToggle.addEventListener('change', () => {
                const enabled = Boolean(aiStreamToggle.checked);
                state.aiStreamEnabled = enabled;
                writeBoolStorage(AI_STREAM_ENABLED_KEY, enabled);

                if (state.aiAssistantWidget) {
                    state.aiAssistantWidget.enableSSE = enabled;
                    state.aiAssistantWidget.persistSSEPreference();
                    if (state.aiAssistantWidget.toggle) {
                        state.aiAssistantWidget.toggle.checked = enabled;
                    }
                    if (enabled) {
                        state.aiAssistantWidget.connectStream();
                    } else {
                        state.aiAssistantWidget.disconnectStream();
                    }
                }

                showToast(enabled ? '实时 AI 推送已开启' : '实时 AI 推送已关闭');
            });
        }

        const aiDrawerTabs = document.getElementById('aiDrawerTabs');
        if (aiDrawerTabs) {
            aiDrawerTabs.addEventListener('click', async (event) => {
                const button = event.target.closest('.drawer-tab[data-ai-tab]');
                if (!button) {
                    return;
                }
                const nextTab = button.dataset.aiTab;
                if (!nextTab) {
                    return;
                }
                await switchAiDrawerTab(nextTab, { refresh: true });
            });
        }

        const aiSuggestionList = document.getElementById('aiSuggestionList');
        if (aiSuggestionList) {
            aiSuggestionList.addEventListener('click', async (event) => {
                const chip = event.target.closest('.suggestion-chip[data-action]');
                if (!chip) {
                    return;
                }

                const suggestionId = chip.dataset.suggestionId;
                const action = chip.dataset.action;
                const suggestion = state.aiSuggestions.find((entry) => entry.id === suggestionId);
                if (!suggestion) {
                    return;
                }

                if (action === 'preview') {
                    await previewSuggestion(suggestion);
                } else if (action === 'apply') {
                    await applySuggestion(suggestion);
                }
            });
        }

        const analyticsRefreshBtn = document.getElementById('analyticsRefreshBtn');
        if (analyticsRefreshBtn) {
            analyticsRefreshBtn.addEventListener('click', async () => {
                await refreshAnalyticsData({ force: true, silent: false });
            });
        }

        const analyticsModeButtons = document.querySelectorAll('[data-analytics-mode]');
        analyticsModeButtons.forEach((button) => {
            button.addEventListener('click', () => {
                const nextMode = String(button.dataset.analyticsMode || '').trim();
                if (!nextMode || nextMode === state.analyticsBreakdownMode) {
                    return;
                }
                state.analyticsBreakdownMode = nextMode === 'employee' ? 'employee' : 'team';
                renderAnalyticsPanel();
            });
        });

        const analyticsBreakdownList = document.getElementById('analyticsBreakdownList');
        if (analyticsBreakdownList) {
            analyticsBreakdownList.addEventListener('click', async (event) => {
                const chip = event.target.closest('[data-action]');
                if (!chip) {
                    return;
                }
                const action = chip.dataset.action;
                const orderId = chip.dataset.orderId;
                if (action === 'locate-analytics-order' && orderId) {
                    setImpactedOrders([orderId], { render: true });
                    await focusOrder(orderId);
                    return;
                }
                if (action === 'focus-analytics-resource') {
                    await switchAnalyticsResourceView(chip.dataset.viewMode, orderId, {
                        resourceType: chip.dataset.resourceType,
                        resourceId: chip.dataset.resourceId,
                        resourceLabel: chip.dataset.resourceLabel,
                        relatedOrderIds: parseCommaSeparatedIds(chip.dataset.relatedOrderIds || ''),
                        sourceKey: chip.dataset.sourceKey
                    });
                    return;
                }
                if (action === 'switch-analytics-view') {
                    await switchAnalyticsResourceView(chip.dataset.viewMode, orderId);
                }
            });
        }

        const conflictSeverityFilter = document.getElementById('conflictSeverityFilter');
        if (conflictSeverityFilter) {
            conflictSeverityFilter.addEventListener('change', () => {
                state.conflictSeverity = conflictSeverityFilter.value || 'all';
                applyConflictFilters();
                renderConflictGovernance();
            });
        }

        const conflictTypeFilter = document.getElementById('conflictTypeFilter');
        if (conflictTypeFilter) {
            conflictTypeFilter.addEventListener('change', () => {
                state.conflictType = conflictTypeFilter.value || 'all';
                applyConflictFilters();
                renderConflictGovernance();
            });
        }

        const conflictQueryInput = document.getElementById('conflictQueryInput');
        if (conflictQueryInput) {
            conflictQueryInput.addEventListener('input', () => {
                state.conflictQuery = normalizeSearchQuery(conflictQueryInput.value);
                applyConflictFilters();
                renderConflictGovernance();
            });

            conflictQueryInput.addEventListener('keydown', (event) => {
                if (event.key === 'Enter') {
                    event.preventDefault();
                    applyConflictFilters();
                    renderConflictGovernance();
                }
            });
        }

        const conflictRefreshBtn = document.getElementById('conflictRefreshBtn');
        if (conflictRefreshBtn) {
            conflictRefreshBtn.addEventListener('click', async () => {
                await refreshConflictData({ force: true, silent: false });
            });
        }

        const conflictList = document.getElementById('conflictList');
        if (conflictList) {
            conflictList.addEventListener('click', async (event) => {
                const chip = event.target.closest('[data-action]');
                if (!chip) {
                    return;
                }
                const action = chip.dataset.action;
                if (action === 'locate-conflict') {
                    const index = Number(chip.dataset.conflictIndex);
                    if (Number.isFinite(index)) {
                        await locateConflictByIndex(index);
                    }
                    return;
                }
                if (action === 'focus-conflict-resource') {
                    const conflictIndex = Number(chip.dataset.conflictIndex);
                    if (Number.isFinite(conflictIndex)) {
                        await locateConflictByIndex(conflictIndex);
                        return;
                    }
                    await applyResourceFocus({
                        resource_type: chip.dataset.resourceType,
                        resource_id: chip.dataset.resourceId,
                        resource_label: chip.dataset.resourceLabel,
                        target_view_mode: chip.dataset.viewMode,
                        related_order_ids: parseCommaSeparatedIds(chip.dataset.relatedOrderIds || ''),
                        source_panel: 'conflict',
                        source_key: chip.dataset.sourceKey,
                        resource_ids: parseCommaSeparatedIds(chip.dataset.resourceIds || ''),
                        lane_ids: parseCommaSeparatedIds(chip.dataset.laneIds || ''),
                        highlight_scope: chip.dataset.highlightScope
                    }, {
                        preferredOrderId: chip.dataset.orderId
                    });
                    return;
                }
                if (action === 'locate-conflict-order') {
                    const orderId = chip.dataset.orderId;
                    if (orderId) {
                        setImpactedOrders([orderId], { render: true });
                        await focusOrder(orderId);
                    }
                }
            });
        }

        const scenarioEquipmentInput = document.getElementById('scenarioEquipmentInput');
        if (scenarioEquipmentInput) {
            scenarioEquipmentInput.addEventListener('input', () => {
                state.scenarioEquipmentInput = scenarioEquipmentInput.value || '';
            });
        }

        const scenarioStandInput = document.getElementById('scenarioStandInput');
        if (scenarioStandInput) {
            scenarioStandInput.addEventListener('input', () => {
                state.scenarioStandInput = scenarioStandInput.value || '';
            });
        }

        const scenarioDelayInput = document.getElementById('scenarioDelayInput');
        if (scenarioDelayInput) {
            scenarioDelayInput.addEventListener('input', () => {
                state.scenarioDelayInput = scenarioDelayInput.value || '';
            });
            scenarioDelayInput.addEventListener('keydown', async (event) => {
                if (event.key === 'Enter' && !event.shiftKey) {
                    event.preventDefault();
                    await previewScenario();
                }
            });
        }

        const scenarioFrozenInput = document.getElementById('scenarioFrozenInput');
        if (scenarioFrozenInput) {
            scenarioFrozenInput.addEventListener('input', () => {
                state.scenarioFrozenInput = scenarioFrozenInput.value || '';
            });
        }

        const scenarioPreviewBtn = document.getElementById('scenarioPreviewBtn');
        if (scenarioPreviewBtn) {
            scenarioPreviewBtn.addEventListener('click', async () => {
                await previewScenario();
            });
        }

        const scenarioClearBtn = document.getElementById('scenarioClearBtn');
        if (scenarioClearBtn) {
            scenarioClearBtn.addEventListener('click', () => {
                clearScenarioPreview();
            });
        }

        const scenarioResultList = document.getElementById('scenarioResultList');
        if (scenarioResultList) {
            scenarioResultList.addEventListener('click', async (event) => {
                const chip = event.target.closest('[data-action]');
                if (!chip) {
                    return;
                }
                const action = chip.dataset.action;
                if (action !== 'locate-scenario-order') {
                    if (action === 'focus-scenario-resource') {
                        const scenarioKind = String(chip.dataset.scenarioKind || '').trim();
                        const scenarioIndex = Number(chip.dataset.scenarioIndex);
                        let resourceFocus = null;
                        if (Number.isFinite(scenarioIndex)) {
                            if (scenarioKind === 'conflict') {
                                const conflicts = Array.isArray(state.scenarioPreview?.projected_conflicts)
                                    ? state.scenarioPreview.projected_conflicts
                                    : [];
                                resourceFocus = buildResourceFocusFromConflictItem(conflicts[scenarioIndex], 'scenario');
                            } else if (scenarioKind === 'recommendation') {
                                const recommendations = Array.isArray(state.scenarioPreview?.recommendations)
                                    ? state.scenarioPreview.recommendations
                                    : [];
                                resourceFocus = buildResourceFocusFromScenarioRecommendation(recommendations[scenarioIndex], 'scenario');
                            }
                        }
                        await applyResourceFocus(resourceFocus || {
                            resource_type: chip.dataset.resourceType,
                            resource_id: chip.dataset.resourceId,
                            resource_label: chip.dataset.resourceLabel,
                            target_view_mode: chip.dataset.viewMode,
                            related_order_ids: parseCommaSeparatedIds(chip.dataset.relatedOrderIds || ''),
                            source_panel: 'scenario',
                            source_key: chip.dataset.sourceKey,
                            resource_ids: parseCommaSeparatedIds(chip.dataset.resourceIds || ''),
                            lane_ids: parseCommaSeparatedIds(chip.dataset.laneIds || ''),
                            highlight_scope: chip.dataset.highlightScope
                        }, {
                            preferredOrderId: chip.dataset.orderId
                        });
                    }
                    return;
                }
                const orderId = chip.dataset.orderId;
                if (orderId) {
                    setImpactedOrders([orderId], { render: true });
                    await focusOrder(orderId);
                }
            });
        }

        const resourceFocusClearBtn = document.getElementById('resourceFocusClearBtn');
        if (resourceFocusClearBtn) {
            resourceFocusClearBtn.addEventListener('click', () => {
                clearResourceFocus({ render: true });
            });
        }

        const replanStrategySelect = document.getElementById('replanStrategy');
        if (replanStrategySelect) {
            replanStrategySelect.addEventListener('change', () => {
                state.replanStrategy = replanStrategySelect.value || 'balanced';
                renderReplanHint();
            });
        }

        const replanMaxSuggestionsSelect = document.getElementById('replanMaxSuggestions');
        if (replanMaxSuggestionsSelect) {
            replanMaxSuggestionsSelect.addEventListener('change', () => {
                const value = Number(replanMaxSuggestionsSelect.value);
                state.replanMaxSuggestions = Number.isFinite(value) ? value : 20;
                renderReplanHint();
            });
        }

        const replanPreviewBtn = document.getElementById('replanPreviewBtn');
        if (replanPreviewBtn) {
            replanPreviewBtn.addEventListener('click', async () => {
                await previewReplan();
            });
        }

        const replanApplyBtn = document.getElementById('replanApplyBtn');
        if (replanApplyBtn) {
            replanApplyBtn.addEventListener('click', async () => {
                await applyReplan();
            });
        }

        const replanClearBtn = document.getElementById('replanClearBtn');
        if (replanClearBtn) {
            replanClearBtn.addEventListener('click', () => {
                clearReplanPreview();
            });
        }

        const replanSuggestionList = document.getElementById('replanSuggestionList');
        if (replanSuggestionList) {
            replanSuggestionList.addEventListener('click', async (event) => {
                const chip = event.target.closest('[data-action]');
                if (!chip) {
                    return;
                }

                const action = chip.dataset.action;
                if (action === 'locate-replan') {
                    const orderId = chip.dataset.orderId;
                    const suggestionIndex = Number(chip.dataset.replanIndex);
                    const snapshotOrder = getReplanSnapshotOrders().find((item) => String(item?.order_id || '').trim() === String(orderId || '').trim());
                    const suggestion = Number.isFinite(suggestionIndex)
                        ? (Array.isArray(state.replanPreview) ? state.replanPreview[suggestionIndex] : null)
                        : null;
                    const resourceFocus = suggestion
                        ? buildResourceFocusFromReplanSuggestion(suggestion, 'replan')
                        : buildResourceFocusFromReplanOrder(snapshotOrder, 'replan');
                    if (resourceFocus) {
                        await applyResourceFocus(resourceFocus, {
                            preferredOrderId: orderId || ''
                        });
                    } else if (orderId) {
                        setImpactedOrders([orderId], { render: true });
                        await focusOrder(orderId);
                    }
                    return;
                }

                if (action === 'locate-related-replan') {
                    const relatedOrderId = chip.dataset.orderId;
                    if (relatedOrderId) {
                        setImpactedOrders([relatedOrderId], { render: true });
                        await focusOrder(relatedOrderId);
                    }
                }
            });
        }

        const backdrop = document.getElementById('backdrop');
        if (backdrop) {
            backdrop.addEventListener('click', () => {
                closeDetailDrawer();
                closeAiDrawer();
                closeChatDrawer();
                closeStatusPanel();
            });
        }

        const guideCloseBtn = document.getElementById('guideCloseBtn');
        if (guideCloseBtn) {
            guideCloseBtn.addEventListener('click', closeGuideModal);
        }

        const guideModal = document.getElementById('guideModal');
        if (guideModal) {
            guideModal.addEventListener('click', (event) => {
                if (event.target === guideModal) {
                    closeGuideModal();
                }
            });
        }

        document.addEventListener('click', (event) => {
            const target = event.target;
            if (!(target instanceof Element)) {
                return;
            }
            if (!isStatusPanelOpen()) {
                return;
            }
            if (target.closest('#statusPanel') || target.closest('#openStatusFloatingBtn')) {
                return;
            }
            closeStatusPanel();
        });

        document.addEventListener('click', (event) => {
            const target = event.target;
            if (!(target instanceof Element)) {
                return;
            }
            if (target.closest('#timelineSearchWrap')) {
                return;
            }
            closeSearchResultPanel();
        });

        window.addEventListener('beforeunload', () => {
            stopTimers();
            if (state.aiAssistantWidget) {
                state.aiAssistantWidget.disconnectStream();
            }
            const workerPoolModule = getDispatchBoardWorkerPool();
            if (workerPoolModule && typeof workerPoolModule.disposeSharedPools === 'function') {
                workerPoolModule.disposeSharedPools();
            }
            disconnectDispatchChatStream();
            if (state.analyticsTrendChart) {
                try {
                    state.analyticsTrendChart.dispose();
                } catch (_) {
                    // ignore dispose errors
                }
                state.analyticsTrendChart = null;
            }
            if (state.cornerInfoFadeTimer) {
                window.clearTimeout(state.cornerInfoFadeTimer);
                state.cornerInfoFadeTimer = null;
            }
            if (state.cornerInfoRafId) {
                window.cancelAnimationFrame(state.cornerInfoRafId);
                state.cornerInfoRafId = null;
            }
            if (state.legendPopoverCloseTimer) {
                window.clearTimeout(state.legendPopoverCloseTimer);
                state.legendPopoverCloseTimer = null;
            }
        });

        window.addEventListener('keydown', (event) => {
            if (isTypingElement(document.activeElement)) {
                return;
            }

            if (event.key === 'Escape') {
                closeDetailDrawer();
                closeAiDrawer();
                closeChatDrawer();
                closeStatusPanel();
                closeOpsMenu();
                closeGanttLegendPopover();
                closeGuideModal();
                closeSearchResultPanel();
                return;
            }

            if (event.key === '?' || (event.shiftKey && event.key === '/')) {
                event.preventDefault();
                openGuideModal();
            }
        });
    }

    function syncActiveButtons(container, selector, datasetKey, value) {
        const buttons = container.querySelectorAll(selector);
        buttons.forEach((button) => {
            const matches = button.dataset[datasetKey] === value;
            button.classList.toggle('active', matches);
        });
    }

    function initializeOpsMenu() {
        const toggle = document.getElementById('opsMenuToggle');
        const menu = document.getElementById('opsMenu');
        if (!toggle || !menu) {
            return;
        }

        updateOpsMenuSettings();

        toggle.addEventListener('click', (event) => {
            event.stopPropagation();
            toggleOpsMenu();
        });

        menu.addEventListener('click', (event) => {
            event.stopPropagation();
        });

        document.addEventListener('click', () => {
            closeOpsMenu();
        });

        window.addEventListener('resize', () => {
            if (isStatusPanelOpen()) {
                positionStatusPanel();
            }
        });
    }

    function initializeGuide() {
        const dismissed = readBoolStorage(GUIDE_STORAGE_KEY, false);
        const guideToggle = document.getElementById('guideDontShow');
        if (guideToggle) {
            guideToggle.checked = dismissed;
        }

        const opsToggle = document.getElementById('opsMenuToggle');
        if (opsToggle && !dismissed) {
            opsToggle.classList.add('discover');
        }

        if (!dismissed) {
            window.setTimeout(() => {
                openGuideModal();
            }, 260);
        }
    }

    function initializeGanttLegendOverlay() {
        const overlay = document.getElementById('ganttLegendOverlay');
        const button = document.getElementById('ganttLegendMoreBtn');
        const popover = document.getElementById('ganttLegendPopover');
        if (!overlay || !button || !popover) {
            return;
        }

        updateGanttLegendDensity();

        button.addEventListener('click', (event) => {
            event.stopPropagation();
            if (state.legendPopoverOpen) {
                closeGanttLegendPopover();
                return;
            }
            openGanttLegendPopover();
        });

        button.addEventListener('mouseenter', () => {
            if (state.legendDensity === 'toggle') {
                return;
            }
            openGanttLegendPopover();
        });

        button.addEventListener('mouseleave', () => {
            if (state.legendDensity === 'toggle') {
                return;
            }
            scheduleLegendPopoverClose(120);
        });

        popover.addEventListener('mouseenter', () => {
            cancelLegendPopoverClose();
        });

        popover.addEventListener('mouseleave', () => {
            scheduleLegendPopoverClose(120);
        });

        document.addEventListener('click', (event) => {
            const target = event.target;
            if (!(target instanceof Element)) {
                return;
            }
            if (target.closest('#ganttLegendOverlay')) {
                return;
            }
            closeGanttLegendPopover();
        });

        window.addEventListener('resize', () => {
            updateGanttLegendDensity();
        });
    }

    function updateGanttLegendDensity() {
        const overlay = document.getElementById('ganttLegendOverlay');
        const button = document.getElementById('ganttLegendMoreBtn');
        if (!overlay || !button) {
            return;
        }

        const width = window.innerWidth || document.documentElement.clientWidth || 0;
        const density = width >= 1440 ? 'full' : (width >= 1200 ? 'compact' : 'toggle');
        state.legendDensity = density;
        overlay.dataset.density = density;
        button.textContent = density === 'toggle' ? '图例' : 'i';
        button.title = density === 'toggle' ? '打开图例说明' : '查看完整图例说明';
        button.setAttribute('aria-label', density === 'toggle' ? '打开图例说明' : '打开完整图例说明');

        if (density !== 'toggle' && state.legendPopoverOpen && !overlay.contains(document.activeElement)) {
            scheduleLegendPopoverClose(180);
        }
    }

    function openGanttLegendPopover() {
        const button = document.getElementById('ganttLegendMoreBtn');
        const popover = document.getElementById('ganttLegendPopover');
        if (!button || !popover) {
            return;
        }

        cancelLegendPopoverClose();
        state.legendPopoverOpen = true;
        button.setAttribute('aria-expanded', 'true');
        popover.hidden = false;
    }

    function closeGanttLegendPopover() {
        const button = document.getElementById('ganttLegendMoreBtn');
        const popover = document.getElementById('ganttLegendPopover');
        cancelLegendPopoverClose();
        state.legendPopoverOpen = false;
        if (button) {
            button.setAttribute('aria-expanded', 'false');
        }
        if (popover) {
            popover.hidden = true;
        }
    }

    function scheduleLegendPopoverClose(delayMs) {
        cancelLegendPopoverClose();
        state.legendPopoverCloseTimer = window.setTimeout(() => {
            closeGanttLegendPopover();
        }, Math.max(80, Number(delayMs) || 0));
    }

    function cancelLegendPopoverClose() {
        if (state.legendPopoverCloseTimer) {
            window.clearTimeout(state.legendPopoverCloseTimer);
            state.legendPopoverCloseTimer = null;
        }
    }

    function openGuideModal() {
        const modal = document.getElementById('guideModal');
        if (!modal) {
            return;
        }

        const opsToggle = document.getElementById('opsMenuToggle');
        if (opsToggle) {
            opsToggle.classList.remove('discover');
        }

        modal.classList.add('show');
    }

    function closeGuideModal() {
        const modal = document.getElementById('guideModal');
        if (!modal) {
            return;
        }

        const guideToggle = document.getElementById('guideDontShow');
        const shouldDismiss = Boolean(guideToggle && guideToggle.checked);
        writeBoolStorage(GUIDE_STORAGE_KEY, shouldDismiss);
        modal.classList.remove('show');
    }

    function toggleOpsMenu() {
        if (state.opsMenuOpen) {
            closeOpsMenu();
            return;
        }
        openOpsMenu();
    }

    function openOpsMenu() {
        const toggle = document.getElementById('opsMenuToggle');
        const menu = document.getElementById('opsMenu');
        if (!toggle || !menu) {
            return;
        }

        state.opsMenuOpen = true;
        toggle.classList.add('active');
        toggle.classList.remove('discover');
        menu.classList.add('open');
        updateOpsMenuSettings();
    }

    function closeOpsMenu() {
        const toggle = document.getElementById('opsMenuToggle');
        const menu = document.getElementById('opsMenu');
        if (!toggle || !menu) {
            return;
        }

        state.opsMenuOpen = false;
        toggle.classList.remove('active');
        menu.classList.remove('open');
    }

    function updateOpsMenuSettings() {
        const refreshSelect = document.getElementById('settingRefreshInterval');
        const cornerFadeToggle = document.getElementById('settingCornerFade');
        const safetyGateFilterSelect = document.getElementById('settingSafetyGateFilter');

        if (refreshSelect) {
            refreshSelect.value = String(state.refreshIntervalMs || DEFAULT_REFRESH_INTERVAL_MS);
        }
        if (cornerFadeToggle) {
            cornerFadeToggle.checked = Boolean(state.cornerInfoAutoFade);
        }
        if (safetyGateFilterSelect) {
            safetyGateFilterSelect.value = String(state.safetyGateFilter || 'all');
        }
    }

    function applyThemeTokens() {
        STATUS_COLORS.pending = getCssVar('--status-pending', STATUS_COLORS.pending);
        STATUS_COLORS.assigned = getCssVar('--status-assigned', STATUS_COLORS.assigned);
        STATUS_COLORS.in_progress = getCssVar('--status-progress', STATUS_COLORS.in_progress);
        STATUS_COLORS.completed = getCssVar('--status-completed', STATUS_COLORS.completed);
        STATUS_COLORS.cancelled = getCssVar('--status-cancelled', STATUS_COLORS.cancelled);

        CHART_THEME.axisLine = getCssVar('--system-gray2', CHART_THEME.axisLine);
        CHART_THEME.axisLabel = getCssVar('--text-secondary', CHART_THEME.axisLabel);
        CHART_THEME.laneLabel = getCssVar('--text-primary', CHART_THEME.laneLabel);
        CHART_THEME.itemSummaryStroke = getCssVar('--system-blue', CHART_THEME.itemSummaryStroke);
        CHART_THEME.itemConflictStroke = getCssVar('--status-alert', CHART_THEME.itemConflictStroke);
        CHART_THEME.nowLine = getCssVar('--system-red', CHART_THEME.nowLine);
        CHART_THEME.nowLabelText = CHART_THEME.nowLine;
        CHART_THEME.laneFocusLabelText = getCssVar('--system-blue-hover', CHART_THEME.laneFocusLabelText);
        CHART_THEME.detailSubText = CHART_THEME.axisLabel;
    }

    function normalizeResourceType(resourceType) {
        const normalized = String(resourceType || '').trim().toLowerCase();
        if (normalized === 'team') {
            return 'team';
        }
        if (normalized === 'employee' || normalized === 'individual' || normalized === 'user') {
            return 'employee';
        }
        return '';
    }

    function getResourceFocusViewMode(resourceType) {
        return normalizeResourceType(resourceType) === 'employee' ? 'employee' : 'team';
    }

    function normalizeTimelineMemberUserId(member) {
        return String(member?.user_id || member?.id || '').trim();
    }

    function normalizeTimelineMemberName(member) {
        return String(member?.username || member?.user_display_name || member?.name || normalizeTimelineMemberUserId(member)).trim();
    }

    function normalizeResourceFocusIds(rawIds, fallbackId) {
        const values = Array.isArray(rawIds) ? rawIds : [];
        const normalized = values
            .map((item) => String(item || '').trim())
            .filter(Boolean);
        const fallback = String(fallbackId || '').trim();
        if (fallback) {
            normalized.unshift(fallback);
        }
        return Array.from(new Set(normalized));
    }

    function normalizeLaneIds(rawLaneIds, fallbackLaneId) {
        return normalizeResourceFocusIds(rawLaneIds, fallbackLaneId);
    }

    function normalizeMemberChangeSummary(summary) {
        if (!summary || typeof summary !== 'object') {
            return {
                replaced_members: [],
                added_members: [],
                removed_members: [],
                unchanged_members: [],
                changed_member_count: 0
            };
        }
        const normalizeMemberRecord = (item, memberKey) => {
            if (!item || typeof item !== 'object') {
                return null;
            }
            const slotCode = String(item.slot_code || '').trim();
            const member = item[memberKey];
            const normalizedMember = member && typeof member === 'object'
                ? {
                    user_id: String(member.user_id || '').trim(),
                    username: String(member.username || '').trim(),
                    slot_code: String(member.slot_code || slotCode || '').trim(),
                    qualification_code: String(member.qualification_code || '').trim(),
                    qualification_level_code: String(member.qualification_level_code || '').trim()
                }
                : null;
            return {
                slot_code: slotCode,
                member: normalizedMember
            };
        };
        const normalizeReplaceRecord = (item) => {
            if (!item || typeof item !== 'object') {
                return null;
            }
            const slotCode = String(item.slot_code || '').trim();
            const before = item.before && typeof item.before === 'object'
                ? {
                    user_id: String(item.before.user_id || '').trim(),
                    username: String(item.before.username || '').trim(),
                    slot_code: String(item.before.slot_code || slotCode || '').trim(),
                    qualification_code: String(item.before.qualification_code || '').trim(),
                    qualification_level_code: String(item.before.qualification_level_code || '').trim()
                }
                : null;
            const after = item.after && typeof item.after === 'object'
                ? {
                    user_id: String(item.after.user_id || '').trim(),
                    username: String(item.after.username || '').trim(),
                    slot_code: String(item.after.slot_code || slotCode || '').trim(),
                    qualification_code: String(item.after.qualification_code || '').trim(),
                    qualification_level_code: String(item.after.qualification_level_code || '').trim()
                }
                : null;
            return {
                slot_code: slotCode,
                before,
                after
            };
        };
        const replacedMembers = (Array.isArray(summary.replaced_members) ? summary.replaced_members : [])
            .map(normalizeReplaceRecord)
            .filter(Boolean);
        const addedMembers = (Array.isArray(summary.added_members) ? summary.added_members : [])
            .map((item) => normalizeMemberRecord(item, 'member'))
            .filter(Boolean);
        const removedMembers = (Array.isArray(summary.removed_members) ? summary.removed_members : [])
            .map((item) => normalizeMemberRecord(item, 'member'))
            .filter(Boolean);
        const unchangedMembers = (Array.isArray(summary.unchanged_members) ? summary.unchanged_members : [])
            .map((item) => normalizeMemberRecord(item, 'member'))
            .filter(Boolean);
        return {
            replaced_members: replacedMembers,
            added_members: addedMembers,
            removed_members: removedMembers,
            unchanged_members: unchangedMembers,
            changed_member_count: Number(summary.changed_member_count || (replacedMembers.length + addedMembers.length + removedMembers.length)) || 0
        };
    }

    function normalizeTaskCrewMembers(rawMembers) {
        return (Array.isArray(rawMembers) ? rawMembers : [])
            .map((member) => ({
                user_id: String(member?.user_id || '').trim(),
                username: String(member?.username || '').trim(),
                slot_code: String(member?.slot_code || '').trim(),
                qualification_code: String(member?.qualification_code || '').trim(),
                qualification_level_code: String(member?.qualification_level_code || '').trim()
            }))
            .filter((member) => member.user_id || member.username);
    }

    function collectCrewFocusMembers(payload = {}) {
        const memberChangeSummary = normalizeMemberChangeSummary(payload.member_change_summary || payload.memberChangeSummary);
        const taskCrew = payload.task_crew || payload.taskCrew || {};
        const taskCrewMembers = normalizeTaskCrewMembers(taskCrew.members);
        const prioritized = [];
        const seen = new Set();
        const pushMember = (member, resourceType = 'employee') => {
            const userId = String(member?.user_id || '').trim();
            const username = String(member?.username || '').trim();
            if (!userId && !username) {
                return;
            }
            const key = `${resourceType}:${userId || username}`;
            if (seen.has(key)) {
                return;
            }
            seen.add(key);
            prioritized.push({
                resource_type: resourceType,
                resource_id: userId,
                resource_label: username || userId,
                slot_code: String(member?.slot_code || '').trim(),
                qualification_code: String(member?.qualification_code || '').trim(),
                qualification_level_code: String(member?.qualification_level_code || '').trim()
            });
        };
        const replacedMembers = memberChangeSummary.replaced_members || [];
        if (replacedMembers[0]?.after) {
            pushMember(replacedMembers[0].after);
        }
        if (replacedMembers[0]?.before) {
            pushMember(replacedMembers[0].before);
        }
        const addedMembers = memberChangeSummary.added_members || [];
        if (addedMembers[0]?.member) {
            pushMember(addedMembers[0].member);
        }
        taskCrewMembers.forEach((member) => pushMember(member));
        addedMembers.forEach((item) => pushMember(item.member || {}));
        replacedMembers.forEach((item) => {
            pushMember(item.after || {});
            pushMember(item.before || {});
        });
        (memberChangeSummary.removed_members || []).forEach((item) => pushMember(item.member || {}));
        if (prioritized.length === 0) {
            const individualUserId = String(payload.individual_user_id || payload.individualUserId || '').trim();
            const individualUsername = String(payload.individual_username || payload.individualUsername || '').trim();
            if (individualUserId || individualUsername) {
                pushMember({
                    user_id: individualUserId,
                    username: individualUsername
                });
            }
        }
        return {
            members: prioritized,
            member_change_summary: memberChangeSummary
        };
    }

    function buildCrewResourceFocus(payload = {}, overrides = {}) {
        const crewInfo = collectCrewFocusMembers({
            ...payload,
            ...overrides
        });
        const members = crewInfo.members || [];
        if (members.length === 0) {
            return null;
        }
        const primaryMember = members[0];
        return buildResourceFocus({
            ...payload,
            ...overrides,
            resource_type: 'employee',
            resource_id: primaryMember.resource_id,
            resource_label: primaryMember.resource_label,
            primary_resource_type: 'employee',
            primary_resource_id: primaryMember.resource_id,
            resource_ids: members.map((member) => member.resource_id || member.resource_label).filter(Boolean),
            highlight_scope: members.length > 1 ? 'crew' : 'single',
            target_view_mode: 'employee',
            member_change_summary: crewInfo.member_change_summary
        });
    }

    function buildResourceFocus(payload = {}) {
        const resourceType = normalizeResourceType(payload.resource_type || payload.resourceType);
        if (!resourceType) {
            return null;
        }
        const resourceId = String(payload.resource_id ?? payload.resourceId ?? '').trim();
        const resourceLabel = String(payload.resource_label ?? payload.resourceLabel ?? '').trim();
        if (!resourceId && !resourceLabel) {
            return null;
        }
        const targetViewMode = String(
            payload.target_view_mode
            || payload.targetViewMode
            || getResourceFocusViewMode(resourceType)
        ).trim() || getResourceFocusViewMode(resourceType);
        const sourceKey = String(payload.source_key ?? payload.sourceKey ?? '').trim() || resourceId || resourceLabel;
        const primaryResourceType = normalizeResourceType(payload.primary_resource_type || payload.primaryResourceType || resourceType) || resourceType;
        const primaryResourceId = String(payload.primary_resource_id ?? payload.primaryResourceId ?? resourceId).trim();
        const resourceIds = normalizeResourceFocusIds(payload.resource_ids || payload.resourceIds, primaryResourceId || resourceId);
        const laneIds = normalizeLaneIds(payload.lane_ids || payload.laneIds, payload.primary_lane_id ?? payload.primaryLaneId ?? payload.lane_id ?? payload.laneId);
        const highlightScope = String(payload.highlight_scope ?? payload.highlightScope ?? (resourceIds.length > 1 ? 'crew' : 'single')).trim() === 'crew'
            ? 'crew'
            : 'single';
        const memberChangeSummary = normalizeMemberChangeSummary(payload.member_change_summary || payload.memberChangeSummary);
        return {
            resource_type: resourceType,
            resource_id: resourceId,
            resource_label: resourceLabel || resourceId,
            primary_resource_type: primaryResourceType,
            primary_resource_id: primaryResourceId || resourceId,
            target_view_mode: targetViewMode,
            lane_id: String(payload.lane_id ?? payload.laneId ?? payload.primary_lane_id ?? payload.primaryLaneId ?? '').trim(),
            primary_lane_id: String(payload.primary_lane_id ?? payload.primaryLaneId ?? payload.lane_id ?? payload.laneId ?? '').trim(),
            resource_ids: resourceIds,
            lane_ids: laneIds,
            highlight_scope: highlightScope,
            member_change_summary: memberChangeSummary,
            related_order_ids: normalizeConflictOrderIds(payload.related_order_ids || payload.relatedOrderIds || []),
            source_panel: String(payload.source_panel ?? payload.sourcePanel ?? '').trim(),
            source_key: sourceKey,
            visible_resource_ids: normalizeResourceFocusIds(payload.visible_resource_ids || payload.visibleResourceIds || [], ''),
            missing_resource_ids: normalizeResourceFocusIds(payload.missing_resource_ids || payload.missingResourceIds || [], '')
        };
    }

    function getActiveResourceFocus() {
        return buildResourceFocus(state.resourceFocus || {});
    }

    function isResourceFocusActive(resourceType, resourceId, resourceLabel) {
        const activeFocus = getActiveResourceFocus();
        const normalizedType = normalizeResourceType(resourceType);
        if (!activeFocus || !normalizedType || activeFocus.resource_type !== normalizedType) {
            return false;
        }
        const normalizedId = String(resourceId || '').trim();
        const normalizedLabel = String(resourceLabel || '').trim();
        if (normalizedId) {
            if (activeFocus.resource_id === normalizedId) {
                return true;
            }
            return Array.isArray(activeFocus.resource_ids) && activeFocus.resource_ids.includes(normalizedId);
        }
        return Boolean(normalizedLabel) && activeFocus.resource_label === normalizedLabel;
    }

    function getResourceFocusDisplayText(resourceFocus) {
        if (!resourceFocus) {
            return '';
        }
        const resourceCount = Array.isArray(resourceFocus.resource_ids) ? resourceFocus.resource_ids.length : 0;
        if (resourceFocus.highlight_scope === 'crew' || resourceCount > 1) {
            return `执行编组 ${Math.max(resourceCount, 1)} 人`;
        }
        const typeLabel = resourceFocus.resource_type === 'employee' ? '个人' : '班组';
        return `${typeLabel} ${resourceFocus.resource_label || resourceFocus.resource_id || '-'}`;
    }

    function doesLaneMatchResourceFocus(lane, resourceFocus) {
        if (!lane || !resourceFocus) {
            return false;
        }
        const resourceType = normalizeResourceType(lane.resource_type);
        if (resourceType && resourceType !== resourceFocus.resource_type) {
            return false;
        }
        const laneId = String(lane.id || '').trim();
        const normalizedLaneIds = Array.isArray(resourceFocus.lane_ids) ? resourceFocus.lane_ids : [];
        if (laneId && normalizedLaneIds.includes(laneId)) {
            return true;
        }
        const laneResourceId = String(lane.resource_id || '').trim();
        const focusResourceIds = Array.isArray(resourceFocus.resource_ids) ? resourceFocus.resource_ids : [];
        if (laneResourceId) {
            if (resourceFocus.resource_id && laneResourceId === resourceFocus.resource_id) {
                return true;
            }
            if (focusResourceIds.includes(laneResourceId)) {
                return true;
            }
        }
        const laneResourceLabel = String(lane.resource_label || lane.label || '').trim();
        return Boolean(resourceFocus.resource_label) && laneResourceLabel === resourceFocus.resource_label;
    }

    function doesItemMatchResourceFocus(item, resourceFocus) {
        if (!item || item.is_flight_summary || !resourceFocus) {
            return false;
        }
        if (resourceFocus.resource_type === 'team') {
            if (resourceFocus.resource_id && String(item.team_id || '').trim() === resourceFocus.resource_id) {
                return true;
            }
            if (resourceFocus.resource_label) {
                return String(item.team_name || item.lane_label || '').trim() === resourceFocus.resource_label;
            }
            return false;
        }

        const focusResourceIds = Array.isArray(resourceFocus.resource_ids) && resourceFocus.resource_ids.length > 0
            ? resourceFocus.resource_ids
            : (resourceFocus.resource_id ? [resourceFocus.resource_id] : []);
        if (focusResourceIds.length > 0) {
            if (focusResourceIds.includes(String(item.focus_user_id || '').trim())) {
                return true;
            }
            if (focusResourceIds.includes(String(item.individual_user_id || '').trim())) {
                return true;
            }
            const members = Array.isArray(item.members) ? item.members : [];
            return members.some((member) => focusResourceIds.includes(normalizeTimelineMemberUserId(member)));
        }

        const label = resourceFocus.resource_label;
        if (!label) {
            return false;
        }
        if (String(item.focus_user_name || '').trim() === label) {
            return true;
        }
        if (String(item.individual_username || '').trim() === label) {
            return true;
        }
        const members = Array.isArray(item.members) ? item.members : [];
        return members.some((member) => normalizeTimelineMemberName(member) === label);
    }

    function resolveResourceFocusFromTimeline(resourceFocus, options = {}) {
        const focus = buildResourceFocus(resourceFocus);
        const timeline = state.timelineData;
        if (!focus || !timeline) {
            return null;
        }
        const lanes = Array.isArray(timeline.lanes) ? timeline.lanes : [];
        const laneMap = new Map(lanes.map((lane) => [String(lane?.id || '').trim(), lane]));
        const preferredOrderId = String(options.preferredOrderId || focus.related_order_ids?.[0] || '').trim();
        const candidateMap = new Map();
        const desiredResourceIds = Array.isArray(focus.resource_ids) && focus.resource_ids.length > 0
            ? focus.resource_ids
            : (focus.resource_id ? [focus.resource_id] : []);

        const ensureCandidate = (laneId) => {
            const normalizedLaneId = String(laneId || '').trim();
            if (!normalizedLaneId) {
                return null;
            }
            if (!candidateMap.has(normalizedLaneId)) {
                candidateMap.set(normalizedLaneId, {
                    lane: laneMap.get(normalizedLaneId) || { id: normalizedLaneId, label: normalizedLaneId },
                    items: []
                });
            }
            return candidateMap.get(normalizedLaneId);
        };

        const initialLaneIds = normalizeLaneIds(focus.lane_ids, focus.primary_lane_id || focus.lane_id);
        for (const directLaneId of initialLaneIds) {
            const directCandidate = ensureCandidate(directLaneId);
            if (directCandidate && doesLaneMatchResourceFocus(directCandidate.lane, focus)) {
                directCandidate.isDirectMatch = true;
            }
        }

        for (const lane of lanes) {
            if (doesLaneMatchResourceFocus(lane, focus)) {
                const candidate = ensureCandidate(lane.id);
                if (candidate) {
                    candidate.isLaneMetadataMatch = true;
                }
            }
        }

        const items = Array.isArray(timeline.items) ? timeline.items : [];
        for (const item of items) {
            if (!item || item.is_flight_summary) {
                continue;
            }
            const laneId = String(item.lane_id || '').trim();
            if (!laneId) {
                continue;
            }
            const candidate = candidateMap.get(laneId);
            if (candidate && candidate.isLaneMetadataMatch) {
                candidate.items.push(item);
                continue;
            }
            if (!doesItemMatchResourceFocus(item, focus)) {
                continue;
            }
            ensureCandidate(laneId)?.items.push(item);
        }

        const nowMs = Date.now();
        const candidates = Array.from(candidateMap.values())
            .map((candidate) => {
                const uniqueOrderIds = Array.from(new Set(
                    candidate.items
                        .map((item) => String(item?.order_id || '').trim())
                        .filter(Boolean)
                ));
                const preferredItem = preferredOrderId
                    ? candidate.items.find((item) => String(item?.order_id || '').trim() === preferredOrderId)
                    : null;
                const inProgressItem = candidate.items.find((item) => String(item?.status || '').trim() === 'in_progress');
                const upcomingItems = candidate.items
                    .filter((item) => toMs(item?.end_time) >= nowMs)
                    .sort((left, right) => toMs(left?.start_time) - toMs(right?.start_time));
                const fallbackItems = candidate.items
                    .slice()
                    .sort((left, right) => toMs(left?.start_time) - toMs(right?.start_time));
                const representativeItem = preferredItem || inProgressItem || upcomingItems[0] || fallbackItems[0] || null;
                const nextActiveStartMs = representativeItem ? toMs(representativeItem.start_time) : Number.MAX_SAFE_INTEGER;
                return {
                    lane: candidate.lane,
                    items: candidate.items,
                    relatedOrderIds: uniqueOrderIds,
                    representativeItem,
                    hasPreferredOrder: Boolean(preferredItem),
                    hasInProgress: Boolean(inProgressItem),
                    nextActiveStartMs
                };
            })
            .filter((candidate) => candidate.items.length > 0);

        if (candidates.length === 0) {
            return null;
        }

        candidates.sort((left, right) => {
            if (left.hasPreferredOrder !== right.hasPreferredOrder) {
                return left.hasPreferredOrder ? -1 : 1;
            }
            if (left.hasInProgress !== right.hasInProgress) {
                return left.hasInProgress ? -1 : 1;
            }
            const startGap = left.nextActiveStartMs - right.nextActiveStartMs;
            if (startGap !== 0) {
                return startGap;
            }
            return String(left.lane?.label || left.lane?.id || '').localeCompare(String(right.lane?.label || right.lane?.id || ''), 'zh-CN');
        });

        const best = candidates[0];
        const matchedLaneIds = candidates.map((candidate) => String(candidate.lane?.id || '').trim()).filter(Boolean);
        const visibleResourceIds = [];
        const laneResourceMap = new Map();
        for (const candidate of candidates) {
            const laneResourceId = String(candidate.lane?.resource_id || '').trim();
            if (laneResourceId) {
                visibleResourceIds.push(laneResourceId);
                laneResourceMap.set(laneResourceId, String(candidate.lane?.id || '').trim());
            }
            for (const item of candidate.items) {
                const focusUserId = String(item?.focus_user_id || '').trim();
                if (focusUserId) {
                    visibleResourceIds.push(focusUserId);
                    laneResourceMap.set(focusUserId, String(candidate.lane?.id || '').trim());
                }
                const individualUserId = String(item?.individual_user_id || '').trim();
                if (individualUserId) {
                    visibleResourceIds.push(individualUserId);
                    laneResourceMap.set(individualUserId, String(candidate.lane?.id || '').trim());
                }
                const members = Array.isArray(item?.members) ? item.members : [];
                members.forEach((member) => {
                    const userId = normalizeTimelineMemberUserId(member);
                    if (userId) {
                        visibleResourceIds.push(userId);
                        laneResourceMap.set(userId, String(candidate.lane?.id || '').trim());
                    }
                });
            }
        }
        const normalizedVisibleResourceIds = Array.from(new Set(visibleResourceIds.filter(Boolean)));
        const primaryResourceId = String(focus.primary_resource_id || focus.resource_id || '').trim();
        const primaryLaneId = laneResourceMap.get(primaryResourceId) || String(best.lane?.id || '').trim();
        const missingResourceIds = desiredResourceIds.filter((resourceId) => !normalizedVisibleResourceIds.includes(resourceId));
        return {
            ...focus,
            lane_id: primaryLaneId,
            primary_lane_id: primaryLaneId,
            lane_ids: Array.from(new Set(matchedLaneIds)),
            resource_label: focus.resource_label || String(best.lane?.resource_label || best.lane?.label || '').trim(),
            related_order_ids: Array.from(new Set(candidates.flatMap((candidate) => candidate.relatedOrderIds || []))),
            visible_resource_ids: normalizedVisibleResourceIds,
            missing_resource_ids: missingResourceIds
        };
    }

    function renderResourceFocusBar() {
        const bar = document.getElementById('resourceFocusBar');
        const text = document.getElementById('resourceFocusText');
        const resourceFocus = getActiveResourceFocus();
        if (!bar || !text) {
            return;
        }
        if (!resourceFocus) {
            bar.classList.remove('active');
            text.textContent = '';
            return;
        }
        const sourceLabel = RESOURCE_FOCUS_PANEL_LABELS[resourceFocus.source_panel] || '调度联动';
        const orderCount = Array.isArray(resourceFocus.related_order_ids) ? resourceFocus.related_order_ids.length : 0;
        const laneCount = Array.isArray(resourceFocus.lane_ids) ? resourceFocus.lane_ids.length : (resourceFocus.lane_id ? 1 : 0);
        const missingCount = Array.isArray(resourceFocus.missing_resource_ids) ? resourceFocus.missing_resource_ids.length : 0;
        const laneHint = laneCount > 0
            ? (missingCount > 0 ? `已高亮 ${laneCount} 条资源行，部分成员不在当前窗口` : `已高亮 ${laneCount} 条资源行`)
            : '待定位资源行';
        text.textContent = `当前聚焦：${getResourceFocusDisplayText(resourceFocus)} | 来源 ${sourceLabel} | 关联 ${orderCount} 单 | ${laneHint}`;
        bar.classList.add('active');
    }

    function shouldClearImpactedOrdersForResourceFocus(resourceFocus) {
        const orderIds = normalizeConflictOrderIds(resourceFocus?.related_order_ids || []);
        if (orderIds.length === 0 || state.impactedOrderIds.size !== orderIds.length) {
            return false;
        }
        return orderIds.every((orderId) => state.impactedOrderIds.has(orderId));
    }

    function clearResourceFocus(options = {}) {
        const currentFocus = getActiveResourceFocus();
        if (!currentFocus) {
            return;
        }
        state.resourceFocus = null;
        if (!options.preserveImpacted && shouldClearImpactedOrdersForResourceFocus(currentFocus)) {
            setImpactedOrders([], { render: false });
        }
        renderResourceFocusBar();
        renderViewModeHint();
        renderAnalyticsPanel();
        renderScenarioPanel();
        if (options.render !== false) {
            renderChart();
        }
        if (!options.silent) {
            showToast('已清除资源聚焦');
        }
    }

    async function applyResourceFocus(resourceFocus, options = {}) {
        const focus = buildResourceFocus(resourceFocus);
        if (!focus) {
            return false;
        }
        const preferredOrderId = String(options.preferredOrderId || focus.related_order_ids?.[0] || '').trim();
        const targetViewMode = String(focus.target_view_mode || getResourceFocusViewMode(focus.resource_type)).trim();
        const viewTabGroup = document.getElementById('viewTabGroup');
        if (targetViewMode && state.viewMode !== targetViewMode) {
            state.viewMode = targetViewMode;
            if (viewTabGroup) {
                syncActiveButtons(viewTabGroup, '.chip-btn[data-view]', 'view', targetViewMode);
            }
            await refreshTimeline();
        } else if (!state.timelineData) {
            await refreshTimeline();
        }

        const resolvedFocus = resolveResourceFocusFromTimeline(focus, { preferredOrderId });
        if (!resolvedFocus) {
            clearResourceFocus({ render: false, preserveImpacted: true, silent: true });
            renderResourceFocusBar();
            renderViewModeHint();
            renderAnalyticsPanel();
            renderScenarioPanel();
            if (preferredOrderId) {
                setImpactedOrders([preferredOrderId], { render: true });
                await focusOrder(preferredOrderId);
            } else {
                renderChart();
            }
            showToast('已定位工单，但当前窗口未找到执行成员资源行');
            return false;
        }

        state.resourceFocus = resolvedFocus;
        renderResourceFocusBar();
        renderViewModeHint();
        renderAnalyticsPanel();
        renderScenarioPanel();
        const impactedOrderIds = resolvedFocus.related_order_ids.length > 0
            ? resolvedFocus.related_order_ids
            : (preferredOrderId ? [preferredOrderId] : []);
        setImpactedOrders(impactedOrderIds, { render: false });

        const representativeOrderId = preferredOrderId || String(resolvedFocus.related_order_ids?.[0] || '').trim();
        if (representativeOrderId) {
            await focusOrder(representativeOrderId);
        } else {
            renderChart();
        }
        if (Array.isArray(resolvedFocus.missing_resource_ids) && resolvedFocus.missing_resource_ids.length > 0) {
            showToast(`已定位 ${resolvedFocus.lane_ids?.length || 1} 条资源行，部分成员不在当前窗口`);
        }
        return true;
    }

    function syncResourceFocusAfterTimelineRefresh() {
        const currentFocus = getActiveResourceFocus();
        if (!currentFocus) {
            return;
        }
        const resolvedFocus = resolveResourceFocusFromTimeline(currentFocus, {
            preferredOrderId: state.highlightedOrderId || currentFocus.related_order_ids?.[0] || ''
        });
        if (!resolvedFocus) {
            clearResourceFocus({ render: false, preserveImpacted: true, silent: true });
            const fallbackOrderId = String(state.highlightedOrderId || currentFocus.related_order_ids?.[0] || '').trim();
            if (fallbackOrderId) {
                setImpactedOrders([fallbackOrderId], { render: true });
                focusOrder(fallbackOrderId);
                showToast('已定位工单，但当前窗口未找到执行成员资源行');
                return;
            }
            showToast('当前时间窗已无聚焦资源，已自动退出资源聚焦');
            return;
        }
        state.resourceFocus = resolvedFocus;
        const impactedOrderIds = resolvedFocus.related_order_ids.length > 0
            ? resolvedFocus.related_order_ids
            : normalizeConflictOrderIds(currentFocus.related_order_ids || []);
        if (impactedOrderIds.length > 0) {
            setImpactedOrders(impactedOrderIds, { render: false });
        }
        renderResourceFocusBar();
        if (Array.isArray(resolvedFocus.missing_resource_ids) && resolvedFocus.missing_resource_ids.length > 0) {
            showToast('部分执行成员不在当前时间窗，已保留可见资源行');
        }
    }

    function getTimelineOrdersByIds(orderIds) {
        const normalizedOrderIds = normalizeConflictOrderIds(orderIds || []);
        if (normalizedOrderIds.length === 0) {
            return [];
        }
        const timelineItems = Array.isArray(state.timelineData?.items) ? state.timelineData.items : [];
        const orderMap = new Map();
        for (const item of timelineItems) {
            const orderId = String(item?.order_id || '').trim();
            if (!orderId || !normalizedOrderIds.includes(orderId) || orderMap.has(orderId)) {
                continue;
            }
            orderMap.set(orderId, item);
        }
        return normalizedOrderIds.map((orderId) => orderMap.get(orderId)).filter(Boolean);
    }

    function buildResourceFocusFromOrderItems(orderItems, basePayload = {}) {
        const normalizedItems = Array.isArray(orderItems) ? orderItems.filter(Boolean) : [];
        if (normalizedItems.length === 0) {
            return null;
        }
        const members = [];
        const seen = new Set();
        const pushMember = (member) => {
            const userId = String(member?.user_id || '').trim();
            const username = String(member?.username || member?.user_display_name || member?.name || '').trim();
            if (!userId && !username) {
                return;
            }
            const key = userId || username;
            if (seen.has(key)) {
                return;
            }
            seen.add(key);
            members.push({
                user_id: userId,
                username,
                slot_code: String(member?.slot_code || '').trim(),
                qualification_code: String(member?.qualification_code || '').trim(),
                qualification_level_code: String(member?.qualification_level_code || '').trim()
            });
        };
        for (const item of normalizedItems) {
            const taskCrewMembers = normalizeTaskCrewMembers(item?.task_crew?.members);
            taskCrewMembers.forEach(pushMember);
            const membersFromItem = Array.isArray(item?.members) ? item.members : [];
            membersFromItem.forEach(pushMember);
            const individualUserId = String(item?.individual_user_id || '').trim();
            const individualUsername = String(item?.individual_username || '').trim();
            if (individualUserId || individualUsername) {
                pushMember({
                    user_id: individualUserId,
                    username: individualUsername
                });
            }
        }
        if (members.length === 0) {
            return null;
        }
        return buildCrewResourceFocus({
            ...basePayload,
            task_crew: {
                members
            }
        });
    }

    function buildResourceFocusFromReplanSuggestion(suggestion, sourcePanel = 'replan') {
        if (!suggestion || typeof suggestion !== 'object') {
            return null;
        }
        const suggestedAssignment = suggestion.suggested_assignment && typeof suggestion.suggested_assignment === 'object'
            ? suggestion.suggested_assignment
            : {};
        const currentAssignment = suggestion.current_assignment && typeof suggestion.current_assignment === 'object'
            ? suggestion.current_assignment
            : {};
        const focus = buildCrewResourceFocus({
            ...suggestedAssignment,
            task_crew: suggestedAssignment.task_crew || currentAssignment.task_crew || {},
            member_change_summary: suggestion.member_change_summary,
            related_order_ids: normalizeConflictOrderIds([
                suggestion.dispatch_order_id,
                suggestion.related_dispatch_order_id
            ]),
            source_panel: sourcePanel,
            source_key: `replan:${String(suggestion.dispatch_order_id || '').trim()}`
        });
        return focus;
    }

    function buildResourceFocusFromReplanOrder(order, sourcePanel = 'replan') {
        if (!order || typeof order !== 'object') {
            return null;
        }
        return buildCrewResourceFocus({
            task_crew: order.current_assignment?.task_crew || order.task_crew || {},
            related_order_ids: normalizeConflictOrderIds([order.order_id]),
            source_panel: sourcePanel,
            source_key: `replan-order:${String(order.order_id || '').trim()}`
        });
    }

    function buildResourceFocusFromScenarioRecommendation(item, sourcePanel = 'scenario') {
        if (!item || typeof item !== 'object') {
            return null;
        }
        const orderId = String(item.dispatch_order_id || item.order_id || '').trim();
        const directCrewFocus = buildCrewResourceFocus({
            ...item,
            related_order_ids: normalizeConflictOrderIds([orderId]),
            source_panel: sourcePanel,
            source_key: `scenario-recommendation:${orderId}`
        });
        if (directCrewFocus) {
            return directCrewFocus;
        }
        const orderItems = getTimelineOrdersByIds([orderId]);
        const orderFocus = buildResourceFocusFromOrderItems(orderItems, {
            related_order_ids: normalizeConflictOrderIds([orderId]),
            source_panel: sourcePanel,
            source_key: `scenario-recommendation:${orderId}`
        });
        if (orderFocus) {
            return orderFocus;
        }
        const resourceType = normalizeResourceType(item.resource_type || item.resourceType);
        if (!resourceType) {
            return null;
        }
        return buildResourceFocus({
            resource_type: resourceType,
            resource_id: String(item.resource_id || item.resourceId || '').trim(),
            resource_label: String(item.resource_name || item.resourceLabel || item.resource_id || item.resourceId || '').trim(),
            target_view_mode: getResourceFocusViewMode(resourceType),
            related_order_ids: normalizeConflictOrderIds([orderId]),
            source_panel: sourcePanel,
            source_key: `scenario-recommendation:${String(item.resource_id || item.resourceId || orderId).trim()}`
        });
    }

    function buildResourceFocusFromConflictItem(item, sourcePanel = 'conflict') {
        const conflictType = String(item?.conflict_type || '').trim();
        const orderIds = normalizeConflictOrderIds(item?.related_dispatch_order_ids);
        const orderItems = getTimelineOrdersByIds(orderIds);
        const crewFocus = buildResourceFocusFromOrderItems(orderItems, {
            related_order_ids: orderIds,
            source_panel: sourcePanel,
            source_key: `${conflictType}:${String(item?.resource_id || item?.resource_name || orderIds[0] || '').trim()}`
        });
        if (crewFocus) {
            return crewFocus;
        }
        const resourceType = conflictType === 'team_overlap'
            ? 'team'
            : (conflictType === 'individual_overlap' ? 'employee' : '');
        if (!resourceType) {
            return null;
        }
        return buildResourceFocus({
            resource_type: resourceType,
            resource_id: String(item?.resource_id || '').trim(),
            resource_label: String(item?.resource_name || item?.resource_id || '').trim(),
            target_view_mode: getResourceFocusViewMode(resourceType),
            related_order_ids: orderIds,
            source_panel: sourcePanel,
            source_key: `${conflictType}:${String(item?.resource_id || item?.resource_name || '').trim()}`
        });
    }

    function getCssVar(name, fallback) {
        const value = window.getComputedStyle(document.documentElement).getPropertyValue(name);
        const trimmed = String(value || '').trim();
        return trimmed || fallback;
    }

    function initializeCornerInfoOverlay() {
        const cornerInfo = document.getElementById('cornerInfo');
        const shell = document.getElementById('ganttStage') || document.querySelector('.gantt-stage') || document.querySelector('.gantt-shell');
        if (!cornerInfo || !shell) {
            return;
        }

        cornerInfo.classList.remove('active', 'faded');
        if (state.cornerInfoAutoFade) {
            scheduleCornerInfoFade(3200);
        }

        shell.addEventListener('pointermove', (event) => {
            state.cornerInfoPointerX = event.clientX;
            state.cornerInfoPointerY = event.clientY;
            if (state.cornerInfoRafId) {
                return;
            }
            state.cornerInfoRafId = window.requestAnimationFrame(() => {
                state.cornerInfoRafId = null;
                updateCornerInfoVisibility(state.cornerInfoPointerX, state.cornerInfoPointerY, shell, cornerInfo);
            });
        });

        shell.addEventListener('pointerleave', () => {
            cornerInfo.classList.remove('active');
            if (state.cornerInfoAutoFade) {
                scheduleCornerInfoFade(420);
            }
        });

        window.addEventListener('resize', () => {
            if (!cornerInfo.classList.contains('active')) {
                scheduleCornerInfoFade(700);
            }
        });
    }

    function updateCornerInfoVisibility(pointerX, pointerY, shell, cornerInfo) {
        const rect = shell.getBoundingClientRect();
        const nearZone = {
            left: rect.left - 10,
            right: rect.left + Math.min(460, rect.width * 0.5),
            top: rect.bottom - Math.min(190, rect.height * 0.3),
            bottom: rect.bottom + 10
        };

        const isNear = pointerX >= nearZone.left
            && pointerX <= nearZone.right
            && pointerY >= nearZone.top
            && pointerY <= nearZone.bottom;

        if (isNear) {
            cornerInfo.classList.add('active');
            cornerInfo.classList.remove('faded');
            if (state.cornerInfoAutoFade) {
                scheduleCornerInfoFade(2800);
            }
            return;
        }

        cornerInfo.classList.remove('active');
        if (state.cornerInfoAutoFade) {
            scheduleCornerInfoFade(850);
        } else {
            cornerInfo.classList.remove('faded');
        }
    }

    function scheduleCornerInfoFade(delayMs) {
        if (state.cornerInfoFadeTimer) {
            window.clearTimeout(state.cornerInfoFadeTimer);
            state.cornerInfoFadeTimer = null;
        }

        if (!state.cornerInfoAutoFade) {
            const cornerInfo = document.getElementById('cornerInfo');
            if (cornerInfo) {
                cornerInfo.classList.remove('faded');
            }
            return;
        }

        state.cornerInfoFadeTimer = window.setTimeout(() => {
            const cornerInfo = document.getElementById('cornerInfo');
            if (!cornerInfo || cornerInfo.classList.contains('active')) {
                return;
            }
            cornerInfo.classList.add('faded');
        }, Math.max(120, Number(delayMs) || 0));
    }

    function positionStatusPanel() {
        const opsDock = document.getElementById('opsDock');
        const statusAnchor = document.getElementById('openStatusFloatingBtn');
        const panel = document.getElementById('statusPanel');
        if (!panel) {
            return;
        }

        const margin = 10;
        const gap = 8;
        const anchorRect = statusAnchor
            ? statusAnchor.getBoundingClientRect()
            : (opsDock
                ? opsDock.getBoundingClientRect()
                : { right: window.innerWidth - margin, bottom: 104 });
        const panelWidth = Math.min(panel.offsetWidth || 360, window.innerWidth - margin * 2);
        const maxPanelHeight = Math.max(200, window.innerHeight - margin * 2);
        panel.style.maxHeight = `${maxPanelHeight}px`;

        const measuredHeight = panel.scrollHeight || panel.offsetHeight || 0;
        const panelHeight = Math.min(maxPanelHeight, Math.max(200, measuredHeight));

        let left = anchorRect.right - panelWidth;
        left = clamp(left, margin, window.innerWidth - panelWidth - margin);

        const spaceBelow = window.innerHeight - anchorRect.bottom - gap - margin;
        const shouldOpenUpward = spaceBelow < panelHeight && anchorRect.top > panelHeight;

        let top = shouldOpenUpward
            ? anchorRect.top - panelHeight - gap
            : anchorRect.bottom + gap;
        top = clamp(top, margin, window.innerHeight - panelHeight - margin);

        panel.style.left = `${Math.round(left)}px`;
        panel.style.top = `${Math.round(top)}px`;
        panel.style.right = 'auto';
        panel.style.bottom = 'auto';
        panel.style.transformOrigin = shouldOpenUpward ? '100% 100%' : '100% 0';
    }

    function isStatusPanelOpen() {
        const panel = document.getElementById('statusPanel');
        return Boolean(panel && panel.classList.contains('open'));
    }

    function renderTerminalTabs() {
        const terminalGroup = document.getElementById('terminalGroup');
        if (!terminalGroup) {
            return;
        }

        terminalGroup.innerHTML = '';
        const terminals = Array.isArray(state.terminals) && state.terminals.length > 0
            ? state.terminals
            : ['all'];

        for (const terminal of terminals) {
            const button = document.createElement('button');
            button.type = 'button';
            button.className = 'chip-btn';
            button.dataset.terminal = terminal;
            button.textContent = terminal === 'all' ? '全部航站楼' : terminal;
            if (terminal === state.terminal) {
                button.classList.add('active');
            }
            terminalGroup.appendChild(button);
        }
    }

    async function loadConfiguredTerminals() {
        const dataLayer = getDispatchBoardData() || await ensureDispatchBoardDataModule();
        let normalizedTerminals = [];

        try {
            normalizedTerminals = await dataLayer.loadConfiguredTerminals({
                timeline: state.timelineData,
                onFetchError(error) {
                    console.warn('加载航站楼配置失败，将使用回退数据:', error);
                }
            });
        } catch (error) {
            console.warn('加载航站楼配置失败，将使用回退数据:', error);
            normalizedTerminals = dataLayer.normalizeTerminalList(dataLayer.collectTerminalsFromTimeline(state.timelineData));
        }

        state.terminals = ['all', ...normalizedTerminals];

        if (!state.terminals.includes(state.terminal)) {
            state.terminal = 'all';
        }

        renderTerminalTabs();
        renderWindowLabel();
    }

    function initChart() {
        const chartDom = document.getElementById('ganttChart');
        if (!chartDom || typeof echarts === 'undefined') {
            showToast('ECharts 未加载，无法渲染甘特图');
            return;
        }

        state.chart = echarts.init(chartDom, null, { renderer: 'canvas' });
        state.chart.on('click', (params) => {
            const raw = params?.data?.raw;
            if (!raw) {
                return;
            }

            // Ctrl/Shift click on a draft order toggles multi-select
            const domEvent = params.event?.event;
            if (domEvent && (domEvent.ctrlKey || domEvent.shiftKey || domEvent.metaKey)) {
                if (isDraftOrder(raw) && raw.order_id) {
                    toggleDraftSelection(raw.order_id);
                    return;
                }
            }

            state.highlightedItemId = raw.id;
            state.highlightedOrderId = raw.order_id || null;
            renderChart();

            renderViewModeHint();
        });

        state.chart.on('dblclick', async (params) => {
            const raw = params?.data?.raw;
            if (!raw) {
                return;
            }

            state.highlightedItemId = raw.id;
            state.highlightedOrderId = raw.order_id || null;
            renderChart();

            if (raw.is_flight_summary) {
                await openFlightSummaryDetail(raw);
            } else if (raw.order_id) {
                await openOrderDetail(raw.order_id);
            }
        });

        window.addEventListener('resize', () => {
            if (state.chart) {
                state.chart.resize();
            }
            if (state.analyticsTrendChart) {
                state.analyticsTrendChart.resize();
            }
        });
    }

    function startTimers() {
        stopTimers();
        state.refreshTimer = window.setInterval(() => {
            refreshTimeline();
        }, Math.max(5000, Number(state.refreshIntervalMs) || DEFAULT_REFRESH_INTERVAL_MS));

        state.nowTimer = window.setInterval(() => {
            if (!state.chart) {
                return;
            }
            if (!state.timelineData || !Array.isArray(state.timelineData.items) || state.timelineData.items.length === 0) {
                return;
            }
            state.chart.setOption({
                series: [{
                    id: 'dispatch-series',
                    markLine: {
                        data: [{ xAxis: Date.now() }]
                    }
                }]
            });
        }, NOW_TICK_INTERVAL_MS);
    }

    function stopTimers() {
        if (state.refreshTimer) {
            window.clearInterval(state.refreshTimer);
            state.refreshTimer = null;
        }
        if (state.nowTimer) {
            window.clearInterval(state.nowTimer);
            state.nowTimer = null;
        }
    }

    async function refreshTimeline() {
        if (state.loading) {
            return;
        }

        state.loading = true;
        const ganttStage = document.getElementById('ganttStage');
        const shouldShowStageLoading = !state.timelineData;
        if (shouldShowStageLoading && ganttStage) {
            showContainerLoading(ganttStage, '正在加载派工时间线...', { lines: 5, minHeight: '420px', preserveChildren: true });
        }
        try {
            const dataLayer = getDispatchBoardData() || await ensureDispatchBoardDataModule();
            const payload = await dataLayer.fetchTimeline({
                viewMode: state.viewMode,
                windowStartMs: state.windowStartMs,
                windowEndMs: state.windowEndMs,
                terminal: state.terminal
            });
            state.timelineData = payload;

            const timelineTerminals = dataLayer.collectTerminalsFromTimeline(payload);
            if (timelineTerminals.length > 0) {
                const merged = dataLayer.normalizeTerminalList([
                    ...state.terminals.filter((item) => item !== 'all'),
                    ...timelineTerminals
                ]);
                state.terminals = ['all', ...merged];
                if (!state.terminals.includes(state.terminal)) {
                    state.terminal = 'all';
                }
                renderTerminalTabs();
            }

            if (payload.window_start) {
                state.windowStartMs = toMs(payload.window_start);
            }
            if (payload.window_end) {
                state.windowEndMs = toMs(payload.window_end);
            }

            syncImpactedItemsFromTimeline();
            syncResourceFocusAfterTimelineRefresh();
            await refreshTimelineSafetyProgress(payload);

            syncSearchWithTimeline();

            renderWindowLabel();
            renderChart();
            renderStatusCounts();
            renderStatusOrderList();
            renderAiMetrics();
            renderAnalyticsPanel();
            renderScenarioPanel();
            renderResourceFocusBar();
            renderViewModeHint();
            if (shouldRefreshAnalyticsOnTimelineRefresh()) {
                await refreshAnalyticsData({ force: false, silent: true });
            }
            if (shouldRefreshConflictDataOnTimelineRefresh()) {
                await refreshConflictData({ force: false, silent: true });
            }
        } catch (error) {
            console.error('加载时间线失败:', error);
            showToast(error.message || '加载时间线失败', 'error');
        } finally {
            if (shouldShowStageLoading && ganttStage) {
                hideContainerLoading(ganttStage);
            }
            state.loading = false;
        }
    }

    function renderChart() {
        if (!state.chart) {
            return;
        }

        const timeline = state.timelineData;
        if (!timeline || !Array.isArray(timeline.items)) {
            state.chart.clear();
            state.chart.setOption({
                graphic: {
                    type: 'text',
                    left: 'center',
                    top: 'middle',
                    style: {
                        text: buildTimelineEmptyMessage(null),
                        fontSize: 13,
                        lineHeight: 20,
                        textAlign: 'center',
                        fill: CHART_THEME.emptyText
                    }
                }
            });
            return;
        }

        const lanes = Array.isArray(timeline.lanes) ? timeline.lanes : [];
        const activeResourceFocus = getActiveResourceFocus();
        const focusedLaneId = String(activeResourceFocus?.primary_lane_id || activeResourceFocus?.lane_id || '').trim();
        const focusedLaneIds = Array.isArray(activeResourceFocus?.lane_ids) && activeResourceFocus.lane_ids.length > 0
            ? activeResourceFocus.lane_ids.map((item) => String(item || '').trim()).filter(Boolean)
            : (focusedLaneId ? [focusedLaneId] : []);
        const yLabels = lanes.length > 0
            ? lanes.map((lane) => lane.label || lane.id || '-')
            : ['暂无资源行'];

        const filteredItems = getFilteredTimelineItems();
        const chartData = filteredItems.map((item) => {
            const statusCode = statusToCode(item.status);
            const isHighlight = item.id === state.highlightedItemId ? 1 : 0;
            const isSummary = item.is_flight_summary ? 1 : 0;
            const isImpacted = isTimelineItemImpacted(item) ? 1 : 0;
            const laneId = String(item.lane_id || '').trim();
            const isFocusedLane = focusedLaneId && laneId === focusedLaneId ? 1 : 0;
            const isSecondaryFocusedLane = !isFocusedLane && focusedLaneIds.includes(laneId) ? 1 : 0;
            const isSelected = state.selectedOrderIds && state.selectedOrderIds.has(item.order_id) ? 1 : 0;

            return {
                value: [
                    toMs(item.start_time),
                    toMs(item.end_time),
                    Number(item.lane_index || 0),
                    Number(item.lane_subtrack || 0),
                    Number(item.lane_subtrack_count || 1),
                    statusCode,
                    isHighlight,
                    isSummary,
                    isImpacted,
                    isFocusedLane,
                    isSecondaryFocusedLane,
                    isSelected
                ],
                raw: item
            };
        });

        const focusedLaneSeriesData = lanes
            .filter((lane) => focusedLaneIds.includes(String(lane?.id || '').trim()))
            .map((lane) => ({
                value: [
                    Number(lane.index || 0),
                    String(lane?.id || '').trim() === focusedLaneId ? 1 : 0
                ]
            }));
        const focusedLaneSeries = focusedLaneSeriesData.length > 0
            ? {
                id: 'dispatch-focused-lane',
                type: 'custom',
                silent: true,
                renderItem: renderFocusedLaneBand,
                encode: {
                    y: 0
                },
                data: focusedLaneSeriesData,
                z: 0
            }
            : null;

        const emptyMessage = buildTimelineEmptyMessage(timeline);

        state.chart.setOption({
            animation: true,
            animationDuration: 220,
            backgroundColor: 'transparent',
            graphic: chartData.length > 0 ? [] : {
                type: 'text',
                left: 'center',
                top: 'middle',
                z: 10,
                style: {
                    text: emptyMessage,
                    lineHeight: 20,
                    textAlign: 'center',
                    fontSize: 13,
                    fill: CHART_THEME.emptyText
                }
            },
            tooltip: {
                trigger: 'item',
                confine: true,
                borderWidth: 1,
                borderColor: CHART_THEME.tooltipBorder,
                formatter: (params) => renderTooltip(params.data?.raw)
            },
            grid: {
                left: 186,
                right: 20,
                top: 16,
                bottom: 38,
                containLabel: false
            },
            xAxis: {
                type: 'time',
                min: state.windowStartMs,
                max: state.windowEndMs,
                axisLine: { lineStyle: { color: CHART_THEME.axisLine } },
                axisLabel: {
                    color: CHART_THEME.axisLabel,
                    fontSize: 13,
                    formatter: (value) => formatAxisTime(value)
                },
                splitLine: {
                    lineStyle: {
                        color: CHART_THEME.splitLine,
                        type: 'dashed'
                    }
                }
            },
            yAxis: {
                type: 'category',
                inverse: true,
                data: yLabels,
                axisTick: { show: false },
                axisLine: { show: false },
                axisLabel: {
                    color: CHART_THEME.laneLabel,
                    fontSize: 13,
                    width: 156,
                    overflow: 'truncate',
                    rich: {
                        focusedLane: {
                            color: CHART_THEME.laneFocusLabelText,
                            backgroundColor: CHART_THEME.laneFocusLabelBg,
                            borderRadius: 7,
                            padding: [3, 8],
                            fontWeight: 700
                        },
                        secondaryFocusedLane: {
                            color: CHART_THEME.laneSecondaryFocusLabelText,
                            backgroundColor: CHART_THEME.laneSecondaryFocusLabelBg,
                            borderRadius: 7,
                            padding: [3, 8],
                            fontWeight: 600
                        }
                    },
                    formatter: (value, index) => {
                        const lane = lanes[index];
                        if (focusedLaneId && lane && String(lane.id || '').trim() === focusedLaneId) {
                            return `{focusedLane|${value}}`;
                        }
                        if (lane && focusedLaneIds.includes(String(lane.id || '').trim())) {
                            return `{secondaryFocusedLane|${value}}`;
                        }
                        return value;
                    }
                }
            },
            dataZoom: [
                {
                    type: 'inside',
                    xAxisIndex: 0,
                    zoomOnMouseWheel: true,
                    moveOnMouseMove: true,
                    moveOnMouseWheel: true
                },
                {
                    type: 'slider',
                    xAxisIndex: 0,
                    bottom: 8,
                    height: 12,
                    borderColor: CHART_THEME.zoomBorder,
                    backgroundColor: CHART_THEME.zoomBg,
                    fillerColor: CHART_THEME.zoomFiller,
                    showDetail: false
                }
            ],
            series: [
                ...(focusedLaneSeries ? [focusedLaneSeries] : []),
                {
                    id: 'dispatch-series',
                    type: 'custom',
                    renderItem: renderDispatchItem,
                    encode: {
                        x: [0, 1],
                        y: 2
                    },
                    data: chartData,
                    markLine: {
                        symbol: ['none', 'none'],
                        silent: true,
                        lineStyle: {
                            color: CHART_THEME.nowLine,
                            width: 1,
                            type: 'dashed'
                        },
                        label: {
                            show: true,
                            formatter: '现在',
                            color: CHART_THEME.nowLabelText,
                            fontWeight: 600,
                            backgroundColor: CHART_THEME.nowLabelBg,
                            borderColor: CHART_THEME.nowLabelBorder,
                            borderWidth: 1,
                            borderRadius: 4,
                            padding: [2, 6]
                        },
                        data: [{ xAxis: Date.now() }]
                    }
                }
            ]
        }, true);
    }

    function renderFocusedLaneBand(params, api) {
        const laneIndex = Number(api.value(0) || 0);
        const isPrimaryLane = Number(api.value(1) || 0) === 1;
        const coord = api.coord([state.windowStartMs, laneIndex]);
        const laneHeight = Math.max(10, api.size([0, 1])[1]);
        const rect = echarts.graphic.clipRectByRect({
            x: params.coordSys.x + 1,
            y: coord[1] - laneHeight / 2 + 1,
            width: Math.max(4, params.coordSys.width - 2),
            height: Math.max(8, laneHeight - 2),
            r: 8
        }, {
            x: params.coordSys.x,
            y: params.coordSys.y,
            width: params.coordSys.width,
            height: params.coordSys.height
        });
        if (!rect) {
            return null;
        }
        return {
            type: 'rect',
            shape: rect,
            style: {
                fill: isPrimaryLane ? CHART_THEME.laneFocusFill : CHART_THEME.laneSecondaryFocusFill,
                stroke: isPrimaryLane ? CHART_THEME.laneFocusStroke : CHART_THEME.laneSecondaryFocusStroke,
                lineWidth: 1
            },
            silent: true
        };
    }

    function renderDispatchItem(params, api) {
        const categoryIndex = api.value(2);
        const startCoord = api.coord([api.value(0), categoryIndex]);
        const endCoord = api.coord([api.value(1), categoryIndex]);
        const laneHeight = Math.max(8, api.size([0, 1])[1]);
        const subtrackIndex = Number(api.value(3) || 0);
        const subtrackCount = Math.max(1, Number(api.value(4) || 1));
        const statusCode = Number(api.value(5) || 0);
        const isHighlight = Number(api.value(6) || 0) === 1;
        const isSummary = Number(api.value(7) || 0) === 1;
        const isImpacted = Number(api.value(8) || 0) === 1;
        const isFocusedLane = Number(api.value(9) || 0) === 1;
        const isSecondaryFocusedLane = Number(api.value(10) || 0) === 1;
        const isSelected = Number(api.value(11) || 0) === 1;
        const rawItem = params.data.raw || {};
        const safetyProgress = getTimelineSafetyProgress(rawItem.order_id);
        const safetyGateState = getTimelineSafetyGateState(safetyProgress);
        const isDraft = isDraftOrder(rawItem);
        const isDraftSelected = isDraft && state.draftSelectedOrderIds.has(rawItem.order_id);
        const isLocked = isLockedOrder(rawItem);
        const hasAlert = hasSemanticAlert(rawItem, {
            isImpacted,
            safetyGateState,
        });

        const x = Math.min(startCoord[0], endCoord[0]);
        const width = Math.max(4, Math.abs(endCoord[0] - startCoord[0]));

        const lanePadding = isSummary ? 2 : 4;
        const subtrackGap = 1;
        const usableLaneHeight = Math.max(6, laneHeight - lanePadding * 2);
        const subtrackHeight = Math.max(6, (usableLaneHeight - (subtrackCount - 1) * subtrackGap) / subtrackCount);
        const y = startCoord[1] - usableLaneHeight / 2 + subtrackIndex * (subtrackHeight + subtrackGap);

        const clippedRect = echarts.graphic.clipRectByRect({
            x,
            y,
            width,
            height: subtrackHeight
        }, {
            x: params.coordSys.x,
            y: params.coordSys.y,
            width: params.coordSys.width,
            height: params.coordSys.height
        });

        if (!clippedRect) {
            return null;
        }

        clippedRect.r = isSummary ? 5 : 3;

        const fillColor = statusCodeToColor(statusCode);
        const statusKey = statusCodeToKey(statusCode);
        const statusSymbol = STATUS_SYMBOLS[statusKey] || '•';
        let borderColor = hasAlert
            ? CHART_THEME.itemConflictStroke
            : (isSelected
            ? '#007AFF'
            : (isHighlight
                ? CHART_THEME.itemHighlightStroke
                : (isImpacted
                    ? CHART_THEME.itemConflictStroke
                    : (isFocusedLane
                        ? CHART_THEME.itemSummaryStroke
                        : (isSecondaryFocusedLane
                            ? CHART_THEME.laneSecondaryFocusLabelText
                            : (isSummary ? fillColor : CHART_THEME.itemStroke))))));
        if (isDraftSelected) {
            borderColor = '#007AFF';
        }
        const labelText = rawItem.label || '';
        const draftPrefix = isDraft ? '草稿 · ' : '';
        const summaryCount = Array.isArray(rawItem.related_order_ids) ? rawItem.related_order_ids.length : 0;
        const displayLabel = clippedRect.width > 64
            ? `${statusSymbol} ${draftPrefix}${labelText}${isSummary && summaryCount > 0 ? ` · ${summaryCount}项` : ''}`
            : '';
        const baseFill = isSummary
            ? adjustColorAlpha(fillColor, hasAlert ? 0.24 : 0.2)
            : (isDraft ? adjustColorAlpha(fillColor, 0.22) : fillColor);
        const isSelectedOrDraftSelected = isSelected || isDraftSelected;
        const rectStyle = {
            fill: baseFill,
            stroke: borderColor,
            lineWidth: isSelectedOrDraftSelected ? 2.2 : (isSummary ? 1.8 : (isHighlight ? 2 : (isImpacted ? 1.8 : (isFocusedLane ? 1.6 : (isSecondaryFocusedLane ? 1.3 : 1))))),
            opacity: isSummary ? 0.98 : (isDraft ? 0.96 : (isFocusedLane ? 0.96 : (isSecondaryFocusedLane ? 0.93 : 0.9))),
            shadowBlur: isSelectedOrDraftSelected ? 10 : (isFocusedLane ? 8 : (isSecondaryFocusedLane ? 4 : 0)),
            shadowColor: isSelectedOrDraftSelected
                ? 'rgba(0, 122, 255, 0.28)'
                : (isFocusedLane
                    ? 'rgba(0, 122, 255, 0.18)'
                    : (isSecondaryFocusedLane ? 'rgba(0, 122, 255, 0.12)' : 'transparent'))
        };
        if (isDraft) {
            rectStyle.lineDash = [6, 3];
        } else if (statusKey === 'cancelled') {
            rectStyle.lineDash = [2, 2];
        }

        const textureChildren = buildStatusTextureShapes(statusKey, clippedRect, { isSummary });
        const summarySegments = isSummary ? buildSummaryStatusSegments(rawItem, clippedRect) : [];
        const safetyGateMarker = (!isSummary && safetyGateState !== 'none' && clippedRect.width >= 22)
            ? {
                type: 'circle',
                shape: {
                    cx: clippedRect.x + clippedRect.width - 6,
                    cy: clippedRect.y + 6,
                    r: 3.2
                },
                style: {
                    fill: SAFETY_GATE_COLORS[safetyGateState] || SAFETY_GATE_COLORS.pending,
                    stroke: 'rgba(255, 255, 255, 0.95)',
                    lineWidth: 1
                },
                silent: true
            }
            : null;
        const conflictMarkerShape = hasAlert ? {
            type: 'rect',
            shape: {
                x: clippedRect.x,
                y: clippedRect.y,
                width: Math.min(4, Math.max(2, clippedRect.width * 0.08)),
                height: clippedRect.height
            },
            style: {
                fill: CHART_THEME.itemConflictStroke,
                opacity: 0.92
            },
            silent: true
        } : null;
        const lockMarkerShape = isLocked && clippedRect.width >= 18 ? {
            type: 'rect',
            shape: {
                x: clippedRect.x + 5,
                y: clippedRect.y + 4,
                width: 5,
                height: 5,
                r: 1.5
            },
            style: {
                fill: '#ffffff',
                stroke: SEMANTIC_COLORS.lock,
                lineWidth: 1.1
            },
            silent: true
        } : null;

        const textShape = {
            type: 'text',
            style: {
                x: clippedRect.x + (hasAlert ? 8 : 6),
                y: clippedRect.y + clippedRect.height / 2,
                text: displayLabel,
                verticalAlign: 'middle',
                fill: isSummary ? SEMANTIC_COLORS.summaryText : CHART_THEME.itemText,
                fontSize: 12,
                fontWeight: isSummary ? 700 : 560,
                width: Math.max(20, clippedRect.width - (hasAlert ? 14 : 10)),
                overflow: 'truncate'
            },
            silent: true
        };

        return {
            type: 'group',
            children: [
                {
                    type: 'rect',
                    shape: clippedRect,
                    style: rectStyle
                },
                ...summarySegments,
                ...(safetyGateMarker ? [safetyGateMarker] : []),
                ...(conflictMarkerShape ? [conflictMarkerShape] : []),
                ...(lockMarkerShape ? [lockMarkerShape] : []),
                ...textureChildren,
                textShape
            ]
        };
    }

    function renderTooltip(item) {
        if (!item) {
            return '无数据';
        }

        const parts = [];
        const title = item.is_flight_summary
            ? `${escapeHtml(item.flight_no || '-')}`
            : `${escapeHtml(item.flight_no || '-')} | ${escapeHtml(item.task_type_name || '-')}`;
        parts.push(`<div style="font-weight:700;margin-bottom:4px;">${title}</div>`);
        parts.push(renderOrderSemanticMeta(item));
        parts.push(`<div>时间：${escapeHtml(formatDateTime(item.start_time))} - ${escapeHtml(formatDateTime(item.end_time))}</div>`);

        if (isTimelineItemImpacted(item)) {
            parts.push('<div>冲突治理：当前关注任务</div>');
        }

        if (item.team_name || item.individual_username) {
            parts.push(`<div>执行：${escapeHtml(item.individual_username || item.team_name || '-')}</div>`);
        }

        if (Array.isArray(item.equipment_codes) && item.equipment_codes.length > 0) {
            parts.push(`<div>设备：${escapeHtml(item.equipment_codes.join(' / '))}</div>`);
        }

        if (!item.is_flight_summary && item.order_id) {
            const progress = getTimelineSafetyProgress(item.order_id);
            if (progress && progress.enforced) {
                const gateState = getTimelineSafetyGateState(progress);
                const gateLabel = gateState === 'ready'
                    ? '清单就绪'
                    : (gateState === 'blocked' ? '清单阻断' : '清单待补齐');
                const completedRequired = Number(progress.completed_required || 0);
                const requiredTotal = Number(progress.required_total || 0);
                parts.push(`<div>安全门禁：${escapeHtml(gateLabel)}（${completedRequired}/${requiredTotal}）</div>`);
            }
        }

        if (item.is_flight_summary) {
            const relatedCount = Array.isArray(item.related_order_ids) ? item.related_order_ids.length : 0;
            parts.push(`<div>覆盖派工：${relatedCount} 项</div>`);
            parts.push(`<div style="margin-top:4px;">状态分布：</div>`);
            parts.push(renderRelatedOrderBreakdown(item.related_orders || []));
        } else {
            if (item.publication_state) {
                parts.push(`<div>发布状态：${escapeHtml(renderPublicationStateLabel(item.publication_state))}</div>`);
            }
            if (isLockedOrder(item)) {
                parts.push(`<div>优化约束：${escapeHtml(renderLockLevelLabel(item.lock_level))}</div>`);
            }
            if (String(item.conflict_reason || '').trim()) {
                parts.push(`<div>冲突原因：${escapeHtml(String(item.conflict_reason).trim())}</div>`);
            } else if (String(item.availability_reason || '').trim()) {
                parts.push(`<div>资源约束：${escapeHtml(String(item.availability_reason).trim())}</div>`);
            }
        }

        return parts.join('');
    }

    async function refreshTimelineSafetyProgress(timeline) {
        try {
            const dataLayer = getDispatchBoardData() || await ensureDispatchBoardDataModule();
            state.timelineSafetyProgressByOrder = await dataLayer.fetchTimelineSafetyProgress(timeline);
        } catch (error) {
            console.warn('批量加载安全清单进度失败:', error);
            state.timelineSafetyProgressByOrder = {};
        }
    }

    function getTimelineSafetyProgress(orderId) {
        if (!orderId) {
            return null;
        }
        return state.timelineSafetyProgressByOrder[String(orderId)] || null;
    }

    function getTimelineSafetyGateState(progress) {
        if (!progress || !progress.enforced) {
            return 'none';
        }
        if (progress.ready) {
            return 'ready';
        }
        if (Number(progress.failed_required_count || 0) > 0) {
            return 'blocked';
        }
        return 'pending';
    }

    function getSafetyGateFilterLabel() {
        switch (state.safetyGateFilter) {
            case 'ready':
                return '仅清单就绪';
            case 'pending':
                return '仅清单待补齐';
            case 'blocked':
                return '仅清单阻断';
            default:
                return '全部任务';
        }
    }

    function isSafetyGateFilterMatch(gateState) {
        const filter = state.safetyGateFilter || 'all';
        if (filter === 'all') {
            return true;
        }
        return gateState === filter;
    }

    function getFilteredTimelineItems() {
        const timelineItems = Array.isArray(state.timelineData?.items) ? state.timelineData.items : [];
        const filter = state.safetyGateFilter || 'all';
        if (filter === 'all') {
            return timelineItems;
        }

        return timelineItems.filter((item) => {
            if (!item) {
                return false;
            }

            if (item.is_flight_summary) {
                const relatedOrderIds = Array.isArray(item.related_order_ids) ? item.related_order_ids : [];
                return relatedOrderIds.some((orderId) => {
                    const progress = getTimelineSafetyProgress(orderId);
                    const gateState = getTimelineSafetyGateState(progress);
                    return isSafetyGateFilterMatch(gateState);
                });
            }

            const progress = getTimelineSafetyProgress(item.order_id);
            const gateState = getTimelineSafetyGateState(progress);
            return isSafetyGateFilterMatch(gateState);
        });
    }

    function buildFilteredStatusOrders() {
        const map = {
            pending: [],
            assigned: [],
            in_progress: [],
            completed: [],
            cancelled: []
        };

        const items = getFilteredTimelineItems();
        for (const item of items) {
            if (!item || item.is_flight_summary) {
                continue;
            }

            const statusKey = String(item.status || 'pending');
            if (!Object.prototype.hasOwnProperty.call(map, statusKey)) {
                continue;
            }

            map[statusKey].push({
                order_id: item.order_id || '',
                label: item.label || item.task_type_name || item.flight_no || '-',
                start_time: item.start_time,
                end_time: item.end_time,
                focus_item_id: item.id || ''
            });
        }

        STATUS_ORDER.forEach((status) => {
            map[status].sort((a, b) => toMs(a.start_time) - toMs(b.start_time));
        });

        return map;
    }

    function buildFilteredStatusCounts(statusOrders) {
        const counts = {
            pending: 0,
            assigned: 0,
            in_progress: 0,
            completed: 0,
            cancelled: 0
        };
        STATUS_ORDER.forEach((status) => {
            counts[status] = Array.isArray(statusOrders[status]) ? statusOrders[status].length : 0;
        });
        return counts;
    }

    function buildTimelineEmptyMessage(timeline) {
        if (!timeline) {
            return '暂无时间线数据\n请稍后刷新重试';
        }

        const terminalText = state.terminal === 'all' ? '全部航站楼' : state.terminal;
        const laneCount = Array.isArray(timeline.lanes) ? timeline.lanes.length : 0;
        if (laneCount === 0) {
            return `当前视角暂无可展示资源行\n建议切换视角或检查基础资源配置`;
        }

        if ((state.safetyGateFilter || 'all') !== 'all') {
            return `当前筛选下无任务（${getSafetyGateFilterLabel()}）\n建议切换门禁筛选或点击“当前时间”重试`;
        }

        return `当前时间窗无任务（${terminalText}）\n建议点击“当前时间”或切换航站楼后重试`;
    }

    function renderViewModeHint() {
        const hint = document.getElementById('viewModeHint');
        if (!hint) {
            return;
        }

        const modeText = VIEW_LABELS[state.viewMode] || state.viewMode;
        const terminalText = state.terminal === 'all' ? '全部航站楼' : state.terminal;
        const adminText = state.isAdmin && state.viewMode === 'flight'
            ? '管理员航班汇总条'
            : '任务明细条';
        const resourceFocus = getActiveResourceFocus();
        const focusText = resourceFocus
            ? ` | 当前聚焦 ${getResourceFocusDisplayText(resourceFocus)}`
            : '';

        const filteredCount = getFilteredTimelineItems().length;
        if (filteredCount === 0) {
            hint.textContent = `${modeText} | ${terminalText} | ${getSafetyGateFilterLabel()}${focusText} | 当前视图暂无任务，请切换筛选或重置时间窗。`;
            return;
        }

        hint.textContent = `${modeText} | ${adminText} | ${terminalText} | ${getSafetyGateFilterLabel()}${focusText} | 单击高亮，双击详情，滚轮缩放时间轴，右上角圆点为安全门禁状态。`;
    }

    async function performTimelineSearch() {
        const input = document.getElementById('timelineSearchInput');
        const query = normalizeSearchQuery(input ? input.value : '');
        state.searchQuery = query;
        state.searchMatchIndex = -1;

        syncSearchWithTimeline();
        if (!query) {
            closeSearchResultPanel();
            return;
        }

        openSearchResultPanel();
        if (state.searchMatches.length === 0) {
            showToast(`未找到包含“${query}”的任务`);
            return;
        }

        await locateSearchMatch(state.searchMatchIndex);
    }

    async function jumpToNextSearchMatch() {
        if (!state.searchQuery) {
            await performTimelineSearch();
            return;
        }

        syncSearchWithTimeline();
        if (state.searchMatches.length === 0) {
            showToast('当前时间窗内没有匹配项');
            return;
        }

        state.searchMatchIndex = (state.searchMatchIndex + 1) % state.searchMatches.length;
        openSearchResultPanel();
        await locateSearchMatch(state.searchMatchIndex);
    }

    async function moveSearchSelection(step) {
        if (!state.searchQuery) {
            return;
        }

        syncSearchWithTimeline();
        if (state.searchMatches.length === 0) {
            openSearchResultPanel();
            return;
        }

        const total = state.searchMatches.length;
        const base = state.searchMatchIndex < 0 ? 0 : state.searchMatchIndex;
        state.searchMatchIndex = (base + step + total) % total;
        openSearchResultPanel();
        await locateSearchMatch(state.searchMatchIndex, { silent: true });
    }

    function syncSearchWithTimeline() {
        const query = normalizeSearchQuery(state.searchQuery);
        if (!query) {
            state.searchMatches = [];
            state.searchMatchIndex = -1;
            updateSearchMeta();
            renderSearchResultPanel();
            return;
        }

        const items = getFilteredTimelineItems();

        const previousItemId = state.searchMatches[state.searchMatchIndex]
            ? state.searchMatches[state.searchMatchIndex].id
            : null;

        const matches = items
            .filter((item) => buildItemSearchText(item).includes(query))
            .sort((a, b) => toMs(a.start_time) - toMs(b.start_time));

        state.searchMatches = matches;
        if (matches.length === 0) {
            state.searchMatchIndex = -1;
        } else if (previousItemId) {
            const matchedIndex = matches.findIndex((item) => item.id === previousItemId);
            state.searchMatchIndex = matchedIndex >= 0 ? matchedIndex : 0;
        } else if (state.searchMatchIndex < 0 || state.searchMatchIndex >= matches.length) {
            state.searchMatchIndex = 0;
        }

        updateSearchMeta();
        renderSearchResultPanel();
    }

    function buildItemSearchText(item) {
        if (!item || typeof item !== 'object') {
            return '';
        }

        const fields = [
            item.flight_no,
            item.flight_id,
            item.label,
            item.task_type_name,
            item.task_type,
            item.order_id,
            item.team_name,
            item.individual_username,
            item.lane_label,
            item.terminal,
            Array.isArray(item.equipment_codes) ? item.equipment_codes.join(' ') : ''
        ];
        return fields
            .map((field) => String(field || '').toLowerCase())
            .join(' ');
    }

    async function locateSearchMatch(index, options = {}) {
        const total = state.searchMatches.length;
        if (total === 0 || index < 0 || index >= total) {
            return;
        }

        state.searchMatchIndex = index;
        const target = state.searchMatches[index];
        updateSearchMeta();
        renderSearchResultPanel();
        await focusTimelineItem(target.id, target.order_id, { openDetail: false });
        if (!options.silent) {
            const orderRef = target.order_id ? ` | 工单 ${target.order_id}` : '';
            showToast(`已定位匹配项 ${index + 1}/${total}${orderRef}`);
        }
    }

    function openSearchResultPanel() {
        if (!state.searchQuery) {
            return;
        }
        state.searchResultOpen = true;
        renderSearchResultPanel();
    }

    function closeSearchResultPanel() {
        state.searchResultOpen = false;
        const panel = document.getElementById('timelineSearchResults');
        if (panel) {
            panel.classList.remove('open');
        }
    }

    function renderSearchResultPanel() {
        const panel = document.getElementById('timelineSearchResults');
        if (!panel) {
            return;
        }

        const query = normalizeSearchQuery(state.searchQuery);
        if (!query) {
            panel.innerHTML = '';
            panel.classList.remove('open');
            return;
        }

        if (state.searchMatches.length === 0) {
            panel.innerHTML = '<div class="search-result-empty">未找到匹配项</div>';
            panel.classList.toggle('open', state.searchResultOpen);
            return;
        }

        const total = state.searchMatches.length;
        const currentIndex = state.searchMatchIndex >= 0 ? state.searchMatchIndex : 0;
        const half = Math.floor(SEARCH_RESULT_RENDER_LIMIT / 2);
        const startIndex = total > SEARCH_RESULT_RENDER_LIMIT
            ? clamp(currentIndex - half, 0, total - SEARCH_RESULT_RENDER_LIMIT)
            : 0;
        const endIndex = Math.min(total, startIndex + SEARCH_RESULT_RENDER_LIMIT);
        const displayMatches = state.searchMatches.slice(startIndex, endIndex);

        const resultItems = displayMatches.map((item, offset) => {
            const index = startIndex + offset;
            const statusKey = item.status || 'pending';
            const statusLabel = STATUS_LABELS[statusKey] || statusKey;
            const statusSymbol = STATUS_SYMBOLS[statusKey] || '•';
            const actorText = item.individual_username || item.team_name || item.lane_label || item.terminal || '-';
            const mainText = `${statusSymbol} ${item.flight_no || '-'} | ${item.task_type_name || item.label || '-'}`;
            const subText = `${statusLabel} | ${formatDateTime(item.start_time)} | ${actorText}`;
            const activeClass = index === state.searchMatchIndex ? 'active' : '';
            return `
                <button type="button" class="search-result-item ${activeClass}" data-match-index="${index}">
                    <div class="search-result-main">${escapeHtml(mainText)}</div>
                    <div class="search-result-sub">${escapeHtml(subText)}</div>
                </button>
            `;
        }).join('');

        const overflowFoot = total > SEARCH_RESULT_RENDER_LIMIT
            ? `<div class="search-result-foot">显示 ${startIndex + 1}-${endIndex} / 共 ${total} 条</div>`
            : '';

        panel.innerHTML = `${resultItems}${overflowFoot}`;
        panel.classList.toggle('open', state.searchResultOpen);
    }

    function updateSearchMeta() {
        const meta = document.getElementById('timelineSearchMeta');
        const nextBtn = document.getElementById('timelineSearchNextBtn');
        if (nextBtn) {
            nextBtn.disabled = state.searchMatches.length === 0;
        }
        if (!meta) {
            return;
        }

        if (!state.searchQuery) {
            meta.textContent = '未搜索';
            return;
        }

        if (state.searchMatches.length === 0) {
            meta.textContent = '0 条匹配';
            return;
        }

        const current = state.searchMatchIndex >= 0 ? (state.searchMatchIndex + 1) : 1;
        meta.textContent = `${current}/${state.searchMatches.length}`;
    }

    function normalizeSearchQuery(value) {
        return String(value || '').trim().toLowerCase();
    }

    function renderWindowLabel() {
        const label = document.getElementById('windowLabel');
        if (!label) {
            return;
        }
        const modeText = VIEW_LABELS[state.viewMode] || state.viewMode;
        const adminText = state.isAdmin && state.viewMode === 'flight'
            ? '管理员总览模式'
            : '任务明细模式（双击看详情）';
        const terminalText = state.terminal === 'all' ? '全部航站楼' : state.terminal;
        label.textContent = `${modeText} | ${adminText} | ${terminalText} | ${getSafetyGateFilterLabel()} | ${formatDateTime(state.windowStartMs)} - ${formatDateTime(state.windowEndMs)}`;
    }

    function getCurrentStatusEntries(statusOrders) {
        const source = statusOrders || buildFilteredStatusOrders();
        const entries = Array.isArray(source[state.selectedStatus])
            ? source[state.selectedStatus]
            : [];
        return [...entries].sort((a, b) => toMs(a.start_time) - toMs(b.start_time));
    }

    function getAllFilteredOrderEntries() {
        const statusOrders = buildFilteredStatusOrders();
        const seen = new Set();
        const result = [];

        STATUS_ORDER.forEach((statusKey) => {
            const entries = Array.isArray(statusOrders[statusKey]) ? statusOrders[statusKey] : [];
            entries.forEach((entry) => {
                const orderId = String(entry.order_id || '').trim();
                if (!orderId || seen.has(orderId)) {
                    return;
                }
                seen.add(orderId);
                result.push({
                    order_id: orderId,
                    status: statusKey,
                    label: entry.label || '-',
                    start_time: entry.start_time,
                    end_time: entry.end_time,
                    focus_item_id: entry.focus_item_id || ''
                });
            });
        });

        return result;
    }

    function ensureSelectedStatusAvailable(statusOrders) {
        const source = statusOrders || buildFilteredStatusOrders();
        const currentEntries = Array.isArray(source[state.selectedStatus]) ? source[state.selectedStatus] : [];
        if (currentEntries.length > 0) {
            return;
        }

        const nextStatus = STATUS_ORDER.find((status) => {
            const entries = Array.isArray(source[status]) ? source[status] : [];
            return entries.length > 0;
        });

        state.selectedStatus = nextStatus || 'pending';
    }

    function toggleStatusOrderSelection(orderId) {
        const normalizedOrderId = String(orderId || '').trim();
        if (!normalizedOrderId) {
            return;
        }

        if (state.statusPanelSelectedOrderIds.has(normalizedOrderId)) {
            state.statusPanelSelectedOrderIds.delete(normalizedOrderId);
        } else {
            state.statusPanelSelectedOrderIds.add(normalizedOrderId);
        }
    }

    function toggleSelectAllCurrentStatusOrders() {
        const entries = getCurrentStatusEntries();
        if (entries.length === 0) {
            return;
        }

        const orderIds = entries
            .map((entry) => String(entry.order_id || '').trim())
            .filter(Boolean);
        if (orderIds.length === 0) {
            return;
        }

        const allSelected = orderIds.every((orderId) => state.statusPanelSelectedOrderIds.has(orderId));
        if (allSelected) {
            orderIds.forEach((orderId) => state.statusPanelSelectedOrderIds.delete(orderId));
            showToast('已取消本列全选');
        } else {
            orderIds.forEach((orderId) => state.statusPanelSelectedOrderIds.add(orderId));
            showToast(`已全选本列 ${orderIds.length} 条工单`);
        }

        renderStatusOrderList();
        renderStatusToolbar();
    }

    function isStatusBatchActive() {
        return Array.isArray(state.statusPanelBatchOrderIds)
            && state.statusPanelBatchOrderIds.length > 0
            && Number.isInteger(state.statusPanelBatchIndex)
            && state.statusPanelBatchIndex >= 0
            && state.statusPanelBatchIndex < state.statusPanelBatchOrderIds.length;
    }

    function stopStatusPanelBatchProcess(options = {}) {
        const silent = options.silent === true;
        state.statusPanelBatchOrderIds = [];
        state.statusPanelBatchIndex = -1;
        if (!silent) {
            showToast('已结束批量处理模式');
        }
        renderDetailDrawer();
        renderStatusToolbar();
    }

    async function startStatusPanelBatchProcess() {
        const selectedOrderIds = Array.from(state.statusPanelSelectedOrderIds)
            .map((orderId) => String(orderId || '').trim())
            .filter(Boolean);

        const batchOrderIds = selectedOrderIds.length > 0
            ? selectedOrderIds
            : getAllFilteredOrderEntries().map((entry) => entry.order_id);

        const uniqueOrderIds = [];
        const seen = new Set();
        batchOrderIds.forEach((orderId) => {
            if (!orderId || seen.has(orderId)) {
                return;
            }
            seen.add(orderId);
            uniqueOrderIds.push(orderId);
        });

        if (uniqueOrderIds.length === 0) {
            showToast('当前筛选下没有可批量处理的工单');
            return;
        }

        state.statusPanelBatchOrderIds = uniqueOrderIds;
        state.statusPanelBatchIndex = 0;
        renderStatusToolbar();

        const firstOrderId = uniqueOrderIds[0];
        await openOrderDetail(firstOrderId);
        await focusOrder(firstOrderId);
        showToast(`已进入批量处理模式 1/${uniqueOrderIds.length}`);
    }

    async function moveStatusBatchOrder(step) {
        if (!isStatusBatchActive()) {
            showToast('当前未处于批量处理模式');
            return;
        }

        const total = state.statusPanelBatchOrderIds.length;
        const nextIndex = clamp(state.statusPanelBatchIndex + step, 0, total - 1);
        if (nextIndex === state.statusPanelBatchIndex) {
            showToast(step > 0 ? '已是最后一条' : '已是第一条');
            return;
        }

        state.statusPanelBatchIndex = nextIndex;
        const orderId = state.statusPanelBatchOrderIds[nextIndex];
        renderStatusToolbar();
        await openOrderDetail(orderId);
        await focusOrder(orderId);
        showToast(`批量处理 ${nextIndex + 1}/${total}`);
    }

    function applyQuickSafetyGateFilter(nextFilter) {
        const normalized = String(nextFilter || '').trim();
        if (!['all', 'ready', 'pending', 'blocked'].includes(normalized)) {
            return;
        }

        state.safetyGateFilter = normalized;
        state.statusPanelSelectedOrderIds.clear();
        stopStatusPanelBatchProcess({ silent: true });

        syncSearchWithTimeline();
        const statusOrders = buildFilteredStatusOrders();
        ensureSelectedStatusAvailable(statusOrders);
        renderChart();
        renderStatusCounts();
        renderStatusOrderList();
        renderStatusToolbar();
        renderViewModeHint();
        renderWindowLabel();
        showToast(`已切换门禁筛选：${getSafetyGateFilterLabel()}`);
    }

    function renderStatusToolbar() {
        const blockedBtn = document.getElementById('statusFilterBlockedBtn');
        const showAllBtn = document.getElementById('statusShowAllBtn');
        const selectAllBtn = document.getElementById('statusSelectAllBtn');
        const batchBtn = document.getElementById('statusBatchOpenBtn');
        const summary = document.getElementById('statusSelectionSummary');

        const currentEntries = getCurrentStatusEntries();
        const currentOrderIds = currentEntries
            .map((entry) => String(entry.order_id || '').trim())
            .filter(Boolean);
        const selectedCountInCurrent = currentOrderIds.filter((orderId) => state.statusPanelSelectedOrderIds.has(orderId)).length;
        const selectedCountTotal = Array.from(state.statusPanelSelectedOrderIds).length;

        if (blockedBtn) {
            blockedBtn.classList.toggle('active', state.safetyGateFilter === 'blocked');
        }
        if (showAllBtn) {
            showAllBtn.classList.toggle('active', state.safetyGateFilter === 'all');
        }
        if (selectAllBtn) {
            selectAllBtn.disabled = currentOrderIds.length === 0;
            const allSelected = currentOrderIds.length > 0 && selectedCountInCurrent === currentOrderIds.length;
            selectAllBtn.textContent = allSelected ? '取消本列全选' : '全选本列';
        }
        if (batchBtn) {
            const batchTotal = isStatusBatchActive() ? state.statusPanelBatchOrderIds.length : 0;
            const selectedText = selectedCountTotal > 0 ? `(${selectedCountTotal})` : '';
            batchBtn.textContent = isStatusBatchActive()
                ? `批处理中 ${state.statusPanelBatchIndex + 1}/${batchTotal}`
                : `批量处理${selectedText}`;
            batchBtn.disabled = isStatusBatchActive()
                || (!isStatusBatchActive() && currentOrderIds.length === 0 && selectedCountTotal === 0);
        }
        if (summary) {
            if (isStatusBatchActive()) {
                summary.textContent = `批量处理中：${state.statusPanelBatchIndex + 1}/${state.statusPanelBatchOrderIds.length}`;
            } else {
                summary.textContent = `已勾选 ${selectedCountTotal} 条 | 当前列 ${currentOrderIds.length} 条`;
            }
        }
    }

    function renderStatusCounts() {
        const container = document.getElementById('statusCounts');
        if (!container) {
            return;
        }

        const statusOrders = buildFilteredStatusOrders();
        ensureSelectedStatusAvailable(statusOrders);
        const counts = buildFilteredStatusCounts(statusOrders);
        container.innerHTML = STATUS_ORDER.map((status) => {
            const value = Number(counts[status] || 0);
            const activeClass = state.selectedStatus === status ? 'active' : '';
            const symbol = STATUS_SYMBOLS[status] || '•';
            return `
                <div class="status-count-card ${activeClass}" data-status="${status}">
                    <span class="status-count-label">
                        <span class="status-symbol ${status.replace('_', '-')}">${escapeHtml(symbol)}</span>
                        <span class="legend-dot ${status.replace('_', '-')}"></span>
                        ${STATUS_LABELS[status] || status}
                    </span>
                    <span class="status-count-value">${value}</span>
                </div>
            `;
        }).join('');
    }

    function renderStatusOrderList() {
        const list = document.getElementById('statusOrderList');
        const tip = document.getElementById('statusListTip');
        if (!list) {
            return;
        }

        const statusOrders = buildFilteredStatusOrders();
        ensureSelectedStatusAvailable(statusOrders);
        const entries = getCurrentStatusEntries(statusOrders);

        if (entries.length === 0) {
            if (!renderUnifiedState(list, 'empty', '当前状态没有可定位任务')) {
                list.innerHTML = '<div class="empty-state">当前状态没有可定位任务</div>';
            }
            if (tip) {
                tip.textContent = `0 条任务（${getSafetyGateFilterLabel()}）`;
            }
            renderStatusToolbar();
            return;
        }

        hideContainerLoading(list);

        const displayEntries = entries.slice(0, STATUS_LIST_LIMIT);
        const statusSymbol = STATUS_SYMBOLS[state.selectedStatus] || '•';
        list.innerHTML = displayEntries.map((entry) => {
            const focusItemId = entry.focus_item_id || '';
            const orderId = String(entry.order_id || '').trim();
            const progress = getTimelineSafetyProgress(entry.order_id);
            const gateState = getTimelineSafetyGateState(progress);
            const gateHint = (progress && progress.enforced)
                ? (gateState === 'ready' ? ' | 清单就绪' : (gateState === 'blocked' ? ' | 清单阻断' : ' | 清单待补齐'))
                : '';
            const isSelected = orderId ? state.statusPanelSelectedOrderIds.has(orderId) : false;
            const selectedClass = isSelected ? 'is-selected' : '';
            return `
                <div class="status-order-item ${selectedClass}" data-focus-item-id="${escapeHtmlAttribute(focusItemId)}" data-order-id="${escapeHtmlAttribute(orderId)}">
                    <p class="main">
                        <button class="status-order-select ${selectedClass}" data-order-id="${escapeHtmlAttribute(orderId)}" aria-pressed="${isSelected ? 'true' : 'false'}" title="${isSelected ? '取消勾选' : '勾选加入批处理'}" aria-label="${isSelected ? '取消勾选工单' : '勾选工单'}">${isSelected ? '✓' : '+'}</button>
                        ${escapeHtml(statusSymbol)} ${escapeHtml(entry.label || '-')}
                    </p>
                    <p class="sub">${escapeHtml(formatDateTime(entry.start_time))} - ${escapeHtml(formatDateTime(entry.end_time))}${escapeHtml(gateHint)}</p>
                </div>
            `;
        }).join('');

        if (tip) {
            if (entries.length > STATUS_LIST_LIMIT) {
                tip.textContent = `共 ${entries.length} 条，当前仅展示前 ${STATUS_LIST_LIMIT} 条（按开始时间排序）`;
            } else {
                tip.textContent = `共 ${entries.length} 条任务（${getSafetyGateFilterLabel()}）`;
            }
        }

        renderStatusToolbar();
    }

    function renderAiMetrics() {
        const timeline = state.timelineData;
        if (!timeline) {
            setMetric('metricConflicts', '0');
            setMetric('metricPending', '0');
            setMetric('metricHeavy', '0');
            return;
        }

        const lanes = Array.isArray(timeline.lanes) ? timeline.lanes : [];
        const localConflictLaneCount = lanes.filter((lane) => Number(lane.subtrack_count || 1) > 1).length;
        const backendConflictCount = Array.isArray(state.conflictsRaw) ? state.conflictsRaw.length : 0;
        const conflictCount = backendConflictCount > 0 ? backendConflictCount : localConflictLaneCount;
        const pendingCount = Number((timeline.status_counts && timeline.status_counts.pending) || 0);
        const heavyCount = countHeavyLoadLanes(timeline);

        setMetric('metricConflicts', String(conflictCount));
        setMetric('metricPending', String(pendingCount));
        setMetric('metricHeavy', String(heavyCount));
    }

    function renderAnalyticsPanel() {
        const refreshBtn = document.getElementById('analyticsRefreshBtn');
        if (refreshBtn) {
            refreshBtn.disabled = state.analyticsLoading;
        }
        const modeButtons = document.querySelectorAll('[data-analytics-mode]');
        modeButtons.forEach((button) => {
            button.classList.toggle('active', button.dataset.analyticsMode === state.analyticsBreakdownMode);
        });

        const hint = document.getElementById('analyticsDataHint');
        if (hint) {
            if (state.analyticsLoading) {
                hint.textContent = '正在刷新当前时间窗运营分析...';
            } else if (state.analyticsError) {
                hint.textContent = `运营分析加载失败：${state.analyticsError}`;
            } else if (state.analyticsSummary) {
                const lastUpdated = state.analyticsLastUpdatedAt > 0
                    ? formatDateTime(state.analyticsLastUpdatedAt)
                    : '-';
                hint.textContent = `当前窗口 ${formatDateTime(state.windowStartMs)} - ${formatDateTime(state.windowEndMs)}，最后更新 ${lastUpdated}`;
            } else {
                hint.textContent = '当前时间窗运营分析未加载';
            }
        }

        const summary = state.analyticsSummary;
        setMetric('analyticsConflictRate', summary ? formatRatePercent(summary.conflict_rate) : '-');
        setMetric('analyticsReplanRate', summary ? formatRatePercent(summary.replan_rate) : '-');
        setMetric('analyticsResponseMinutes', summary ? formatMinutes(summary.avg_dispatch_response_minutes) : '-');
        setMetric('analyticsBalanceScore', summary ? formatDecimal(summary.team_load_balance_score) : '-');
        setMetric('analyticsIdleRate', summary ? formatRatePercent(summary.equipment_idle_rate) : '-');
        setMetric('analyticsOntimeRate', summary ? formatRatePercent(summary.key_order_ontime_rate) : '-');
        renderAnalyticsTrendChart();

        const list = document.getElementById('analyticsBreakdownList');
        if (!list) {
            return;
        }

        const isEmployeeMode = state.analyticsBreakdownMode === 'employee';
        const items = isEmployeeMode
            ? buildEmployeeAnalyticsBreakdownFromTimeline().slice(0, 8)
            : (Array.isArray(state.analyticsBreakdown) ? state.analyticsBreakdown.slice(0, 5) : []);

        if (!isEmployeeMode && state.analyticsLoading && (!Array.isArray(state.analyticsBreakdown) || state.analyticsBreakdown.length === 0)) {
            if (!showContainerLoading(list, '正在加载运营分析...', { minHeight: '120px' })) {
                list.innerHTML = '<div class="empty-state" style="height:auto;padding:12px 0;">正在加载运营分析...</div>';
            }
            return;
        }

        if (!isEmployeeMode && state.analyticsError && (!Array.isArray(state.analyticsBreakdown) || state.analyticsBreakdown.length === 0)) {
            if (!renderUnifiedState(list, 'error', state.analyticsError)) {
                list.innerHTML = `<div class="empty-state" style="height:auto;padding:12px 0;">${escapeHtml(state.analyticsError)}</div>`;
            }
            return;
        }
        if (items.length === 0) {
            if (!renderUnifiedState(list, 'empty', `暂无${isEmployeeMode ? '个人' : '班组'}分解数据`)) {
                list.innerHTML = `<div class="empty-state" style="height:auto;padding:12px 0;">暂无${isEmployeeMode ? '个人' : '班组'}分解数据</div>`;
            }
            return;
        }

        hideContainerLoading(list);

        list.innerHTML = isEmployeeMode
            ? items.map(renderEmployeeAnalyticsCard).join('')
            : items.map(renderTeamAnalyticsCard).join('');
    }

    async function refreshAnalyticsData(options = {}) {
        const force = Boolean(options.force);
        const silent = Boolean(options.silent);
        const now = Date.now();
        if (!force && now - state.analyticsLastUpdatedAt < ANALYTICS_REFRESH_MIN_INTERVAL_MS) {
            return;
        }
        if (state.analyticsLoading && !force) {
            return;
        }

        state.analyticsLoading = true;
        state.analyticsError = '';
        renderAnalyticsPanel();

        const requestSeq = state.analyticsRequestSeq + 1;
        state.analyticsRequestSeq = requestSeq;

        try {
            const dataLayer = getDispatchBoardData() || await ensureDispatchBoardDataModule();
            const analyticsPayload = await dataLayer.fetchAnalytics({
                windowStartMs: state.windowStartMs,
                windowEndMs: state.windowEndMs
            });
            if (requestSeq !== state.analyticsRequestSeq) {
                return;
            }

            state.analyticsSummary = analyticsPayload.summary;
            state.analyticsBreakdown = analyticsPayload.breakdown;
            state.analyticsTrend = analyticsPayload.trend;
            state.analyticsLastUpdatedAt = Date.now();
            state.analyticsError = '';
            renderAnalyticsPanel();

            if (!silent) {
                const count = Number(state.analyticsSummary?.assigned_order_count || 0);
                showToast(`运营分析已刷新（已分配 ${count} 单）`);
            }
        } catch (error) {
            if (requestSeq !== state.analyticsRequestSeq) {
                return;
            }
            state.analyticsError = error.message || '运营分析加载失败';
            renderAnalyticsPanel();
            if (!silent) {
                showToast(state.analyticsError);
            }
        } finally {
            if (requestSeq === state.analyticsRequestSeq) {
                state.analyticsLoading = false;
                renderAnalyticsPanel();
            }
        }
    }

    function renderTeamAnalyticsCard(item) {
        const label = String(item?.group_label || item?.group_key || '未命名班组');
        const representativeOrderId = findRepresentativeOrderIdForTeam(item?.group_key, item?.group_label);
        const relatedOrderIds = collectOrderIdsForTeamResource(item?.group_key, item?.group_label);
        const isFocused = isResourceFocusActive('team', item?.group_key, label);
        return `
            <div class="ai-suggestion${isFocused ? ' is-selected' : ''}">
                <div class="analytics-breakdown-head">
                    <h4>${escapeHtml(label)}</h4>
                    <span class="suggestion-chip">${escapeHtml(formatRatePercent(item?.conflict_rate))}</span>
                </div>
                <div class="analytics-breakdown-meta">
                    <span>工单 ${escapeHtml(String(Number(item?.order_count || 0)))}</span>
                    <span>已分配 ${escapeHtml(String(Number(item?.assigned_order_count || 0)))}</span>
                    <span>已完成 ${escapeHtml(String(Number(item?.completed_order_count || 0)))}</span>
                    <span>重排率 ${escapeHtml(formatRatePercent(item?.replan_rate))}</span>
                    <span>响应 ${escapeHtml(formatMinutes(item?.avg_dispatch_response_minutes))}</span>
                </div>
                <div class="ai-suggestion-actions">
                    <button class="suggestion-chip" data-action="focus-analytics-resource" data-view-mode="team" data-order-id="${escapeHtmlAttribute(representativeOrderId || '')}" data-resource-type="team" data-resource-id="${escapeHtmlAttribute(String(item?.group_key || '').trim())}" data-resource-label="${escapeHtmlAttribute(label)}" data-related-order-ids="${escapeHtmlAttribute(relatedOrderIds.join(','))}" data-source-key="${escapeHtmlAttribute(String(item?.group_key || label).trim())}">${isFocused ? '已聚焦资源行' : '定位班组资源行'}</button>
                </div>
            </div>
        `;
    }

    function renderEmployeeAnalyticsCard(item) {
        const label = String(item?.group_label || item?.group_key || '未命名员工');
        const teamText = Array.isArray(item?.team_labels) && item.team_labels.length > 0
            ? item.team_labels.join(' / ')
            : '未关联班组';
        const orderIds = normalizeConflictOrderIds(item?.order_ids || []);
        const isFocused = isResourceFocusActive('employee', item?.group_key, label);
        return `
            <div class="ai-suggestion${isFocused ? ' is-selected' : ''}">
                <div class="analytics-breakdown-head">
                    <h4>${escapeHtml(label)}</h4>
                    <span class="suggestion-chip">${escapeHtml(formatMinutes(item?.occupied_minutes || 0))}</span>
                </div>
                <div class="analytics-breakdown-meta">
                    <span>工单 ${escapeHtml(String(Number(item?.order_count || 0)))}</span>
                    <span>已完成 ${escapeHtml(String(Number(item?.completed_order_count || 0)))}</span>
                    <span>进行中 ${escapeHtml(String(Number(item?.in_progress_count || 0)))}</span>
                    <span>班组 ${escapeHtml(teamText)}</span>
                </div>
                <div class="ai-suggestion-actions">
                    <button class="suggestion-chip" data-action="focus-analytics-resource" data-view-mode="employee" data-order-id="${escapeHtmlAttribute(item?.representative_order_id || '')}" data-resource-type="employee" data-resource-id="${escapeHtmlAttribute(String(item?.group_key || '').trim())}" data-resource-label="${escapeHtmlAttribute(label)}" data-related-order-ids="${escapeHtmlAttribute(orderIds.join(','))}" data-source-key="${escapeHtmlAttribute(String(item?.group_key || label).trim())}">${isFocused ? '已聚焦资源行' : '定位员工资源行'}</button>
                </div>
            </div>
        `;
    }

    function buildEmployeeAnalyticsBreakdownFromTimeline() {
        const timelineItems = Array.isArray(state.timelineData?.items) ? state.timelineData.items : [];
        const grouped = new Map();

        const ensureGroup = (userId, label, teamLabel, orderId, status, durationMinutes) => {
            const key = String(userId || label || '').trim();
            if (!key) {
                return;
            }
            if (!grouped.has(key)) {
                grouped.set(key, {
                    group_key: key,
                    group_label: String(label || userId || '未命名员工').trim(),
                    order_ids: new Set(),
                    order_count: 0,
                    occupied_minutes: 0,
                    completed_order_count: 0,
                    in_progress_count: 0,
                    team_labels: new Set(),
                    representative_order_id: String(orderId || '').trim()
                });
            }
            const entry = grouped.get(key);
            if (orderId && !entry.order_ids.has(orderId)) {
                entry.order_ids.add(orderId);
                entry.order_count += 1;
                if (!entry.representative_order_id) {
                    entry.representative_order_id = String(orderId).trim();
                }
                if (status === 'completed') {
                    entry.completed_order_count += 1;
                }
                if (status === 'in_progress') {
                    entry.in_progress_count += 1;
                }
            }
            entry.occupied_minutes += durationMinutes;
            if (teamLabel) {
                entry.team_labels.add(String(teamLabel).trim());
            }
        };

        for (const item of timelineItems) {
            if (!item || item.is_flight_summary) {
                continue;
            }
            const orderId = String(item.order_id || '').trim();
            const status = String(item.status || '').trim();
            const durationMinutes = Math.max(0, toMs(item.end_time) - toMs(item.start_time)) / 60000;
            const directUserId = String(item.individual_user_id || '').trim();
            const directLabel = String(item.individual_username || '').trim();
            const teamLabel = String(item.team_name || '').trim();

            if (directUserId || directLabel) {
                ensureGroup(directUserId || directLabel, directLabel || directUserId, teamLabel, orderId, status, durationMinutes);
            }

            const members = Array.isArray(item.members) ? item.members : [];
            for (const member of members) {
                const userId = normalizeTimelineMemberUserId(member);
                const label = normalizeTimelineMemberName(member);
                ensureGroup(userId || label, label, teamLabel, orderId, status, durationMinutes);
            }
        }

        return Array.from(grouped.values())
            .map((item) => ({
                ...item,
                order_ids: Array.from(item.order_ids),
                team_labels: Array.from(item.team_labels),
                occupied_minutes: Number(item.occupied_minutes || 0)
            }))
            .sort((left, right) => {
                const orderGap = Number(right.order_count || 0) - Number(left.order_count || 0);
                if (orderGap !== 0) {
                    return orderGap;
                }
                const durationGap = Number(right.occupied_minutes || 0) - Number(left.occupied_minutes || 0);
                if (durationGap !== 0) {
                    return durationGap;
                }
                return String(left.group_label || '').localeCompare(String(right.group_label || ''), 'zh-CN');
            });
    }

    function collectOrderIdsForTeamResource(groupKey, groupLabel) {
        const items = Array.isArray(state.timelineData?.items) ? state.timelineData.items : [];
        const normalizedKey = String(groupKey || '').trim();
        const normalizedLabel = String(groupLabel || '').trim();
        return Array.from(new Set(items
            .filter((item) => {
                if (!item || item.is_flight_summary) {
                    return false;
                }
                const teamId = String(item.team_id || '').trim();
                const teamName = String(item.team_name || '').trim();
                if (normalizedKey && teamId) {
                    return teamId === normalizedKey;
                }
                return Boolean(normalizedLabel) && teamName === normalizedLabel;
            })
            .map((item) => String(item.order_id || '').trim())
            .filter(Boolean)));
    }

    function findRepresentativeOrderIdForTeam(groupKey, groupLabel) {
        return collectOrderIdsForTeamResource(groupKey, groupLabel)[0] || '';
    }

    async function switchAnalyticsResourceView(viewMode, orderId, resourceOptions = {}) {
        const normalizedView = viewMode === 'employee' ? 'employee' : 'team';
        const resourceType = normalizeResourceType(resourceOptions.resourceType || (normalizedView === 'employee' ? 'employee' : 'team'));
        if (!resourceType) {
            if (orderId) {
                setImpactedOrders([orderId], { render: true });
                await focusOrder(orderId);
            } else {
                showToast(`已切换到${VIEW_LABELS[normalizedView] || normalizedView}`);
            }
            return false;
        }
        return await applyResourceFocus({
            resource_type: resourceType,
            resource_id: resourceOptions.resourceId,
            resource_label: resourceOptions.resourceLabel,
            target_view_mode: normalizedView,
            related_order_ids: resourceOptions.relatedOrderIds || [],
            source_panel: 'analytics',
            source_key: resourceOptions.sourceKey || resourceOptions.resourceId || resourceOptions.resourceLabel || orderId || normalizedView
        }, {
            preferredOrderId: orderId
        });
    }

    function renderAnalyticsTrendChart() {
        const chartDom = document.getElementById('analyticsTrendChart');
        const hint = document.getElementById('analyticsTrendHint');
        if (!chartDom) {
            return;
        }

        const trendItems = Array.isArray(state.analyticsTrend) ? state.analyticsTrend : [];
        if (typeof echarts === 'undefined') {
            if (state.analyticsTrendChart) {
                try {
                    state.analyticsTrendChart.dispose();
                } catch (_) {
                    // ignore dispose errors
                }
                state.analyticsTrendChart = null;
            }
            chartDom.innerHTML = '<div class="empty-state" style="height:100%;">ECharts 未加载</div>';
            if (hint) {
                hint.textContent = '趋势图不可用：ECharts 未加载。';
            }
            return;
        }

        if (trendItems.length === 0 && !state.analyticsLoading) {
            if (state.analyticsTrendChart) {
                try {
                    state.analyticsTrendChart.dispose();
                } catch (_) {
                    // ignore dispose errors
                }
                state.analyticsTrendChart = null;
            }
            chartDom.innerHTML = '<div class="empty-state" style="height:100%;">当前时间窗暂无趋势数据</div>';
            if (hint) {
                hint.textContent = '按当前时间窗小时粒度展示工单、冲突与响应趋势。';
            }
            return;
        }

        chartDom.innerHTML = '';
        const existing = echarts.getInstanceByDom(chartDom);
        state.analyticsTrendChart = existing || echarts.init(chartDom, null, { renderer: 'canvas' });

        const labels = trendItems.map((item) => formatAxisTime(item?.bucket_start));
        const orderCounts = trendItems.map((item) => Number(item?.order_count || 0));
        const conflictCounts = trendItems.map((item) => Number(item?.conflict_order_count || 0));
        const responseMinutes = trendItems.map((item) => Number(item?.avg_dispatch_response_minutes || 0));

        state.analyticsTrendChart.setOption({
            animation: false,
            grid: {
                left: 40,
                right: 44,
                top: 18,
                bottom: 28
            },
            tooltip: {
                trigger: 'axis',
                backgroundColor: 'rgba(255,255,255,0.98)',
                borderColor: 'rgba(15, 23, 42, 0.12)',
                textStyle: {
                    color: '#253243'
                }
            },
            legend: {
                top: 0,
                right: 0,
                itemWidth: 10,
                itemHeight: 10,
                textStyle: {
                    color: '#5f7082',
                    fontSize: 11
                },
                data: ['工单量', '冲突工单', '平均响应']
            },
            xAxis: {
                type: 'category',
                data: labels,
                boundaryGap: false,
                axisLine: {
                    lineStyle: {
                        color: 'rgba(15, 23, 42, 0.16)'
                    }
                },
                axisLabel: {
                    color: '#5f7082',
                    fontSize: 11
                }
            },
            yAxis: [
                {
                    type: 'value',
                    name: '单量',
                    minInterval: 1,
                    axisLine: {
                        show: false
                    },
                    axisLabel: {
                        color: '#5f7082',
                        fontSize: 11
                    },
                    splitLine: {
                        lineStyle: {
                            color: 'rgba(15, 23, 42, 0.06)'
                        }
                    }
                },
                {
                    type: 'value',
                    name: '分钟',
                    axisLine: {
                        show: false
                    },
                    axisLabel: {
                        color: '#5f7082',
                        fontSize: 11
                    },
                    splitLine: {
                        show: false
                    }
                }
            ],
            series: [
                {
                    name: '工单量',
                    type: 'line',
                    smooth: true,
                    symbol: 'circle',
                    symbolSize: 6,
                    data: orderCounts,
                    lineStyle: {
                        width: 2,
                        color: '#007AFF'
                    },
                    itemStyle: {
                        color: '#007AFF'
                    },
                    areaStyle: {
                        color: 'rgba(0, 122, 255, 0.08)'
                    }
                },
                {
                    name: '冲突工单',
                    type: 'line',
                    smooth: true,
                    symbol: 'circle',
                    symbolSize: 6,
                    data: conflictCounts,
                    lineStyle: {
                        width: 2,
                        color: '#FF9500'
                    },
                    itemStyle: {
                        color: '#FF9500'
                    }
                },
                {
                    name: '平均响应',
                    type: 'line',
                    smooth: true,
                    yAxisIndex: 1,
                    symbol: 'circle',
                    symbolSize: 6,
                    data: responseMinutes,
                    lineStyle: {
                        width: 2,
                        color: '#34C759'
                    },
                    itemStyle: {
                        color: '#34C759'
                    }
                }
            ]
        });

        if (hint) {
            hint.textContent = trendItems.length > 0
                ? `已按小时展示 ${trendItems.length} 个时间桶的工单、冲突与响应趋势。`
                : '按当前时间窗小时粒度展示工单、冲突与响应趋势。';
        }
    }

    function setMetric(id, value) {
        const el = document.getElementById(id);
        if (el) {
            el.textContent = value;
        }
    }

    async function focusTimelineItem(focusItemId, orderId, options = {}) {
        const openDetail = options.openDetail !== false;
        const items = (state.timelineData && state.timelineData.items) || [];
        let target = items.find((item) => item.id === focusItemId);

        if (!target && orderId) {
            target = items.find((item) => item.order_id === orderId);
        }

        if (!target) {
            showToast('未找到对应任务，可能已不在当前时间窗');
            return;
        }

        state.highlightedItemId = target.id;
        state.highlightedOrderId = target.order_id || null;

        const startMs = toMs(target.start_time);
        const endMs = toMs(target.end_time);
        state.windowStartMs = startMs - 20 * 60 * 1000;
        state.windowEndMs = endMs + 60 * 60 * 1000;

        renderWindowLabel();
        renderChart();
        renderViewModeHint();

        if (!openDetail) {
            return;
        }

        if (target.is_flight_summary) {
            await openFlightSummaryDetail(target);
        } else if (target.order_id) {
            await openOrderDetail(target.order_id);
        }
    }

    async function focusOrder(orderId) {
        if (!orderId) {
            return;
        }
        const items = (state.timelineData && state.timelineData.items) || [];
        const target = items.find((item) => item.order_id === orderId);
        if (target) {
            await focusTimelineItem(target.id, orderId);
        }
    }

    async function openOrderDetail(orderId) {
        if (!orderId) {
            return;
        }

        try {
            const dataLayer = getDispatchBoardData() || await ensureDispatchBoardDataModule();
            const order = await dataLayer.fetchOrder(orderId);
            state.detailMode = 'order';
            state.detailOrder = order;
            state.detailFlightSummary = null;
            state.detailFlightOrders = [];
            state.detailSafetyChecklist = null;
            state.detailSafetyLoading = false;
            state.detailSafetyError = '';
            state.detailSafetyGateHint = null;
            state.detailSafetySubmittingKey = '';

            if (isStatusBatchActive()) {
                const batchIndex = state.statusPanelBatchOrderIds.indexOf(String(orderId));
                if (batchIndex >= 0) {
                    state.statusPanelBatchIndex = batchIndex;
                }
            }

            renderDetailDrawer();
            openDrawer('detailDrawer');
            await loadOrderSafetyChecklist(orderId, { silent: true });
        } catch (error) {
            console.error('加载派工详情失败:', error);
            showToast(error.message || '加载派工详情失败');
        }
    }

    async function openFlightSummaryDetail(summaryItem) {
        if (!summaryItem || !summaryItem.flight_id) {
            return;
        }

        state.detailMode = 'flight';
        state.detailFlightSummary = summaryItem;
        state.detailOrder = null;
        state.detailFlightOrders = [];
        renderDetailDrawer();
        openDrawer('detailDrawer');

        try {
            const dataLayer = getDispatchBoardData() || await ensureDispatchBoardDataModule();
            state.detailFlightOrders = await dataLayer.fetchOrdersByFlight(summaryItem.flight_id);
            renderDetailDrawer();
        } catch (error) {
            console.error('加载航班派工明细失败:', error);
            showToast(error.message || '加载航班派工明细失败');
        }
    }

    async function loadOrderSafetyChecklist(orderId, options = {}) {
        if (!orderId) {
            return;
        }

        const silent = options.silent === true;
        state.detailSafetyLoading = true;
        if (!silent) {
            renderDetailDrawer();
        }

        try {
            const dataLayer = getDispatchBoardData() || await ensureDispatchBoardDataModule();
            const payload = await dataLayer.fetchOrderSafetyChecklist(orderId);
            state.detailSafetyChecklist = payload;
            state.detailSafetyError = '';
            if (payload && payload.ready) {
                state.detailSafetyGateHint = null;
            }
        } catch (error) {
            state.detailSafetyChecklist = null;
            state.detailSafetyError = error.message || '加载安全清单失败';
        } finally {
            state.detailSafetyLoading = false;
            renderDetailDrawer();
        }
    }

    async function completeDispatchOrder(orderId) {
        if (!orderId) {
            return;
        }

        const actualEndInput = window.prompt('请输入真实结束时间（yyyy-MM-dd HH:mm）', formatLocalDateTimeInput(new Date()));
        if (actualEndInput === null) {
            return;
        }
        const actualEndTime = parseLocalDateTimeInput(actualEndInput);
        if (!actualEndTime) {
            showToast('时间格式错误，请使用 yyyy-MM-dd HH:mm');
            return;
        }

        const promptResult = window.prompt('请输入完工备注（可选）', '');
        if (promptResult === null) {
            return;
        }

        const noteInput = promptResult;
        const payload = {
            actual_end_time: actualEndTime,
            completion_notes: noteInput.trim() || null
        };

        try {
            await apiCall(`/api/v2/dispatch-orders/${encodeURIComponent(orderId)}/complete`, {
                method: 'POST',
                body: JSON.stringify(payload)
            });
            state.detailSafetyGateHint = null;
            showToast('派工单已完成');
            await refreshTimeline();
            await openOrderDetail(orderId);
        } catch (error) {
            const gateHint = error && typeof error === 'object' ? error.detailPayload : null;
            if (gateHint && typeof gateHint === 'object') {
                state.detailSafetyGateHint = gateHint;
                renderDetailDrawer();
                showToast(gateHint.message || error.message || '完工失败');
                return;
            }

            showToast(error.message || '完工失败');
        }
    }

    async function reportEstimatedCompletion(orderId) {
        if (!orderId) {
            return;
        }

        const etaInput = window.prompt('请输入预计完成时间（yyyy-MM-dd HH:mm）', formatLocalDateTimeInput(new Date(Date.now() + 30 * 60 * 1000)));
        if (etaInput === null) {
            return;
        }
        const estimatedCompletionTime = parseLocalDateTimeInput(etaInput);
        if (!estimatedCompletionTime) {
            showToast('时间格式错误，请使用 yyyy-MM-dd HH:mm');
            return;
        }

        const noteInput = window.prompt('请输入回报说明（可选）', '') || '';
        const payload = {
            estimated_completion_time: estimatedCompletionTime,
            note: noteInput.trim() || null
        };

        try {
            const response = await apiCall(`/api/v2/dispatch-orders/${encodeURIComponent(orderId)}/eta-report`, {
                method: 'POST',
                body: JSON.stringify(payload)
            });
            state.replanPreview = Array.isArray(response?.suggestions) ? response.suggestions : [];
            renderReplanHint();
            showToast(response?.has_conflicts ? '已回报预计完成时间，并生成重排建议' : '已回报预计完成时间');
            await refreshTimeline();
            await openOrderDetail(orderId);
        } catch (error) {
            showToast(error.message || '预计完成时间回报失败');
        }
    }

    async function submitSafetyChecklistItem(orderId, itemCode, result) {
        if (!orderId || !itemCode || !result) {
            return;
        }

        const nextResult = String(result).trim().toLowerCase();
        if (!['pass', 'fail', 'na'].includes(nextResult)) {
            return;
        }

        if (state.detailSafetySubmittingKey) {
            return;
        }

        let note = null;
        if (nextResult === 'fail') {
            const promptResult = window.prompt('请输入不通过说明（可选）', '');
            if (promptResult === null) {
                return;
            }
            note = promptResult;
        } else if (nextResult === 'na') {
            const promptResult = window.prompt('请输入不适用说明（可选）', '');
            if (promptResult === null) {
                return;
            }
            note = promptResult;
        }

        state.detailSafetySubmittingKey = `${itemCode}:${nextResult}`;
        renderDetailDrawer();

        try {
            await apiCall(
                `/api/v2/dispatch-orders/${encodeURIComponent(orderId)}/safety-checklist/items/${encodeURIComponent(itemCode)}`,
                {
                    method: 'POST',
                    body: JSON.stringify({
                        result: nextResult,
                        note: note && note.trim() ? note.trim() : null
                    })
                }
            );
            state.detailSafetyGateHint = null;
            showToast(`清单项已更新：${translateChecklistResult(nextResult)}`);
            await loadOrderSafetyChecklist(orderId, { silent: true });
        } catch (error) {
            showToast(error.message || '提交安全清单失败');
        } finally {
            state.detailSafetySubmittingKey = '';
            renderDetailDrawer();
        }
    }

    function renderDetailDrawer() {
        const title = document.getElementById('detailTitle');
        const content = document.getElementById('detailContent');
        const actions = document.getElementById('detailActions');

        if (!title || !content || !actions) {
            return;
        }

        if (state.detailMode === 'flight' && state.detailFlightSummary) {
            const summary = state.detailFlightSummary;
            title.textContent = `${summary.flight_no || summary.flight_id} 全流程`;

            const relatedOrders = Array.isArray(state.detailFlightOrders) ? state.detailFlightOrders : [];
            const orderListHtml = relatedOrders.length > 0
                ? `
                    <div class="detail-order-list">
                        ${relatedOrders.map((order) => `
                            <div class="detail-order-item" data-order-id="${escapeHtmlAttribute(order.id || '')}">
                                <div style="font-weight:600;">${escapeHtml(order.task_type_name || order.task_type || '-')}</div>
                                <div class="detail-meta-row" style="margin:6px 0 0;">${renderStatusBadge(order.status)}${normalizePublicationState(order.publication_state) ? renderPublicationBadge(order.publication_state) : ''}</div>
                                <div style="margin-top:4px;color:${CHART_THEME.detailSubText};">${escapeHtml(formatDateTime(order.planned_start_time))}</div>
                            </div>
                        `).join('')}
                    </div>
                `
                : '<div class="empty-state" style="height:auto;padding:10px 0;">正在加载航班派工明细...</div>';

            content.innerHTML = `
                <div class="section-title">航班总览</div>
                ${renderOrderSemanticMeta(summary)}
                ${renderKvRow('航班号', summary.flight_no || '-')}
                ${renderKvRow('航班ID', summary.flight_id || '-')}
                ${renderKvRow('时间范围', `${formatDateTime(summary.start_time)} - ${formatDateTime(summary.end_time)}`)}
                ${renderKvRow('覆盖派工', String((summary.related_order_ids || []).length))}
                ${renderKvHtmlRow('当前主状态', renderStatusBadge(summary.status))}

                <div class="section-title">状态分布</div>
                ${renderRelatedOrderBreakdown(summary.related_orders || [])}
                <div class="section-title">派工明细</div>
                ${orderListHtml}
            `;

            actions.innerHTML = `
                <button class="action-btn" data-action="open-flight-chat">进入航班群聊</button>
                <button class="action-btn" data-action="close">关闭</button>
            `;
            return;
        }

        if (state.detailMode === 'order' && state.detailOrder) {
            const order = state.detailOrder;
            title.textContent = `派工单 ${order.id || '-'}`;

            const taskCrewMembers = Array.isArray(order?.task_crew?.members) ? order.task_crew.members : [];
            const memberNames = (taskCrewMembers.length > 0 ? taskCrewMembers : (order.members || []))
                .map((member) => {
                    const username = String(member?.username || member?.user_id || '').trim();
                    const slotCode = String(member?.slot_code || '').trim();
                    const levelCode = String(member?.qualification_level_code || '').trim();
                    const suffix = [slotCode, levelCode].filter(Boolean).join(' / ');
                    if (!username && !suffix) {
                        return '';
                    }
                    return suffix ? `${username || '-'} (${suffix})` : username;
                })
                .filter(Boolean)
                .join(' / ') || '-';
            const qualificationGapText = (Array.isArray(order?.qualification_gap) ? order.qualification_gap : [])
                .map((gap) => {
                    const slotCode = String(gap?.slot_code || '').trim();
                    const qualificationCode = String(gap?.qualification_code || '').trim();
                    const minLevelCode = String(gap?.min_level_code || '').trim();
                    return [slotCode, qualificationCode, minLevelCode].filter(Boolean).join(' / ');
                })
                .filter(Boolean)
                .join(' ; ') || '-';
            const equipmentCodes = (order.equipment_codes || []).join(' / ') || '-';
            const canOperateSafetyChecklist = canOperateOrder(order);
            const canSubmitSafetyChecklist = canOperateSafetyChecklist
                && ['assigned', 'in_progress'].includes(order.status);
            const canCompleteOrder = canOperateSafetyChecklist && order.status === 'in_progress';
            const canReportIssue = canOperateSafetyChecklist && ['assigned', 'in_progress'].includes(order.status);
            const canPublishOrder = canManageDispatchOrder(order);
            const completionReady = !state.detailSafetyChecklist
                || !state.detailSafetyChecklist.enforced
                || Boolean(state.detailSafetyChecklist.ready)
                || Boolean(state.detailSafetyChecklist.can_soft_complete);
            const completionButtonText = state.detailSafetyChecklist
                && state.detailSafetyChecklist.enforced
                && !state.detailSafetyChecklist.ready
                && state.detailSafetyChecklist.can_soft_complete
                ? '软闭环完工'
                : '提交完工';
            const replanContextHtml = renderDetailReplanContext(order);

            content.innerHTML = `
                <div class="section-title">任务信息</div>
                ${renderOrderSemanticMeta(order)}
                <div class="kv-row"><span class="kv-key">来源标识</span><span class="kv-value">${renderOrderOriginBadge(order)}</span></div>
                ${renderKvRow('航班', order.flight_id || '-')}
                ${renderKvRow('作业类型', order.task_type_name || order.task_type || '-')}
                ${renderKvRow('机位', order.stand_code || order.stand_id || '-')}
                ${renderKvRow('登机口', order.gate || '-')}
                ${renderKvHtmlRow('状态', renderStatusBadge(order.status))}
                ${renderKvRow('来源', order.origin_label || order.source || '-')}
                ${renderKvRow('派工方式', order.dispatch_type === 'auto' ? '自动' : '手动')}
                ${renderKvHtmlRow('发布状态', renderPublicationBadge(order.publication_state))}
                ${isLockedOrder(order) ? renderKvHtmlRow('优化约束', renderLockBadge(order.lock_level)) : ''}
                ${String(order.conflict_reason || '').trim() ? renderKvRow('冲突原因', String(order.conflict_reason).trim()) : ''}
                ${!String(order.conflict_reason || '').trim() && String(order.availability_reason || '').trim() ? renderKvRow('资源约束', String(order.availability_reason).trim()) : ''}

                <div class="section-title">时间信息</div>
                ${renderKvRow('计划开始', formatDateTime(order.planned_start_time))}
                ${renderKvRow('计划结束', formatDateTime(order.planned_end_time))}
                ${renderKvRow('实际开始', formatDateTime(order.actual_start_time))}
                ${renderKvRow('实际结束', formatDateTime(order.actual_end_time))}
                ${renderKvRow('预计完成', formatDateTime(order.estimated_completion_time))}
                ${renderKvRow('有效结束', formatDateTime(order.effective_end_time || order.actual_end_time || order.planned_end_time))}

                <div class="section-title">资源信息</div>
                ${renderKvRow('归属班组', order.team_name || '-')}
                ${renderKvRow('负责人', order.individual_username || '-')}
                ${renderKvRow('执行编组', memberNames)}
                ${renderKvRow('资质缺口', qualificationGapText)}
                ${renderKvRow('设备', equipmentCodes)}

                ${replanContextHtml}
                ${renderReceiptSummaryBlock(order.notification_receipt_summary)}

                ${renderSafetyChecklistSection(order, {
                    editable: canSubmitSafetyChecklist
                })}
            `;

            const canCancel = ['pending', 'assigned', 'in_progress'].includes(order.status);
            const batchActive = isStatusBatchActive();
            const batchTotal = batchActive ? state.statusPanelBatchOrderIds.length : 0;
            const batchIndex = batchActive ? (state.statusPanelBatchIndex + 1) : 0;
            actions.innerHTML = `
                <button class="action-btn" data-action="close">关闭</button>
                ${batchActive
                    ? `<button class="action-btn" data-action="batch-prev" ${state.statusPanelBatchIndex <= 0 ? 'disabled' : ''}>上一条</button>
                       <button class="action-btn" data-action="batch-next" ${state.statusPanelBatchIndex >= batchTotal - 1 ? 'disabled' : ''}>下一条</button>
                       <span class="detail-batch-badge">批量处理 ${batchIndex}/${batchTotal}</span>
                       <button class="action-btn" data-action="batch-stop">结束批处理</button>`
                    : ''}
                <button class="action-btn" data-action="locate">定位</button>
                <button class="action-btn" data-action="open-flight-chat">进入航班群聊</button>
                <button class="action-btn" data-action="govern-conflict">冲突治理</button>
                <button class="action-btn" data-action="reassign">重新分配</button>
                ${canPublishOrder ? '<button class="action-btn primary" data-action="publish-order">正式发布</button>' : ''}
                <button class="action-btn" data-action="refresh-safety-checklist">刷新清单</button>
                ${canCancel ? '<button class="action-btn" data-action="cancel">取消派工</button>' : ''}
                ${canReportIssue ? '<button class="action-btn" data-action="report-issue">异常首报</button>' : ''}
                ${canCompleteOrder ? '<button class="action-btn" data-action="eta-report">回报预计完成</button>' : ''}
                ${canCompleteOrder
                    ? `<button class="action-btn primary" data-action="complete-order" ${completionReady ? '' : 'disabled'}>${completionReady ? completionButtonText : '待关键项就绪'}</button>`
                    : ''}
            `;
            return;
        }

        title.textContent = '详情';
        if (!renderUnifiedState(content, 'empty', '暂无详情数据')) {
            content.innerHTML = '<div class="empty-state">暂无详情数据</div>';
        }
        actions.innerHTML = '<button class="action-btn" data-action="close">关闭</button>';
    }

    function renderKvRow(key, value) {
        return `
            <div class="kv-row">
                <span class="kv-key">${escapeHtml(key)}</span>
                <span class="kv-value">${escapeHtml(value || '-')}</span>
            </div>
        `;
    }

    function renderKvHtmlRow(key, html) {
        return `
            <div class="kv-row">
                <span class="kv-key">${escapeHtml(key)}</span>
                <span class="kv-value">${html || '-'}</span>
            </div>
        `;
    }

    function getStatusCssClass(status) {
        return String(status || 'pending').trim().toLowerCase().replace(/_/g, '-');
    }

    function renderStatusBadge(status, labelOverride = '') {
        const normalized = String(status || 'pending').trim().toLowerCase() || 'pending';
        const cssClass = getStatusCssClass(normalized);
        const symbol = STATUS_SYMBOLS[normalized] || '•';
        const label = labelOverride || STATUS_LABELS[normalized] || normalized || '-';
        return `<span class="status-badge ${escapeHtmlAttribute(cssClass)}"><span class="status-symbol ${escapeHtmlAttribute(cssClass)}" aria-hidden="true">${escapeHtml(symbol)}</span>${escapeHtml(label)}</span>`;
    }

    function renderSemanticPill(label, className = '') {
        const classes = ['semantic-pill'];
        if (className) {
            classes.push(className);
        }
        return `<span class="${escapeHtmlAttribute(classes.join(' '))}">${escapeHtml(label)}</span>`;
    }

    function renderPublicationBadge(value) {
        const normalized = normalizePublicationState(value);
        if (!normalized) {
            return '';
        }
        if (normalized === 'prepublished') {
            return renderSemanticPill('预发布草稿', 'is-draft');
        }
        if (normalized === 'published') {
            return renderSemanticPill('正式发布');
        }
        if (normalized === 'cancelled') {
            return renderSemanticPill('已取消', 'is-alert');
        }
        return renderSemanticPill(renderPublicationStateLabel(value));
    }

    function renderLockBadge(value) {
        if (!isLockedOrder({ lock_level: value })) {
            return '';
        }
        return renderSemanticPill(renderLockLevelLabel(value), 'is-lock');
    }

    function renderAlertPills(raw) {
        const parts = [];
        if (hasQualificationGap(raw)) {
            parts.push(renderSemanticPill('资质缺口', 'is-alert'));
        }
        if (String(raw?.conflict_reason || '').trim()) {
            parts.push(renderSemanticPill('冲突待治理', 'is-alert'));
        } else if (String(raw?.availability_reason || '').trim()) {
            parts.push(renderSemanticPill('资源受限', 'is-alert'));
        }
        return parts;
    }

    function renderOrderSemanticMeta(raw) {
        const parts = [renderStatusBadge(raw?.status)];
        if (!raw?.is_flight_summary) {
            const publicationBadge = renderPublicationBadge(raw?.publication_state);
            if (publicationBadge) {
                parts.push(publicationBadge);
            }
        }
        const lockBadge = renderLockBadge(raw?.lock_level);
        if (lockBadge) {
            parts.push(lockBadge);
        }
        parts.push(...renderAlertPills(raw));
        return `<div class="detail-meta-row">${parts.join('')}</div>`;
    }

    function renderRelatedOrderBreakdown(relatedOrders) {
        const counts = summarizeRelatedStatuses(relatedOrders);
        const segments = STATUS_ORDER
            .filter((statusKey) => Number(counts[statusKey] || 0) > 0)
            .map((statusKey) => `${renderStatusBadge(statusKey, `${STATUS_LABELS[statusKey] || statusKey} ${counts[statusKey]}`)}`);
        return segments.length > 0
            ? `<div class="detail-meta-row">${segments.join('')}</div>`
            : '<div class="empty-state" style="height:auto;padding:6px 0;">暂无状态分布</div>';
    }

    function renderOrderOriginBadge(order) {
        const originType = String(order?.origin_type || order?.source || 'manual').trim().toLowerCase();
        const label = originType === 'workflow' ? '流程派工' : '人工派工';
        const background = originType === 'workflow' ? 'rgba(79, 70, 229, 0.14)' : 'rgba(217, 119, 6, 0.14)';
        const color = originType === 'workflow' ? '#4338ca' : '#b45309';
        return `<span style="display:inline-flex;align-items:center;padding:2px 8px;border-radius:999px;font-size:12px;font-weight:600;background:${background};color:${color};">${escapeHtml(label)}</span>`;
    }

    function renderReceiptSummaryBlock(summary) {
        const payload = summary && typeof summary === 'object' ? summary : {};
        const total = Number(payload.total_count || 0);
        if (total <= 0) {
            return '';
        }
        return `
            <div class="section-title">通知回执</div>
            ${renderKvRow('总数', String(total))}
            ${renderKvRow('待确认', String(Number(payload.pending_count || 0)))}
            ${renderKvRow('已确认', String(Number(payload.acknowledged_count || 0)))}
            ${renderKvRow('已拒绝', String(Number(payload.rejected_count || 0)))}
        `;
    }

    function formatLocalDateTimeInput(value) {
        const date = value instanceof Date ? value : new Date(value);
        if (Number.isNaN(date.getTime())) {
            return '';
        }
        const pad = (num) => String(num).padStart(2, '0');
        return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
    }

    function parseLocalDateTimeInput(value) {
        const text = String(value || '').trim();
        if (!text) {
            return null;
        }
        const matched = text.match(/^(\d{4})-(\d{2})-(\d{2})\s+(\d{2}):(\d{2})$/);
        if (!matched) {
            return null;
        }
        const [, year, month, day, hour, minute] = matched;
        const date = new Date(Number(year), Number(month) - 1, Number(day), Number(hour), Number(minute), 0, 0);
        if (Number.isNaN(date.getTime())) {
            return null;
        }
        return date.toISOString();
    }

    function getCurrentUserId() {
        const user = state.user || {};
        const userId = user.sub || user.id || user.user_id || '';
        return String(userId || '').trim();
    }

    function hasDispatchManagePermission() {
        if (state.isAdmin) {
            return true;
        }
        const permissions = Array.isArray(state.user?.permissions) ? state.user.permissions : [];
        return permissions.includes('dispatch:manage');
    }

    function canManageDispatchOrder(order) {
        if (!order || !hasDispatchManagePermission()) {
            return false;
        }
        return String(order.publication_state || '').trim().toLowerCase() === 'prepublished';
    }

    function isDraftOrder(raw) {
        if (!raw) return false;
        const pubState = normalizePublicationState(raw.publication_state);
        const status = String(raw.status || '').trim().toLowerCase();
        const taskCrew = raw.task_crew;
        const hasNoCrew = !taskCrew || (typeof taskCrew === 'object' && (!taskCrew.members || taskCrew.members.length === 0));
        return pubState === 'prepublished' && status === 'pending' && hasNoCrew;
    }

    function adjustColorAlpha(hexColor, alpha) {
        if (!hexColor || typeof hexColor !== 'string') return hexColor;
        const hex = hexColor.replace('#', '');
        if (hex.length < 6) return hexColor;
        const r = parseInt(hex.substring(0, 2), 16);
        const g = parseInt(hex.substring(2, 4), 16);
        const b = parseInt(hex.substring(4, 6), 16);
        return `rgba(${r}, ${g}, ${b}, ${alpha})`;
    }

    function toggleDraftSelection(orderId) {
        if (!orderId) return;
        if (state.draftSelectedOrderIds.has(orderId)) {
            state.draftSelectedOrderIds.delete(orderId);
        } else {
            state.draftSelectedOrderIds.add(orderId);
        }
        renderDraftActionBar();
        if (state.chart) {
            state.chart.setOption({}, true);
            renderChart();
        }
    }

    function clearDraftSelection() {
        state.draftSelectedOrderIds.clear();
        renderDraftActionBar();
        if (state.chart) {
            renderChart();
        }
    }

    function renderDraftActionBar() {
        let bar = document.getElementById('draftActionBar');
        const count = state.draftSelectedOrderIds.size;
        if (count === 0) {
            if (bar) bar.style.display = 'none';
            return;
        }
        if (!bar) {
            bar = document.createElement('div');
            bar.id = 'draftActionBar';
            bar.style.cssText = `
                position: fixed;
                bottom: 24px;
                left: 50%;
                transform: translateX(-50%);
                z-index: 9998;
                display: inline-flex;
                align-items: center;
                gap: 10px;
                padding: 10px 20px;
                border-radius: 14px;
                border: 1px solid rgba(0, 122, 255, 0.28);
                background: rgba(255, 255, 255, 0.96);
                backdrop-filter: blur(14px);
                box-shadow: 0 12px 28px rgba(0, 30, 70, 0.18);
                font-size: 13px;
                font-weight: 600;
                color: #1d3c5a;
            `;
            document.body.appendChild(bar);
        }
        bar.style.display = 'inline-flex';
        const optimizingLabel = state.draftOptimizing ? ' (求解中...)' : '';
        bar.innerHTML = `
            <span>已选择 <strong style="color:#007AFF">${count}</strong> 个草稿工单${optimizingLabel}</span>
            <button id="draftOptimizeBtn" style="
                height: 34px; padding: 0 16px; border-radius: 9px;
                border: 1px solid rgba(0, 122, 255, 0.5);
                background: #007AFF; color: #fff;
                font-size: 13px; font-weight: 650; cursor: pointer;
                box-shadow: 0 4px 10px rgba(0, 122, 255, 0.22);
            " ${state.draftOptimizing ? 'disabled' : ''}>
                优化分配
            </button>
            <button id="draftClearBtn" style="
                height: 34px; padding: 0 12px; border-radius: 9px;
                border: 1px solid rgba(15, 23, 42, 0.14);
                background: #fff; color: #42576d;
                font-size: 13px; font-weight: 600; cursor: pointer;
            ">
                清除选择
            </button>
        `;
        const optimizeBtn = document.getElementById('draftOptimizeBtn');
        if (optimizeBtn) {
            optimizeBtn.onclick = () => triggerDraftOptimize();
        }
        const clearBtn = document.getElementById('draftClearBtn');
        if (clearBtn) {
            clearBtn.onclick = () => clearDraftSelection();
        }
    }

    async function triggerDraftOptimize() {
        const selectedIds = Array.from(state.draftSelectedOrderIds);
        if (selectedIds.length === 0) return;

        state.draftOptimizing = true;
        state.draftOptimizeError = '';
        renderDraftActionBar();

        try {
            // 1. Fetch replan snapshot
            const snapshotResponse = await apiCall(buildReplanSnapshotUrl());
            const snapshot = snapshotResponse?.data && typeof snapshotResponse.data === 'object'
                ? snapshotResponse.data
                : snapshotResponse;

            if (!snapshot || !snapshot.snapshot_id) {
                throw new Error('无法获取优化快照');
            }

            // 2. Run WASM solver
            showToast(`正在为 ${selectedIds.length} 个草稿工单优化分配...`);
            const solverResult = await previewReplanViaFrontendWasm(snapshot);

            if (!solverResult) {
                throw new Error('WASM 求解失败');
            }

            // 3. Extract assignments for selected drafts
            const personnelSlots = Array.isArray(solverResult.personnel_slot_assignments)
                ? solverResult.personnel_slot_assignments
                : [];
            const equipmentSlots = Array.isArray(solverResult.equipment_slot_assignments)
                ? solverResult.equipment_slot_assignments
                : [];

            // Build assignments keyed by order_id
            const assignmentsByOrder = {};
            for (const slot of personnelSlots) {
                const orderId = String(slot.order_id || '').trim();
                if (!orderId || !selectedIds.includes(orderId)) continue;
                if (!assignmentsByOrder[orderId]) {
                    assignmentsByOrder[orderId] = { order_id: orderId, task_crew: { members: [] }, equipment_assignment: [] };
                }
                assignmentsByOrder[orderId].task_crew.members.push({
                    user_id: slot.user_id || slot.employee_id,
                    username: slot.username || slot.user_name || '',
                    slot_code: slot.slot_code || '',
                    qualification_code: slot.qualification_code || '',
                    source_team_id: slot.source_team_id || '',
                });
            }
            for (const eqSlot of equipmentSlots) {
                const orderId = String(eqSlot.order_id || '').trim();
                if (!orderId || !selectedIds.includes(orderId)) continue;
                if (!assignmentsByOrder[orderId]) {
                    assignmentsByOrder[orderId] = { order_id: orderId, task_crew: { members: [] }, equipment_assignment: [] };
                }
                assignmentsByOrder[orderId].equipment_assignment.push({
                    equipment_id: eqSlot.equipment_id,
                    equipment_name: eqSlot.equipment_name || '',
                    equipment_type: eqSlot.equipment_type || '',
                });
            }

            // Ensure all selected orders have entries
            for (const id of selectedIds) {
                if (!assignmentsByOrder[id]) {
                    assignmentsByOrder[id] = { order_id: id, task_crew: { members: [] }, equipment_assignment: [] };
                }
            }

            const solverMeta = solverResult.solver_run_metadata || {};

            // 4. Show preview modal
            showDraftPublishModal(assignmentsByOrder, solverMeta);
        } catch (err) {
            state.draftOptimizeError = String(err.message || err || '优化失败');
            showToast(state.draftOptimizeError);
        } finally {
            state.draftOptimizing = false;
            renderDraftActionBar();
        }
    }

    function showDraftPublishModal(assignmentsByOrder, solverMeta) {
        closeDraftPublishModal();
        const overlay = document.createElement('div');
        overlay.id = 'draftPublishOverlay';
        overlay.style.cssText = `
            position: fixed; inset: 0; z-index: 10000;
            background: rgba(0, 10, 30, 0.45);
            display: flex; align-items: center; justify-content: center;
            backdrop-filter: blur(4px);
        `;

        const assignments = Object.values(assignmentsByOrder);
        const totalMembers = assignments.reduce((s, a) => s + (a.task_crew?.members?.length || 0), 0);
        const totalEquipment = assignments.reduce((s, a) => s + (a.equipment_assignment?.length || 0), 0);
        const solverTime = Number(solverMeta.solver_time_ms || 0).toFixed(0);
        const isOptimal = solverMeta.is_optimal !== false;

        const rowsHtml = assignments.map((a, i) => {
            const crewNames = (a.task_crew?.members || []).map(m => m.username || m.user_id || '?').join(', ') || '<em style="color:#8E8E93">无</em>';
            const eqNames = (a.equipment_assignment || []).map(e => e.equipment_name || e.equipment_id || '?').join(', ') || '<em style="color:#8E8E93">无</em>';
            return `
                <tr style="border-bottom:1px solid rgba(15,23,42,0.06);${i % 2 ? 'background:rgba(0,122,255,0.02)' : ''}">
                    <td style="padding:8px 10px;font-size:12px;color:#42576d;font-family:monospace">${a.order_id.substring(0, 8)}…</td>
                    <td style="padding:8px 10px;font-size:12px;color:#1d3c5a">${crewNames}</td>
                    <td style="padding:8px 10px;font-size:12px;color:#1d3c5a">${eqNames}</td>
                </tr>
            `;
        }).join('');

        overlay.innerHTML = `
            <div style="
                background: #fff; border-radius: 16px;
                box-shadow: 0 24px 48px rgba(0,20,50,0.22);
                width: min(640px, 90vw); max-height: 80vh;
                display: flex; flex-direction: column;
                overflow: hidden;
            ">
                <div style="padding:20px 24px 12px; border-bottom:1px solid rgba(15,23,42,0.08)">
                    <h3 style="margin:0; font-size:16px; font-weight:700; color:#0f1f33">
                        🏁 优化分配预览
                    </h3>
                    <div style="margin-top:8px; display:flex; gap:16px; font-size:12px; color:#5f7082">
                        <span>工单 <strong style="color:#007AFF">${assignments.length}</strong></span>
                        <span>人员 <strong style="color:#34C759">${totalMembers}</strong></span>
                        <span>设备 <strong style="color:#FF9500">${totalEquipment}</strong></span>
                        <span>求解 ${solverTime}ms</span>
                        <span>${isOptimal ? '✅ 最优' : '⚠️ 近似'}</span>
                    </div>
                </div>

                <div style="flex:1; overflow-y:auto; padding:12px 24px">
                    <table style="width:100%; border-collapse:collapse">
                        <thead>
                            <tr style="border-bottom:2px solid rgba(15,23,42,0.1)">
                                <th style="padding:8px 10px; text-align:left; font-size:11px; color:#8a97a8; font-weight:600; text-transform:uppercase">工单</th>
                                <th style="padding:8px 10px; text-align:left; font-size:11px; color:#8a97a8; font-weight:600; text-transform:uppercase">人员分配</th>
                                <th style="padding:8px 10px; text-align:left; font-size:11px; color:#8a97a8; font-weight:600; text-transform:uppercase">设备分配</th>
                            </tr>
                        </thead>
                        <tbody>${rowsHtml}</tbody>
                    </table>
                </div>

                <div style="
                    padding:14px 24px; border-top:1px solid rgba(15,23,42,0.08);
                    display:flex; justify-content:flex-end; gap:10px;
                ">
                    <button id="draftPublishCancelBtn" style="
                        height:36px; padding:0 18px; border-radius:10px;
                        border:1px solid rgba(15,23,42,0.14); background:#fff;
                        color:#42576d; font-size:13px; font-weight:600; cursor:pointer;
                    ">取消</button>
                    <button id="draftPublishConfirmBtn" style="
                        height:36px; padding:0 20px; border-radius:10px;
                        border:1px solid rgba(52,199,89,0.5); background:#34C759;
                        color:#fff; font-size:13px; font-weight:700; cursor:pointer;
                        box-shadow:0 4px 12px rgba(52,199,89,0.25);
                    ">确认发布</button>
                </div>
            </div>
        `;

        document.body.appendChild(overlay);

        document.getElementById('draftPublishCancelBtn').onclick = () => closeDraftPublishModal();
        document.getElementById('draftPublishConfirmBtn').onclick = () => publishDraftOrders(assignments);
        overlay.addEventListener('click', (e) => {
            if (e.target === overlay) closeDraftPublishModal();
        });
    }

    async function publishDraftOrders(assignments) {
        const confirmBtn = document.getElementById('draftPublishConfirmBtn');
        if (confirmBtn) {
            confirmBtn.disabled = true;
            confirmBtn.textContent = '发布中...';
        }

        try {
            const result = await apiCall('/api/v2/dispatch-orders/batch-publish-drafts', {
                method: 'POST',
                body: JSON.stringify({ assignments }),
            });
            const data = result?.data && typeof result.data === 'object' ? result.data : result;
            const publishedCount = Number(data?.total || 0);

            closeDraftPublishModal();
            clearDraftSelection();
            showToast(`✅ 已成功发布 ${publishedCount} 个派工单`);

            // Refresh timeline to reflect published orders
            await refreshTimeline();
        } catch (err) {
            showToast(`发布失败：${err.message || '未知错误'}`);
            if (confirmBtn) {
                confirmBtn.disabled = false;
                confirmBtn.textContent = '确认发布';
            }
        }
    }

    function closeDraftPublishModal() {
        const overlay = document.getElementById('draftPublishOverlay');
        if (overlay) overlay.remove();
    }

    function canOperateOrder(order) {
        if (!order) {
            return false;
        }
        if (state.isAdmin) {
            return true;
        }

        const userId = getCurrentUserId();
        if (!userId) {
            return false;
        }

        if (order.individual_user_id && String(order.individual_user_id) === userId) {
            return true;
        }

        const members = Array.isArray(order.members) ? order.members : [];
        return members.some((member) => {
            if (!member || String(member.user_id || '') !== userId) {
                return false;
            }
            return member.is_active !== false;
        });
    }

    function renderPublicationStateLabel(value) {
        const normalized = String(value || '').trim().toLowerCase();
        if (normalized === 'prepublished') {
            return '预发布';
        }
        if (normalized === 'published') {
            return '已发布';
        }
        if (normalized === 'cancelled') {
            return '已取消';
        }
        return value || '-';
    }

    function renderLockLevelLabel(value) {
        const normalized = String(value || '').trim().toLowerCase();
        if (!normalized || normalized === 'optimizable') {
            return '可优化';
        }
        if (normalized === 'locked') {
            return '已锁定';
        }
        if (normalized === 'frozen') {
            return '冻结';
        }
        if (normalized === 'manual') {
            return '人工锁定';
        }
        return value || '-';
    }

    function translateChecklistResult(result) {
        if (result === 'pass') {
            return '通过';
        }
        if (result === 'fail') {
            return '不通过';
        }
        if (result === 'na') {
            return '不适用';
        }
        return '待检查';
    }

    function getChecklistBadgeClass(result) {
        if (result === 'pass') {
            return 'is-pass';
        }
        if (result === 'fail') {
            return 'is-fail';
        }
        if (result === 'na') {
            return 'is-na';
        }
        return 'is-pending';
    }

    function getChecklistLevel(item) {
        return String(item?.level || 'critical').trim().toLowerCase() === 'routine' ? 'routine' : 'critical';
    }

    async function submitRoutineChecklistBatch(orderId) {
        if (!orderId || !state.detailSafetyChecklist || state.detailSafetySubmittingKey) {
            return;
        }

        const checklistItems = Array.isArray(state.detailSafetyChecklist.items) ? state.detailSafetyChecklist.items : [];
        const pendingRoutineItems = checklistItems
            .filter((item) => getChecklistLevel(item) === 'routine')
            .filter((item) => {
                const status = String(item.result || item.status || '').trim().toLowerCase();
                return !status || status === 'pending';
            })
            .map((item) => ({
                item_code: String(item.item_code || '').trim(),
                result: 'pass',
                note: null,
                handled_on_site: false,
            }))
            .filter((item) => item.item_code);

        if (pendingRoutineItems.length <= 0) {
            showToast('常规项已全部确认，无需重复提交');
            return;
        }

        state.detailSafetySubmittingKey = 'batch:routine';
        renderDetailDrawer();
        try {
            await apiCall(
                `/api/v2/dispatch-orders/${encodeURIComponent(orderId)}/safety-checklist/batch-submit`,
                {
                    method: 'POST',
                    body: JSON.stringify({ items: pendingRoutineItems }),
                }
            );
            state.detailSafetyGateHint = null;
            showToast(`已批量确认 ${pendingRoutineItems.length} 项常规检查`);
            await loadOrderSafetyChecklist(orderId, { silent: true });
        } catch (error) {
            showToast(error.message || '批量提交常规检查失败');
        } finally {
            state.detailSafetySubmittingKey = '';
            renderDetailDrawer();
        }
    }

    function ensureScriptLoaded(url, globalName) {
        if (globalName && window[globalName]) {
            return Promise.resolve();
        }
        const existing = document.querySelector(`script[src="${url}"]`);
        if (existing) {
            return new Promise((resolve, reject) => {
                existing.addEventListener('load', () => resolve(), { once: true });
                existing.addEventListener('error', () => reject(new Error(`脚本加载失败：${url}`)), { once: true });
            });
        }
        return new Promise((resolve, reject) => {
            const script = document.createElement('script');
            script.src = url;
            script.async = true;
            script.addEventListener('load', () => resolve(), { once: true });
            script.addEventListener('error', () => reject(new Error(`脚本加载失败：${url}`)), { once: true });
            document.head.appendChild(script);
        });
    }

    function ensureFeedbackComponents() {
        if (window.Toast && window.EmptyError && window.Loading) {
            return Promise.resolve();
        }
        if (state.feedbackComponentsPromise) {
            return state.feedbackComponentsPromise;
        }
        state.feedbackComponentsPromise = Promise.all([
            ensureScriptLoaded(FEEDBACK_COMPONENT_URLS.toast, 'Toast'),
            ensureScriptLoaded(FEEDBACK_COMPONENT_URLS.loading, 'Loading'),
            ensureScriptLoaded(FEEDBACK_COMPONENT_URLS.emptyError, 'EmptyError'),
        ]).finally(() => {
            state.feedbackComponentsPromise = null;
        });
        return state.feedbackComponentsPromise;
    }

    async function showIssueUploadToast(type, message, options = {}) {
        try {
            await ensureFeedbackComponents();
        } catch (_error) {
            // ignore component loading failures and fall back below
        }
        if (window.Toast && typeof window.Toast.show === 'function') {
            window.Toast.show(type, message, options);
            return;
        }
        showToast(message);
    }

    function createAbortError(message) {
        const error = new Error(message || '已取消上传');
        error.name = 'AbortError';
        return error;
    }

    /**
     * Compress an image file using Canvas API.
     * Resizes to max 1920px width/height, outputs JPEG at 80% quality.
     * Returns a new File object with the compressed data.
     * Non-image files are returned unchanged.
     */
    async function compressImageFile(file) {
        if (!file || !file.type.startsWith('image/')) {
            return { file, compressed: false, originalSize: file.size, compressedSize: file.size };
        }
        const originalSize = file.size;
        try {
            const img = await new Promise((resolve, reject) => {
                const image = new Image();
                image.onload = () => resolve(image);
                image.onerror = () => reject(new Error('图片加载失败'));
                image.src = URL.createObjectURL(file);
            });
            const MAX_DIMENSION = 1920;
            let width = img.naturalWidth;
            let height = img.naturalHeight;
            if (width > MAX_DIMENSION || height > MAX_DIMENSION) {
                if (width >= height) {
                    height = Math.round((height / width) * MAX_DIMENSION);
                    width = MAX_DIMENSION;
                } else {
                    width = Math.round((width / height) * MAX_DIMENSION);
                    height = MAX_DIMENSION;
                }
            }
            const canvas = document.createElement('canvas');
            canvas.width = width;
            canvas.height = height;
            const ctx = canvas.getContext('2d');
            ctx.drawImage(img, 0, 0, width, height);
            URL.revokeObjectURL(img.src);

            const blob = await new Promise((resolve) => canvas.toBlob(resolve, 'image/jpeg', 0.8));
            const compressedSize = blob.size;
            const ext = file.name.replace(/\.[^.]+$/, '') + '.jpg';
            const compressedFile = new File([blob], ext, { type: 'image/jpeg', lastModified: Date.now() });

            return {
                file: compressedFile,
                compressed: true,
                originalSize,
                compressedSize,
            };
        } catch (error) {
            console.warn('图片压缩失败，使用原始文件上传:', error);
            return { file, compressed: false, originalSize, compressedSize: originalSize };
        }
    }

    function getUploadRequestHeaders() {
        const headers = {};
        const token = window.Auth && typeof window.Auth.getToken === 'function'
            ? String(window.Auth.getToken() || '').trim()
            : '';
        if (token) {
            headers.Authorization = `Bearer ${token}`;
        }
        return headers;
    }

    function parseUploadResponse(status, responseText) {
        const rawText = String(responseText || '').trim();
        let payload = null;
        if (rawText) {
            try {
                payload = JSON.parse(rawText);
            } catch (_error) {
                payload = null;
            }
        }
        if (status < 200 || status >= 300) {
            const detail = payload && (payload.detail || payload.message || payload.error);
            if (detail && typeof detail === 'object') {
                const joined = [
                    detail.message,
                    ...(Array.isArray(detail.blocking_issues) ? detail.blocking_issues : []),
                ].filter(Boolean).join(' / ');
                throw new Error(joined || `上传附件失败 (HTTP ${status})`);
            }
            throw new Error(detail || `上传附件失败 (HTTP ${status})`);
        }
        if (payload && payload.success === false) {
            throw new Error(payload.message || '上传附件失败');
        }
        return payload && payload.data ? payload.data : payload;
    }

    async function uploadWithProgress(url, options = {}) {
        const { formData, signal, onProgress } = options;
        const runRequest = (allowRefresh) => new Promise((resolve, reject) => {
            if (signal && signal.aborted) {
                reject(createAbortError('已取消上传'));
                return;
            }
            const xhr = new XMLHttpRequest();
            const handleAbort = () => {
                xhr.abort();
            };
            const cleanup = () => {
                if (signal) {
                    signal.removeEventListener('abort', handleAbort);
                }
            };
            xhr.open('POST', url, true);
            const headers = getUploadRequestHeaders();
            Object.entries(headers).forEach(([key, value]) => {
                if (value !== undefined && value !== null && value !== '') {
                    xhr.setRequestHeader(key, value);
                }
            });
            if (typeof onProgress === 'function') {
                onProgress(0);
            }
            xhr.upload.addEventListener('progress', (event) => {
                if (!event.lengthComputable || typeof onProgress !== 'function') {
                    return;
                }
                const percent = Math.max(0, Math.min(100, Math.round((event.loaded / event.total) * 100)));
                onProgress(percent);
            });
            xhr.addEventListener('load', async () => {
                cleanup();
                if (xhr.status === 401 && allowRefresh && window.Auth && typeof window.Auth.refreshToken === 'function') {
                    try {
                        const refreshed = await window.Auth.refreshToken();
                        if (refreshed) {
                            resolve(runRequest(false));
                            return;
                        }
                    } catch (_error) {
                        // fall through and surface the response error below
                    }
                }
                try {
                    if (typeof onProgress === 'function') {
                        onProgress(100);
                    }
                    resolve(parseUploadResponse(xhr.status, xhr.responseText));
                } catch (error) {
                    reject(error);
                }
            });
            xhr.addEventListener('error', () => {
                cleanup();
                reject(new Error('上传失败，请检查网络后重试'));
            });
            xhr.addEventListener('abort', () => {
                cleanup();
                reject(createAbortError('已取消上传'));
            });
            if (signal) {
                signal.addEventListener('abort', handleAbort, { once: true });
            }
            xhr.send(formData);
        });
        return await runRequest(true);
    }

    async function selectDispatchIssueAsset(accept) {
        return await new Promise((resolve) => {
            const input = document.createElement('input');
            input.type = 'file';
            input.accept = accept;
            input.style.display = 'none';
            input.addEventListener('change', () => {
                const file = input.files && input.files[0];
                input.remove();
                resolve(file || null);
            }, { once: true });
            document.body.appendChild(input);
            input.click();
        });
    }

    function renderFallbackUploadRetry(host, message, onRetry) {
        host.innerHTML = `
            <div class="mobile-upload-progress__fallback-error">
                <p class="mobile-upload-progress__meta">${escapeHtml(message)}</p>
                <button type="button" class="mobile-upload-progress__button">重试上传</button>
            </div>
        `;
        const button = host.querySelector('button');
        if (button) {
            button.addEventListener('click', () => onRetry(), { once: true });
        }
    }

    function createDispatchIssueUploadDialog(file, compressionInfo) {
        const overlay = document.createElement('div');
        overlay.className = 'mobile-upload-overlay';
        overlay.innerHTML = `
            <section class="mobile-upload-panel" role="dialog" aria-modal="true" aria-labelledby="dispatchIssueUploadTitle">
                <div class="mobile-upload-progress">
                    <div class="mobile-upload-progress__header">
                        <div>
                            <div class="mobile-upload-progress__title" id="dispatchIssueUploadTitle">附件上传进度</div>
                            <div class="mobile-upload-progress__subtitle">${escapeHtml(String(file?.name || '未命名附件'))}</div>
                        </div>
                        <div class="mobile-upload-progress__percent" data-role="percent">0%</div>
                    </div>
                    <div class="mobile-upload-progress__track" data-role="track" role="progressbar" aria-label="附件上传进度" aria-valuemin="0" aria-valuemax="100" aria-valuenow="0">
                        <span class="mobile-upload-progress__bar" data-role="bar" style="width:0%"></span>
                    </div>
                    <div class="mobile-upload-progress__meta" data-role="meta">正在上传附件（0%）</div>
                    <div class="mobile-upload-progress__compression" data-role="compression" style="display:none"></div>
                    <div class="mobile-upload-progress__actions">
                        <button type="button" class="mobile-upload-progress__button mobile-upload-progress__button--secondary" data-role="action">取消上传</button>
                    </div>
                    <div class="mobile-upload-progress__error-host" data-role="error-host"></div>
                </div>
            </section>
        `;
        document.body.appendChild(overlay);
        const percentNode = overlay.querySelector('[data-role="percent"]');
        const trackNode = overlay.querySelector('[data-role="track"]');
        const barNode = overlay.querySelector('[data-role="bar"]');
        const metaNode = overlay.querySelector('[data-role="meta"]');
        const actionButton = overlay.querySelector('[data-role="action"]');
        const errorHost = overlay.querySelector('[data-role="error-host"]');
        const compressionNode = overlay.querySelector('[data-role="compression"]');

        return {
            destroy() {
                overlay.remove();
            },
            setProgress(progress, message) {
                const normalized = Math.max(0, Math.min(100, Number(progress || 0)));
                if (percentNode) {
                    percentNode.textContent = `${normalized}%`;
                }
                if (trackNode) {
                    trackNode.setAttribute('aria-valuenow', String(normalized));
                }
                if (barNode) {
                    barNode.style.width = `${normalized}%`;
                }
                if (metaNode) {
                    metaNode.textContent = message || `正在上传附件（${normalized}%）`;
                }
            },
            showCompression(info) {
                if (!compressionNode || !info) return;
                compressionNode.style.display = 'block';
                compressionNode.textContent = `已压缩：${formatFileSize(info.originalSize)} → ${formatFileSize(info.compressedSize)}（节省 ${info.ratio}%）`;
            },
            setAction(label, handler, isSecondary = true) {
                if (!actionButton) {
                    return;
                }
                actionButton.textContent = label;
                actionButton.className = `mobile-upload-progress__button${isSecondary ? ' mobile-upload-progress__button--secondary' : ''}`;
                actionButton.onclick = handler;
            },
            clearError() {
                if (errorHost) {
                    errorHost.replaceChildren();
                }
            },
            showRetry(message, onRetry) {
                if (!errorHost) {
                    return;
                }
                if (window.EmptyError && typeof window.EmptyError.show === 'function') {
                    window.EmptyError.show(errorHost, 'error', message, onRetry);
                    return;
                }
                renderFallbackUploadRetry(errorHost, message, onRetry);
            },
        };
    }

    async function performDispatchIssueAssetUpload(file, options = {}) {
        const formData = new FormData();
        formData.append('file', file);
        formData.append('category', 'dispatch_issue');
        return await uploadWithProgress(`${window.location.origin}/api/v2/mobile/uploads`, {
            formData,
            signal: options.signal,
            onProgress: options.onProgress,
        });
    }

    async function uploadDispatchIssueAsset(accept) {
        const file = await selectDispatchIssueAsset(accept);
        if (!file) {
            return null;
        }
        void ensureFeedbackComponents();

        // T19: Image compression before upload
        let uploadFile = file;
        let compressionInfo = null;
        if (file.type && file.type.startsWith('image/')) {
            const result = await compressImageFile(file);
            uploadFile = result.file;
            if (result.compressed) {
                compressionInfo = {
                    originalSize: result.originalSize,
                    compressedSize: result.compressedSize,
                    ratio: Math.round((1 - result.compressedSize / result.originalSize) * 100),
                };
            }
        }

        return await new Promise((resolve) => {
            const dialog = createDispatchIssueUploadDialog(uploadFile, compressionInfo);
            let settled = false;
            let abortController = null;
            const finish = (value) => {
                if (settled) {
                    return;
                }
                settled = true;
                dialog.destroy();
                resolve(value);
            };
            const closePanel = () => finish(null);

            // T20: Exponential backoff retry loop
            const MAX_RETRIES = 3;
            let attempt = 0;

            const doUpload = async () => {
                abortController = new AbortController();
                dialog.clearError();
                dialog.setProgress(0, attempt > 0
                    ? `重试中 (${attempt}/${MAX_RETRIES})，正在上传...`
                    : '正在上传附件（0%）');
                dialog.setAction('取消上传', () => abortController.abort(), true);
                try {
                    const upload = await performDispatchIssueAssetUpload(uploadFile, {
                        signal: abortController.signal,
                        onProgress(progress) {
                            dialog.setProgress(progress, attempt > 0
                                ? `重试中 (${attempt}/${MAX_RETRIES})，上传进度 ${progress}%`
                                : `正在上传附件（${progress}%）`);
                        },
                    });
                    dialog.setProgress(100, '附件上传完成（100%）');
                    if (compressionInfo) {
                        dialog.showCompression(compressionInfo);
                        await showIssueUploadToast('success', `附件上传完成（已压缩 ${compressionInfo.ratio}%）`);
                    } else if (attempt > 0) {
                        await showIssueUploadToast('success', `附件上传完成（重试 ${attempt} 次后成功）`);
                    } else {
                        await showIssueUploadToast('success', '附件上传完成');
                    }
                    finish(upload || null);
                } catch (error) {
                    if (error && error.name === 'AbortError') {
                        await showIssueUploadToast('info', '已取消附件上传');
                        finish(null);
                        return;
                    }
                    const message = error.message || '上传附件失败';
                    console.error('上传异常首报附件失败:', error);
                    attempt++;
                    if (attempt > MAX_RETRIES) {
                        // All retries exhausted — show error with manual retry
                        await showIssueUploadToast('error', `上传失败，已重试 ${MAX_RETRIES} 次`);
                        dialog.setAction('关闭', () => closePanel(), true);
                        dialog.showRetry(message, () => {
                            attempt = 0; // Reset for manual retry
                            void doUpload();
                        });
                        return;
                    }
                    // Show retry countdown, then auto-retry
                    await showIssueUploadToast('error', `${message}，${Math.pow(2, attempt - 1)}s 后自动重试 (${attempt}/${MAX_RETRIES})`);
                    const delayMs = Math.pow(2, attempt - 1) * 1000;
                    dialog.setProgress(0, `重试中 (${attempt}/${MAX_RETRIES})，${delayMs / 1000}s 后自动重试...`);
                    dialog.setAction('取消重试', () => abortController.abort(), true);
                    // Cancellable delay
                    try {
                        await new Promise((resolveDelay, rejectDelay) => {
                            const timer = setTimeout(resolveDelay, delayMs);
                            const onAbort = () => { clearTimeout(timer); rejectDelay(createAbortError('已取消重试')); };
                            abortController.signal.addEventListener('abort', onAbort, { once: true });
                        });
                    } catch (abortErr) {
                        if (abortErr && abortErr.name === 'AbortError') {
                            await showIssueUploadToast('info', '已取消附件上传');
                            finish(null);
                            return;
                        }
                    }
                    void doUpload();
                }
            };
            void doUpload();
        });
    }

    async function openQuickIssueReport(orderId) {
        if (!orderId) {
            return;
        }

        const rawMode = window.prompt('请选择异常首报方式：text / photo / voice', 'text');
        if (rawMode === null) {
            return;
        }
        const inputMode = String(rawMode || 'text').trim().toLowerCase();
        if (!['text', 'photo', 'voice'].includes(inputMode)) {
            showToast('首报方式仅支持 text、photo、voice');
            return;
        }

        const severityRaw = window.prompt('请选择异常级别：low / medium / high / critical', 'medium');
        if (severityRaw === null) {
            return;
        }
        const severity = String(severityRaw || 'medium').trim().toLowerCase();
        if (!['low', 'medium', 'high', 'critical'].includes(severity)) {
            showToast('异常级别仅支持 low、medium、high、critical');
            return;
        }

        const payload = { input_mode: inputMode, severity };
        if (inputMode === 'text') {
            const note = window.prompt('请输入一句话异常首报（15 秒内完成即可）', '');
            if (note === null) {
                return;
            }
            payload.note = String(note || '').trim() || '现场异常首报';
        } else if (inputMode === 'photo') {
            showToast('请选择一张现场图片，系统会自动上传并补齐上下文');
            const upload = await uploadDispatchIssueAsset('image/*');
            if (!upload?.upload_id) {
                return;
            }
            payload.attachments = [upload.upload_id];
            const note = window.prompt('补充一句说明（可选）', '');
            if (note !== null && String(note || '').trim()) {
                payload.note = String(note).trim();
            }
        } else {
            showToast('请选择一段现场语音，系统会自动上传并补齐上下文');
            const upload = await uploadDispatchIssueAsset('audio/*');
            if (!upload?.upload_id) {
                return;
            }
            payload.voice_attachment_id = upload.upload_id;
            payload.attachments = [upload.upload_id];
            const note = window.prompt('补充一句说明（可选）', '');
            if (note !== null && String(note || '').trim()) {
                payload.note = String(note).trim();
            }
        }

        try {
            await apiCall(`/api/v2/dispatch-orders/${encodeURIComponent(orderId)}/report-issue`, {
                method: 'POST',
                body: JSON.stringify(payload),
            });
            showToast('异常首报已提交');
            await openOrderDetail(orderId);
        } catch (error) {
            showToast(error.message || '异常首报失败');
        }
    }

    function renderSafetyChecklistSection(order, options = {}) {
        const editable = options.editable === true;
        const checklist = state.detailSafetyChecklist;

        const loadingHtml = state.detailSafetyLoading
            ? '<div class="safety-empty">安全清单加载中...</div>'
            : '';

        if (!state.detailSafetyLoading && state.detailSafetyError) {
            return `
                <div class="section-title">安全清单</div>
                <div class="safety-box">
                    <div class="safety-alert is-error">${escapeHtml(state.detailSafetyError)}</div>
                    <div class="safety-empty">
                        <button class="suggestion-chip" data-action="refresh-safety-checklist">重试加载</button>
                    </div>
                </div>
            `;
        }

        if (!checklist) {
            return `
                <div class="section-title">安全清单</div>
                <div class="safety-box">
                    ${loadingHtml || '<div class="safety-empty">当前暂无安全清单数据</div>'}
                </div>
            `;
        }

        const enforced = Boolean(checklist.enforced);
        const ready = Boolean(checklist.ready);
        const requiredTotal = Number(checklist.required_total || 0);
        const completedRequired = Number(checklist.completed_required || 0);
        const pendingRequiredItems = Array.isArray(checklist.pending_required_items)
            ? checklist.pending_required_items
            : [];
        const failedRequiredItems = Array.isArray(checklist.failed_required_items)
            ? checklist.failed_required_items
            : [];
        const checklistItems = Array.isArray(checklist.items) ? checklist.items : [];
        const criticalItems = checklistItems.filter((item) => getChecklistLevel(item) === 'critical');
        const routineItems = checklistItems.filter((item) => getChecklistLevel(item) === 'routine');
        const pendingRoutineItems = routineItems.filter((item) => {
            const status = String(item.result || item.status || '').trim().toLowerCase();
            return !status || status === 'pending';
        });
        const routineBatchBusy = state.detailSafetySubmittingKey === 'batch:routine';

        const summaryPills = [];
        summaryPills.push(`
            <span class="safety-pill ${enforced ? 'is-active' : 'is-muted'}">
                ${enforced ? '门禁启用' : '门禁未启用'}
            </span>
        `);
        if (enforced) {
            summaryPills.push(`
                <span class="safety-pill ${ready ? 'is-pass' : 'is-warning'}">
                    必填 ${escapeHtml(String(completedRequired))}/${escapeHtml(String(requiredTotal))}
                </span>
            `);
        }
        if (checklist.template_version) {
            summaryPills.push(`
                <span class="safety-pill is-muted">
                    版本 ${escapeHtml(checklist.template_version)}
                </span>
            `);
        }
        if (routineItems.length > 0) {
            summaryPills.push(`
                <span class="safety-pill is-muted">
                    常规项 ${escapeHtml(String(Number(checklist.completed_routine || 0)))}/${escapeHtml(String(Number(checklist.routine_total || 0)))}
                </span>
            `);
        }

        const pendingText = pendingRequiredItems.length > 0
            ? `待完成：${escapeHtml(pendingRequiredItems.join(' / '))}`
            : '';
        const failedText = failedRequiredItems.length > 0
            ? `不通过：${escapeHtml(failedRequiredItems.join(' / '))}`
            : '';

        const gateHint = state.detailSafetyGateHint;
        const gateHintHtml = gateHint && typeof gateHint === 'object'
            ? `
                <div class="safety-alert is-warning">
                    <div>${escapeHtml(gateHint.message || '安全清单未完成，无法完工')}</div>
                    <div class="safety-alert-sub">
                        ${escapeHtml(
                            [
                                ...(Array.isArray(gateHint.pending_required_items) ? gateHint.pending_required_items.map((item) => `待完成:${item}`) : []),
                                ...(Array.isArray(gateHint.failed_required_items) ? gateHint.failed_required_items.map((item) => `不通过:${item}`) : [])
                            ].join(' / ') || '请先补齐必填项'
                        )}
                    </div>
                </div>
            `
            : '';

        const renderChecklistRows = (items, { allowActions }) => items.length > 0
            ? items.map((item) => {
                const itemCode = String(item.item_code || '');
                const result = String(item.result || '').trim().toLowerCase();
                const status = result || String(item.status || 'pending').trim().toLowerCase();
                const badgeClass = getChecklistBadgeClass(status);
                const canUseNa = Boolean(item.allow_na);
                const isBusy = Boolean(state.detailSafetySubmittingKey);
                const canOperateItem = allowActions && editable && ['assigned', 'in_progress'].includes(order.status || '');
                const passLabel = '通过';
                const failLabel = '不通过';
                const naLabel = '不适用';

                return `
                    <div class="safety-item-card">
                        <div class="safety-item-head">
                            <div class="safety-item-title-wrap">
                                <p class="safety-item-title">${escapeHtml(item.title || itemCode || '-')}</p>
                                <p class="safety-item-code">${escapeHtml(itemCode || '-')}</p>
                            </div>
                            <div class="safety-item-tags">
                                <span class="safety-pill ${getChecklistLevel(item) === 'routine' ? 'is-muted' : 'is-active'}">${getChecklistLevel(item) === 'routine' ? '常规' : '关键'}</span>
                                <span class="safety-pill ${item.required ? 'is-warning' : 'is-muted'}">${item.required ? '必填' : '可选'}</span>
                                <span class="safety-pill ${badgeClass}">${escapeHtml(translateChecklistResult(status))}</span>
                            </div>
                        </div>

                        <div class="safety-item-meta">
                            <span>检查人：${escapeHtml(item.checked_by_username || item.checked_by || '-')}</span>
                            <span>时间：${escapeHtml(formatDateTime(item.checked_at))}</span>
                        </div>

                        ${item.note ? `<p class="safety-item-note">备注：${escapeHtml(item.note)}</p>` : ''}

                        ${canOperateItem ? `
                            <div class="safety-item-actions">
                                <button class="safety-item-action" data-item-code="${escapeHtmlAttribute(itemCode)}" data-result="pass" ${isBusy ? 'disabled' : ''}>${passLabel}</button>
                                <button class="safety-item-action is-danger" data-item-code="${escapeHtmlAttribute(itemCode)}" data-result="fail" ${isBusy ? 'disabled' : ''}>${failLabel}</button>
                                ${canUseNa ? `<button class="safety-item-action" data-item-code="${escapeHtmlAttribute(itemCode)}" data-result="na" ${isBusy ? 'disabled' : ''}>${naLabel}</button>` : ''}
                            </div>
                        ` : ''}
                    </div>
                `;
            }).join('')
            : '<div class="safety-empty">当前模板未配置检查项</div>';

        const routineBatchAction = editable && routineItems.length > 0
            ? `
                <div class="safety-alert is-warning">
                    <div>常规项不再逐项提交，确认完成后一次性批量提交。</div>
                    <div class="safety-alert-sub">待批量确认 ${escapeHtml(String(pendingRoutineItems.length))} 项；仅关键项会阻断完工。</div>
                    <div style="margin-top:10px;">
                        <button class="suggestion-chip" data-action="submit-routine-batch" ${routineBatchBusy || pendingRoutineItems.length <= 0 ? 'disabled' : ''}>
                            ${pendingRoutineItems.length > 0 ? `常规项已检查（${pendingRoutineItems.length}）` : '常规项已全部确认'}
                        </button>
                    </div>
                </div>
            `
            : '';

        const criticalSection = criticalItems.length > 0
            ? `
                <div class="section-title" style="margin-top: 18px;">关键安全项</div>
                <div class="safety-item-list">
                    ${renderChecklistRows(criticalItems, { allowActions: true })}
                </div>
            `
            : '';

        const routineSection = routineItems.length > 0
            ? `
                <div class="section-title" style="margin-top: 18px;">常规安全项</div>
                ${routineBatchAction}
                <div class="safety-item-list">
                    ${renderChecklistRows(routineItems, { allowActions: false })}
                </div>
            `
            : '';
        const emptySection = checklistItems.length <= 0
            ? '<div class="safety-empty">当前模板未配置检查项</div>'
            : '';

        return `
            <div class="section-title">安全清单</div>
            <div class="safety-box">
                <div class="safety-summary">
                    ${summaryPills.join('')}
                </div>
                ${pendingText ? `<p class="safety-summary-text">${pendingText}</p>` : ''}
                ${failedText ? `<p class="safety-summary-text is-danger">${failedText}</p>` : ''}
                ${gateHintHtml}
                ${loadingHtml}
                ${emptySection}
                ${criticalSection}
                ${routineSection}
            </div>
        `;
    }

    async function cancelDispatchOrder(orderId) {
        const reason = window.prompt('请输入取消原因（可选）', '') || '';
        try {
            const query = reason ? `?reason=${encodeURIComponent(reason)}` : '';
            await apiCall(`/api/v2/dispatch-orders/${encodeURIComponent(orderId)}/cancel${query}`, {
                method: 'POST'
            });
            showToast('派工单已取消');
            closeDetailDrawer();
            await refreshTimeline();
        } catch (error) {
            console.error('取消派工失败:', error);
            showToast(error.message || '取消派工失败');
        }
    }

    async function publishDispatchOrder(orderId) {
        if (!orderId) {
            return;
        }

        try {
            const payload = unwrapApiData(await apiCall(`/api/v2/dispatch-orders/${encodeURIComponent(orderId)}/publish`, {
                method: 'POST'
            }));
            showToast(payload?.message || '派工单已正式发布');
            await refreshTimeline();
            await openOrderDetail(orderId);
        } catch (error) {
            console.error('正式发布派工失败:', error);
            showToast(error.message || '正式发布派工失败');
        }
    }

    function closeStatusPanel() {
        const panel = document.getElementById('statusPanel');
        if (panel) {
            panel.classList.remove('open');
        }
    }

    function toggleStatusPanel() {
        if (isStatusPanelOpen()) {
            closeStatusPanel();
            return;
        }
        openStatusPanel();
    }

    function openStatusPanel() {
        const panel = document.getElementById('statusPanel');
        if (!panel) {
            return;
        }

        renderStatusToolbar();
        positionStatusPanel();
        panel.classList.add('open');
    }

    function openDrawer(drawerId) {
        const drawer = document.getElementById(drawerId);
        if (!drawer) {
            return;
        }
        // Clear any inline display style that might override CSS visibility
        drawer.style.display = '';
        drawer.classList.add('open');
        setBackdropVisible(true);
    }

    function closeDrawer(drawerId) {
        const drawer = document.getElementById(drawerId);
        if (!drawer) {
            return;
        }
        drawer.classList.remove('open');
        if (!isAnyDrawerOpen()) {
            setBackdropVisible(false);
        }
    }

    function isAnyDrawerOpen() {
        const detailDrawer = document.getElementById('detailDrawer');
        const aiDrawer = document.getElementById('aiDrawer');
        const chatDrawer = document.getElementById('chatDrawer');
        return Boolean(
            (detailDrawer && detailDrawer.classList.contains('open')) ||
            (aiDrawer && aiDrawer.classList.contains('open')) ||
            (chatDrawer && chatDrawer.classList.contains('open'))
        );
    }

    function setBackdropVisible(visible) {
        const backdrop = document.getElementById('backdrop');
        if (!backdrop) {
            return;
        }
        backdrop.classList.toggle('show', visible);
    }

    function closeDetailDrawer() {
        closeDrawer('detailDrawer');
    }

    async function openAiDrawer(targetTab = 'assistant', options = {}) {
        const bridge = getDispatchAiBridge();
        const normalized = targetTab === 'conflict' ? 'conflict' : 'assistant';
        state.aiDrawerTab = normalized;
        if (bridge && typeof bridge.openDrawer === 'function') {
            bridge.openDrawer(normalized, {
                refresh: options.refresh !== false,
                context: state.detailOrder ? { dispatch_order_id: state.detailOrder.id, flight_id: state.detailOrder.flight_id } : undefined,
            });
            return;
        }
        renderAiMetrics();
        renderAiSuggestions();
        renderAnalyticsPanel();
        renderConflictGovernance();
        renderScenarioPanel();
        renderReplanHint();
        await switchAiDrawerTab(targetTab, { refresh: options.refresh !== false });
        openDrawer('aiDrawer');
    }

    function closeAiDrawer() {
        const bridge = getDispatchAiBridge();
        if (bridge && typeof bridge.closeDrawer === 'function') {
            bridge.closeDrawer();
            return;
        }
        closeDrawer('aiDrawer');
    }

    function readInitialChatOpenParams() {
        const params = new URLSearchParams(window.location.search || '');
        const openRaw = String(params.get('open_chat') || params.get('openChat') || '').trim().toLowerCase();
        const shouldOpen = ['1', 'true', 'yes', 'y'].includes(openRaw);
        const flightId = String(params.get('flight_id') || params.get('flightId') || '').trim();
        const focusFlightId = String(params.get('focus_flight_id') || params.get('focusFlightId') || flightId).trim();
        const windowStartMs = toMs(params.get('window_start') || params.get('windowStart'));
        const windowEndMs = toMs(params.get('window_end') || params.get('windowEnd'));
        return {
            shouldOpen,
            flightId,
            focusFlightId,
            windowStartMs,
            windowEndMs,
        };
    }

    function applyInitialWindowParamsFromLocation() {
        const options = readInitialChatOpenParams();
        if (!options.windowStartMs || !options.windowEndMs || options.windowEndMs <= options.windowStartMs) {
            return false;
        }

        state.windowStartMs = options.windowStartMs;
        state.windowEndMs = options.windowEndMs;
        renderWindowLabel();
        renderViewModeHint();
        return true;
    }

    function clearInitialChatOpenParams() {
        if (!window.history || typeof window.history.replaceState !== 'function') {
            return;
        }

        try {
            const url = new URL(window.location.href);
            const keys = [
                'dispatch_ui_v',
                'open_chat',
                'openChat',
                'flight_id',
                'flightId',
                'focus_flight_id',
                'focusFlightId',
                'window_start',
                'windowStart',
                'window_end',
                'windowEnd',
            ];
            let changed = false;
            for (const key of keys) {
                if (!url.searchParams.has(key)) {
                    continue;
                }
                url.searchParams.delete(key);
                changed = true;
            }
            if (!changed) {
                return;
            }
            const nextUrl = `${url.pathname}${url.search}${url.hash}`;
            window.history.replaceState(window.history.state, document.title, nextUrl);
        } catch (_error) {
            // ignore malformed URL parsing errors
        }
    }

    async function handleInitialChatOpenFromLocation() {
        const options = readInitialChatOpenParams();
        if (!options.shouldOpen) {
            return;
        }

        clearInitialChatOpenParams();
        if (!state.chatEnabled) {
            showToast('群聊功能未启用');
            return;
        }

        const focusFlightId = String(options.focusFlightId || options.flightId || '').trim();
        const focused = focusFlightId
            ? await focusFlightById(focusFlightId, { openDetail: true, silent: true })
            : false;

        if (options.flightId) {
            const opened = await openChatDrawer({
                flightId: options.flightId,
                fallbackToFirstGroup: true,
                silentMissingMembership: true,
            });
            if (!opened && focused) {
                showToast('已定位到对应航班；当前账号不在该航班群聊中。');
            }
            return;
        }

        await openChatDrawer();
    }

    function initializeDispatchChatUI() {
        updateChatUnreadBadge();
        renderChatGroupList();
        renderChatMessages();
        renderChatComposer();
    }

    function setDispatchChatEnabled(enabled) {
        state.chatEnabled = Boolean(enabled);
        const entryButtonIds = ['openChatFloatingBtn', 'openChatCornerBadgeBtn'];
        for (const id of entryButtonIds) {
            const button = document.getElementById(id);
            if (!button) {
                continue;
            }
            button.style.display = '';
            button.disabled = !state.chatEnabled;
            button.setAttribute('aria-disabled', state.chatEnabled ? 'false' : 'true');
            if (!state.chatEnabled) {
                button.title = '群聊功能未启用';
            } else {
                button.removeAttribute('title');
            }
        }

        if (!state.chatEnabled) {
            state.chatGroups = [];
            state.chatUnreadTotal = 0;
            state.chatSelectedGroupId = '';
            state.chatMessages = [];
            state.chatMessagesHasMore = false;
            state.chatMessagesNextBeforeSeq = null;
            state.chatInputDraft = '';
            state.chatAtAll = false;
            closeChatDrawer();
            disconnectDispatchChatStream();
        }

        updateChatUnreadBadge();
        renderChatGroupList();
        renderChatMessages();
        renderChatComposer();
    }

    function updateChatUnreadBadge() {
        const unread = Math.max(0, Number(state.chatUnreadTotal || 0));
        const badgeIds = ['chatUnreadBadge', 'chatCornerUnreadBadge'];

        for (const id of badgeIds) {
            const badge = document.getElementById(id);
            if (!badge) {
                continue;
            }
            if (!state.chatEnabled || unread <= 0) {
                badge.hidden = true;
                badge.textContent = '0';
                continue;
            }
            badge.hidden = false;
            badge.textContent = unread > 99 ? '99+' : String(unread);
        }
    }

    function getSelectedChatGroup() {
        const groupId = String(state.chatSelectedGroupId || '').trim();
        if (!groupId) {
            return null;
        }
        return state.chatGroups.find((group) => String(group?.group_id || '') === groupId) || null;
    }

    function sortChatGroups() {
        state.chatGroups.sort((left, right) => {
            const leftArchived = Boolean(left?.read_only) || String(left?.status || '').toLowerCase() === 'archived';
            const rightArchived = Boolean(right?.read_only) || String(right?.status || '').toLowerCase() === 'archived';
            if (leftArchived !== rightArchived) {
                return leftArchived ? 1 : -1;
            }

            const leftTime = toMs(left?.last_message_at) || toMs(left?.updated_at) || 0;
            const rightTime = toMs(right?.last_message_at) || toMs(right?.updated_at) || 0;
            if (leftTime !== rightTime) {
                return rightTime - leftTime;
            }

            return String(left?.group_name || '').localeCompare(String(right?.group_name || ''), 'zh-CN');
        });
    }

    function syncChatUnreadTotalFromGroups() {
        state.chatUnreadTotal = state.chatGroups.reduce((sum, group) => {
            return sum + Math.max(0, Number(group?.unread_count || 0));
        }, 0);
    }

    function upsertChatGroup(group) {
        if (!group || !group.group_id) {
            return;
        }

        const groupId = String(group.group_id);
        const index = state.chatGroups.findIndex((item) => String(item?.group_id || '') === groupId);
        if (index >= 0) {
            state.chatGroups[index] = {
                ...state.chatGroups[index],
                ...group,
            };
        } else {
            state.chatGroups.push({
                ...group,
            });
        }
        sortChatGroups();
    }

    function renderChatGroupList() {
        const list = document.getElementById('chatGroupList');
        const meta = document.getElementById('chatGroupMeta');
        if (!list || !meta) {
            return;
        }

        if (!state.chatEnabled) {
            meta.textContent = '未启用';
            list.innerHTML = '<div class=\"chat-group-empty\">群聊功能未开启</div>';
            return;
        }

        meta.textContent = `${state.chatGroups.length} 个群`;

        if (state.chatLoadingGroups && state.chatGroups.length === 0) {
            list.innerHTML = '<div class=\"chat-group-empty\">群列表加载中...</div>';
            return;
        }

        if (state.chatGroups.length === 0) {
            list.innerHTML = '<div class=\"chat-group-empty\">当前暂无可见群聊</div>';
            return;
        }

        list.innerHTML = state.chatGroups.map((group) => {
            const groupId = String(group?.group_id || '');
            const isSelected = groupId === state.chatSelectedGroupId;
            const unreadCount = Math.max(0, Number(group?.unread_count || 0));
            const isArchived = Boolean(group?.read_only) || String(group?.status || '').toLowerCase() === 'archived';
            const timeText = group?.last_message_at ? formatDateTime(group.last_message_at) : '-';
            const preview = truncateText(group?.last_message_preview || '暂无消息', 40);

            return `
                <button class=\"chat-group-item ${isSelected ? 'is-selected' : ''}\" data-group-id=\"${escapeHtmlAttribute(groupId)}\" aria-label=\"打开群组 ${escapeHtml(group?.group_name || groupId)}\">
                    <div class=\"chat-group-main\">
                        <span class=\"chat-group-title\">${escapeHtml(group?.group_name || groupId)}</span>
                        ${isArchived ? '<span class=\"chat-group-status\">已归档</span>' : ''}
                    </div>
                    <div class=\"chat-group-sub\">${escapeHtml(preview)}</div>
                    <div class=\"chat-group-meta-row\">
                        <span>${escapeHtml(timeText)}</span>
                        ${unreadCount > 0 ? `<span class=\"chat-group-unread\">${unreadCount > 99 ? '99+' : unreadCount}</span>` : ''}
                    </div>
                </button>
            `;
        }).join('');
    }

    function getChatMessageKey(message) {
        if (!message || typeof message !== 'object') {
            return '';
        }
        const messageId = String(message.message_id || '').trim();
        if (messageId) {
            return messageId;
        }
        const groupId = String(message.group_id || '').trim();
        const seqNo = Number(message.seq_no || 0);
        if (groupId && seqNo > 0) {
            return `${groupId}:${seqNo}`;
        }
        return '';
    }

    function dedupeAndSortChatMessages(messages) {
        const seen = new Set();
        const deduped = [];
        for (const item of messages || []) {
            const key = getChatMessageKey(item) || `${Math.random()}`;
            if (seen.has(key)) {
                continue;
            }
            seen.add(key);
            deduped.push(item);
        }
        deduped.sort((left, right) => Number(left?.seq_no || 0) - Number(right?.seq_no || 0));
        return deduped;
    }

    function appendChatMessage(message) {
        if (!message || typeof message !== 'object') {
            return;
        }
        state.chatMessages = dedupeAndSortChatMessages([...state.chatMessages, message]);
    }

    function scrollChatToBottom() {
        const messageList = document.getElementById('chatMessageList');
        if (!messageList) {
            return;
        }
        messageList.scrollTop = messageList.scrollHeight;
    }

    function isChatDrawerOpen() {
        const chatDrawer = document.getElementById('chatDrawer');
        return Boolean(chatDrawer && chatDrawer.classList.contains('open'));
    }

    function renderChatMessages() {
        const messageList = document.getElementById('chatMessageList');
        const emptyTip = document.getElementById('chatEmptyTip');
        const activeTitle = document.getElementById('chatActiveTitle');
        const activeSubtitle = document.getElementById('chatActiveSubtitle');
        const archivePill = document.getElementById('chatArchivePill');
        if (!messageList || !emptyTip || !activeTitle || !activeSubtitle || !archivePill) {
            return;
        }

        const selectedGroup = getSelectedChatGroup();
        if (!selectedGroup) {
            activeTitle.textContent = '请选择群组';
            activeSubtitle.textContent = '仅成员可见';
            archivePill.hidden = true;
            messageList.innerHTML = '';
            emptyTip.hidden = false;
            emptyTip.textContent = state.chatEnabled ? '选择左侧群组开始沟通' : '群聊功能未开启';
            return;
        }

        const memberCount = Number(selectedGroup.member_count || 0);
        const isArchived = Boolean(selectedGroup.read_only) || String(selectedGroup.status || '').toLowerCase() === 'archived';
        activeTitle.textContent = selectedGroup.group_name || selectedGroup.group_id || '-';
        activeSubtitle.textContent = `航班 ${selectedGroup.flight_id || '-'} | 成员 ${memberCount}`;
        archivePill.hidden = !isArchived;

        if (state.chatLoadingMessages && state.chatMessages.length === 0) {
            messageList.innerHTML = '<div class=\"chat-message-loading\">消息加载中...</div>';
            emptyTip.hidden = true;
            return;
        }

        if (state.chatMessages.length === 0) {
            messageList.innerHTML = '';
            emptyTip.hidden = false;
            emptyTip.textContent = '暂无消息，发送第一条沟通信息';
            return;
        }

        const currentUserId = getCurrentUserId();
        messageList.innerHTML = state.chatMessages.map((message) => {
            const messageType = String(message?.message_type || 'text').toLowerCase();
            const senderId = String(message?.sender_user_id || '').trim();
            const senderName = String(message?.sender_username || senderId || '系统').trim() || '系统';
            const isMine = senderId && currentUserId && senderId === currentUserId;
            const contentHtml = escapeHtml(String(message?.content || '')).replace(/\\n/g, '<br>');
            const timeText = formatDateTime(message?.sent_at);
            const atAllTag = message?.is_at_all ? '<span class=\"chat-message-atall\">@全体</span>' : '';

            if (messageType === 'system') {
                return `
                    <div class=\"chat-system-message\">
                        <span>${contentHtml}</span>
                    </div>
                `;
            }

            return `
                <div class=\"chat-message-row ${isMine ? 'is-mine' : ''}\">
                    <div class=\"chat-message-meta\">
                        <span>${escapeHtml(isMine ? '我' : senderName)}</span>
                        <span>${escapeHtml(timeText)}</span>
                    </div>
                    <div class=\"chat-message-bubble\">
                        ${atAllTag}
                        <div class=\"chat-message-content\">${contentHtml}</div>
                    </div>
                </div>
            `;
        }).join('');

        emptyTip.hidden = true;
    }

    function renderChatComposer() {
        const composer = document.getElementById('chatComposer');
        const input = document.getElementById('chatInput');
        const inputCount = document.getElementById('chatInputCount');
        const sendBtn = document.getElementById('chatSendBtn');
        const atAllToggle = document.getElementById('chatAtAllToggle');
        const readonlyTip = document.getElementById('chatReadonlyTip');
        if (!composer || !input || !inputCount || !sendBtn || !atAllToggle || !readonlyTip) {
            return;
        }

        const selectedGroup = getSelectedChatGroup();
        const isArchived = Boolean(selectedGroup?.read_only) || String(selectedGroup?.status || '').toLowerCase() === 'archived';
        const disabled = !selectedGroup || isArchived || state.chatSending;

        input.value = state.chatInputDraft || '';
        atAllToggle.checked = Boolean(state.chatAtAll);
        atAllToggle.disabled = disabled;
        input.disabled = disabled;
        sendBtn.disabled = disabled || !String(state.chatInputDraft || '').trim();
        inputCount.textContent = `${String(state.chatInputDraft || '').length}/2000`;
        readonlyTip.hidden = !selectedGroup || !isArchived;
    }

    async function initializeDispatchChatFeature() {
        // Connect global SSEHub before chat stream
        if (typeof SSEHub !== 'undefined' && typeof SSEHub.connect === 'function') {
            SSEHub.connect();
        }
        const loaded = await loadChatGroups({ silent: true });
        if (!loaded) {
            return;
        }
        connectDispatchChatStream();
    }

    async function loadChatGroups(options = {}) {
        if (!state.chatEnabled) {
            return false;
        }

        state.chatLoadingGroups = true;
        renderChatGroupList();

        try {
            const payload = await apiCall('/api/v2/dispatch/collaboration/groups?status=all&limit=120&offset=0');
            const items = Array.isArray(payload?.items) ? payload.items : [];
            const selectedGroupId = state.chatSelectedGroupId;

            state.chatGroups = items.map((item) => ({ ...item }));
            sortChatGroups();

            if (Number.isFinite(Number(payload?.unread_total))) {
                state.chatUnreadTotal = Math.max(0, Number(payload.unread_total));
            } else {
                syncChatUnreadTotalFromGroups();
            }

            if (selectedGroupId && !state.chatGroups.some((group) => String(group?.group_id || '') === selectedGroupId)) {
                state.chatSelectedGroupId = '';
                state.chatMessages = [];
                state.chatMessagesHasMore = false;
                state.chatMessagesNextBeforeSeq = null;
            }

            renderChatGroupList();
            renderChatMessages();
            renderChatComposer();
            updateChatUnreadBadge();
            return true;
        } catch (error) {
            if (error && Number(error.status) === 503) {
                setDispatchChatEnabled(false);
                return false;
            }
            if (!options.silent) {
                showToast(error.message || '加载群聊列表失败');
            }
            return false;
        } finally {
            state.chatLoadingGroups = false;
            renderChatGroupList();
        }
    }

    async function ensureChatGroupsLoaded(options = {}) {
        const force = options.force === true;
        if (!force && state.chatGroups.length > 0) {
            return true;
        }
        return await loadChatGroups({ silent: options.silent !== false });
    }

    async function openChatGroupByFlight(flightId, options = {}) {
        const normalizedFlightId = String(flightId || '').trim();
        if (!normalizedFlightId) {
            return false;
        }

        try {
            const group = await apiCall(`/api/v2/dispatch/collaboration/groups/by-flight/${encodeURIComponent(normalizedFlightId)}`);
            upsertChatGroup(group);
            syncChatUnreadTotalFromGroups();
            updateChatUnreadBadge();
            renderChatGroupList();
            await selectChatGroup(group.group_id, { refreshMessages: true, markRead: true });
            return true;
        } catch (error) {
            if (Number(error?.status) === 404) {
                if (!options.silentMissingMembership) {
                    showToast('你不在该航班群聊中');
                }
                return false;
            }
            showToast(error.message || '打开航班群聊失败');
            return false;
        }
    }

    async function openChatDrawer(options = {}) {
        if (!state.chatEnabled) {
            showToast('群聊功能未启用');
            return false;
        }

        const loaded = await ensureChatGroupsLoaded({ silent: false });
        if (!loaded) {
            return false;
        }

        openDrawer('chatDrawer');

        const flightId = String(options.flightId || '').trim();
        if (flightId) {
            const opened = await openChatGroupByFlight(flightId, {
                silentMissingMembership: options.silentMissingMembership === true,
            });
            if (opened) {
                return true;
            }

            const shouldFallbackToFirstGroup = options.fallbackToFirstGroup !== false;
            if (shouldFallbackToFirstGroup && !state.chatSelectedGroupId && state.chatGroups.length > 0) {
                await selectChatGroup(state.chatGroups[0].group_id, { refreshMessages: true, markRead: true });
            } else {
                renderChatGroupList();
                renderChatMessages();
                renderChatComposer();
            }
            return false;
        }

        if (!state.chatSelectedGroupId && state.chatGroups.length > 0) {
            await selectChatGroup(state.chatGroups[0].group_id, { refreshMessages: true, markRead: true });
            return true;
        }

        renderChatGroupList();
        renderChatMessages();
        renderChatComposer();
        return state.chatGroups.length > 0;
    }

    async function focusFlightById(flightId, options = {}) {
        const normalizedFlightId = String(flightId || '').trim();
        if (!normalizedFlightId) {
            return false;
        }

        const items = Array.isArray(state.timelineData?.items) ? state.timelineData.items : [];
        const directSummary = items.find((item) => item && item.is_flight_summary && String(item.flight_id || '').trim() === normalizedFlightId);
        const directOrder = items.find((item) => item && String(item.flight_id || '').trim() === normalizedFlightId);
        const target = directSummary || directOrder || null;

        if (target) {
            await focusTimelineItem(target.id, target.order_id || '', { openDetail: options.openDetail !== false });
            return true;
        }

        try {
            const payload = await apiCall(`/api/v2/dispatch-orders?flight_id=${encodeURIComponent(normalizedFlightId)}&page=1&page_size=100`);
            const orders = Array.isArray(payload) ? payload : [];
            const firstOrder = orders.find((item) => String(item?.id || item?.order_id || '').trim());
            if (firstOrder) {
                await openOrderDetail(firstOrder.id || firstOrder.order_id);
                return true;
            }
        } catch (error) {
            console.warn('按航班定位派工详情失败:', error);
        }

        if (!options.silent) {
            showToast('已打开派工面板，但当前时间窗内未找到对应航班');
        }
        return false;
    }

    function closeChatDrawer() {
        closeDrawer('chatDrawer');
    }

    async function selectChatGroup(groupId, options = {}) {
        const normalizedGroupId = String(groupId || '').trim();
        if (!normalizedGroupId) {
            return;
        }

        const changed = state.chatSelectedGroupId !== normalizedGroupId;
        state.chatSelectedGroupId = normalizedGroupId;
        if (changed) {
            state.chatMessages = [];
            state.chatMessagesHasMore = false;
            state.chatMessagesNextBeforeSeq = null;
            state.chatInputDraft = '';
            state.chatAtAll = false;
        }

        renderChatGroupList();
        renderChatMessages();
        renderChatComposer();

        if (options.refreshMessages !== false) {
            await loadChatMessages(normalizedGroupId, { prepend: false });
        }

        if (options.markRead !== false) {
            await markChatGroupRead(normalizedGroupId, null, { silent: true });
        }
    }

    async function loadChatMessages(groupId, options = {}) {
        const normalizedGroupId = String(groupId || '').trim();
        if (!normalizedGroupId || state.chatLoadingMessages) {
            return;
        }

        const prepend = options.prepend === true;
        const beforeSeq = Number.isFinite(Number(options.beforeSeq)) ? Number(options.beforeSeq) : null;
        state.chatLoadingMessages = true;
        renderChatMessages();

        const messageList = document.getElementById('chatMessageList');
        const previousHeight = messageList ? messageList.scrollHeight : 0;
        const previousTop = messageList ? messageList.scrollTop : 0;

        try {
            const params = new URLSearchParams();
            params.set('limit', prepend ? '40' : '50');
            if (beforeSeq && beforeSeq > 0) {
                params.set('before_seq', String(beforeSeq));
            }

            const payload = await apiCall(
                `/api/v2/dispatch/collaboration/groups/${encodeURIComponent(normalizedGroupId)}/messages?${params.toString()}`
            );
            const items = Array.isArray(payload?.items) ? payload.items : [];

            if (state.chatSelectedGroupId !== normalizedGroupId) {
                return;
            }

            if (prepend) {
                state.chatMessages = dedupeAndSortChatMessages([...items, ...state.chatMessages]);
            } else {
                state.chatMessages = dedupeAndSortChatMessages(items);
            }

            state.chatMessagesHasMore = Boolean(payload?.has_more);
            state.chatMessagesNextBeforeSeq = Number(payload?.next_before_seq || 0) || null;

            renderChatMessages();
            renderChatComposer();

            if (prepend && messageList) {
                const nextHeight = messageList.scrollHeight;
                messageList.scrollTop = Math.max(0, nextHeight - previousHeight + previousTop);
            } else {
                scrollChatToBottom();
            }
        } catch (error) {
            showToast(error.message || '加载群消息失败');
        } finally {
            state.chatLoadingMessages = false;
            renderChatMessages();
            renderChatComposer();
        }
    }

    async function markChatGroupRead(groupId, readSeq, options = {}) {
        const normalizedGroupId = String(groupId || '').trim();
        if (!normalizedGroupId || !state.chatEnabled) {
            return;
        }

        const body = {};
        if (Number.isFinite(Number(readSeq)) && Number(readSeq) > 0) {
            body.read_seq = Number(readSeq);
        }

        try {
            const payload = await apiCall(`/api/v2/dispatch/collaboration/groups/${encodeURIComponent(normalizedGroupId)}/read`, {
                method: 'POST',
                body: JSON.stringify(body),
            });

            const group = state.chatGroups.find((item) => String(item?.group_id || '') === normalizedGroupId);
            if (group) {
                group.unread_count = Math.max(0, Number(payload?.unread_count || 0));
            }
            if (Number.isFinite(Number(payload?.unread_total))) {
                state.chatUnreadTotal = Math.max(0, Number(payload.unread_total));
            } else {
                syncChatUnreadTotalFromGroups();
            }

            renderChatGroupList();
            updateChatUnreadBadge();
        } catch (error) {
            if (!options.silent) {
                showToast(error.message || '标记已读失败');
            }
        }
    }

    async function sendChatMessage() {
        if (!state.chatEnabled || state.chatSending) {
            return;
        }

        const selectedGroup = getSelectedChatGroup();
        if (!selectedGroup) {
            return;
        }

        const isArchived = Boolean(selectedGroup.read_only) || String(selectedGroup.status || '').toLowerCase() === 'archived';
        if (isArchived) {
            showToast('群聊已归档，只读不可发送');
            return;
        }

        const content = String(state.chatInputDraft || '').trim();
        if (!content) {
            return;
        }

        state.chatSending = true;
        renderChatComposer();

        try {
            const message = await apiCall(`/api/v2/dispatch/collaboration/groups/${encodeURIComponent(selectedGroup.group_id)}/messages`, {
                method: 'POST',
                body: JSON.stringify({
                    content,
                    at_all: Boolean(state.chatAtAll),
                }),
            });

            appendChatMessage(message);
            state.chatInputDraft = '';
            state.chatAtAll = false;

            const group = getSelectedChatGroup();
            if (group) {
                group.last_message_preview = String(message?.content || '');
                group.last_message_at = message?.sent_at || new Date().toISOString();
                group.last_message_seq = Number(message?.seq_no || group.last_message_seq || 0);
                group.unread_count = 0;
            }
            syncChatUnreadTotalFromGroups();

            renderChatMessages();
            renderChatComposer();
            renderChatGroupList();
            updateChatUnreadBadge();
            scrollChatToBottom();

            await markChatGroupRead(selectedGroup.group_id, Number(message?.seq_no || 0), { silent: true });
        } catch (error) {
            showToast(error.message || '发送消息失败');
        } finally {
            state.chatSending = false;
            renderChatComposer();
        }
    }

    function applyChatMessageEvent(payload) {
        if (!payload || typeof payload !== 'object') {
            return;
        }

        const groupId = String(payload.group_id || '').trim();
        const message = payload.message;
        if (!groupId || !message) {
            return;
        }

        let group = state.chatGroups.find((item) => String(item?.group_id || '') === groupId);
        if (!group) {
            group = {
                group_id: groupId,
                flight_id: String(payload.flight_id || ''),
                group_name: `航班 ${String(payload.flight_id || '-')}`,
                status: 'active',
                read_only: false,
                member_count: 0,
                unread_count: 0,
            };
            state.chatGroups.push(group);
        }

        group.last_message_preview = String(message?.content || '');
        group.last_message_at = message?.sent_at || new Date().toISOString();
        group.last_message_seq = Number(message?.seq_no || group.last_message_seq || 0);
        group.unread_count = Math.max(0, Number(payload.unread_count || 0));

        if (Number.isFinite(Number(payload.unread_total))) {
            state.chatUnreadTotal = Math.max(0, Number(payload.unread_total));
        } else {
            syncChatUnreadTotalFromGroups();
        }

        sortChatGroups();

        if (state.chatSelectedGroupId === groupId) {
            appendChatMessage(message);
            renderChatMessages();
            scrollChatToBottom();

            const messageSender = String(message?.sender_user_id || '').trim();
            const currentUserId = getCurrentUserId();
            if (isChatDrawerOpen() && messageSender && currentUserId && messageSender !== currentUserId) {
                markChatGroupRead(groupId, Number(message?.seq_no || 0), { silent: true });
            }
        }

        renderChatGroupList();
        renderChatComposer();
        updateChatUnreadBadge();
    }

    function applyChatGroupUpsertEvent(payload) {
        if (!payload || typeof payload !== 'object') {
            return;
        }
        const group = payload.group;
        if (!group || !group.group_id) {
            return;
        }
        upsertChatGroup(group);
        if (!state.chatSelectedGroupId) {
            state.chatSelectedGroupId = String(group.group_id);
        }
        syncChatUnreadTotalFromGroups();
        renderChatGroupList();
        renderChatMessages();
        renderChatComposer();
        updateChatUnreadBadge();
    }

    function applyChatGroupArchivedEvent(payload) {
        if (!payload || typeof payload !== 'object') {
            return;
        }
        const groupId = String(payload.group_id || '').trim();
        if (!groupId) {
            return;
        }

        const group = state.chatGroups.find((item) => String(item?.group_id || '') === groupId);
        if (!group) {
            return;
        }

        group.status = 'archived';
        group.read_only = true;
        group.archived_at = payload.archived_at || group.archived_at || null;
        sortChatGroups();
        renderChatGroupList();
        renderChatMessages();
        renderChatComposer();
    }

    function applyChatReadSyncedEvent(payload) {
        if (!payload || typeof payload !== 'object') {
            return;
        }
        const groupId = String(payload.group_id || '').trim();
        if (!groupId) {
            return;
        }

        const group = state.chatGroups.find((item) => String(item?.group_id || '') === groupId);
        if (group) {
            group.unread_count = Math.max(0, Number(payload.unread_count || 0));
        }
        if (Number.isFinite(Number(payload.unread_total))) {
            state.chatUnreadTotal = Math.max(0, Number(payload.unread_total));
        } else {
            syncChatUnreadTotalFromGroups();
        }

        renderChatGroupList();
        updateChatUnreadBadge();
    }

    function handleDispatchChatPayload(payload, explicitEvent) {
        if (!payload || typeof payload !== 'object') {
            return;
        }

        const eventName = String(explicitEvent || '').trim();
        const payloadType = String(payload.type || '').trim().toLowerCase();

        if (eventName === 'initial' || payloadType === 'dispatch_chat_initial') {
            const items = Array.isArray(payload.items) ? payload.items : [];
            state.chatGroups = items.map((item) => ({ ...item }));
            sortChatGroups();
            if (Number.isFinite(Number(payload.unread_total))) {
                state.chatUnreadTotal = Math.max(0, Number(payload.unread_total));
            } else {
                syncChatUnreadTotalFromGroups();
            }

            if (state.chatSelectedGroupId && !state.chatGroups.some((group) => String(group?.group_id || '') === state.chatSelectedGroupId)) {
                state.chatSelectedGroupId = '';
                state.chatMessages = [];
                state.chatMessagesHasMore = false;
                state.chatMessagesNextBeforeSeq = null;
            }

            renderChatGroupList();
            renderChatMessages();
            renderChatComposer();
            updateChatUnreadBadge();
            return;
        }

        if (eventName === 'chat_message' || payloadType === 'dispatch_chat_message') {
            applyChatMessageEvent(payload);
            return;
        }

        if (eventName === 'chat_group_upserted' || payloadType === 'dispatch_chat_group_upserted') {
            applyChatGroupUpsertEvent(payload);
            return;
        }

        if (eventName === 'chat_group_archived' || payloadType === 'dispatch_chat_group_archived') {
            applyChatGroupArchivedEvent(payload);
            return;
        }

        if (eventName === 'chat_read_synced' || payloadType === 'dispatch_chat_read_synced') {
            applyChatReadSyncedEvent(payload);
        }
    }

    function disconnectDispatchChatStream(clearReconnect) {
        if (state._chatSseHubHandlers) {
            Object.keys(state._chatSseHubHandlers).forEach(function (eventName) {
                var key = eventName === '_message' ? 'message' : eventName;
                SSEHub.off(key, state._chatSseHubHandlers[eventName]);
            });
            state._chatSseHubHandlers = null;
        }
        state.chatStream = null;

        if (clearReconnect !== false && state.chatReconnectTimer) {
            window.clearTimeout(state.chatReconnectTimer);
            state.chatReconnectTimer = null;
        }
    }

    function scheduleDispatchChatReconnect() {
        // No-op: SSEHub handles reconnection
    }

    function connectDispatchChatStream() {
        if (!state.chatEnabled) {
            return;
        }

        disconnectDispatchChatStream(false);

        var parsePayload = function (raw) {
            if (!raw) { return null; }
            if (typeof raw === 'object') { return raw; }
            try { return JSON.parse(raw); } catch (_e) { return null; }
        };

        var makeHandler = function (eventName) {
            return function (event) {
                var payload = parsePayload(event && event.data);
                if (!payload) { return; }
                handleDispatchChatPayload(payload, eventName);
            };
        };

        var chatEvents = ['dispatch_chat_initial', 'chat_message', 'chat_group_upserted', 'chat_group_archived', 'chat_read_synced'];
        state._chatSseHubHandlers = {};
        chatEvents.forEach(function (eventName) {
            var handler = makeHandler(eventName);
            state._chatSseHubHandlers[eventName] = handler;
            SSEHub.on(eventName, handler);
        });

        var genericHandler = function (event) {
            var payload = parsePayload(event && event.data);
            if (!payload) { return; }
            handleDispatchChatPayload(payload, '');
        };
        state._chatSseHubHandlers['_message'] = genericHandler;
        SSEHub.on('message', genericHandler);

        state.chatStream = { _sseHub: true };
    }

    function applySettings() {
        const refreshSelect = document.getElementById('settingRefreshInterval');
        const cornerFadeToggle = document.getElementById('settingCornerFade');
        const safetyGateFilterSelect = document.getElementById('settingSafetyGateFilter');

        if (refreshSelect) {
            const nextInterval = Number(refreshSelect.value);
            if (Number.isFinite(nextInterval) && nextInterval >= 5000) {
                state.refreshIntervalMs = nextInterval;
            }
        }

        state.cornerInfoAutoFade = !cornerFadeToggle || Boolean(cornerFadeToggle.checked);
        if (state.cornerInfoAutoFade) {
            scheduleCornerInfoFade(500);
        } else {
            const cornerInfo = document.getElementById('cornerInfo');
            if (cornerInfo) {
                cornerInfo.classList.remove('faded');
            }
        }

        if (safetyGateFilterSelect) {
            const nextFilter = String(safetyGateFilterSelect.value || 'all');
            if (['all', 'ready', 'pending', 'blocked'].includes(nextFilter)) {
                const changed = state.safetyGateFilter !== nextFilter;
                state.safetyGateFilter = nextFilter;
                if (changed) {
                    state.statusPanelSelectedOrderIds.clear();
                    stopStatusPanelBatchProcess({ silent: true });
                }
            }
        }

        syncSearchWithTimeline();
        renderChart();
        renderStatusCounts();
        renderStatusOrderList();
        renderViewModeHint();

        startTimers();
        closeOpsMenu();
        showToast(`设置已应用（门禁筛选：${getSafetyGateFilterLabel()}）`);
    }

    function truncateText(text, limit = 180) {
        if (text === null || text === undefined) {
            return '';
        }
        const normalized = String(text).replace(/\s+/g, ' ').trim();
        if (!normalized) {
            return '';
        }
        if (normalized.length <= limit) {
            return normalized;
        }
        return `${normalized.slice(0, Math.max(0, limit - 1))}…`;
    }

    function buildDispatchAdvisorPrompt(objective, timeline, pendingOrders, backendConflicts, heavyLanes) {
        const objectiveLabels = {
            clear_pending: '优先清空待派工',
            resolve_conflicts: '优先消解资源冲突',
            balance_load: '优先均衡负载',
            delay_prevention: '优先预防延误',
        };

        const statusCounts = timeline?.status_counts || {};
        const windowText = `${formatDateTime(state.windowStartMs)} - ${formatDateTime(state.windowEndMs)}`;
        const topPending = pendingOrders.slice(0, 3).map((item) => item.label).filter(Boolean);
        const topConflict = backendConflicts.slice(0, 2).map((item) => {
            const severity = String(item?.severity || 'medium').toUpperCase();
            const typeLabel = conflictTypeLabel(item?.conflict_type || '');
            const message = truncateText(item?.message || '', 72);
            return `${severity}/${typeLabel}${message ? `: ${message}` : ''}`;
        });
        const heavyLaneLabels = heavyLanes.slice(0, 3).map((lane) => lane.label || lane.id).filter(Boolean);

        return [
            '请作为机场运行调度专家，基于以下甘特调度数据给出可执行建议。',
            `调度目标: ${objectiveLabels[objective] || '综合优化'}`,
            `时间窗: ${windowText}`,
            `当前航站楼筛选: ${state.terminal || 'all'}`,
            `任务统计: 待派工 ${statusCounts.pending || 0} / 已分配 ${statusCounts.assigned || 0} / 进行中 ${statusCounts.in_progress || 0}`,
            `冲突数量: ${backendConflicts.length}`,
            `高负载资源行: ${heavyLanes.length}`,
            `重点待派工: ${topPending.length ? topPending.join('、') : '无'}`,
            `重点冲突: ${topConflict.length ? topConflict.join('；') : '无'}`,
            `重点高负载: ${heavyLaneLabels.length ? heavyLaneLabels.join('、') : '无'}`,
            '请输出 3-5 条建议，按优先级排序，并尽量给出操作顺序。',
        ].join('\n');
    }

    function extractAdvisorRecommendationText(result) {
        if (result === null || result === undefined) {
            return '';
        }
        if (typeof result === 'string') {
            return result;
        }
        if (typeof result.recommendations === 'string') {
            return result.recommendations;
        }
        if (Array.isArray(result.recommendations)) {
            return result.recommendations.join('\n');
        }
        if (typeof result.report === 'string') {
            return result.report;
        }
        if (typeof result.message === 'string') {
            return result.message;
        }
        return '';
    }

    async function buildAdvisorSuggestion(objective, timeline, pendingOrders, backendConflicts, heavyLanes) {
        const prompt = buildDispatchAdvisorPrompt(objective, timeline, pendingOrders, backendConflicts, heavyLanes);
        const urgency = objective === 'resolve_conflicts' || backendConflicts.length > 0 ? '高' : '中';
        const response = await apiCall('/api/v2/ai/tools/execute', {
            method: 'POST',
            body: JSON.stringify({
                tool_name: 'get_handling_recommendation',
                tool_args: {
                    incident_description: prompt,
                    urgency,
                },
            }),
        });

        if (!response || response.success !== true) {
            return null;
        }

        const recommendationText = extractAdvisorRecommendationText(response?.data?.result);
        if (!recommendationText) {
            return null;
        }

        return {
            id: 'advisor-priority',
            kind: 'ai_advisor',
            title: 'AI 调度建议',
            detail: truncateText(recommendationText, 160),
            hint: '可先预览定位，再结合右侧冲突治理面板执行。',
            orderId: pendingOrders[0]?.order_id || null,
            focusItemId: pendingOrders[0]?.focus_item_id || null,
            fullText: recommendationText,
        };
    }

    async function generateAiSuggestions() {
        const objectiveSelect = document.getElementById('aiObjective');
        const objective = objectiveSelect ? objectiveSelect.value : 'clear_pending';
        const timeline = state.timelineData;

        if (!timeline) {
            state.aiSuggestions = [];
            renderAiSuggestions();
            return;
        }

        if (objective === 'resolve_conflicts' && (!Array.isArray(state.conflictsRaw) || state.conflictsRaw.length === 0)) {
            await refreshConflictData({ force: true, silent: true });
        }

        const suggestions = [];
        const pendingOrders = ((timeline.status_orders || {}).pending || []);
        const conflictLanes = (timeline.lanes || []).filter((lane) => Number(lane.subtrack_count || 1) > 1);
        const heavyLanes = getHeavyLanes(timeline);
        const backendConflicts = Array.isArray(state.conflictsRaw) ? state.conflictsRaw : [];
        let advisorSuggestion = null;

        try {
            advisorSuggestion = await buildAdvisorSuggestion(objective, timeline, pendingOrders, backendConflicts, heavyLanes);
        } catch (error) {
            console.warn('buildAdvisorSuggestion failed:', error);
        }

        if (pendingOrders.length > 0) {
            const firstPending = pendingOrders[0];
            suggestions.push({
                id: 'pending-priority',
                title: `优先清理待派工（${pendingOrders.length}项）`,
                detail: `建议先处理最早待派工任务：${firstPending.label}，避免持续挤压后续时间窗。`,
                focusItemId: firstPending.focus_item_id,
                orderId: firstPending.order_id,
                hint: '可先打开状态定位器 -> 待派工 -> 按时间顺序执行。'
            });
        }

        if (backendConflicts.length > 0) {
            const sortedConflicts = [...backendConflicts]
                .sort((left, right) => conflictSeverityWeight(right.severity) - conflictSeverityWeight(left.severity));
            const firstConflict = sortedConflicts[0];
            const orderIds = normalizeConflictOrderIds(firstConflict.related_dispatch_order_ids);
            const primaryOrderId = orderIds[0] || null;
            const focusTarget = resolveFocusTargetForOrder(primaryOrderId);
            const conflictTypeLabel = CONFLICT_TYPE_LABELS[firstConflict.conflict_type] || firstConflict.conflict_type || '冲突';
            const severityLabel = String(firstConflict.severity || 'medium').toUpperCase();
            suggestions.push({
                id: 'conflict-backend-primary',
                kind: 'backend_conflict',
                title: `冲突治理优先级 (${backendConflicts.length}项)`,
                detail: `${severityLabel} | ${conflictTypeLabel}：${firstConflict.message || '存在资源冲突，建议优先处理。'}`,
                focusItemId: focusTarget ? focusTarget.itemId : null,
                orderId: primaryOrderId,
                orderIds,
                conflictType: firstConflict.conflict_type,
                severity: firstConflict.severity,
                hint: '可切换到“冲突治理”视图筛选并联动定位，再做重排预览。'
            });
        } else if (conflictLanes.length > 0) {
            const firstConflict = conflictLanes[0];
            const candidate = (timeline.items || []).find((item) => item.lane_id === firstConflict.id && Number(item.lane_subtrack || 0) > 0)
                || (timeline.items || []).find((item) => item.lane_id === firstConflict.id);
            suggestions.push({
                id: 'conflict-lane',
                title: `冲突行优先消解（${conflictLanes.length}条）`,
                detail: `资源行“${firstConflict.label}”存在并发子轨，建议优先调整任务开始时间或替换资源。`,
                focusItemId: candidate ? candidate.id : null,
                orderId: candidate ? candidate.order_id : null,
                hint: '可切换到对应资源视角，观察同一资源行的冲突段。'
            });
        }

        if (heavyLanes.length > 0) {
            const firstHeavy = heavyLanes[0];
            const candidate = (timeline.items || []).find((item) => item.lane_id === firstHeavy.id);
            suggestions.push({
                id: 'load-balance',
                title: `负载均衡建议（${heavyLanes.length}条高负载行）`,
                detail: `“${firstHeavy.label}”当前任务负载明显高于均值，建议拆分到空闲资源。`,
                focusItemId: candidate ? candidate.id : null,
                orderId: candidate ? candidate.order_id : null,
                hint: '可转到班组/员工/设备视角进行资源重分配。'
            });
        }

        if (objective === 'delay_prevention') {
            const urgent = pendingOrders.slice(0, 1).concat(((timeline.status_orders || {}).in_progress || []).slice(0, 1));
            if (urgent.length > 0) {
                suggestions.unshift({
                    id: 'delay-guard',
                    title: '延误预防优先链',
                    detail: `建议先确保 ${urgent.map((item) => item.label).join(' -> ')} 的执行连续性。`,
                    focusItemId: urgent[0].focus_item_id,
                    orderId: urgent[0].order_id,
                    hint: '优先保证航班关键节点不中断，必要时抢占低优先级资源。'
                });
            }
        }

        if (objective === 'resolve_conflicts') {
            suggestions.sort((left, right) => {
                const leftScore = left.id === 'conflict-backend-primary' ? 3 : (left.id === 'conflict-lane' ? 2 : 1);
                const rightScore = right.id === 'conflict-backend-primary' ? 3 : (right.id === 'conflict-lane' ? 2 : 1);
                return rightScore - leftScore;
            });
        } else if (objective === 'balance_load') {
            suggestions.sort((a, b) => (a.id === 'load-balance' ? -1 : 1) - (b.id === 'load-balance' ? -1 : 1));
        } else if (objective === 'clear_pending') {
            suggestions.sort((a, b) => (a.id === 'pending-priority' ? -1 : 1) - (b.id === 'pending-priority' ? -1 : 1));
        }

        if (advisorSuggestion) {
            suggestions.unshift(advisorSuggestion);
            if (state.aiAssistantWidget) {
                state.aiAssistantWidget.pushMessage({
                    title: '派工 AI 建议已更新',
                    body: truncateText(advisorSuggestion.fullText || advisorSuggestion.detail || '', 140),
                    type: 'success',
                });
            }
        } else if (state.aiAssistantWidget) {
            state.aiAssistantWidget.pushMessage({
                title: '已生成本地排班建议',
                body: '后端 AI 建议暂不可用，已回退为规则建议。',
                type: 'warning',
            });
        }

        state.aiSuggestions = suggestions;
        renderAiSuggestions();
        showToast(`已生成 ${suggestions.length} 条排班建议`);
    }

    function renderAiSuggestions() {
        const list = document.getElementById('aiSuggestionList');
        if (!list) {
            return;
        }

        if (!state.aiSuggestions || state.aiSuggestions.length === 0) {
            list.innerHTML = '<div class="empty-state" style="height:auto;padding:12px 0;">点击“生成建议”开始智能排班</div>';
            return;
        }

        list.innerHTML = state.aiSuggestions.map((suggestion) => {
            return `
                <div class="ai-suggestion">
                    <h4>${escapeHtml(suggestion.title)}</h4>
                    <p>${escapeHtml(suggestion.detail)}</p>
                    <p>${escapeHtml(suggestion.hint || '')}</p>
                    <div class="ai-suggestion-actions">
                        <button class="suggestion-chip" data-action="preview" data-suggestion-id="${escapeHtmlAttribute(suggestion.id)}">预览定位</button>
                        <button class="suggestion-chip" data-action="apply" data-suggestion-id="${escapeHtmlAttribute(suggestion.id)}">加入执行清单</button>
                    </div>
                </div>
            `;
        }).join('');
    }

    async function previewSuggestion(suggestion) {
        if (!suggestion) {
            return;
        }
        if (suggestion.focusItemId || suggestion.orderId) {
            await focusTimelineItem(suggestion.focusItemId, suggestion.orderId);
            showToast('已定位建议对应任务');
            return;
        }
        showToast('当前建议暂无可定位目标');
    }

    async function applySuggestion(suggestion) {
        if (!suggestion) {
            return;
        }

        if (suggestion.kind === 'backend_conflict') {
            if (Array.isArray(suggestion.orderIds) && suggestion.orderIds.length > 0) {
                setImpactedOrders(suggestion.orderIds, { render: true });
                state.conflictSelectedOrderId = suggestion.orderIds[0];
            }
            state.conflictType = suggestion.conflictType || 'all';
            state.conflictSeverity = suggestion.severity || 'all';
            await openAiDrawer('conflict', { refresh: true });
            showToast('已切换到冲突治理视图，可继续预览重排');
            return;
        }

        if (suggestion.id === 'pending-priority') {
            state.selectedStatus = 'pending';
            renderStatusCounts();
            renderStatusOrderList();
            openStatusPanel();
            showToast('已将待派工优先级建议加入执行清单');
            return;
        }

        if (suggestion.id === 'conflict-lane' && state.viewMode === 'flight') {
            state.viewMode = 'team';
            const viewTabGroup = document.getElementById('viewTabGroup');
            if (viewTabGroup) {
                syncActiveButtons(viewTabGroup, '.chip-btn[data-view]', 'view', 'team');
            }
            await refreshTimeline();
        }

        if (suggestion.kind === 'ai_advisor' && state.aiAssistantWidget) {
            state.aiAssistantWidget.setOpen(true);
        }

        if (suggestion.focusItemId || suggestion.orderId) {
            await focusTimelineItem(suggestion.focusItemId, suggestion.orderId);
        }
        showToast('建议已加入人工执行流程（需确认后实施）');
    }

    async function switchAiDrawerTab(tab, options = {}) {
        const normalized = tab === 'conflict' ? 'conflict' : 'assistant';
        state.aiDrawerTab = normalized;

        const bridge = getDispatchAiBridge();
        if (bridge && typeof bridge.setActiveTab === 'function') {
            bridge.setActiveTab(normalized, { refresh: options.refresh !== false });
            return;
        }

        const aiStreamToggle = document.getElementById('aiStreamToggle');
        if (aiStreamToggle) {
            aiStreamToggle.checked = Boolean(state.aiStreamEnabled);
        }

        const tabContainer = document.getElementById('aiDrawerTabs');
        if (tabContainer) {
            syncActiveButtons(tabContainer, '.drawer-tab[data-ai-tab]', 'aiTab', normalized);
        }

        document.querySelectorAll('[data-ai-panel]').forEach((panel) => {
            const isTarget = panel.dataset.aiPanel === normalized;
            panel.classList.toggle('active', isTarget);
        });

        if (normalized === 'assistant') {
            renderAnalyticsPanel();
            if (options.refresh !== false || !state.analyticsSummary) {
                await refreshAnalyticsData({ force: false, silent: true });
            }
            return;
        }

        if (normalized === 'conflict') {
            applyConflictFilters();
            renderConflictGovernance();
            if (options.refresh !== false || state.conflictsRaw.length === 0) {
                await refreshConflictData({ force: false, silent: true });
            }
        }
    }

    function renderConflictGovernance() {
        const severitySelect = document.getElementById('conflictSeverityFilter');
        if (severitySelect) {
            severitySelect.value = state.conflictSeverity || 'all';
        }
        const queryInput = document.getElementById('conflictQueryInput');
        if (queryInput && queryInput.value !== (state.conflictQuery || '')) {
            queryInput.value = state.conflictQuery || '';
        }
        const replanStrategySelect = document.getElementById('replanStrategy');
        if (replanStrategySelect) {
            replanStrategySelect.value = state.replanStrategy || 'balanced';
        }
        const replanMaxSelect = document.getElementById('replanMaxSuggestions');
        if (replanMaxSelect) {
            replanMaxSelect.value = String(state.replanMaxSuggestions || 20);
        }

        renderConflictTypeFilterOptions();
        renderConflictMetrics();
        renderConflictDataHint();
        renderConflictList();
        renderScenarioPanel();
        renderReplanPreview();
        renderReplanHint();
        renderReplanActionState();
    }

    function renderConflictTypeFilterOptions() {
        const select = document.getElementById('conflictTypeFilter');
        if (!select) {
            return;
        }

        const types = Array.from(new Set((state.conflictsRaw || [])
            .map((item) => String(item?.conflict_type || '').trim())
            .filter(Boolean)));
        types.sort((left, right) => left.localeCompare(right, 'zh-CN'));

        const currentValue = state.conflictType || 'all';
        const options = ['<option value="all">全部冲突类型</option>'];
        for (const type of types) {
            options.push(
                `<option value="${escapeHtmlAttribute(type)}">${escapeHtml(conflictTypeLabel(type))}</option>`
            );
        }
        select.innerHTML = options.join('');

        if (types.includes(currentValue)) {
            select.value = currentValue;
        } else {
            state.conflictType = 'all';
            select.value = 'all';
        }
    }

    function renderConflictMetrics() {
        const totalEl = document.getElementById('conflictMetricTotal');
        const highEl = document.getElementById('conflictMetricHigh');
        const ordersEl = document.getElementById('conflictMetricOrders');

        const rawConflicts = Array.isArray(state.conflictsRaw) ? state.conflictsRaw : [];
        const total = rawConflicts.length;
        const high = rawConflicts.filter((item) => {
            const severity = String(item?.severity || '').toLowerCase();
            return severity === 'critical' || severity === 'high';
        }).length;

        const impactedOrderSet = new Set();
        for (const conflict of rawConflicts) {
            for (const orderId of normalizeConflictOrderIds(conflict?.related_dispatch_order_ids)) {
                impactedOrderSet.add(orderId);
            }
        }

        if (totalEl) {
            totalEl.textContent = String(total);
        }
        if (highEl) {
            highEl.textContent = String(high);
        }
        if (ordersEl) {
            ordersEl.textContent = String(impactedOrderSet.size);
        }
    }

    function renderConflictDataHint() {
        const hint = document.getElementById('conflictDataHint');
        if (!hint) {
            return;
        }

        if (state.conflictLoading) {
            hint.textContent = '正在刷新冲突数据...';
            return;
        }

        if (state.conflictError) {
            hint.textContent = `冲突数据加载失败：${state.conflictError}`;
            return;
        }

        const count = Array.isArray(state.conflictsFiltered) ? state.conflictsFiltered.length : 0;
        const total = Array.isArray(state.conflictsRaw) ? state.conflictsRaw.length : 0;
        const timestamp = state.conflictLastUpdatedAt > 0
            ? formatDateTime(state.conflictLastUpdatedAt)
            : '-';
        hint.textContent = `已筛选 ${count} / ${total} 项，最后更新 ${timestamp}`;
    }

    function renderConflictList() {
        const list = document.getElementById('conflictList');
        if (!list) {
            return;
        }

        if (state.conflictLoading && (!state.conflictsRaw || state.conflictsRaw.length === 0)) {
            if (!showContainerLoading(list, '正在加载冲突数据...', { minHeight: '120px' })) {
                list.innerHTML = '<div class="empty-state" style="height:auto;padding:12px 0;">正在加载冲突数据...</div>';
            }
            return;
        }

        const conflicts = Array.isArray(state.conflictsFiltered) ? state.conflictsFiltered : [];
        if (conflicts.length === 0) {
            const message = state.conflictError
                ? '冲突数据暂不可用，请稍后刷新重试'
                : '当前时间窗未发现冲突';
            if (!renderUnifiedState(list, state.conflictError ? 'error' : 'empty', message)) {
                list.innerHTML = `<div class="empty-state" style="height:auto;padding:12px 0;">${escapeHtml(message)}</div>`;
            }
            return;
        }

        hideContainerLoading(list);

        const visibleConflicts = conflicts.slice(0, CONFLICT_LIST_LIMIT);
        list.innerHTML = visibleConflicts.map((conflict, index) => {
            const severity = String(conflict?.severity || 'medium').toLowerCase();
            const conflictType = String(conflict?.conflict_type || 'unknown');
            const orderIds = normalizeConflictOrderIds(conflict?.related_dispatch_order_ids);
            const resourceText = String(conflict?.resource_name || conflict?.resource_id || '').trim();
            const suggestedAction = String(conflict?.suggested_action || '').trim();
            const isSelected = orderIds.some((orderId) => orderId === state.conflictSelectedOrderId);
            const resourceFocus = buildResourceFocusFromConflictItem(conflict, 'conflict');
            const isFocusedResource = resourceFocus
                ? isResourceFocusActive(resourceFocus.resource_type, resourceFocus.resource_id, resourceFocus.resource_label)
                : false;

            const orderChips = orderIds.length > 0
                ? `<div class="conflict-order-links">${orderIds.map((orderId) => `
                    <button class="conflict-order-chip" data-action="locate-conflict-order" data-order-id="${escapeHtmlAttribute(orderId)}">${escapeHtml(orderId)}</button>
                `).join('')}</div>`
                : '';

            return `
                <div class="ai-suggestion${(isSelected || isFocusedResource) ? ' is-selected' : ''}">
                    <div class="conflict-item-head">
                        <h4 class="conflict-item-title">${escapeHtml(conflict.message || '检测到冲突')}</h4>
                        <button class="suggestion-chip" data-action="locate-conflict" data-conflict-index="${index}">定位</button>
                    </div>
                    <div class="conflict-tags">
                        <span class="conflict-tag severity-${escapeHtmlAttribute(severity)}">${escapeHtml(conflictSeverityLabel(severity))}</span>
                        <span class="conflict-tag">${escapeHtml(conflictTypeLabel(conflictType))}</span>
                        ${resourceText ? `<span class="conflict-tag">${escapeHtml(resourceText)}</span>` : ''}
                    </div>
                    ${suggestedAction ? `<p>${escapeHtml(suggestedAction)}</p>` : ''}
                    ${resourceFocus ? `<div class="ai-suggestion-actions"><button class="suggestion-chip" data-action="focus-conflict-resource" data-conflict-index="${escapeHtmlAttribute(String(index))}" data-order-id="${escapeHtmlAttribute(orderIds[0] || '')}" data-resource-type="${escapeHtmlAttribute(resourceFocus.resource_type)}" data-resource-id="${escapeHtmlAttribute(resourceFocus.resource_id)}" data-resource-label="${escapeHtmlAttribute(resourceFocus.resource_label)}" data-view-mode="${escapeHtmlAttribute(resourceFocus.target_view_mode)}" data-related-order-ids="${escapeHtmlAttribute(orderIds.join(','))}" data-resource-ids="${escapeHtmlAttribute((resourceFocus.resource_ids || []).join(','))}" data-lane-ids="${escapeHtmlAttribute((resourceFocus.lane_ids || []).join(','))}" data-highlight-scope="${escapeHtmlAttribute(resourceFocus.highlight_scope || 'single')}" data-source-key="${escapeHtmlAttribute(resourceFocus.source_key)}">${isFocusedResource ? '已聚焦资源行' : '定位资源行'}</button></div>` : ''}
                    ${orderChips}
                </div>
            `;
        }).join('');

        if (conflicts.length > CONFLICT_LIST_LIMIT) {
            list.insertAdjacentHTML(
                'beforeend',
                `<div class="empty-state" style="height:auto;padding:8px 0;">共 ${conflicts.length} 项，仅显示前 ${CONFLICT_LIST_LIMIT} 项</div>`
            );
        }
    }

    function replanSuggestionTypeLabel(type) {
        switch (String(type || '').trim()) {
            case 'assigned_conflict_resolution':
                return '冲突修复项';
            case 'unassigned_new_assignment':
                return '新增派单项';
            case 'unassigned_late_assignment':
                return '迟到派单项';
            default:
                return '重排建议';
        }
    }

    function formatAssignmentSummary(assignment) {
        const parts = [];
        const assigneeType = String(assignment?.assignee_type || '').trim();
        const teamId = String(assignment?.team_id || '').trim();
        const userId = String(assignment?.individual_user_id || '').trim();
        const taskCrewMembers = Array.isArray(assignment?.task_crew?.members)
            ? assignment.task_crew.members
            : [];
        const equipmentIds = Array.isArray(assignment?.equipment_ids)
            ? assignment.equipment_ids.map((item) => String(item || '').trim()).filter(Boolean)
            : [];
        if (assigneeType === 'team' && teamId) {
            parts.push(`归属班组 ${teamId}`);
        }
        if (assigneeType === 'individual' && userId) {
            parts.push(`人员 ${userId}`);
        }
        if (taskCrewMembers.length > 0) {
            const crewText = taskCrewMembers
                .map((member) => {
                    const userLabel = String(member?.username || member?.user_id || '').trim();
                    const slotCode = String(member?.slot_code || '').trim();
                    const levelCode = String(member?.qualification_level_code || '').trim();
                    const suffix = [slotCode, levelCode].filter(Boolean).join(' / ');
                    if (!userLabel && !suffix) {
                        return '';
                    }
                    return suffix ? `${userLabel || '-'} (${suffix})` : userLabel;
                })
                .filter(Boolean)
                .join(', ');
            if (crewText) {
                parts.push(`编组 ${crewText}`);
            }
        }
        if (!assigneeType) {
            if (teamId) {
                parts.push(`归属班组 ${teamId}`);
            }
            if (userId) {
                parts.push(`人员 ${userId}`);
            }
        }
        if (equipmentIds.length > 0) {
            parts.push(`设备 ${equipmentIds.join(', ')}`);
        }
        return parts.length > 0 ? parts.join(' · ') : '未指定资源';
    }

    function formatTaskCrewMemberLabel(member) {
        const userLabel = String(member?.username || member?.user_id || '').trim();
        const slotCode = String(member?.slot_code || '').trim();
        const levelCode = String(member?.qualification_level_code || '').trim();
        const suffix = [slotCode, levelCode].filter(Boolean).join(' / ');
        if (!userLabel && !suffix) {
            return '';
        }
        return suffix ? `${userLabel || '-'} (${suffix})` : userLabel;
    }

    function renderMemberChangeSummary(summary) {
        if (!summary || typeof summary !== 'object') {
            return '';
        }
        const replacedMembers = Array.isArray(summary.replaced_members) ? summary.replaced_members : [];
        const addedMembers = Array.isArray(summary.added_members) ? summary.added_members : [];
        const removedMembers = Array.isArray(summary.removed_members) ? summary.removed_members : [];
        const lines = [];
        if (replacedMembers.length > 0) {
            const text = replacedMembers.map((item) => {
                const slotCode = String(item?.slot_code || '').trim() || '-';
                const beforeLabel = formatTaskCrewMemberLabel(item?.before || {});
                const afterLabel = formatTaskCrewMemberLabel(item?.after || {});
                return `${slotCode}: ${beforeLabel || '-'} -> ${afterLabel || '-'}`;
            }).join('；');
            lines.push(`<p class="replan-impact">成员替换：${escapeHtml(text)}</p>`);
        }
        if (addedMembers.length > 0) {
            const text = addedMembers.map((item) => formatTaskCrewMemberLabel(item?.member || {})).filter(Boolean).join('；');
            if (text) {
                lines.push(`<p class="replan-impact">新增成员：${escapeHtml(text)}</p>`);
            }
        }
        if (removedMembers.length > 0) {
            const text = removedMembers.map((item) => formatTaskCrewMemberLabel(item?.member || {})).filter(Boolean).join('；');
            if (text) {
                lines.push(`<p class="replan-impact">移出成员：${escapeHtml(text)}</p>`);
            }
        }
        return lines.join('');
    }

    function renderQualificationGapSummary(gaps) {
        const normalized = Array.isArray(gaps) ? gaps : [];
        if (normalized.length === 0) {
            return '';
        }
        const text = normalized.map((item) => {
            const slotCode = String(item?.slot_code || '').trim();
            const qualificationCode = String(item?.qualification_code || '').trim();
            const levelCode = String(item?.min_level_code || '').trim();
            return [slotCode, qualificationCode, levelCode].filter(Boolean).join(' / ');
        }).filter(Boolean).join('；');
        if (!text) {
            return '';
        }
        return `<p class="replan-impact">资质缺口：${escapeHtml(text)}</p>`;
    }

    function findReplanSuggestionForOrder(order) {
        const orderId = String(order?.id || order?.order_id || '').trim();
        if (!orderId) {
            return null;
        }
        const suggestions = getReplanOrderResults();
        return suggestions.find((item) => {
            const dispatchOrderId = String(item?.dispatch_order_id || item?.order_id || '').trim();
            const relatedOrderId = String(item?.related_dispatch_order_id || '').trim();
            return dispatchOrderId === orderId || relatedOrderId === orderId;
        }) || null;
    }

    function renderDetailReplanContext(order) {
        const suggestion = findReplanSuggestionForOrder(order);
        const memberChangeHtml = renderMemberChangeSummary(suggestion?.member_change_summary || order?.member_change_summary);
        const qualificationGapHtml = renderQualificationGapSummary(suggestion?.qualification_gap || order?.qualification_gap);
        const requiresManualConfirmation = suggestion?.requires_manual_confirmation ?? order?.requires_manual_confirmation;
        const assignmentDiff = suggestion
            ? `<p class="replan-impact">编组建议：${escapeHtml(formatAssignmentSummary(suggestion?.current_assignment))} -> ${escapeHtml(formatAssignmentSummary(suggestion?.suggested_assignment))}</p>`
            : '';
        const manualConfirmationHtml = requiresManualConfirmation
            ? '<p class="replan-impact">需要人工确认</p>'
            : '';
        if (!assignmentDiff && !memberChangeHtml && !qualificationGapHtml && !manualConfirmationHtml) {
            return '';
        }
        return `
                <div class="section-title">重排联动</div>
                ${assignmentDiff}
                ${memberChangeHtml}
                ${qualificationGapHtml}
                ${manualConfirmationHtml}
        `;
    }

    function replanSuggestionSortRank(type) {
        switch (String(type || '').trim()) {
            case 'assigned_conflict_resolution':
                return 0;
            case 'unassigned_new_assignment':
                return 1;
            case 'unassigned_late_assignment':
                return 2;
            default:
                return 99;
        }
    }

    function sortReplanSuggestions(items) {
        const normalized = Array.isArray(items) ? items.slice() : [];
        normalized.sort((left, right) => {
            const rankGap = replanSuggestionSortRank(left?.suggestion_type) - replanSuggestionSortRank(right?.suggestion_type);
            if (rankGap !== 0) {
                return rankGap;
            }
            const impactGap = Number(right?.impact_score || 0) - Number(left?.impact_score || 0);
            if (impactGap !== 0) {
                return impactGap;
            }
            return String(left?.dispatch_order_id || '').localeCompare(String(right?.dispatch_order_id || ''), 'zh-CN');
        });
        return normalized;
    }

    function getReplanSnapshotOrders() {
        const optimizableOrders = Array.isArray(state.replanSnapshot?.optimizable_orders)
            ? state.replanSnapshot.optimizable_orders
            : [];
        const fixedAnchorOrders = Array.isArray(state.replanSnapshot?.fixed_anchor_orders)
            ? state.replanSnapshot.fixed_anchor_orders
            : [];
        if (optimizableOrders.length > 0 || fixedAnchorOrders.length > 0) {
            return [...optimizableOrders, ...fixedAnchorOrders];
        }
        const orders = Array.isArray(state.replanSnapshot?.orders) ? state.replanSnapshot.orders : [];
        const fixedOrders = Array.isArray(state.replanSnapshot?.fixed_orders) ? state.replanSnapshot.fixed_orders : [];
        return [...orders, ...fixedOrders];
    }

    function getReplanOrderResults() {
        if (Array.isArray(state.replanSolverResult?.order_results) && state.replanSolverResult.order_results.length > 0) {
            return state.replanSolverResult.order_results;
        }
        return Array.isArray(state.replanPreview) ? state.replanPreview : [];
    }

    function getReplanMetadataOrderIds(field) {
        const values = Array.isArray(state.replanSolverMetadata?.[field]) ? state.replanSolverMetadata[field] : [];
        return Array.from(new Set(values.map((item) => String(item || '').trim()).filter(Boolean)));
    }

    function getReplanPreviewSections() {
        const suggestions = sortReplanSuggestions(getReplanOrderResults());
        const orderMap = new Map();
        getReplanSnapshotOrders().forEach((order) => {
            const orderId = String(order?.order_id || '').trim();
            if (orderId) {
                orderMap.set(orderId, order);
            }
        });
        const materializeOrders = (orderIds) => orderIds.map((orderId) => {
            return orderMap.get(orderId) || {
                order_id: orderId,
                status: '',
                order_class: '',
                planned_start_time: null,
                planned_end_time: null,
                required_start_time: null,
                current_assignment: {},
            };
        });

        return {
            assignedRepairs: suggestions.filter((item) => item?.suggestion_type === 'assigned_conflict_resolution'),
            unresolvedAssigned: materializeOrders(getReplanMetadataOrderIds('unresolved_assigned_conflict_order_ids')),
            onTimeUnassigned: suggestions.filter((item) => item?.suggestion_type === 'unassigned_new_assignment'),
            lateUnassigned: suggestions.filter((item) => item?.suggestion_type === 'unassigned_late_assignment'),
            unplannedUnassigned: materializeOrders(getReplanMetadataOrderIds('unassigned_unplanned_order_ids')),
        };
    }

    function hasReplanPreviewResult() {
        const sections = getReplanPreviewSections();
        return Boolean(state.replanSnapshotId)
            || Boolean(state.replanSolverVersion)
            || state.replanPreview.length > 0
            || sections.unresolvedAssigned.length > 0
            || sections.unplannedUnassigned.length > 0;
    }

    function renderReplanSuggestionCard(suggestion, index = -1) {
        const orderId = String(suggestion?.dispatch_order_id || '');
        const relatedOrderId = String(suggestion?.related_dispatch_order_id || '');
        const reasonText = REPLAN_REASON_LABELS[suggestion?.reason] || suggestion?.reason || '重排建议';
        const suggestionTypeText = replanSuggestionTypeLabel(suggestion?.suggestion_type);
        const currentAssignmentText = formatAssignmentSummary(suggestion?.current_assignment);
        const suggestedAssignmentText = formatAssignmentSummary(suggestion?.suggested_assignment);
        const latenessMinutes = Number(suggestion?.lateness_minutes || 0);
        const latenessText = latenessMinutes > 0
            ? `<p class="replan-impact">迟到：${escapeHtml(String(latenessMinutes))} 分钟</p>`
            : '';
        const memberChangeText = renderMemberChangeSummary(suggestion?.member_change_summary);
        const qualificationGapText = renderQualificationGapSummary(suggestion?.qualification_gap);
        const manualConfirmationText = suggestion?.requires_manual_confirmation
            ? '<p class="replan-impact">需要人工确认</p>'
            : '';
        return `
            <div class="ai-suggestion">
                <h4>${escapeHtml(orderId)} · ${escapeHtml(suggestionTypeText)} · ${escapeHtml(reasonText)}</h4>
                <p class="replan-subline">${escapeHtml(formatDateTime(suggestion.original_start_time))} - ${escapeHtml(formatDateTime(suggestion.original_end_time))} -> ${escapeHtml(formatDateTime(suggestion.suggested_start_time))} - ${escapeHtml(formatDateTime(suggestion.suggested_end_time))}</p>
                <p class="replan-subline">资源：${escapeHtml(currentAssignmentText)} -> ${escapeHtml(suggestedAssignmentText)}</p>
                <p class="replan-impact">影响分值：${escapeHtml(String(suggestion.impact_score ?? 0))}</p>
                ${latenessText}
                ${memberChangeText}
                ${qualificationGapText}
                ${manualConfirmationText}
                <div class="ai-suggestion-actions">
                    <button class="suggestion-chip" data-action="locate-replan" data-order-id="${escapeHtmlAttribute(orderId)}" data-replan-index="${escapeHtmlAttribute(String(index))}">定位建议工单</button>
                    ${relatedOrderId ? `<button class="suggestion-chip" data-action="locate-related-replan" data-order-id="${escapeHtmlAttribute(relatedOrderId)}">定位关联工单</button>` : ''}
                </div>
            </div>
        `;
    }

    function renderReplanOrderCard(order, title, description) {
        const orderId = String(order?.order_id || '');
        const currentAssignmentText = formatAssignmentSummary(order?.current_assignment);
        const requiredStartText = order?.required_start_time
            ? `<p class="replan-impact">要求到场：${escapeHtml(formatDateTime(order.required_start_time))}</p>`
            : '';
        const qualificationGapText = renderQualificationGapSummary(order?.qualification_gap);
        return `
            <div class="ai-suggestion">
                <h4>${escapeHtml(orderId)} · ${escapeHtml(title)}</h4>
                <p class="replan-subline">${escapeHtml(formatDateTime(order?.planned_start_time || order?.effective_start_time))} - ${escapeHtml(formatDateTime(order?.planned_end_time || order?.effective_end_time))}</p>
                <p class="replan-subline">当前资源：${escapeHtml(currentAssignmentText)}</p>
                <p class="replan-impact">${escapeHtml(description)}</p>
                ${requiredStartText}
                ${qualificationGapText}
                <div class="ai-suggestion-actions">
                    <button class="suggestion-chip" data-action="locate-replan" data-order-id="${escapeHtmlAttribute(orderId)}">定位工单</button>
                </div>
            </div>
        `;
    }

    function renderReplanSection(title, itemsHtml) {
        if (!Array.isArray(itemsHtml) || itemsHtml.length === 0) {
            return '';
        }
        return `
            <div class="replan-section" style="margin-bottom:16px;">
                <div class="section-title">${escapeHtml(title)}</div>
                ${itemsHtml.join('')}
            </div>
        `;
    }

    function renderReplanHint(message) {
        const hint = document.getElementById('replanHint');
        if (!hint) {
            return;
        }
        if (message) {
            hint.textContent = message;
            return;
        }

        if (state.replanApplying) {
            hint.textContent = '正在应用重排，请稍候...';
            return;
        }
        if (state.replanLoading) {
            hint.textContent = '正在生成重排预览...';
            return;
        }
        if (state.replanError) {
            hint.textContent = state.replanError;
            return;
        }
        if (hasReplanPreviewResult()) {
            const sections = getReplanPreviewSections();
            const totalLateness = Number(
                state.replanSolverMetadata?.total_lateness_minutes
                ?? state.replanSolverMetadata?.objective_values?.total_lateness_minutes
                ?? 0
            );
            const modeLabel = buildReplanModeLabel();
            hint.textContent = `预览完成：A 修复 ${sections.assignedRepairs.length} / A 未解 ${sections.unresolvedAssigned.length} / B 准时 ${sections.onTimeUnassigned.length} / B 迟到 ${sections.lateUnassigned.length} / B 未派出 ${sections.unplannedUnassigned.length}，累计迟到 ${totalLateness.toFixed(0)} 分钟${modeLabel ? ` · ${modeLabel}` : ''}`;
            return;
        }
        hint.textContent = '请先点击“预览重排”，确认后再应用。';
    }

    function renderReplanActionState() {
        const previewBtn = document.getElementById('replanPreviewBtn');
        const applyBtn = document.getElementById('replanApplyBtn');
        const clearBtn = document.getElementById('replanClearBtn');

        if (previewBtn) {
            previewBtn.disabled = state.replanLoading || state.replanApplying;
        }
        if (applyBtn) {
            applyBtn.disabled = state.replanLoading || state.replanApplying || state.replanPreview.length === 0;
        }
        if (clearBtn) {
            clearBtn.disabled = state.replanLoading || state.replanApplying;
        }
    }

    function renderReplanPreview() {
        const list = document.getElementById('replanSuggestionList');
        if (!list) {
            return;
        }

        if (state.replanLoading && state.replanPreview.length === 0) {
            if (!showContainerLoading(list, '正在生成重排预览...', { minHeight: '120px' })) {
                list.innerHTML = '<div class="empty-state" style="height:auto;padding:12px 0;">正在生成重排预览...</div>';
            }
            return;
        }

        if (state.replanError && !hasReplanPreviewResult()) {
            if (!renderUnifiedState(list, 'error', state.replanError)) {
                list.innerHTML = `<div class="empty-state" style="height:auto;padding:12px 0;">${escapeHtml(state.replanError)}</div>`;
            }
            return;
        }

        if (!hasReplanPreviewResult()) {
            if (!renderUnifiedState(list, 'empty', '暂无重排建议，点击“预览重排”开始计算')) {
                list.innerHTML = '<div class="empty-state" style="height:auto;padding:12px 0;">暂无重排建议，点击“预览重排”开始计算</div>';
            }
            return;
        }

        const sections = getReplanPreviewSections();
        const htmlSections = [
            renderReplanSection('A 类冲突修复项', sections.assignedRepairs.map((item) => {
                const index = state.replanPreview.indexOf(item);
                return renderReplanSuggestionCard(item, index);
            })),
            renderReplanSection('A 类仍未解项', sections.unresolvedAssigned.map((order) => renderReplanOrderCard(order, 'A 类仍未解', '当前窗口内无法在不破坏前层目标的前提下完成修复'))),
            renderReplanSection('B 类新增准时派单', sections.onTimeUnassigned.map((item) => {
                const index = state.replanPreview.indexOf(item);
                return renderReplanSuggestionCard(item, index);
            })),
            renderReplanSection('B 类迟到派单', sections.lateUnassigned.map((item) => {
                const index = state.replanPreview.indexOf(item);
                return renderReplanSuggestionCard(item, index);
            })),
            renderReplanSection('B 类未派出项', sections.unplannedUnassigned.map((order) => renderReplanOrderCard(order, 'B 类未派出', '当前剩余资源池下未生成可应用派单方案'))),
        ].filter(Boolean);

        hideContainerLoading(list);
        if (htmlSections.length === 0) {
            if (!renderUnifiedState(list, 'empty', '当前窗口无可应用建议，且未发现未解冲突或未派出工单')) {
                list.innerHTML = '<div class="empty-state" style="height:auto;padding:12px 0;">当前窗口无可应用建议，且未发现未解冲突或未派出工单</div>';
            }
            return;
        }

        list.innerHTML = htmlSections.join('');
    }

    function renderScenarioPanel() {
        const equipmentInput = document.getElementById('scenarioEquipmentInput');
        if (equipmentInput && equipmentInput.value !== (state.scenarioEquipmentInput || '')) {
            equipmentInput.value = state.scenarioEquipmentInput || '';
        }
        const standInput = document.getElementById('scenarioStandInput');
        if (standInput && standInput.value !== (state.scenarioStandInput || '')) {
            standInput.value = state.scenarioStandInput || '';
        }
        const delayInput = document.getElementById('scenarioDelayInput');
        if (delayInput && delayInput.value !== (state.scenarioDelayInput || '')) {
            delayInput.value = state.scenarioDelayInput || '';
        }
        const frozenInput = document.getElementById('scenarioFrozenInput');
        if (frozenInput && frozenInput.value !== (state.scenarioFrozenInput || '')) {
            frozenInput.value = state.scenarioFrozenInput || '';
        }

        const previewBtn = document.getElementById('scenarioPreviewBtn');
        if (previewBtn) {
            previewBtn.disabled = state.scenarioLoading;
        }
        const clearBtn = document.getElementById('scenarioClearBtn');
        if (clearBtn) {
            clearBtn.disabled = state.scenarioLoading;
        }

        const preview = state.scenarioPreview;
        setMetric('scenarioImpactedCount', preview ? String(Number(preview?.impact_summary?.impacted_orders || 0)) : '-');
        setMetric('scenarioConflictCount', preview ? String(Number(preview?.impact_summary?.projected_conflicts || 0)) : '-');
        setMetric('scenarioDelayedCount', preview ? String(Number(preview?.impact_summary?.delayed_orders || 0)) : '-');
        setMetric('scenarioRiskLevel', preview ? formatRiskLevel(preview?.risk_level) : '-');
        setMetric('scenarioManualConfirmation', preview ? formatBooleanLabel(preview?.requires_manual_confirmation) : '-');
        setMetric('scenarioChangedCount', preview ? String((Array.isArray(preview?.changed_orders) ? preview.changed_orders.length : 0)) : '-');
        setMetricStateClass('scenarioRiskLevel', preview ? `severity-${normalizeRiskLevel(preview?.risk_level)}` : '');
        setMetricStateClass('scenarioManualConfirmation', preview
            ? (preview?.requires_manual_confirmation ? 'confirmation-required' : 'confirmation-auto')
            : '');

        renderScenarioHint();

        const list = document.getElementById('scenarioResultList');
        if (!list) {
            return;
        }

        if (state.scenarioLoading && !preview) {
            if (!showContainerLoading(list, '正在生成场景预览...', { minHeight: '120px' })) {
                list.innerHTML = '<div class="empty-state" style="height:auto;padding:12px 0;">正在生成场景预览...</div>';
            }
            return;
        }
        if (state.scenarioError && !preview) {
            if (!renderUnifiedState(list, 'error', state.scenarioError)) {
                list.innerHTML = `<div class="empty-state" style="height:auto;padding:12px 0;">${escapeHtml(state.scenarioError)}</div>`;
            }
            return;
        }
        if (!preview) {
            if (!renderUnifiedState(list, 'empty', '暂无场景预览结果')) {
                list.innerHTML = '<div class="empty-state" style="height:auto;padding:12px 0;">暂无场景预览结果</div>';
            }
            return;
        }

        hideContainerLoading(list);

        const impactedOrders = Array.isArray(preview?.impacted_orders) ? preview.impacted_orders : [];
        const projectedConflicts = Array.isArray(preview?.projected_conflicts) ? preview.projected_conflicts : [];
        const recommendations = Array.isArray(preview?.recommendations) ? preview.recommendations : [];
        const changedOrders = Array.isArray(preview?.changed_orders) ? preview.changed_orders : [];

        const sections = [
            renderScenarioSection(
                `受影响工单 (${impactedOrders.length})`,
                impactedOrders.slice(0, 6).map(renderScenarioImpactCard)
            ),
            renderScenarioSection(
                `预计冲突 (${projectedConflicts.length})`,
                projectedConflicts.slice(0, 6).map((item, index) => renderScenarioConflictCard(item, index))
            ),
            renderScenarioSection(
                `处置建议 (${recommendations.length})`,
                recommendations.slice(0, 6).map((item, index) => renderScenarioRecommendationCard(item, index))
            )
        ].filter(Boolean);

        if (changedOrders.length > 0) {
            sections.unshift(`
                <div class="ai-suggestion">
                    <div class="scenario-item-head">
                        <h4>变更工单范围</h4>
                        <span class="scenario-meta-pill ${escapeHtmlAttribute(buildSeverityClassName(preview?.risk_level))}">${escapeHtml(formatRiskLevel(preview?.risk_level))}</span>
                    </div>
                    <div class="scenario-meta-row">
                        ${changedOrders.slice(0, 12).map((orderId) => `
                            <button class="suggestion-chip" data-action="locate-scenario-order" data-order-id="${escapeHtmlAttribute(orderId)}">${escapeHtml(orderId)}</button>
                        `).join('')}
                    </div>
                </div>
            `);
        }

        list.innerHTML = sections.length > 0
            ? sections.join('')
            : '<div class="empty-state" style="height:auto;padding:12px 0;">当前场景未产生可展示影响</div>';
    }

    function renderScenarioHint(message) {
        const hint = document.getElementById('scenarioHint');
        if (!hint) {
            return;
        }
        if (message) {
            hint.textContent = message;
            return;
        }
        if (state.scenarioLoading) {
            hint.textContent = '正在根据当前时间窗生成场景预览...';
            return;
        }
        if (state.scenarioError) {
            hint.textContent = state.scenarioError;
            return;
        }
        if (state.scenarioPreview) {
            const generatedAt = formatDateTime(state.scenarioPreview.generated_at);
            hint.textContent = `预览完成：风险 ${formatRiskLevel(state.scenarioPreview.risk_level)}，生成时间 ${generatedAt}`;
            return;
        }
        hint.textContent = '输入场景后点击“预览场景”。';
    }

    function renderScenarioSection(title, itemsHtml) {
        if (!Array.isArray(itemsHtml) || itemsHtml.length === 0) {
            return '';
        }
        return `
            <div class="replan-section" style="margin-bottom:16px;">
                <div class="section-title">${escapeHtml(title)}</div>
                ${itemsHtml.join('')}
            </div>
        `;
    }

    function renderScenarioImpactCard(item) {
        const orderId = String(item?.dispatch_order_id || '').trim();
        const flightId = String(item?.flight_id || '').trim();
        const projectedWindow = item?.projected_start_time || item?.projected_end_time
            ? `${formatDateTime(item?.projected_start_time)} - ${formatDateTime(item?.projected_end_time)}`
            : '未产生新的预测时间';
        return `
            <div class="ai-suggestion">
                <div class="scenario-item-head">
                    <h4>${escapeHtml(orderId || '未知工单')} · ${escapeHtml(flightId || '未关联航班')}</h4>
                    <span class="scenario-meta-pill ${escapeHtmlAttribute(buildSeverityClassName(item?.severity))}">${escapeHtml(formatRiskLevel(item?.severity))}</span>
                </div>
                <p class="replan-subline">${escapeHtml(String(item?.impact_type || 'impact'))}</p>
                <p class="replan-impact">${escapeHtml(String(item?.message || ''))}</p>
                <div class="scenario-meta-row">
                    <span class="scenario-meta-pill">原计划 ${escapeHtml(formatDateTime(item?.original_start_time))} - ${escapeHtml(formatDateTime(item?.original_end_time))}</span>
                    <span class="scenario-meta-pill">预测 ${escapeHtml(projectedWindow)}</span>
                </div>
                ${orderId ? `<div class="ai-suggestion-actions"><button class="suggestion-chip" data-action="locate-scenario-order" data-order-id="${escapeHtmlAttribute(orderId)}">定位工单</button></div>` : ''}
            </div>
        `;
    }

    function renderScenarioConflictCard(item, index = -1) {
        const orderIds = normalizeConflictOrderIds(item?.related_dispatch_order_ids);
        const resourceText = String(item?.resource_name || item?.resource_id || '').trim();
        const resourceFocus = buildResourceFocusFromConflictItem(item, 'scenario');
        const isFocusedResource = resourceFocus
            ? isResourceFocusActive(resourceFocus.resource_type, resourceFocus.resource_id, resourceFocus.resource_label)
            : false;
        return `
            <div class="ai-suggestion">
                <div class="scenario-item-head">
                    <h4>${escapeHtml(String(item?.message || '预计产生冲突'))}</h4>
                    <span class="scenario-meta-pill ${escapeHtmlAttribute(buildSeverityClassName(item?.severity))}">${escapeHtml(conflictSeverityLabel(item?.severity))}</span>
                </div>
                <div class="scenario-meta-row">
                    <span class="scenario-meta-pill">${escapeHtml(conflictTypeLabel(item?.conflict_type))}</span>
                    ${resourceText ? `<span class="scenario-meta-pill">${escapeHtml(resourceText)}</span>` : ''}
                </div>
                ${resourceFocus ? `<div class="ai-suggestion-actions"><button class="suggestion-chip" data-action="focus-scenario-resource" data-scenario-kind="conflict" data-scenario-index="${escapeHtmlAttribute(String(index))}" data-order-id="${escapeHtmlAttribute(orderIds[0] || '')}" data-resource-type="${escapeHtmlAttribute(resourceFocus.resource_type)}" data-resource-id="${escapeHtmlAttribute(resourceFocus.resource_id)}" data-resource-label="${escapeHtmlAttribute(resourceFocus.resource_label)}" data-view-mode="${escapeHtmlAttribute(resourceFocus.target_view_mode)}" data-related-order-ids="${escapeHtmlAttribute(orderIds.join(','))}" data-resource-ids="${escapeHtmlAttribute((resourceFocus.resource_ids || []).join(','))}" data-lane-ids="${escapeHtmlAttribute((resourceFocus.lane_ids || []).join(','))}" data-highlight-scope="${escapeHtmlAttribute(resourceFocus.highlight_scope || 'single')}" data-source-key="${escapeHtmlAttribute(resourceFocus.source_key)}">${isFocusedResource ? '已聚焦资源行' : '定位资源行'}</button></div>` : ''}
                ${orderIds.length > 0 ? `
                    <div class="ai-suggestion-actions">
                        ${orderIds.slice(0, 4).map((orderId) => `
                            <button class="suggestion-chip" data-action="locate-scenario-order" data-order-id="${escapeHtmlAttribute(orderId)}">${escapeHtml(orderId)}</button>
                        `).join('')}
                    </div>
                ` : ''}
            </div>
        `;
    }

    function renderScenarioRecommendationCard(item, index = -1) {
        const orderId = String(item?.dispatch_order_id || '').trim();
        const resourceFocus = buildResourceFocusFromScenarioRecommendation(item, 'scenario');
        const isFocusedResource = resourceFocus
            ? isResourceFocusActive(resourceFocus.resource_type, resourceFocus.resource_id, resourceFocus.resource_label)
            : false;
        return `
            <div class="ai-suggestion">
                <div class="scenario-item-head">
                    <h4>${escapeHtml(orderId || '未知工单')} · ${escapeHtml(String(item?.action || '待处置'))}</h4>
                    <span class="scenario-meta-pill ${escapeHtmlAttribute(item?.requires_manual_confirmation ? 'confirmation-required' : 'confirmation-auto')}">${escapeHtml(formatBooleanLabel(item?.requires_manual_confirmation))}</span>
                </div>
                <p class="replan-impact">${escapeHtml(String(item?.reason || ''))}</p>
                <div class="ai-suggestion-actions">
                    ${resourceFocus ? `<button class="suggestion-chip" data-action="focus-scenario-resource" data-scenario-kind="recommendation" data-scenario-index="${escapeHtmlAttribute(String(index))}" data-order-id="${escapeHtmlAttribute(orderId)}" data-resource-type="${escapeHtmlAttribute(resourceFocus.resource_type)}" data-resource-id="${escapeHtmlAttribute(resourceFocus.resource_id)}" data-resource-label="${escapeHtmlAttribute(resourceFocus.resource_label)}" data-view-mode="${escapeHtmlAttribute(resourceFocus.target_view_mode)}" data-related-order-ids="${escapeHtmlAttribute((resourceFocus.related_order_ids || []).join(','))}" data-resource-ids="${escapeHtmlAttribute((resourceFocus.resource_ids || []).join(','))}" data-lane-ids="${escapeHtmlAttribute((resourceFocus.lane_ids || []).join(','))}" data-highlight-scope="${escapeHtmlAttribute(resourceFocus.highlight_scope || 'single')}" data-source-key="${escapeHtmlAttribute(resourceFocus.source_key)}">${isFocusedResource ? '已聚焦资源行' : '定位资源行'}</button>` : ''}
                    ${orderId ? `<button class="suggestion-chip" data-action="locate-scenario-order" data-order-id="${escapeHtmlAttribute(orderId)}">定位工单</button>` : ''}
                </div>
            </div>
        `;
    }

    function parseCommaSeparatedIds(text) {
        return Array.from(new Set(
            String(text || '')
                .split(/[\n,，;；]+/)
                .map((item) => String(item || '').trim())
                .filter(Boolean)
        ));
    }

    function parseScenarioDelayInput(text) {
        const tokens = String(text || '')
            .split(/[\n,，;；]+/)
            .map((item) => String(item || '').trim())
            .filter(Boolean);
        const items = [];
        for (const token of tokens) {
            const pieces = token.split(/[:：]/);
            if (pieces.length !== 2) {
                return {
                    items: [],
                    error: '延迟工单格式应为 工单ID:分钟，例如 order-1:20'
                };
            }
            const orderId = String(pieces[0] || '').trim();
            const delayMinutes = Number(String(pieces[1] || '').trim());
            if (!orderId || !Number.isInteger(delayMinutes) || delayMinutes < 1 || delayMinutes > 720) {
                return {
                    items: [],
                    error: '延迟分钟必须是 1 到 720 的整数，例如 order-1:20'
                };
            }
            items.push({
                dispatch_order_id: orderId,
                delay_minutes: delayMinutes
            });
        }
        return { items, error: '' };
    }

    async function previewScenario() {
        if (state.scenarioLoading) {
            return;
        }

        const delayedOrdersResult = parseScenarioDelayInput(state.scenarioDelayInput);
        if (delayedOrdersResult.error) {
            state.scenarioError = delayedOrdersResult.error;
            state.scenarioPreview = null;
            renderScenarioPanel();
            showToast(delayedOrdersResult.error);
            return;
        }

        state.scenarioLoading = true;
        state.scenarioError = '';
        renderScenarioPanel();

        try {
            const payload = await apiCall('/api/v2/dispatch/scenarios/preview', {
                method: 'POST',
                body: JSON.stringify({
                    window_start: new Date(state.windowStartMs).toISOString(),
                    window_end: new Date(state.windowEndMs).toISOString(),
                    equipment_unavailable_ids: parseCommaSeparatedIds(state.scenarioEquipmentInput),
                    closed_stand_ids: parseCommaSeparatedIds(state.scenarioStandInput),
                    delayed_orders: delayedOrdersResult.items,
                    frozen_order_ids: parseCommaSeparatedIds(state.scenarioFrozenInput)
                })
            });
            const preview = unwrapApiData(payload);
            state.scenarioPreview = preview && typeof preview === 'object' ? preview : null;
            state.scenarioError = '';

            const impactedOrderIds = Array.from(new Set([
                ...(Array.isArray(preview?.changed_orders) ? preview.changed_orders : []),
                ...(Array.isArray(preview?.impacted_orders) ? preview.impacted_orders.map((item) => item?.dispatch_order_id) : [])
            ].map((item) => String(item || '').trim()).filter(Boolean)));
            setImpactedOrders(impactedOrderIds, { render: true });

            renderScenarioPanel();
            showToast(`场景预览已生成（受影响 ${impactedOrderIds.length} 单）`);
        } catch (error) {
            state.scenarioPreview = null;
            state.scenarioError = error.message || '场景预览失败';
            setImpactedOrders([], { render: true });
            renderScenarioPanel();
            showToast(state.scenarioError);
        } finally {
            state.scenarioLoading = false;
            renderScenarioPanel();
        }
    }

    function clearScenarioPreview(options = {}) {
        state.scenarioPreview = null;
        state.scenarioError = '';
        state.scenarioEquipmentInput = '';
        state.scenarioStandInput = '';
        state.scenarioDelayInput = '';
        state.scenarioFrozenInput = '';
        setImpactedOrders([], { render: true });
        renderScenarioPanel();
        if (!options.silent) {
            showToast('已清空场景输入与预览结果');
        }
    }

    function applyConflictFilters() {
        const raw = Array.isArray(state.conflictsRaw) ? state.conflictsRaw : [];
        const severityFilter = String(state.conflictSeverity || 'all').toLowerCase();
        const typeFilter = String(state.conflictType || 'all');
        const query = normalizeSearchQuery(state.conflictQuery);

        state.conflictsFiltered = raw
            .filter((conflict) => {
                const severity = String(conflict?.severity || 'medium').toLowerCase();
                const conflictType = String(conflict?.conflict_type || '');
                if (severityFilter !== 'all' && severity !== severityFilter) {
                    return false;
                }
                if (typeFilter !== 'all' && conflictType !== typeFilter) {
                    return false;
                }
                if (!query) {
                    return true;
                }
                return buildConflictTextBlob(conflict).includes(query);
            })
            .sort((left, right) => {
                const severityGap = conflictSeverityWeight(right?.severity) - conflictSeverityWeight(left?.severity);
                if (severityGap !== 0) {
                    return severityGap;
                }
                return String(left?.conflict_type || '').localeCompare(String(right?.conflict_type || ''), 'zh-CN');
            });
    }

    async function refreshConflictData(options = {}) {
        const force = Boolean(options.force);
        const silent = Boolean(options.silent);
        const now = Date.now();
        if (!force && now - state.conflictLastUpdatedAt < CONFLICT_REFRESH_MIN_INTERVAL_MS) {
            return;
        }
        if (state.conflictLoading && !force) {
            return;
        }

        state.conflictLoading = true;
        state.conflictError = '';
        renderConflictGovernance();

        const requestSeq = state.conflictRequestSeq + 1;
        state.conflictRequestSeq = requestSeq;

        try {
            const params = new URLSearchParams();
            params.set('window_start', new Date(state.windowStartMs).toISOString());
            params.set('window_end', new Date(state.windowEndMs).toISOString());
            params.set('limit', '300');

            const payload = await apiCall(`/api/v2/dispatch-orders/conflicts?${params.toString()}`);
            if (requestSeq !== state.conflictRequestSeq) {
                return;
            }

            state.conflictsRaw = Array.isArray(payload?.conflicts) ? payload.conflicts : [];
            state.conflictLastUpdatedAt = Date.now();
            state.conflictError = '';
            applyConflictFilters();
            renderConflictGovernance();
            renderAiMetrics();

            if (!silent) {
                showToast(`已刷新冲突数据（${state.conflictsRaw.length}项）`);
            }
        } catch (error) {
            if (requestSeq !== state.conflictRequestSeq) {
                return;
            }
            state.conflictError = error.message || '加载失败';
            applyConflictFilters();
            renderConflictGovernance();
            if (!silent) {
                showToast(state.conflictError || '冲突数据刷新失败');
            }
        } finally {
            if (requestSeq === state.conflictRequestSeq) {
                state.conflictLoading = false;
                renderConflictGovernance();
            }
        }
    }

    function shouldRefreshConflictDataOnTimelineRefresh() {
        const drawer = document.getElementById('aiDrawer');
        const isDrawerOpen = Boolean(drawer && drawer.classList.contains('open'));
        if (!isDrawerOpen && !isConflictObjectiveSelected()) {
            return false;
        }
        if (state.aiDrawerTab !== 'conflict' && !isConflictObjectiveSelected()) {
            return false;
        }
        return Date.now() - state.conflictLastUpdatedAt >= CONFLICT_REFRESH_MIN_INTERVAL_MS;
    }

    function shouldRefreshAnalyticsOnTimelineRefresh() {
        const drawer = document.getElementById('aiDrawer');
        const isDrawerOpen = Boolean(drawer && drawer.classList.contains('open'));
        if (!isDrawerOpen) {
            return false;
        }
        if (state.aiDrawerTab !== 'assistant') {
            return false;
        }
        return Date.now() - state.analyticsLastUpdatedAt >= ANALYTICS_REFRESH_MIN_INTERVAL_MS;
    }

    async function locateConflictByIndex(index) {
        const conflict = Array.isArray(state.conflictsFiltered) ? state.conflictsFiltered[index] : null;
        if (!conflict) {
            return;
        }

        const orderIds = normalizeConflictOrderIds(conflict.related_dispatch_order_ids);
        const resourceFocus = buildResourceFocusFromConflictItem(conflict, 'conflict');
        if (resourceFocus) {
            state.conflictSelectedOrderId = orderIds[0] || null;
            await applyResourceFocus(resourceFocus, {
                preferredOrderId: orderIds[0] || ''
            });
            renderConflictGovernance();
            return;
        }
        if (orderIds.length === 0) {
            showToast('当前冲突缺少可定位工单');
            return;
        }

        state.conflictSelectedOrderId = orderIds[0];
        setImpactedOrders(orderIds, { render: true });
        await focusOrder(orderIds[0]);
        renderConflictGovernance();
    }

    async function previewReplan() {
        if (state.replanLoading || state.replanApplying) {
            return;
        }

        state.replanLoading = true;
        state.replanError = '';
        renderReplanHint();
        renderReplanActionState();

        const requestSeq = state.replanRequestSeq + 1;
        state.replanRequestSeq = requestSeq;

        try {
            const snapshotResponse = await apiCall(buildReplanSnapshotUrl());
            const snapshot = snapshotResponse?.data && typeof snapshotResponse.data === 'object'
                ? snapshotResponse.data
                : snapshotResponse;
            const previewPayload = await previewReplanViaFrontendWasm(snapshot);
            if (requestSeq !== state.replanRequestSeq) {
                return;
            }

            const orderResults = sortReplanSuggestions(
                Array.isArray(previewPayload?.order_results) ? previewPayload.order_results : []
            );
            state.replanPreview = orderResults;
            state.replanSnapshot = snapshot || null;
            state.replanSnapshotId = String(snapshot?.snapshot_id || '');
            state.replanSolverVersion = String(snapshot?.solver_version || '');
            state.replanSolverMode = 'frontend_wasm';
            state.replanSolverMetadata = previewPayload?.solver_run_metadata && typeof previewPayload.solver_run_metadata === 'object'
                ? previewPayload.solver_run_metadata
                : {};
            state.replanSolverResult = {
                order_results: Array.isArray(previewPayload?.order_results) ? previewPayload.order_results : orderResults,
                personnel_slot_assignments: Array.isArray(previewPayload?.personnel_slot_assignments) ? previewPayload.personnel_slot_assignments : [],
                equipment_slot_assignments: Array.isArray(previewPayload?.equipment_slot_assignments) ? previewPayload.equipment_slot_assignments : [],
                continuity_decisions: Array.isArray(previewPayload?.continuity_decisions) ? previewPayload.continuity_decisions : [],
                objective_breakdown: previewPayload?.objective_breakdown && typeof previewPayload.objective_breakdown === 'object'
                    ? previewPayload.objective_breakdown
                    : {}
            };
            state.replanError = '';

            const impactedOrderIds = new Set();
            if (orderResults.length > 0) {
                orderResults.forEach((item) => {
                    if (item?.dispatch_order_id) {
                        impactedOrderIds.add(String(item.dispatch_order_id));
                    }
                    if (item?.related_dispatch_order_id) {
                        impactedOrderIds.add(String(item.related_dispatch_order_id));
                    }
                });
            }
            getReplanMetadataOrderIds('unresolved_assigned_conflict_order_ids').forEach((orderId) => impactedOrderIds.add(orderId));
            getReplanMetadataOrderIds('unassigned_unplanned_order_ids').forEach((orderId) => impactedOrderIds.add(orderId));
            setImpactedOrders(Array.from(impactedOrderIds), { render: true });
            renderReplanPreview();
            renderReplanHint();
            renderReplanActionState();
            const sections = getReplanPreviewSections();
            showToast(`重排预览已生成（建议 ${orderResults.length} 条，未解 ${sections.unresolvedAssigned.length} 条，未派出 ${sections.unplannedUnassigned.length} 条）`);
        } catch (error) {
            if (requestSeq !== state.replanRequestSeq) {
                return;
            }
            state.replanPreview = [];
            state.replanSnapshot = null;
            state.replanSnapshotId = '';
            state.replanSolverVersion = '';
            state.replanSolverMode = 'idle';
            state.replanSolverMetadata = {};
            state.replanSolverResult = null;
            state.replanError = `重排预览失败：${error.message || '未知错误'}`;
            setImpactedOrders([], { render: true });
            renderReplanPreview();
            renderReplanHint();
            renderReplanActionState();
            showToast(error.message || '重排预览失败');
        } finally {
            if (requestSeq === state.replanRequestSeq) {
                state.replanLoading = false;
                renderReplanHint();
                renderReplanActionState();
            }
        }
    }

    async function applyReplan() {
        if (state.replanLoading || state.replanApplying) {
            return;
        }
        if (!Array.isArray(state.replanPreview) || state.replanPreview.length === 0) {
            showToast('请先生成重排预览');
            return;
        }

        const suggestionCount = state.replanPreview.length;
        const impactTotal = state.replanPreview.reduce((sum, item) => sum + Number(item.impact_score || 0), 0);
        const confirmed = window.confirm(
            `将按“${state.replanStrategy}”策略应用 ${suggestionCount} 条重排建议，累计影响约 ${impactTotal.toFixed(1)} 分钟。是否继续？`
        );
        if (!confirmed) {
            return;
        }

        state.replanApplying = true;
        state.replanError = '';
        renderReplanHint();
        renderReplanActionState();

        const requestSeq = state.replanRequestSeq + 1;
        state.replanRequestSeq = requestSeq;

        try {
            if (!state.replanSnapshotId || !state.replanSolverVersion) {
                throw new Error('当前预览缺少快照信息，请重新预览');
            }
            const applyPayload = await apiCall('/api/v2/dispatch-orders/replan-apply', {
                method: 'POST',
                body: JSON.stringify({
                    snapshot_id: state.replanSnapshotId,
                    solver_version: state.replanSolverVersion,
                    strategy: state.replanStrategy,
                    order_results: Array.isArray(state.replanSolverResult?.order_results) ? state.replanSolverResult.order_results : state.replanPreview,
                    personnel_slot_assignments: Array.isArray(state.replanSolverResult?.personnel_slot_assignments) ? state.replanSolverResult.personnel_slot_assignments : [],
                    equipment_slot_assignments: Array.isArray(state.replanSolverResult?.equipment_slot_assignments) ? state.replanSolverResult.equipment_slot_assignments : [],
                    continuity_decisions: Array.isArray(state.replanSolverResult?.continuity_decisions) ? state.replanSolverResult.continuity_decisions : [],
                    objective_breakdown: state.replanSolverResult?.objective_breakdown && typeof state.replanSolverResult.objective_breakdown === 'object'
                        ? state.replanSolverResult.objective_breakdown
                        : {},
                    solver_run_metadata: state.replanSolverMetadata || {}
                })
            });
            if (requestSeq !== state.replanRequestSeq) {
                return;
            }

            const applyData = applyPayload?.data && typeof applyPayload.data === 'object' ? applyPayload.data : applyPayload;
            const appliedSuggestions = Array.isArray(applyData?.order_results)
                ? applyData.order_results
                : (Array.isArray(applyData?.suggestions) ? applyData.suggestions : []);
            const notificationSummary = applyData?.notification_summary && typeof applyData.notification_summary === 'object'
                ? applyData.notification_summary
                : {};
            const notifiedCount = Number(notificationSummary?.total_sent_count || 0);
            const receiptRequiredCount = Number(notificationSummary?.receipt_required_count || 0);
            state.replanPreview = appliedSuggestions;
            state.replanSolverResult = null;
            state.replanError = '';
            renderReplanPreview();
            renderReplanHint('重排已应用，正在刷新时间线与冲突列表...');

            await refreshTimeline();
            await refreshConflictData({ force: true, silent: true });
            clearReplanPreview({ silent: true });
            const receiptText = receiptRequiredCount > 0 ? `，${receiptRequiredCount} 条通知待确认` : '';
            showToast(`已应用重排（${appliedSuggestions.length}条），已通知 ${notifiedCount} 人${receiptText}`);
        } catch (error) {
            if (requestSeq !== state.replanRequestSeq) {
                return;
            }
            state.replanError = `应用失败：${error.message || '未知错误'}`;
            renderReplanHint();
            showToast(error.message || '应用重排失败');
        } finally {
            if (requestSeq === state.replanRequestSeq) {
                state.replanApplying = false;
                renderReplanHint();
                renderReplanActionState();
            }
        }
    }

    function clearReplanPreview(options = {}) {
        state.replanPreview = [];
        state.replanSnapshot = null;
        state.replanSnapshotId = '';
        state.replanSolverVersion = '';
        state.replanSolverMode = 'idle';
        state.replanSolverMetadata = {};
        state.replanSolverResult = null;
        state.replanError = '';
        if (options.keepImpacted !== true) {
            setImpactedOrders([], { render: true });
        }
        renderReplanPreview();
        renderReplanHint();
        renderReplanActionState();
        if (!options.silent) {
            showToast('已清空重排预览');
        }
    }

    function buildReplanSnapshotUrl() {
        const params = new URLSearchParams({
            window_start: new Date(state.windowStartMs).toISOString(),
            window_end: new Date(state.windowEndMs).toISOString(),
            strategy: state.replanStrategy,
            max_suggestions: String(Math.max(1, Number(state.replanMaxSuggestions) || 20))
        });
        return `/api/v2/dispatch-orders/replan-snapshot?${params.toString()}`;
    }

    function buildReplanModeLabel() {
        const solver = String(state.replanSolverMetadata?.solver || '').trim();
        if (solver) {
            return solver;
        }
        const solverVersion = String(state.replanSolverVersion || '').trim();
        if (solverVersion) {
            return solverVersion;
        }
        return state.replanSolverMode === 'frontend_wasm' ? 'frontend_wasm' : '';
    }

    async function previewReplanViaFrontendWasm(snapshot) {
        const workerPoolModule = getDispatchBoardWorkerPool() || await ensureDispatchBoardWorkerPoolModule();
        const workerPool = workerPoolModule.getSharedPool({
            workerUrl: FRONTEND_REPLAN_WORKER_URL,
            workerOptions: { type: 'module' },
            maxWorkers: 2,
            timeoutMs: 30000
        });
        const payload = await workerPool.run(snapshot, { timeoutMs: 30000 });
        const orderResults = sortReplanSuggestions(
            Array.isArray(payload?.order_results) ? payload.order_results : []
        ).slice(0, Math.max(1, Number(snapshot?.max_suggestions || state.replanMaxSuggestions) || 20));
        const solverRunMetadata = payload?.solver_run_metadata && typeof payload.solver_run_metadata === 'object'
            ? payload.solver_run_metadata
            : {};
        return {
            order_results: orderResults,
            personnel_slot_assignments: Array.isArray(payload?.personnel_slot_assignments) ? payload.personnel_slot_assignments : [],
            equipment_slot_assignments: Array.isArray(payload?.equipment_slot_assignments) ? payload.equipment_slot_assignments : [],
            continuity_decisions: Array.isArray(payload?.continuity_decisions) ? payload.continuity_decisions : [],
            objective_breakdown: payload?.objective_breakdown && typeof payload.objective_breakdown === 'object'
                ? payload.objective_breakdown
                : {},
            solver_run_metadata: solverRunMetadata
        };
    }

    function buildConflictTextBlob(conflict) {
        const parts = [
            conflict?.message,
            conflict?.resource_name,
            conflict?.resource_id,
            conflict?.conflict_type,
            conflict?.severity,
            conflict?.suggested_action,
            normalizeConflictOrderIds(conflict?.related_dispatch_order_ids).join(' ')
        ];
        return normalizeSearchQuery(parts.filter(Boolean).join(' '));
    }

    function normalizeConflictOrderIds(rawOrderIds) {
        const result = [];
        const seen = new Set();
        if (!Array.isArray(rawOrderIds)) {
            return result;
        }

        for (const rawOrderId of rawOrderIds) {
            const orderId = String(rawOrderId || '').trim();
            if (!orderId || seen.has(orderId)) {
                continue;
            }
            seen.add(orderId);
            result.push(orderId);
        }
        return result;
    }

    function conflictSeverityWeight(severity) {
        const normalized = String(severity || '').toLowerCase();
        const index = CONFLICT_SEVERITY_ORDER.indexOf(normalized);
        if (index < 0) {
            return 0;
        }
        return CONFLICT_SEVERITY_ORDER.length - index;
    }

    function conflictSeverityLabel(severity) {
        const normalized = String(severity || '').toLowerCase();
        switch (normalized) {
            case 'critical':
                return 'Critical';
            case 'high':
                return 'High';
            case 'medium':
                return 'Medium';
            case 'low':
                return 'Low';
            default:
                return 'Unknown';
        }
    }

    function conflictTypeLabel(type) {
        const normalized = String(type || '').trim();
        if (!normalized) {
            return '未知冲突';
        }
        return CONFLICT_TYPE_LABELS[normalized] || normalized;
    }

    function resolveFocusTargetForOrder(orderId) {
        const normalizedOrderId = String(orderId || '').trim();
        if (!normalizedOrderId) {
            return null;
        }
        const items = Array.isArray(state.timelineData?.items) ? state.timelineData.items : [];
        const direct = items.find((item) => item.order_id === normalizedOrderId);
        if (direct) {
            return { itemId: direct.id, orderId: normalizedOrderId };
        }
        const summary = items.find((item) => {
            const relatedOrderIds = Array.isArray(item?.related_order_ids) ? item.related_order_ids : [];
            return relatedOrderIds.includes(normalizedOrderId);
        });
        if (summary) {
            return { itemId: summary.id, orderId: normalizedOrderId };
        }
        return { itemId: null, orderId: normalizedOrderId };
    }

    function setImpactedOrders(orderIds, options = {}) {
        const normalized = normalizeConflictOrderIds(orderIds || []);
        state.impactedOrderIds = new Set(normalized);
        syncImpactedItemsFromTimeline();
        if (options.render) {
            renderChart();
        }
    }

    function syncImpactedItemsFromTimeline() {
        const nextImpactedItems = new Set();
        const timelineItems = Array.isArray(state.timelineData?.items) ? state.timelineData.items : [];
        if (state.impactedOrderIds.size === 0) {
            state.impactedItemIds = nextImpactedItems;
            return;
        }

        for (const item of timelineItems) {
            if (!item) {
                continue;
            }
            if (item.order_id && state.impactedOrderIds.has(item.order_id)) {
                nextImpactedItems.add(item.id);
                continue;
            }
            const relatedOrderIds = Array.isArray(item.related_order_ids) ? item.related_order_ids : [];
            if (relatedOrderIds.some((orderId) => state.impactedOrderIds.has(orderId))) {
                nextImpactedItems.add(item.id);
            }
        }
        state.impactedItemIds = nextImpactedItems;
    }

    function isTimelineItemImpacted(item) {
        if (!item) {
            return false;
        }
        if (state.impactedItemIds.has(item.id)) {
            return true;
        }
        if (item.order_id && state.impactedOrderIds.has(item.order_id)) {
            return true;
        }
        const relatedOrderIds = Array.isArray(item.related_order_ids) ? item.related_order_ids : [];
        return relatedOrderIds.some((orderId) => state.impactedOrderIds.has(orderId));
    }

    function isConflictObjectiveSelected() {
        const objectiveSelect = document.getElementById('aiObjective');
        if (!objectiveSelect) {
            return false;
        }
        return objectiveSelect.value === 'resolve_conflicts';
    }

    function countHeavyLoadLanes(timeline) {
        const heavyLanes = getHeavyLanes(timeline);
        return heavyLanes.length;
    }

    function getHeavyLanes(timeline) {
        const laneDuration = new Map();
        const items = Array.isArray(timeline.items) ? timeline.items : [];

        for (const item of items) {
            if (!item || item.is_flight_summary) {
                continue;
            }
            const laneId = item.lane_id;
            if (!laneId) {
                continue;
            }
            const duration = Math.max(0, toMs(item.end_time) - toMs(item.start_time));
            laneDuration.set(laneId, (laneDuration.get(laneId) || 0) + duration);
        }

        const entries = Array.from(laneDuration.entries());
        if (entries.length <= 1) {
            return [];
        }

        const average = entries.reduce((sum, entry) => sum + entry[1], 0) / entries.length;
        const lanes = Array.isArray(timeline.lanes) ? timeline.lanes : [];
        const laneMap = new Map(lanes.map((lane) => [lane.id, lane]));

        return entries
            .filter((entry) => entry[1] > average * 1.5)
            .map((entry) => laneMap.get(entry[0]) || { id: entry[0], label: entry[0] })
            .sort((a, b) => (a.label || '').localeCompare(b.label || '', 'zh-CN'));
    }

    function resetWindowToNow() {
        const now = Date.now();
        state.windowStartMs = now - DEFAULT_PAST_MINUTES * 60 * 1000;
        state.windowEndMs = now + DEFAULT_FUTURE_MINUTES * 60 * 1000;
        renderWindowLabel();
        renderViewModeHint();
    }

    async function apiCall(url, options = {}) {
        const dataLayer = getDispatchBoardData();
        if (!dataLayer || typeof dataLayer.apiCall !== 'function') {
            throw new Error('派工调度数据模块不可用');
        }
        return dataLayer.apiCall(url, options);
    }

    function unwrapApiData(payload) {
        const dataLayer = getDispatchBoardData();
        if (!dataLayer || typeof dataLayer.unwrapApiData !== 'function') {
            return payload?.data !== undefined ? payload.data : payload;
        }
        return dataLayer.unwrapApiData(payload);
    }

    function setMetricStateClass(id, className) {
        const el = document.getElementById(id);
        if (!el) {
            return;
        }
        el.classList.remove(
            'severity-critical',
            'severity-high',
            'severity-medium',
            'severity-low',
            'confirmation-required',
            'confirmation-auto'
        );
        if (className) {
            el.classList.add(className);
        }
    }

    function statusToCode(status) {
        switch (status) {
            case 'pending':
                return 0;
            case 'assigned':
                return 1;
            case 'in_progress':
                return 2;
            case 'completed':
                return 3;
            case 'cancelled':
                return 4;
            default:
                return 0;
        }
    }

    function statusCodeToKey(statusCode) {
        switch (statusCode) {
            case 0:
                return 'pending';
            case 1:
                return 'assigned';
            case 2:
                return 'in_progress';
            case 3:
                return 'completed';
            case 4:
                return 'cancelled';
            default:
                return 'pending';
        }
    }

    function statusCodeToColor(statusCode) {
        switch (statusCode) {
            case 0:
                return STATUS_COLORS.pending;
            case 1:
                return STATUS_COLORS.assigned;
            case 2:
                return STATUS_COLORS.in_progress;
            case 3:
                return STATUS_COLORS.completed;
            case 4:
                return STATUS_COLORS.cancelled;
            default:
                return STATUS_COLORS.pending;
        }
    }

    function normalizePublicationState(value) {
        return String(value || '').trim().toLowerCase();
    }

    function isLockedOrder(raw) {
        const level = String(raw?.lock_level || '').trim().toLowerCase();
        return Boolean(level) && !['optimizable', 'none'].includes(level);
    }

    function hasQualificationGap(raw) {
        return Array.isArray(raw?.qualification_gap) && raw.qualification_gap.length > 0;
    }

    function hasSemanticAlert(raw, options = {}) {
        return Boolean(options.isImpacted)
            || Boolean(String(raw?.conflict_reason || '').trim())
            || Boolean(String(raw?.availability_reason || '').trim())
            || hasQualificationGap(raw)
            || options.safetyGateState === 'blocked';
    }

    function summarizeRelatedStatuses(relatedOrders) {
        const counts = {};
        const entries = Array.isArray(relatedOrders) ? relatedOrders : [];
        for (const order of entries) {
            const statusKey = String(order?.status || 'pending').trim() || 'pending';
            counts[statusKey] = Number(counts[statusKey] || 0) + 1;
        }
        return counts;
    }

    function buildSummaryStatusSegments(rawItem, rect) {
        const counts = summarizeRelatedStatuses(rawItem?.related_orders);
        const statuses = STATUS_ORDER.filter((statusKey) => Number(counts[statusKey] || 0) > 0);
        if (!rect || rect.width < 36 || statuses.length <= 1) {
            return [];
        }

        const total = statuses.reduce((sum, statusKey) => sum + Number(counts[statusKey] || 0), 0);
        if (total <= 0) {
            return [];
        }

        const bandHeight = Math.min(4, Math.max(2, rect.height * 0.22));
        let cursorX = rect.x;
        return statuses.map((statusKey, index) => {
            const rawWidth = index === statuses.length - 1
                ? (rect.x + rect.width - cursorX)
                : Math.max(3, Math.round(rect.width * (Number(counts[statusKey] || 0) / total)));
            const shape = {
                type: 'rect',
                shape: {
                    x: cursorX,
                    y: rect.y + rect.height - bandHeight,
                    width: rawWidth,
                    height: bandHeight
                },
                style: {
                    fill: statusCodeToColor(statusToCode(statusKey)),
                    opacity: 0.92
                },
                silent: true
            };
            cursorX += rawWidth;
            return shape;
        });
    }

    function buildStatusTextureShapes(statusKey, rect, options = {}) {
        const shapes = [];
        const minWidth = 18;
        const minHeight = 7;
        if (!rect || rect.width < minWidth || rect.height < minHeight) {
            return shapes;
        }

        if (statusKey === 'pending') {
            const step = 8;
            const stroke = 'rgba(255, 255, 255, 0.24)';
            for (let x = rect.x - rect.height; x < rect.x + rect.width; x += step) {
                shapes.push({
                    type: 'line',
                    shape: {
                        x1: x,
                        y1: rect.y + rect.height,
                        x2: x + rect.height,
                        y2: rect.y
                    },
                    style: {
                        stroke,
                        lineWidth: 1
                    },
                    silent: true
                });
            }
            return shapes;
        }

        if (statusKey === 'in_progress') {
            const step = 7;
            const stroke = 'rgba(255, 255, 255, 0.22)';
            for (let x = rect.x + 3; x < rect.x + rect.width; x += step) {
                shapes.push({
                    type: 'line',
                    shape: {
                        x1: x,
                        y1: rect.y + 1,
                        x2: x,
                        y2: rect.y + rect.height - 1
                    },
                    style: {
                        stroke,
                        lineWidth: 1
                    },
                    silent: true
                });
            }
            return shapes;
        }

        if (statusKey === 'cancelled') {
            const stroke = 'rgba(255, 255, 255, 0.35)';
            shapes.push({
                type: 'line',
                shape: {
                    x1: rect.x + 2,
                    y1: rect.y + 1,
                    x2: rect.x + rect.width - 2,
                    y2: rect.y + rect.height - 1
                },
                style: {
                    stroke,
                    lineWidth: 1.2
                },
                silent: true
            });
            shapes.push({
                type: 'line',
                shape: {
                    x1: rect.x + rect.width - 2,
                    y1: rect.y + 1,
                    x2: rect.x + 2,
                    y2: rect.y + rect.height - 1
                },
                style: {
                    stroke,
                    lineWidth: 1.2
                },
                silent: true
            });
            return shapes;
        }

        if (options.isSummary && rect.width >= 42) {
            const stroke = 'rgba(255, 255, 255, 0.2)';
            shapes.push({
                type: 'line',
                shape: {
                    x1: rect.x + 5,
                    y1: rect.y + 4,
                    x2: rect.x + rect.width - 5,
                    y2: rect.y + 4
                },
                style: {
                    stroke,
                    lineWidth: 1
                },
                silent: true
            });
            return shapes;
        }

        return shapes;
    }

    function formatAxisTime(value) {
        const date = new Date(value);
        const h = String(date.getHours()).padStart(2, '0');
        const m = String(date.getMinutes()).padStart(2, '0');
        return `${h}:${m}`;
    }

    function formatDecimal(value, digits = 1) {
        const number = Number(value || 0);
        if (!Number.isFinite(number)) {
            return '-';
        }
        return number.toFixed(digits);
    }

    function formatRatePercent(value) {
        const number = Number(value || 0);
        if (!Number.isFinite(number)) {
            return '-';
        }
        return `${(number * 100).toFixed(1)}%`;
    }

    function formatMinutes(value) {
        const number = Number(value || 0);
        if (!Number.isFinite(number)) {
            return '-';
        }
        return `${number.toFixed(1)} 分钟`;
    }

    function formatRiskLevel(value) {
        switch (String(value || '').trim().toLowerCase()) {
            case 'critical':
                return 'Critical';
            case 'high':
                return 'High';
            case 'medium':
                return 'Medium';
            case 'low':
                return 'Low';
            default:
                return '-';
        }
    }

    function normalizeRiskLevel(value) {
        switch (String(value || '').trim().toLowerCase()) {
            case 'critical':
                return 'critical';
            case 'high':
                return 'high';
            case 'medium':
                return 'medium';
            case 'low':
                return 'low';
            default:
                return '';
        }
    }

    function buildSeverityClassName(value) {
        const level = normalizeRiskLevel(value);
        return level ? `severity-${level}` : '';
    }

    function formatBooleanLabel(value) {
        return value ? '需要人工确认' : '可自动评估';
    }

    function formatDateTime(value) {
        const ms = toMs(value);
        if (!ms) {
            return '-';
        }

        const date = new Date(ms);
        const month = String(date.getMonth() + 1).padStart(2, '0');
        const day = String(date.getDate()).padStart(2, '0');
        const hour = String(date.getHours()).padStart(2, '0');
        const minute = String(date.getMinutes()).padStart(2, '0');
        return `${month}-${day} ${hour}:${minute}`;
    }

    function toMs(value) {
        if (!value) {
            return 0;
        }
        if (typeof value === 'number') {
            return value;
        }
        const parsed = Date.parse(value);
        return Number.isNaN(parsed) ? 0 : parsed;
    }

    function clamp(value, min, max) {
        if (!Number.isFinite(value)) {
            return min;
        }
        if (max < min) {
            return min;
        }
        return Math.min(max, Math.max(min, value));
    }

    function readBoolStorage(key, fallback) {
        try {
            const value = window.localStorage.getItem(key);
            if (value === null) {
                return fallback;
            }
            return value === '1';
        } catch (error) {
            return fallback;
        }
    }

    function writeBoolStorage(key, value) {
        try {
            if (value) {
                window.localStorage.setItem(key, '1');
            } else {
                window.localStorage.removeItem(key);
            }
        } catch (error) {
            // ignore storage errors
        }
    }

    function isTypingElement(element) {
        if (!(element instanceof HTMLElement)) {
            return false;
        }
        if (element.isContentEditable) {
            return true;
        }
        const tag = element.tagName;
        return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT';
    }



    function escapeHtml(value) {
        return String(value ?? '')
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;')
            .replace(/'/g, '&#39;');
    }

    function formatFileSize(bytes) {
        if (bytes < 1024) return `${bytes} B`;
        if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
        return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    }

    function escapeHtmlAttribute(value) {
        return escapeHtml(value).replace(/`/g, '&#96;');
    }
})();

