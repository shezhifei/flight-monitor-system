var API_BASE = API_BASE || `${window.location.origin}/api/v2`;

const NL_QUERY_API_BASE = `${window.location.origin}/api/v2/ai/nl-query`;

const DISPATCH_NOTIFY_API_BASE = `${window.location.origin}/api/v2/notifications/dispatch`;

const ANOMALY_API_BASE = `${window.location.origin}/api/v2/anomalies`;

const flightListElement = document.getElementById('flightList');

const flightDetailElement = document.getElementById('flightDetail');

const refreshBtn = document.getElementById('refreshBtn');

const lastUpdatedElement = document.getElementById('lastUpdated');

let selectedFlightId = null;

let selectedCaseId = null;

let originalFlights = [];

let editMode = {};

let originalFlightSnapshots = {};

const EMPTY_DISPLAY_TEXT = '--';

const BUSINESS_FILTER_STORAGE_KEY = 'flightMonitorBusinessFilters';

const DEFAULT_BUSINESS_FILTERS = {
    aircraftBodyFilter: 'all',
    commercialSignedFilter: 'yes',
    anomalyFilter: 'all',
    delayFilter: 'all',
    vipFilter: 'all',
    quickTurnFilter: 'all'
};

let latestWorkerRequestId = 0;

let latestWorkerResponseId = 0;

let isDetailDrawerOpen = false;

const UI_CONSTANTS = {
    loaderDelayMs: 180,
    sseReconnectMs: 5000,
    searchDebounceMs: 300,
    listKeyboardStep: 1,
};

const FLIGHT_LIST_PAGE_SIZE = 500;

const FLOATING_BADGE_STACK_ORDER = [
    'updateBadge',
    'openAnomalyBadgeBtn',
    'dispatchNotifyBadgeBtn',
    'openFlightChatBadgeBtn',
    'openDispatchChatBadgeBtn',
];

const CONNECTION_STATUS = {
    connecting: { text: '实时连接中', cls: 'connecting' },
    online: { text: '实时连接正常', cls: 'online' },
    reconnecting: { text: '实时连接重试中', cls: 'reconnecting' },
    offline: { text: '实时连接中断', cls: 'offline' },
};

let globalRequestCount = 0;

let loaderDelayTimer = null;

let currentConnectionStatusKey = '';

let loaderWasShown = false;

let sseReconnectTimer = null;

let suppressFlightReconnect = false;

let anomalyReconnectTimer = null;

let realtimeFullSyncTimer = null;

let aiCapabilityState = {
    loaded: false,
    aiReady: false,
    aiExecutePermission: false,
    aiChatPermission: false,
    missingReasons: [],
    error: '',
};

let flightInsightLoadingState = {
    history: false,
    journey: false,
};

let currentInsightResultPayload = null;

let flightChatConversationId = null;

let flightChatSending = false;

let flightChatInsightSeq = 0;

const flightChatInsightPayloads = new Map();

let flightChatActiveRequestId = null;

let flightChatActiveExecutionId = null;

let flightChatActiveAssistantBubble = null;

let flightChatActiveProgressPanel = null;

let flightChatToolCallSeen = false;

let flightChatActiveRequestBodyStream = false;

let flightChatEventSource = null;

let flightChatExecutionPollTimer = null;

let flightChatLastPollFingerprint = '';

const FLIGHT_CHAT_EXECUTION_POLL_INTERVAL_MS = 5000;

let dispatchChatPanel = null;

let notificationReconnectTimer = null;

const SENT_RECEIPT_REMINDER_GRACE_MS = 5000;
const SENT_RECEIPT_REMINDER_RETRY_BASE_MS = 15000;
const SENT_RECEIPT_REMINDER_RETRY_MAX_MS = 120000;
const SENT_RECEIPT_REMINDER_RETRY_MAX_ATTEMPTS = 5;
const SENT_RECEIPT_REMINDER_PRESENT_RETRY_MS = 3000;

const notificationSeenIds = new Set();

const notificationPanelEntryIds = new Set();

const notificationToastIds = new Set();

const notificationCriticalQueueIds = new Set();

const pendingCriticalNotifications = [];

let activeCriticalNotificationId = null;

let flights = [];

let airportContext = {
    code: '',
    display_name: '本站',
    name_aliases: [],
};

let businessFilters = { ...DEFAULT_BUSINESS_FILTERS };

let isBusinessFilterPanelOpen = false;

let lastBusinessFilterPanelOpen = false;

function normalizeAirportContextV2(rawContext) {
    const context = rawContext && typeof rawContext === 'object' ? rawContext : {};
    const aliases = Array.isArray(context.name_aliases)
        ? context.name_aliases.map((alias) => String(alias || '').trim()).filter(Boolean)
        : [];
    return {
        code: String(context.code || '').trim().toUpperCase(),
        display_name: String(context.display_name || '').trim() || '本站',
        name_aliases: Array.from(new Set(aliases)),
    };
}

async function loadAirportContextV2() {
    try {
        const response = await Auth.fetch(`${API_BASE}/system/airport-context`);
        const payload = await response.json().catch(() => ({}));
        if (!response.ok) {
            throw new Error(payload.detail || payload.message || `HTTP ${response.status}`);
        }
        airportContext = normalizeAirportContextV2(payload);
    } catch (error) {
        console.warn('加载机场上下文失败，使用默认上下文:', error);
        airportContext = normalizeAirportContextV2(airportContext);
    }
}

function formatMissionLabel(rawMission) {
    const parsed = parseMissionValue(rawMission);
    if (!parsed.raw) {
        return '';
    }
    if (!parsed.label) {
        return parsed.raw;
    }
    return parsed.suffix ? `${parsed.label}（${parsed.suffix}）` : parsed.label;
}

function getLegMissionLabelV2(flight, legType) {
    const leg = getLegPayloadV2(flight, legType);
    if (!leg) {
        return '';
    }
    return formatMissionLabel(leg.mission);
}

function applyDispatchNotifyUserFilter() {
    const searchInput = document.getElementById('dispatchNotifySearchInput');
    const keyword = String(searchInput?.value || '').trim().toLowerCase();

    dispatchNotifyModalState.filteredUsers = dispatchNotifyModalState.users.filter((user) => {
        if (!keyword) {
            return true;
        }
        const haystack = [
            user.user_id,
            user.username,
            user.job_title,
            user.department,
            user.status,
        ].join(' ').toLowerCase();
        return haystack.includes(keyword);
    });
    renderDispatchNotifyUserList();
}

function updateFlightLocally(flightId, flightData, changedFields) {
    // Find the flight in the flights array - use string comparison for flightId to handle UUID
    const flightIndex = flights.findIndex(f => String(f.flight_id) === String(flightId));
    let nextFlight = null;
    if (flightIndex !== -1) {
        // Update the flight data
        nextFlight = { ...flights[flightIndex], ...flightData, flight_id: flightData.flight_id || flightId };
        flights[flightIndex] = nextFlight;
        // 标记时间需要重新格式化
        flights[flightIndex]._timesFormatted = false;
        preprocessFlightTimes(flights[flightIndex]);
        nextFlight = flights[flightIndex];
    } else {
        // If flight doesn't exist, add it (for new flights)
        nextFlight = { ...flightData, flight_id: flightData.flight_id || flightId };
        preprocessFlightTimes(nextFlight);
        flights.push(nextFlight);
    }

    // Update the original flights array as well
    const originalFlightIndex = originalFlights.findIndex(f => String(f.flight_id) === String(flightId));
    if (originalFlightIndex !== -1) {
        originalFlights[originalFlightIndex] = { ...originalFlights[originalFlightIndex], ...flightData, flight_id: flightData.flight_id || flightId };
    } else {
        originalFlights.push(nextFlight);
    }

    const shouldRefilter = hasActiveSearchOrBusinessFilters();

    // Delta Update Strategy: Update specific DOM elements instead of full re-render
    if (!shouldRefilter) {
        updateFlightRow(flightId, nextFlight, changedFields);
        updateFlightCard(flightId, nextFlight, changedFields);
    }

    // If this is the selected flight, update the detail view as well
    if (isSameFlightId(selectedFlightId, flightId)) {
        renderFlightDetail();
    }

    // Update last updated time
    updateLastUpdated();
}

function normalizeBusinessFilters(raw = {}) {
    const merged = {
        ...DEFAULT_BUSINESS_FILTERS,
        ...(raw || {})
    };

    if (!['all', 'wide', 'narrow'].includes(merged.aircraftBodyFilter)) {
        merged.aircraftBodyFilter = DEFAULT_BUSINESS_FILTERS.aircraftBodyFilter;
    }
    if (!['all', 'yes', 'no'].includes(merged.commercialSignedFilter)) {
        merged.commercialSignedFilter = DEFAULT_BUSINESS_FILTERS.commercialSignedFilter;
    }
    if (!['all', 'only'].includes(merged.anomalyFilter)) {
        merged.anomalyFilter = DEFAULT_BUSINESS_FILTERS.anomalyFilter;
    }
    if (!['all', 'only'].includes(merged.delayFilter)) {
        merged.delayFilter = DEFAULT_BUSINESS_FILTERS.delayFilter;
    }
    if (!['all', 'only'].includes(merged.vipFilter)) {
        merged.vipFilter = DEFAULT_BUSINESS_FILTERS.vipFilter;
    }
    if (!['all', 'only'].includes(merged.quickTurnFilter)) {
        merged.quickTurnFilter = DEFAULT_BUSINESS_FILTERS.quickTurnFilter;
    }

    return merged;
}

function loadBusinessFiltersFromStorage() {
    try {
        const saved = localStorage.getItem(BUSINESS_FILTER_STORAGE_KEY);
        if (saved) {
            businessFilters = normalizeBusinessFilters(JSON.parse(saved));
        } else {
            businessFilters = { ...DEFAULT_BUSINESS_FILTERS };
        }
    } catch (e) {
        businessFilters = { ...DEFAULT_BUSINESS_FILTERS };
    }
}

function saveBusinessFiltersToStorage() {
    try {
        localStorage.setItem(BUSINESS_FILTER_STORAGE_KEY, JSON.stringify(businessFilters));
    } catch (e) {
        // ignore storage exceptions
    }
}

function syncBusinessFilterUI() {
    const aircraftFilter = document.getElementById('aircraftBodyFilter');
    const signedFilter = document.getElementById('commercialSignedFilter');
    const anomalyFilter = document.getElementById('anomalyFilter');
    const delayFilter = document.getElementById('delayFilter');
    const vipFilter = document.getElementById('vipFilter');
    const quickTurnFilter = document.getElementById('quickTurnFilter');

    if (aircraftFilter) aircraftFilter.value = businessFilters.aircraftBodyFilter;
    if (signedFilter) signedFilter.value = businessFilters.commercialSignedFilter;
    if (anomalyFilter) anomalyFilter.value = businessFilters.anomalyFilter;
    if (delayFilter) delayFilter.value = businessFilters.delayFilter;
    if (vipFilter) vipFilter.value = businessFilters.vipFilter;
    if (quickTurnFilter) quickTurnFilter.value = businessFilters.quickTurnFilter;
    updateBusinessResetButtonState();
}

function setBusinessFilterPanelExpanded(expanded) {
    const panel = document.getElementById('businessFilterBar');
    const toggleBtn = document.getElementById('businessFilterToggle');

    isBusinessFilterPanelOpen = !!expanded;

    if (panel) {
        panel.classList.toggle('expanded', isBusinessFilterPanelOpen);
    }

    if (toggleBtn) {
        toggleBtn.classList.toggle('active', isBusinessFilterPanelOpen);
        toggleBtn.setAttribute('aria-expanded', isBusinessFilterPanelOpen ? 'true' : 'false');
    }

    if (lastBusinessFilterPanelOpen !== isBusinessFilterPanelOpen) {
        announce(isBusinessFilterPanelOpen ? '业务筛选面板已展开' : '业务筛选面板已收起');
        lastBusinessFilterPanelOpen = isBusinessFilterPanelOpen;
    }
}

function isBusinessFilterDefaultState() {
    return Object.keys(DEFAULT_BUSINESS_FILTERS).every(
        (key) => businessFilters[key] === DEFAULT_BUSINESS_FILTERS[key]
    );
}

function updateBusinessFiltersFromUI(persist = true) {
    const aircraftFilter = document.getElementById('aircraftBodyFilter');
    const signedFilter = document.getElementById('commercialSignedFilter');
    const anomalyFilter = document.getElementById('anomalyFilter');
    const delayFilter = document.getElementById('delayFilter');
    const vipFilter = document.getElementById('vipFilter');
    const quickTurnFilter = document.getElementById('quickTurnFilter');

    businessFilters = normalizeBusinessFilters({
        aircraftBodyFilter: aircraftFilter ? aircraftFilter.value : businessFilters.aircraftBodyFilter,
        commercialSignedFilter: signedFilter ? signedFilter.value : businessFilters.commercialSignedFilter,
        anomalyFilter: anomalyFilter ? anomalyFilter.value : businessFilters.anomalyFilter,
        delayFilter: delayFilter ? delayFilter.value : businessFilters.delayFilter,
        vipFilter: vipFilter ? vipFilter.value : businessFilters.vipFilter,
        quickTurnFilter: quickTurnFilter ? quickTurnFilter.value : businessFilters.quickTurnFilter
    });

    if (persist) {
        saveBusinessFiltersToStorage();
    }
}

function hasActiveBusinessFilters() {
    return !isBusinessFilterDefaultState();
}

function hasActiveSearchOrBusinessFilters() {
    const searchInput = document.getElementById('searchInput');
    const query = searchInput ? searchInput.value.trim() : '';
    return query.length > 0 || hasActiveBusinessFilters();
}

function getSearchFieldFilters() {
    const elFlightNo = document.getElementById('searchFlightNo');
    const elDestination = document.getElementById('searchDestination');
    const elDestinationName = document.getElementById('searchDestinationName');
    const elOrigin = document.getElementById('searchOrigin');
    const elOriginName = document.getElementById('searchOriginName');
    const elStatus = document.getElementById('searchStatus');
    const elAircraftType = document.getElementById('searchAircraftType');
    const elStand = document.getElementById('searchStand');
    const elGate = document.getElementById('searchGate');
    const elMission = document.getElementById('searchMission');
    const elFlightType = document.getElementById('searchFlightType');

    return {
        searchFlightNo: elFlightNo ? elFlightNo.checked : true,
        searchDestination: elDestination ? elDestination.checked : true,
        searchDestinationName: elDestinationName ? elDestinationName.checked : true,
        searchOrigin: elOrigin ? elOrigin.checked : true,
        searchOriginName: elOriginName ? elOriginName.checked : true,
        searchStatus: elStatus ? elStatus.checked : true,
        searchAircraftType: elAircraftType ? elAircraftType.checked : true,
        searchStand: elStand ? elStand.checked : true,
        searchGate: elGate ? elGate.checked : true,
        searchMission: elMission ? elMission.checked : true,
        searchFlightType: elFlightType ? elFlightType.checked : true
    };
}

function applyBusinessFiltersLocal(sourceFlights, filters = businessFilters) {
    const safeFlights = Array.isArray(sourceFlights) ? sourceFlights : [];
    const f = normalizeBusinessFilters(filters);

    const allDefault = f.aircraftBodyFilter === 'all' && f.commercialSignedFilter === 'all'
        && f.anomalyFilter === 'all' && f.delayFilter === 'all'
        && f.vipFilter === 'all' && f.quickTurnFilter === 'all';
    if (allDefault) {
        return safeFlights;
    }

    return safeFlights.filter(flight => {
        if (f.aircraftBodyFilter !== 'all') {
            const isWideBody = isWideBodyAircraft(flight.aircraft_type_detail);
            if (f.aircraftBodyFilter === 'wide' && !isWideBody) return false;
            if (f.aircraftBodyFilter === 'narrow' && isWideBody) return false;
        }

        if (f.commercialSignedFilter !== 'all') {
            const signed = normalizeSignedFlag(flight.is_commercial_signed);
            if (f.commercialSignedFilter === 'yes' && signed !== true) return false;
            if (f.commercialSignedFilter === 'no' && signed !== false) return false;
        }

        if (f.anomalyFilter === 'only' && !(getAnomalyCountForFlight(flight) > 0)) return false;
        if (f.delayFilter === 'only' && !isDelayedFlight(flight)) return false;
        if (f.vipFilter === 'only' && !hasVipMarker(flight)) return false;
        if (f.quickTurnFilter === 'only' && !Boolean(flight?.is_quick_turnaround)) return false;

        return true;
    });
}

function buildFilterStatusText() {
    const parts = [];
    const labelMap = {
        aircraftBodyFilter: { wide: '宽体机', narrow: '窄体机' },
        commercialSignedFilter: { yes: '已签约', no: '未签约' },
        anomalyFilter: { only: '仅异常' },
        delayFilter: { only: '仅延误' },
        vipFilter: { only: '仅VIP' },
        quickTurnFilter: { only: '仅快速过站' },
    };
    for (const [key, map] of Object.entries(labelMap)) {
        const value = businessFilters[key];
        if (value && value !== 'all' && value !== DEFAULT_BUSINESS_FILTERS[key]) {
            parts.push(map[value] || value);
        }
    }
    // 默认已签约也要显示
    if (businessFilters.commercialSignedFilter === 'yes') {
        if (!parts.includes('已签约')) parts.push('已签约');
    }
    return parts.length > 0 ? parts.join(' · ') : '';
}

function updateFilterSummaryMeta(filteredCount = flights.length, totalCount = originalFlights.length) {
    const resultPill = document.getElementById('flightResultPill');
    if (resultPill) {
        const statusText = buildFilterStatusText();
        const query = getCurrentSearchQuery();
        const parts = ['当前显示 ' + filteredCount + '/' + totalCount];
        if (statusText) parts.push(statusText);
        if (query) parts.push('搜索"' + query + '"');
        resultPill.textContent = parts.join(' · ');
    }
}

function updateBusinessFilterSummary(filteredCount = flights.length, totalCount = originalFlights.length) {
    updateFilterSummaryMeta(filteredCount, totalCount);
    updateBusinessResetButtonState();
    updateFilterCounts();
}

function updateFilterCounts() {
    const baseFlights = applyBusinessFiltersLocal(originalFlights, {
        ...businessFilters,
        anomalyFilter: 'all',
        delayFilter: 'all',
        vipFilter: 'all',
        quickTurnFilter: 'all',
        aircraftBodyFilter: 'all',
        commercialSignedFilter: businessFilters.commercialSignedFilter
    });
    const countMap = {
        anomalyFilterCount: baseFlights.filter(f => getAnomalyCountForFlight(f) > 0).length,
        delayFilterCount: baseFlights.filter(f => isDelayedFlight(f)).length,
        vipFilterCount: baseFlights.filter(f => hasVipMarker(f)).length,
        quickTurnFilterCount: baseFlights.filter(f => Boolean(f?.is_quick_turnaround)).length,
    };
    for (const [id, count] of Object.entries(countMap)) {
        const el = document.getElementById(id);
        if (el) el.textContent = String(count);
    }
}

function applyCurrentFilters() {
    performSearch({ preserveSelection: true });
}
