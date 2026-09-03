(function () {
    'use strict';

    const DEFAULT_WINDOW_HOURS = 6;
    const DEFAULT_REFRESH_INTERVAL_MS = 15000;
    const MIN_REFRESH_INTERVAL_MS = 5000;
    const DASHBOARD_FLIGHTS_PAGE_SIZE = 500;
    const DASHBOARD_FLIGHTS_MAX_PAGES = 20;
    const MAX_EVENT_ITEMS = 120;
    const FLIGHT_STATUS_LABELS = {
        scheduled: '计划',
        boarding: '登机中',
        departed: '已起飞',
        arrived: '已到达',
        delayed: '延误',
        cancelled: '取消',
        in_progress: '执行中'
    };

    const state = {
        user: null,
        airportContext: {
            code: '',
            display_name: '本站',
            name_aliases: [],
        },
        windowHours: DEFAULT_WINDOW_HOURS,
        refreshIntervalMs: DEFAULT_REFRESH_INTERVAL_MS,
        autoRefresh: true,
        refreshTimer: null,
        clockTimer: null,
        streamRefreshTimer: null,
        flightReconnectTimer: null,
        anomalyReconnectTimer: null,
        suppressFlightReconnect: false,
        flightStream: null,
        anomalyStream: null,
        events: [],
        lastRefreshAt: null,
        dispatchWindowStart: null,
        dispatchWindowEnd: null,
    };

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

    function renderUnifiedState(container, type, message) {
        if (!container) {
            return false;
        }
        if (container.id === 'dispatchSummaryList' || container.id === 'windowPressureList' || container.id === 'terminalLoadList' || container.id === 'standHeatmapChart') {
            return false;
        }
        if (window.EmptyError && typeof window.EmptyError.show === 'function') {
            window.EmptyError.show(container, type, message);
            return true;
        }
        return false;
    }

    /* ---- ECharts shared theme ---- */
    const DH = {
        text: '#8298b5', textBright: '#e8f0fa',
        accent: '#4db8ff', ok: '#2ec47e', warn: '#f0a030', crit: '#ef5350',
        grid: 'rgba(100,140,190,0.08)', axis: 'rgba(100,140,190,0.2)',
        tooltip: { backgroundColor: 'rgba(8,16,28,0.92)', borderColor: 'rgba(100,140,190,0.2)', textStyle: { color: '#e8f0fa', fontSize: 12 } },
    };
    const _cmdCharts = {};
    function ensureChart(id) {
        if (_cmdCharts[id]) return _cmdCharts[id];
        const el = document.getElementById(id);
        if (!el) return null;
        const c = echarts.init(el, null, { renderer: 'canvas' });
        _cmdCharts[id] = c;
        return c;
    }
    window.addEventListener('resize', () => Object.values(_cmdCharts).forEach(c => c.resize()));

    document.addEventListener('DOMContentLoaded', init);

    async function init() {
        const authenticated = await Auth.requireAuthAsync();
        if (!authenticated) {
            return;
        }

        await loadAirportContext();
        await loadCurrentUser();

        // Render unified header with stream chips as extraRight
        if (window.Header) {
            window.Header.render('#header-host', {
                title: '指挥中心',
                subtitle: 'Operations Command',
                showBack: true,
                backHref: '/frontend/html/dashboard.html',
                user: state.user,
                extraRight: '<span class="stream-chip connecting" id="unifiedStreamChip">实时星阵总线</span>' +
                    '<span class="clock-text" id="nowClock">--:--:--</span>' +
                    '<span class="meta-text" id="nowDate">--</span>' +
                    '<span class="meta-text" id="operatorText">值班员: --</span>',
                onLogout: function() {
                    if (typeof window.logout === 'function') window.logout();
                }
            });
        }

        if (window.Breadcrumb && typeof window.Breadcrumb.render === 'function') {
            window.Breadcrumb.render('#breadcrumb-host', [
                { label: '工作台', href: '/frontend/html/dashboard.html' },
                { label: '指挥中心', current: true }
            ]);
        }

        bindControls();
        updateClock();
        state.clockTimer = window.setInterval(updateClock, 1000);

        appendEvent('system', '指挥中心已启动', 'info', { source: 'bootstrap' });
        await refreshDashboard({ silent: true, reason: 'bootstrap' });
        startAutoRefresh();
        connectFlightStream();
        connectAnomalyStream();
        // Connect global SSEHub after auth is ready
        if (typeof SSEHub !== 'undefined' && typeof SSEHub.connect === 'function') {
            SSEHub.connect();
        }
    }

    async function loadCurrentUser() {
        const result = await safeApi('/api/v2/auth/me');
        if (!result.ok) {
            state.user = { username: '未知用户' };
            return;
        }
        state.user = result.data || { username: '未知用户' };
        const operatorText = document.getElementById('operatorText');
        if (operatorText) {
            const roleText = state.user.is_admin ? '管理员' : ((state.user.roles || [])[0] || '值班用户');
            operatorText.textContent = `值班员: ${state.user.username || '用户'} (${roleText})`;
        }
    }

    function normalizeAirportContext(rawContext) {
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

    async function loadAirportContext() {
        const result = await safeApi('/api/v2/system/airport-context');
        if (!result.ok) {
            state.airportContext = normalizeAirportContext(state.airportContext);
            return;
        }
        state.airportContext = normalizeAirportContext(result.data);
    }

    function normalizeRouteStation(rawStation) {
        if (!rawStation || typeof rawStation !== 'object') {
            return null;
        }
        const code = String(rawStation.code || '').trim().toUpperCase();
        const name = String(rawStation.name || '').trim();
        if (!code && !name) {
            return null;
        }
        return {
            code,
            name: name || null,
        };
    }

    function getLegStations(flight, legType, fieldName) {
        const leg = getLegPayload(flight, legType);
        if (!leg || !Array.isArray(leg[fieldName])) {
            return [];
        }
        return leg[fieldName]
            .map((station) => normalizeRouteStation(station))
            .filter(Boolean);
    }

    function getStationListText(flight, legType, fieldName) {
        const stations = getLegStations(flight, legType, fieldName);
        return stations
            .map((station) => station.name || station.code)
            .filter(Boolean)
            .join(', ');
    }

    function getAirportDisplayName() {
        return state.airportContext.display_name || state.airportContext.code || '本站';
    }

    function bindControls() {
        const windowHoursSelect = document.getElementById('windowHoursSelect');
        if (windowHoursSelect) {
            windowHoursSelect.addEventListener('change', async () => {
                const nextWindow = Number(windowHoursSelect.value);
                state.windowHours = Number.isFinite(nextWindow) ? nextWindow : DEFAULT_WINDOW_HOURS;
                await refreshDashboard({ silent: false, reason: 'window_change' });
            });
        }

        const refreshIntervalSelect = document.getElementById('refreshIntervalSelect');
        if (refreshIntervalSelect) {
            refreshIntervalSelect.addEventListener('change', () => {
                const nextInterval = Number(refreshIntervalSelect.value);
                state.refreshIntervalMs = Math.max(MIN_REFRESH_INTERVAL_MS, Number.isFinite(nextInterval) ? nextInterval : DEFAULT_REFRESH_INTERVAL_MS);
                startAutoRefresh();
                updateAutoRefreshText();
                appendEvent('system', `自动刷新间隔调整为 ${Math.round(state.refreshIntervalMs / 1000)} 秒`, 'info');
            });
        }

        const refreshNowBtn = document.getElementById('refreshNowBtn');
        if (refreshNowBtn) {
            refreshNowBtn.addEventListener('click', async () => {
                showButtonLoading(refreshNowBtn, '刷新中...');
                try {
                    await refreshDashboard({ silent: false, reason: 'manual_refresh' });
                } finally {
                    hideButtonLoading(refreshNowBtn);
                }
            });
        }

        const toggleRefreshBtn = document.getElementById('toggleRefreshBtn');
        if (toggleRefreshBtn) {
            toggleRefreshBtn.addEventListener('click', () => {
                state.autoRefresh = !state.autoRefresh;
                startAutoRefresh();
                updateAutoRefreshText();
                appendEvent(
                    'system',
                    state.autoRefresh ? '自动刷新已恢复' : '自动刷新已暂停',
                    state.autoRefresh ? 'info' : 'warn'
                );
            });
        }

        const clearEventsBtn = document.getElementById('clearEventsBtn');
        if (clearEventsBtn) {
            clearEventsBtn.addEventListener('click', () => {
                state.events = [];
                renderEventFeed();
            });
        }

        const fullScreenBtn = document.getElementById('fullScreenBtn');
        if (fullScreenBtn) {
            fullScreenBtn.addEventListener('click', async () => {
                if (!document.fullscreenElement) {
                    try {
                        await document.documentElement.requestFullscreen();
                        appendEvent('system', '已进入全屏模式', 'info');
                    } catch (_error) {
                        appendEvent('system', '全屏模式启动失败', 'warn');
                    }
                    return;
                }

                try {
                    await document.exitFullscreen();
                } catch (_error) {
                    appendEvent('system', '退出全屏失败', 'warn');
                }
            });
        }

        window.addEventListener('beforeunload', () => {
            stopAutoRefresh();
            if (state.clockTimer) {
                window.clearInterval(state.clockTimer);
                state.clockTimer = null;
            }
            if (state.streamRefreshTimer) {
                window.clearTimeout(state.streamRefreshTimer);
                state.streamRefreshTimer = null;
            }
            state.suppressFlightReconnect = true;
            if (state.flightReconnectTimer) {
                window.clearTimeout(state.flightReconnectTimer);
                state.flightReconnectTimer = null;
            }
            closeStreams();
        });
    }

    async function refreshDashboard(options = {}) {
        const now = new Date();
        const dispatchWindowStart = new Date(now.getTime() - 60 * 60 * 1000);
        const dispatchWindowEnd = new Date(now.getTime() + state.windowHours * 60 * 60 * 1000);
        state.dispatchWindowStart = dispatchWindowStart;
        state.dispatchWindowEnd = dispatchWindowEnd;

        const dispatchTimelineParams = new URLSearchParams({
            view_mode: 'flight',
            window_start: dispatchWindowStart.toISOString(),
            window_end: dispatchWindowEnd.toISOString(),
        });
        const conflictParams = new URLSearchParams({
            window_start: dispatchWindowStart.toISOString(),
            window_end: dispatchWindowEnd.toISOString(),
            limit: '200',
        });

        const [
            flightsResult,
            kpiSnapshotResult,
            anomalyStatsResult,
            anomalyListResult,
            dispatchTimelineResult,
            dispatchConflictsResult,
            healthPingResult,
        ] = await Promise.all([
            loadDashboardFlightsPaged(),
            safeApi('/api/v2/kpi/snapshot?time_range=today'),
            safeApi('/api/v2/anomalies/stats'),
            safeApi('/api/v2/anomalies?status=open&limit=40&offset=0'),
            safeApi(`/api/v2/dispatch-orders/timeline?${dispatchTimelineParams.toString()}`),
            safeApi(`/api/v2/dispatch-orders/conflicts?${conflictParams.toString()}`),
            safeApi('/api/v2/system/runtime/health/ping'),
        ]);

        const flights = Array.isArray(flightsResult.data) ? flightsResult.data : [];
        const kpiSnapshot = kpiSnapshotResult.ok && kpiSnapshotResult.data ? kpiSnapshotResult.data : null;
        const anomalyStats = anomalyStatsResult.ok && anomalyStatsResult.data ? anomalyStatsResult.data : {};
        const anomalyItems = anomalyListResult.ok && anomalyListResult.data
            ? (Array.isArray(anomalyListResult.data.items) ? anomalyListResult.data.items : [])
            : [];
        const dispatchTimeline = dispatchTimelineResult.ok && dispatchTimelineResult.data ? dispatchTimelineResult.data : null;
        const dispatchConflicts = dispatchConflictsResult.ok && dispatchConflictsResult.data ? dispatchConflictsResult.data : null;

        const commandSnapshot = buildCommandSnapshot({
            flights,
            kpiSnapshot,
            anomalyStats,
            anomalyItems,
            dispatchTimeline,
            dispatchConflicts,
            healthPingResult,
            dispatchTimelineResult,
            dispatchConflictsResult,
            now,
        });

        renderMetrics(commandSnapshot);
        renderOperationsVerdict(commandSnapshot);
        renderDecisionQueue(commandSnapshot);
        renderWindowPressure(commandSnapshot);
        renderTerminalLoad(flights);
        renderDispatchSummary(dispatchTimeline, dispatchConflicts, dispatchTimelineResult, dispatchConflictsResult);
        renderStandHeatmap(anomalyItems);

        state.lastRefreshAt = now;
        updateLastRefreshDisplay(now);
        updateFooterWindow();

        if (!options.silent) {
            appendEvent(
                'snapshot',
                `全局态势已刷新：航班 ${flights.length} 架次，异常 ${anomalyItems.length} 条`,
                'info',
                { reason: options.reason || 'refresh' }
            );
        }
    }

    async function loadDashboardFlightsPaged() {
        const merged = [];
        const seen = new Set();

        for (let page = 1; page <= DASHBOARD_FLIGHTS_MAX_PAGES; page += 1) {
            const result = await safeApi(`/api/v2/flights?page=${page}&page_size=${DASHBOARD_FLIGHTS_PAGE_SIZE}`);
            if (!result.ok) {
                if (page === 1) {
                    return result;
                }
                break;
            }

            const pageItems = Array.isArray(result.data) ? result.data : [];
            for (const flight of pageItems) {
                const flightId = String((flight && flight.flight_id) || '').trim();
                if (!flightId) {
                    merged.push(flight);
                    continue;
                }
                if (seen.has(flightId)) {
                    continue;
                }
                seen.add(flightId);
                merged.push(flight);
            }

            if (pageItems.length < DASHBOARD_FLIGHTS_PAGE_SIZE) {
                break;
            }
        }

        return {
            ok: true,
            status: 200,
            data: merged,
            raw: { success: true, data: merged },
        };
    }

    function buildCommandSnapshot(payload) {
        const flights = payload.flights || [];
        const anomalyStats = payload.anomalyStats || {};
        const anomalyItems = payload.anomalyItems || [];
        const dispatchTimeline = payload.dispatchTimeline || {};
        const dispatchConflicts = payload.dispatchConflicts || {};
        const kpiSnapshot = payload.kpiSnapshot || {};
        const now = payload.now instanceof Date ? payload.now : new Date();
        const windowEnd = new Date(now.getTime() + state.windowHours * 60 * 60 * 1000);
        const statusCounts = buildFlightStatusCounts(flights);
        const dispatchCounts = dispatchTimeline.status_counts || {};

        const totalFlights = flights.length;
        const activeFlights = flights.filter(isActiveFlight).length;
        const delayedFlights = flights.filter(isDelayedFlight).length;
        const openAnomalyCount = Number(anomalyStats.open || anomalyItems.length || 0);
        const criticalAnomalyCount = Number(anomalyStats.critical || anomalyItems.filter((item) => isHighSeverity(item.severity)).length || 0);
        const dispatchPending = Number(dispatchCounts.pending || 0);
        const dispatchInProgress = Number(dispatchCounts.in_progress || 0);
        const dispatchConflictCount = Number(dispatchConflicts.conflict_count || 0);
        const serviceRateNumber = Number(kpiSnapshot.service_node_compliance_rate) || 0;
        const equipmentRateNumber = Number(kpiSnapshot.equipment_utilization_rate) || 0;
        const otpRateNumber = Number(kpiSnapshot.on_time_departure_rate) || 0;
        const systemHealthOk = payload.healthPingResult.ok && payload.healthPingResult.data && payload.healthPingResult.data.status === 'ok';
        const topHotspot = buildAnomalyHotspotSummary(anomalyItems);
        const topTerminal = buildTerminalPressureSummary(flights);

        const flightRiskEntries = flights
            .map((flight) => {
                const departureTime = pickDepartureTime(flight);
                if (!departureTime) {
                    return null;
                }
                const departureMs = departureTime.getTime();
                if (departureMs < now.getTime() || departureMs > windowEnd.getTime()) {
                    return null;
                }
                const delayMinutes = calculateDelayMinutes(flight);
                const timeToDepartureMinutes = Math.max(0, Math.round((departureMs - now.getTime()) / 60000));
                const riskScore = buildFlightRiskScore(flight, delayMinutes, timeToDepartureMinutes);
                return {
                    flight,
                    departureTime,
                    delayMinutes,
                    timeToDepartureMinutes,
                    riskScore,
                    priorityClass: getFlightPriorityClass(delayMinutes, normalizeStatus(flight.status), timeToDepartureMinutes),
                };
            })
            .filter(Boolean)
            .sort((left, right) => {
                if (right.riskScore !== left.riskScore) {
                    return right.riskScore - left.riskScore;
                }
                return left.departureTime.getTime() - right.departureTime.getTime();
            });

        const riskFlights60 = flightRiskEntries.filter((entry) => entry.timeToDepartureMinutes <= 60 && entry.priorityClass !== 'info');
        const immediateRiskFlights = riskFlights60.filter((entry) => entry.timeToDepartureMinutes <= 30);
        const decisionEntries = buildDecisionEntries({
            anomalyItems,
            flightRiskEntries,
            dispatchPending,
            dispatchInProgress,
            dispatchConflictCount,
            dispatchConflictsResult: payload.dispatchConflictsResult,
            now,
        });

        return {
            flights,
            statusCounts,
            anomalyItems,
            totalFlights,
            activeFlights,
            delayedFlights,
            openAnomalyCount,
            criticalAnomalyCount,
            dispatchPending,
            dispatchInProgress,
            dispatchConflictCount,
            serviceRateNumber,
            equipmentRateNumber,
            otpRateNumber,
            systemHealthOk,
            topHotspot,
            topTerminal,
            now,
            windowEnd,
            flightRiskEntries,
            riskFlights60,
            immediateRiskFlights,
            decisionEntries,
            dispatchConflictsResult: payload.dispatchConflictsResult,
        };
    }

    function renderMetrics(snapshot) {
        const p1Count = snapshot.decisionEntries.filter((entry) => entry.priorityRank === 'P1').length;
        const p2Count = snapshot.decisionEntries.filter((entry) => entry.priorityRank === 'P2').length;

        setText('metricDecisionCount', String(snapshot.decisionEntries.length));
        setText('metricDecisionCountSub', `P1 ${p1Count} 项 | P2 ${p2Count} 项`);
        setText('metricRiskFlights', String(snapshot.riskFlights60.length));
        setText('metricRiskFlightsSub', `30 分钟内 ${snapshot.immediateRiskFlights.length} 架`);
        setText('metricOpenAnomalies', String(snapshot.openAnomalyCount));
        setText('metricOpenAnomaliesSub', `高优先级 ${snapshot.criticalAnomalyCount} 条`);

        if (snapshot.dispatchConflictsResult.ok) {
            setText('metricDispatchBlockers', String(snapshot.dispatchConflictCount + snapshot.dispatchPending));
            setText('metricDispatchBlockersSub', `冲突 ${snapshot.dispatchConflictCount} | 待派工 ${snapshot.dispatchPending}`);
        } else {
            setText('metricDispatchBlockers', snapshot.dispatchConflictsResult.status === 403 ? '无权限' : '--');
            setText('metricDispatchBlockersSub', snapshot.dispatchConflictsResult.status === 403 ? '缺少 dispatch:view' : '阻塞数据不可用');
        }

        setText('metricDelayPressure', String(snapshot.delayedFlights));
        setText('metricDelayPressureSub', `出港准点率 ${toPercent(snapshot.otpRateNumber)} | 服务履约 ${toPercent(snapshot.serviceRateNumber)}`);
        setText('metricSystemHealth', snapshot.systemHealthOk ? '在线' : '降级');
        setText(
            'metricSystemHealthSub',
            snapshot.systemHealthOk
                ? `运行中 ${snapshot.activeFlights} | 设备 ${toPercent(snapshot.equipmentRateNumber)}`
                : '仅部分数据可用'
        );
    }

    function renderOperationsVerdict(snapshot) {
        const chip = document.getElementById('opsVerdictChip');
        if (!chip) {
            return;
        }

        let verdictState = 'is-ok';
        let verdictLabel = '可控';
        let verdictTitle = '可控';
        let verdictDetail = `风险离港 ${snapshot.riskFlights60.length} | 异常 ${snapshot.openAnomalyCount} | 热区 ${snapshot.topHotspot.label}`;

        if (!snapshot.systemHealthOk || snapshot.criticalAnomalyCount > 0 || snapshot.dispatchConflictCount >= 3 || snapshot.riskFlights60.length >= 6) {
            verdictState = 'is-critical';
            verdictLabel = '失稳';
            verdictTitle = '高压失稳';
            verdictDetail = `风险离港 ${snapshot.riskFlights60.length} | 冲突 ${snapshot.dispatchConflictCount} | 严重异常 ${snapshot.criticalAnomalyCount} | 热区 ${snapshot.topHotspot.label}`;
        } else if (snapshot.riskFlights60.length >= 3 || snapshot.openAnomalyCount >= 20 || snapshot.dispatchPending >= 5 || snapshot.delayedFlights >= 16) {
            verdictState = 'is-warn';
            verdictLabel = '盯防';
            verdictTitle = '高压盯防';
            verdictDetail = `风险离港 ${snapshot.riskFlights60.length} | 待派工 ${snapshot.dispatchPending} | 热区 ${snapshot.topHotspot.label}`;
        }

        chip.classList.remove('is-ok', 'is-info', 'is-warn', 'is-critical');
        chip.classList.add(verdictState);
        chip.textContent = verdictLabel;
        setText('opsVerdictTitle', verdictTitle);
        setText('opsVerdictDetail', verdictDetail);
        setText(
            'opsVerdictMeta',
            `${formatTime(snapshot.now)} - ${formatTime(snapshot.windowEnd)} | 延误 ${snapshot.delayedFlights} | 履约 ${toPercent(snapshot.serviceRateNumber)}`
        );
    }

    function buildDecisionEntries(payload) {
        const entries = [];
        const now = payload.now instanceof Date ? payload.now : new Date();

        const anomalyEntries = (payload.anomalyItems || [])
            .map((item) => {
                const severity = String(item.severity || 'medium').toLowerCase();
                const detectedAt = pickDate(item.detected_at);
                const ageMinutes = detectedAt ? Math.max(0, Math.round((now.getTime() - detectedAt.getTime()) / 60000)) : 999;
                const urgencyScore = severityWeight(severity) * 42 + Math.max(0, 35 - Math.min(ageMinutes, 35));
                return {
                    sourceType: 'anomaly',
                    score: urgencyScore,
                    priorityClass: isHighSeverity(severity) ? 'critical' : (severity === 'medium' ? 'warn' : 'info'),
                    title: item.title || item.anomaly_type || '异常告警',
                    context: `${describeAnomalyTarget(item)} | ${describeAnomalyReason(item)} | ${formatDateTime(item.detected_at)}`,
                    urgencyText: ageMinutes <= 15 ? '立即核对' : `${Math.max(15, ageMinutes)} 分钟内复查`,
                    ownerHint: '先找异常处理席',
                    badges: [
                        { label: severityLabel(severity), className: isHighSeverity(severity) ? 'critical' : 'warn' },
                        { label: 'anomaly', className: 'anomaly' },
                    ],
                };
            })
            .sort((left, right) => right.score - left.score)
            .slice(0, 4);

        const flightEntries = (payload.flightRiskEntries || [])
            .filter((entry) => entry.priorityClass !== 'info')
            .slice(0, 4)
            .map((entry) => {
                const status = normalizeStatus(entry.flight.status);
                const route = buildRouteText(entry.flight);
                return {
                    sourceType: 'flight',
                    score: entry.riskScore,
                    priorityClass: entry.priorityClass,
                    title: `${getFlightNo(entry.flight)} ${entry.timeToDepartureMinutes <= 0 ? '已到起飞时点' : `${entry.timeToDepartureMinutes} 分钟后离港`}`,
                    context: `${route} | ${entry.delayMinutes > 0 ? `预计延误 ${entry.delayMinutes} 分钟` : `状态 ${labelStatus(status)}`} | ${formatTime(entry.departureTime)}`,
                    urgencyText: entry.timeToDepartureMinutes <= 30 ? '30 分钟内处理' : '提前盯防',
                    ownerHint: '先找离港调度席',
                    badges: [
                        { label: entry.delayMinutes > 0 ? `延误 ${entry.delayMinutes}m` : labelStatus(status), className: entry.priorityClass },
                        { label: 'flight', className: 'flight' },
                    ],
                };
            });

        entries.push(...anomalyEntries, ...flightEntries);

        if (payload.dispatchConflictsResult.ok && (payload.dispatchConflictCount > 0 || payload.dispatchPending > 0)) {
            entries.push({
                sourceType: 'dispatch',
                score: payload.dispatchConflictCount * 55 + payload.dispatchPending * 7 + payload.dispatchInProgress * 3,
                priorityClass: payload.dispatchConflictCount > 0 ? 'critical' : 'warn',
                title: `调度窗口存在 ${payload.dispatchConflictCount} 项资源冲突，待派工 ${payload.dispatchPending} 项`,
                context: `进行中 ${payload.dispatchInProgress} 项 | 当前窗口需要优先核对资源重叠与派工积压`,
                urgencyText: payload.dispatchConflictCount > 0 ? '当前窗口处理' : '尽快复查',
                ownerHint: '先找资源调度席',
                badges: [
                    { label: `冲突 ${payload.dispatchConflictCount}`, className: payload.dispatchConflictCount > 0 ? 'critical' : 'warn' },
                    { label: 'dispatch', className: 'dispatch' },
                ],
            });
        }

        return entries
            .sort((left, right) => right.score - left.score)
            .slice(0, 7)
            .map((entry, index) => ({
                ...entry,
                priorityRank: index === 0 ? 'P1' : (index <= 2 ? 'P2' : 'P3'),
            }));
    }

    function renderDecisionQueue(snapshot) {
        const list = document.getElementById('decisionQueueList');
        if (!list) {
            return;
        }

        const p1Count = snapshot.decisionEntries.filter((entry) => entry.priorityRank === 'P1').length;
        setText('decisionQueueMeta', `P1 ${p1Count} | ${formatTime(snapshot.now)}–${formatTime(snapshot.windowEnd)}`);

        if (!snapshot.decisionEntries.length) {
            if (!renderUnifiedState(list, 'empty', '当前窗口暂无需要立即动作的对象')) {
                list.innerHTML = '<div class="empty">当前窗口暂无需要立即动作的对象</div>';
            }
            return;
        }

        list.innerHTML = snapshot.decisionEntries.map((entry) => `
            <div class="priority-item decision-item priority-${entry.priorityClass}">
                <div class="priority-main">
                    <span class="priority-kicker">${escapeHtml(entry.sourceType.toUpperCase())}</span>
                    <div class="priority-title">${escapeHtml(entry.title)}</div>
                    <div class="priority-context">${escapeHtml(entry.context)}</div>
                    <div class="decision-tags">${entry.badges.map((badge) => `<span class="tag ${escapeHtml(badge.className)}">${escapeHtml(badge.label)}</span>`).join('')}</div>
                </div>
                <div class="decision-side">
                    <span class="decision-rank">${escapeHtml(entry.priorityRank)}</span>
                    <span class="decision-caption">${escapeHtml(`${entry.ownerHint} · ${entry.urgencyText}`)}</span>
                </div>
            </div>
        `).join('');
    }

    function renderWindowPressure(snapshot) {
        const chart = ensureChart('windowPressureList');
        if (!chart) return;

        const buckets = [
            { label: '0-30 分钟', items: snapshot.flightRiskEntries.filter((entry) => entry.timeToDepartureMinutes <= 30) },
            { label: '31-60 分钟', items: snapshot.flightRiskEntries.filter((entry) => entry.timeToDepartureMinutes > 30 && entry.timeToDepartureMinutes <= 60) },
            { label: '60 分钟后', items: snapshot.flightRiskEntries.filter((entry) => entry.timeToDepartureMinutes > 60) },
        ];
        setText('windowPressureMeta', `${formatTime(snapshot.now)} - ${formatTime(snapshot.windowEnd)}`);

        const labels = buckets.map(b => b.label);
        const totalData = buckets.map(b => b.items.length);
        const riskData = buckets.map(b => b.items.filter(e => e.priorityClass !== 'info').length);

        chart.setOption({
            grid: { top: 20, right: 12, bottom: 24, left: 76 },
            tooltip: { ...DH.tooltip, trigger: 'axis',
                formatter: params => params.map(p => `${p.marker} ${p.seriesName}: <b>${p.value}</b> 架`).join('<br/>')
            },
            legend: { show: true, right: 8, top: 0, textStyle: { color: DH.text, fontSize: 10 }, itemWidth: 10, itemHeight: 10 },
            xAxis: { type: 'value', axisLabel: { color: DH.text, fontSize: 10 }, splitLine: { lineStyle: { color: DH.grid } }, axisLine: { show: false } },
            yAxis: { type: 'category', data: labels, axisLine: { lineStyle: { color: DH.axis } }, axisLabel: { color: DH.text, fontSize: 10 }, axisTick: { show: false } },
            series: [
                { name: '总离港', type: 'bar', data: totalData, barMaxWidth: 18, itemStyle: { color: DH.accent, borderRadius: [0, 3, 3, 0] } },
                { name: '高风险', type: 'bar', data: riskData, barMaxWidth: 18, itemStyle: { color: DH.crit, borderRadius: [0, 3, 3, 0] } },
            ]
        }, true);
    }

    function renderStatusDistribution(flights) {
        const list = document.getElementById('statusDistributionList');
        if (!list) {
            return;
        }

        const counts = buildFlightStatusCounts(flights);
        const entries = Object.entries(counts)
            .filter((entry) => entry[1] > 0)
            .sort((left, right) => right[1] - left[1]);

        if (entries.length === 0) {
            if (!renderUnifiedState(list, 'empty', '暂无航班状态数据')) {
                list.innerHTML = '<div class="empty">暂无航班状态数据</div>';
            }
            return;
        }

        const total = entries.reduce((sum, entry) => sum + entry[1], 0);
        list.innerHTML = entries.map((entry) => {
            const status = entry[0];
            const count = entry[1];
            const ratio = total > 0 ? (count / total) * 100 : 0;
            return `
                <div class="distribution-segment">
                    <div>
                        <strong>${escapeHtml(labelStatus(status))}</strong>
                        <span class="distribution-caption">${count} 架次 · 占比 ${ratio.toFixed(0)}%</span>
                    </div>
                    <span>${ratio.toFixed(0)}%</span>
                    <div class="distribution-meter"><span style="width:${Math.max(ratio, 6)}%"></span></div>
                </div>
            `;
        }).join('');
    }

    function buildAnomalyHotspotSummary(anomalies) {
        const counts = {};
        for (const item of anomalies || []) {
            const ctx = item && typeof item.context_data === 'object' ? item.context_data : {};
            const key = String(ctx.resource_value || ctx.stand || item.stand_id || item.stand || '未知区').trim() || '未知区';
            counts[key] = (counts[key] || 0) + 1;
        }
        const entries = Object.entries(counts).sort((left, right) => right[1] - left[1]);
        if (!entries.length) {
            return { label: '异常热区未形成', count: 0 };
        }
        return { label: `机位 ${entries[0][0]}`, count: entries[0][1] };
    }

    function buildTerminalPressureSummary(flights) {
        const counts = {};
        for (const flight of flights || []) {
            const terminal = String(flight.terminal || '未分配').trim() || '未分配';
            counts[terminal] = (counts[terminal] || 0) + 1;
        }
        const entries = Object.entries(counts).sort((left, right) => right[1] - left[1]);
        if (!entries.length) {
            return { label: '航站楼压力未集中', count: 0 };
        }
        return { label: `航站楼 ${entries[0][0]}`, count: entries[0][1] };
    }

    function renderTerminalLoad(flights) {
        const chart = ensureChart('terminalLoadList');
        if (!chart) return;

        const terminalMap = new Map();
        for (const flight of flights) {
            const terminal = String(flight.terminal || '未分配').trim() || '未分配';
            terminalMap.set(terminal, (terminalMap.get(terminal) || 0) + 1);
        }
        const entries = Array.from(terminalMap.entries()).sort((a, b) => b[1] - a[1]).slice(0, 6);
        const topTerminal = entries[0];
        setText('terminalLoadMeta', topTerminal ? `最拥挤 ${topTerminal[0]} · ${topTerminal[1]} 架次` : '按航班数统计');

        if (!entries.length) {
            chart.clear();
            chart.showLoading({ text: '暂无航站楼数据', color: DH.accent, textColor: DH.text, maskColor: 'rgba(5,11,20,0.6)' });
            return;
        }
        chart.hideLoading();

        const colors = [DH.accent, DH.ok, DH.warn, '#8b6cef', DH.crit, '#5cc9d0'];
        chart.setOption({
            tooltip: { ...DH.tooltip, trigger: 'item', formatter: '{b}: {c} 架次 ({d}%)' },
            series: [{
                type: 'pie', radius: ['28%', '68%'], center: ['50%', '52%'],
                roseType: 'radius',
                label: { color: DH.textBright, fontSize: 11, formatter: '{b}\n{c}' },
                labelLine: { lineStyle: { color: DH.axis } },
                itemStyle: { borderColor: '#050b14', borderWidth: 2, borderRadius: 4 },
                data: entries.map(([name, count], i) => ({ name, value: count, itemStyle: { color: colors[i % colors.length] } })),
            }]
        }, true);
    }


    let standHeatmapInstance = null;
    function renderStandHeatmap(anomalies) {
        const chartDom = document.getElementById('standHeatmapChart');
        if (!chartDom) return;

        if (!standHeatmapInstance) {
            standHeatmapInstance = echarts.init(chartDom);
            window.addEventListener('resize', () => {
                standHeatmapInstance.resize();
            });
        }

        const standCounts = {};
        for (const item of anomalies) {
            const ctx = item.context_data || {};
            const standId = ctx.resource_value || ctx.stand || item.stand_id || item.stand || '未知区';
            if (!standCounts[standId]) {
                standCounts[standId] = 0;
            }
            standCounts[standId] += 1;
        }

        const dataArr = Object.entries(standCounts)
            .map(([name, count]) => ({
                name: `机位 ${name}`,
                value: count
            }))
            .sort((a, b) => b.value - a.value);

        const heatmapMeta = document.getElementById('riskHeatmapMeta');
        if (dataArr.length === 0) {
            if (heatmapMeta) {
                heatmapMeta.textContent = '当前无高风险聚集区';
            }
            dataArr.push({ name: '无异常', value: 0 });
        } else if (heatmapMeta) {
            heatmapMeta.textContent = `最高风险 ${dataArr[0].name} · ${dataArr[0].value} 条`;
        }

        const option = {
            tooltip: {
                trigger: 'item',
                formatter: '{b}: {c} 次',
                ...DH.tooltip
            },
            visualMap: {
                show: false,
                min: 0,
                max: Math.max(...dataArr.map(d => d.value), 5),
                inRange: { color: ['#0f2238', '#1b5f8d', '#26b3ff', '#ff6f66'] }
            },
            series: [{
                name: '热区', type: 'treemap', width: '100%', height: '100%', roam: false, nodeClick: false,
                breadcrumb: { show: false },
                label: { show: true, color: '#f3faff', fontSize: 13 },
                itemStyle: {
                    borderColor: '#050b14',
                    borderWidth: 2,
                    gapWidth: 2
                },
                data: dataArr
            }]
        };
        standHeatmapInstance.setOption(option);
    }

    function renderUpcomingFlights(flights, now) {
        const list = document.getElementById('upcomingFlightsList');
        const meta = document.getElementById('upcomingWindowMeta');
        if (!list) {
            return;
        }

        const windowEnd = new Date(now.getTime() + state.windowHours * 60 * 60 * 1000);
        if (meta) {
            meta.textContent = `${formatTime(now)} - ${formatTime(windowEnd)}`;
        }

        const candidates = flights
            .map((flight) => {
                const departureTime = pickDepartureTime(flight);
                if (!departureTime) {
                    return null;
                }
                const departureMs = departureTime.getTime();
                if (departureMs < now.getTime() || departureMs > windowEnd.getTime()) {
                    return null;
                }
                return {
                    flight,
                    departureTime,
                    delayMinutes: calculateDelayMinutes(flight),
                    timeToDepartureMinutes: Math.max(0, Math.round((departureMs - now.getTime()) / 60000)),
                };
            })
            .filter(Boolean)
            .map((entry) => ({
                ...entry,
                riskScore: buildFlightRiskScore(entry.flight, entry.delayMinutes, entry.timeToDepartureMinutes),
            }))
            .sort((left, right) => {
                if (right.riskScore !== left.riskScore) {
                    return right.riskScore - left.riskScore;
                }
                return left.departureTime.getTime() - right.departureTime.getTime();
            })
            .slice(0, 6);

        if (candidates.length === 0) {
            if (!renderUnifiedState(list, 'empty', '窗口内暂无重点航班')) {
                list.innerHTML = '<div class="empty">窗口内暂无重点航班</div>';
            }
            return;
        }

        list.innerHTML = candidates.map((entry) => {
            const flight = entry.flight;
            const flightNo = getFlightNo(flight);
            const status = normalizeStatus(flight.status);
            const delayMinutes = entry.delayMinutes;
            const timeToDepartureText = entry.timeToDepartureMinutes <= 0 ? '已到起飞时点' : `${entry.timeToDepartureMinutes} 分钟后起飞`;
            const severity = getFlightPriorityClass(delayMinutes, status, entry.timeToDepartureMinutes);
            const tagClass = severity;
            const tagText = severity === 'critical' ? '高风险' : (severity === 'warn' ? '盯防' : '关注');
            const route = buildRouteText(flight);
            const contextBits = [
                route,
                timeToDepartureText,
                delayMinutes > 0 ? `预计延误 ${delayMinutes} 分钟` : `状态 ${labelStatus(status)}`,
            ];
            return `
                <div class="priority-item priority-${tagClass}">
                    <div class="priority-main">
                        <span class="priority-kicker">Flight Watch</span>
                        <div class="priority-title"><strong>${escapeHtml(flightNo)}</strong> ${escapeHtml(timeToDepartureText)}</div>
                        <div class="priority-context">${escapeHtml(contextBits.join(' | '))}</div>
                    </div>
                    <div class="priority-side">
                        <span class="tag ${tagClass}">${escapeHtml(tagText)}</span>
                        <span class="priority-caption">${escapeHtml(formatTime(entry.departureTime))}</span>
                    </div>
                </div>
            `;
        }).join('');
    }

    function renderDispatchSummary(dispatchTimeline, dispatchConflicts, dispatchTimelineResult, dispatchConflictsResult) {
        const list = document.getElementById('dispatchSummaryList');
        const meta = document.getElementById('dispatchWindowMeta');
        if (!list) {
            return;
        }

        if (meta && state.dispatchWindowStart && state.dispatchWindowEnd) {
            meta.textContent = `${formatTime(state.dispatchWindowStart)} - ${formatTime(state.dispatchWindowEnd)}`;
        }

        if (!dispatchTimelineResult.ok) {
            if (dispatchTimelineResult.status === 403) {
                if (!renderUnifiedState(list, 'forbidden', '无派工查看权限（dispatch:view）')) {
                    list.innerHTML = '<div class="empty">无派工查看权限（dispatch:view）</div>';
                }
                return;
            }
            if (!renderUnifiedState(list, 'error', '派工时间线数据暂不可用')) {
                list.innerHTML = '<div class="empty">派工时间线数据暂不可用</div>';
            }
            return;
        }

        const statusCounts = (dispatchTimeline && dispatchTimeline.status_counts) || {};
        const statuses = [
            ['pending', '待派工'],
            ['assigned', '已分配'],
            ['in_progress', '进行中'],
            ['completed', '已完成'],
            ['cancelled', '已取消'],
        ];

        const rows = statuses.map((entry) => {
            const key = entry[0];
            const label = entry[1];
            return {
                label,
                value: Number(statusCounts[key] || 0),
                tagClass: key === 'pending' || key === 'in_progress' ? 'warn' : 'info',
            };
        });

        if (dispatchConflictsResult.ok) {
            rows.unshift({
                label: '资源冲突',
                value: Number((dispatchConflicts && dispatchConflicts.conflict_count) || 0),
                tagClass: 'critical',
                note: '优先核对同一窗口资源占用重叠',
            });
        }

        const maxValue = Math.max(1, ...rows.map((row) => row.value));

        const chart = ensureChart('dispatchSummaryList');
        if (!chart) return;
        chart.hideLoading();

        const colorMap = { critical: DH.crit, warn: DH.warn, info: DH.accent };
        chart.setOption({
            grid: { top: 8, right: 48, bottom: 4, left: 68 },
            tooltip: { ...DH.tooltip, trigger: 'axis', formatter: params => params.map(p => `${p.name}: <b>${p.value}</b>`).join('<br/>') },
            xAxis: { type: 'value', show: false },
            yAxis: { type: 'category', data: rows.map(r => r.label), axisLine: { show: false }, axisLabel: { color: DH.text, fontSize: 10 }, axisTick: { show: false }, inverse: true },
            series: [{
                type: 'bar', barMaxWidth: 16,
                data: rows.map(r => ({
                    value: r.value,
                    itemStyle: {
                        color: new echarts.graphic.LinearGradient(0, 0, 1, 0, [
                            { offset: 0, color: colorMap[r.tagClass] || DH.accent },
                            { offset: 1, color: (colorMap[r.tagClass] || DH.accent) + '44' }
                        ]),
                        borderRadius: [0, 3, 3, 0]
                    }
                })),
                label: { show: true, position: 'right', color: DH.textBright, fontSize: 11 }
            }]
        }, true);
    }

    function renderAnomalyQueue(anomalyItems, anomalyStats, anomalyListResult) {
        const list = document.getElementById('anomalyQueueList');
        const meta = document.getElementById('anomalyQueueMeta');
        if (!list) {
            return;
        }

        const totalText = Number(anomalyStats.total || anomalyItems.length || 0);
        if (meta) {
            meta.textContent = `总数 ${totalText} | 未关闭 ${Number(anomalyStats.open || anomalyItems.length || 0)}`;
        }
        setText('anomalySummaryText', `高优先级 ${Number(anomalyStats.critical || anomalyItems.filter((item) => isHighSeverity(item.severity)).length || 0)} 条，需要先核对影响对象与处理窗口。`);

        if (!anomalyListResult.ok) {
            if (anomalyListResult.status === 403) {
                if (!renderUnifiedState(list, 'forbidden', '无异常查看权限（flight:read）')) {
                    list.innerHTML = '<div class="empty">无异常查看权限（flight:read）</div>';
                }
                return;
            }
            if (!renderUnifiedState(list, 'error', '异常队列加载失败')) {
                list.innerHTML = '<div class="empty">异常队列加载失败</div>';
            }
            return;
        }

        const sorted = [...anomalyItems]
            .sort((left, right) => severityWeight(right.severity) - severityWeight(left.severity))
            .slice(0, 6);

        if (sorted.length === 0) {
            if (!renderUnifiedState(list, 'empty', '当前暂无未关闭异常')) {
                list.innerHTML = '<div class="empty">当前暂无未关闭异常</div>';
            }
            return;
        }

        list.innerHTML = sorted.map((item) => {
            const severity = String(item.severity || 'medium').toLowerCase();
            const tagClass = isHighSeverity(severity) ? 'critical' : (severity === 'medium' ? 'warn' : 'info');
            const detected = formatDateTime(item.detected_at);
            const title = item.title || item.anomaly_type || '异常告警';
            const target = describeAnomalyTarget(item);
            const reason = describeAnomalyReason(item);
            return `
                <div class="priority-item priority-${tagClass}">
                    <div class="priority-main">
                        <span class="priority-kicker">Priority Alert</span>
                        <div class="priority-title">${escapeHtml(title)}</div>
                        <div class="priority-context">${escapeHtml(target)} | ${escapeHtml(reason)} | ${escapeHtml(detected)}</div>
                    </div>
                    <div class="priority-side">
                        <span class="tag ${tagClass}">${escapeHtml(severityLabel(severity))}</span>
                        <span class="priority-caption">${escapeHtml(item.flight_id || '未绑定航班')}</span>
                    </div>
                </div>
            `;
        }).join('');
    }

    function updateLastRefreshDisplay(now) {
        setText('metricLastRefresh', formatTime(now));
        setText('metricAutoRefreshSub', state.autoRefresh ? `自动刷新 ${Math.round(state.refreshIntervalMs / 1000)} 秒` : '自动刷新已暂停');
    }

    function updateFooterWindow() {
        const footerWindowText = document.getElementById('footerWindowText');
        if (!footerWindowText || !state.dispatchWindowStart || !state.dispatchWindowEnd) {
            return;
        }
        footerWindowText.textContent = `窗口: ${formatDateTime(state.dispatchWindowStart)} - ${formatDateTime(state.dispatchWindowEnd)}`;
    }

    function connectFlightStream() {
        const token = Auth.getToken();
        if (!token) {
            setStreamChip('flightStreamChip', 'offline', '航班流 无令牌');
            return;
        }

        if (typeof Auth.refreshSSEToken === 'function') {
            Auth.refreshSSEToken();
        }

        setStreamChip('flightStreamChip', 'connecting', '航班流 连接中');
        state.suppressFlightReconnect = false;
        if (state.flightReconnectTimer) {
            window.clearTimeout(state.flightReconnectTimer);
            state.flightReconnectTimer = null;
        }
        
        // 全面转向 SSE
        connectFlightSSE(token);
    }

    function connectFlightSSE(_token) {
        SSEHub.on('flights', function (event) {
            var payload = parseJsonSafe(event.data);
            if (payload) { handleFlightStreamPayload(payload); }
        });
        SSEHub.on('flight_status_changes', function (event) {
            var payload = parseJsonSafe(event.data);
            if (payload) { handleFlightStreamPayload(payload); }
        });
        SSEHub.on('kpi_updated', function (event) {
            var payload = parseJsonSafe(event.data);
            if (payload) { handleFlightStreamPayload(payload); }
        });
        SSEHub.on('global_status', function (event) {
            var payload = parseJsonSafe(event.data);
            if (payload) { handleFlightStreamPayload(payload); }
        });

        // Heartbeat monitoring
        SSEHub.on('heartbeat', function () {
            setStreamChip('flightStreamChip', 'online', '航班流 在线');
        });

        SSEHub.onStatusChange(function (nextStatus) {
            if (nextStatus === 'online') {
                setStreamChip('flightStreamChip', 'online', '航班流 在线');
                appendEvent('stream', '航班实时流已连接', 'info', { stream: 'flight', transport: 'sse' });
            } else if (nextStatus === 'reconnecting' || nextStatus === 'offline') {
                setStreamChip('flightStreamChip', 'offline', '航班流 重连中');
            }
        });

        state.flightStream = { _sseHub: true };
    }



    function connectAnomalyStream() {
        const token = Auth.getToken();
        if (!token) {
            setStreamChip('anomalyStreamChip', 'offline', '异常流 无令牌');
            return;
        }

        if (typeof Auth.refreshSSEToken === 'function') {
            Auth.refreshSSEToken();
        }

        setStreamChip('anomalyStreamChip', 'connecting', '异常流 连接中');
        if (state.anomalyReconnectTimer) {
            window.clearTimeout(state.anomalyReconnectTimer);
            state.anomalyReconnectTimer = null;
        }
        
        // 全面转向 SSE
        connectAnomalySSE(token);
    }

    function connectAnomalySSE(_token) {
        SSEHub.on('anomaly_alerts', function (event) {
            var payload = parseJsonSafe(event.data);
            if (payload) { handleAnomalyStreamPayload(payload); }
        });

        SSEHub.onStatusChange(function (nextStatus) {
            if (nextStatus === 'online') {
                setStreamChip('anomalyStreamChip', 'online', '异常流 在线');
                appendEvent('stream', '异常实时流已连接', 'info', { stream: 'anomaly', transport: 'sse' });
            } else if (nextStatus === 'reconnecting' || nextStatus === 'offline') {
                setStreamChip('anomalyStreamChip', 'offline', '异常流 重连中');
            }
        });

        state.anomalyStream = { _sseHub: true };
    }



    function closeStreams() {
        state.suppressFlightReconnect = true;
        if (state.flightReconnectTimer) {
            window.clearTimeout(state.flightReconnectTimer);
            state.flightReconnectTimer = null;
        }
        if (state.anomalyReconnectTimer) {
            window.clearTimeout(state.anomalyReconnectTimer);
            state.anomalyReconnectTimer = null;
        }
        // Note: We do NOT call SSEHub.disconnect() here because other components
        // on the same page may still be using it. SSEHub lifecycle is global.
        state.flightStream = null;
        state.anomalyStream = null;
    }

    function handleFlightStreamPayload(payload) {
        if (!payload || typeof payload !== 'object') {
            return;
        }

        if (payload.type === 'heartbeat') {
            return;
        }

        if (payload.type === 'initial_data') {
            appendEvent('flight', `收到航班初始快照 (${Array.isArray(payload.flights) ? payload.flights.length : 0} 条)`, 'info');
            return;
        }

        const flightId = payload.flight_id || payload.flight_data?.flight_id || payload.flight?.flight_id;
        const eventType = payload.type || payload.event || 'flight_update';
        appendEvent('flight', `航班 ${flightId || '-'} 发生更新 (${eventType})`, 'info', payload);
        scheduleStreamTriggeredRefresh();
    }

    function handleAnomalyStreamPayload(payload) {
        if (!payload || typeof payload !== 'object') {
            return;
        }

        if (payload.type === 'heartbeat') {
            return;
        }

        if (payload.type === 'initial_data') {
            appendEvent('anomaly', `收到异常初始快照 (${Array.isArray(payload.items) ? payload.items.length : 0} 条)`, 'info');
            return;
        }

        const anomalyTitle = payload.title || payload.notification?.title || payload.anomaly?.title || '异常告警更新';
        const severity = String(payload.severity || payload.notification?.severity || payload.anomaly?.severity || 'medium').toLowerCase();
        appendEvent('anomaly', anomalyTitle, isHighSeverity(severity) ? 'critical' : 'warn', payload);
        scheduleStreamTriggeredRefresh();
    }

    function scheduleStreamTriggeredRefresh() {
        if (state.streamRefreshTimer) {
            return;
        }
        state.streamRefreshTimer = window.setTimeout(async () => {
            state.streamRefreshTimer = null;
            await refreshDashboard({ silent: true, reason: 'stream_event' });
        }, 1200);
    }

    function appendEvent(source, message, severity, context) {
        state.events.unshift({
            source: source || 'system',
            message: message || '-',
            severity: normalizeSeverityTag(severity),
            context: context || null,
            createdAt: new Date(),
        });
        if (state.events.length > MAX_EVENT_ITEMS) {
            state.events = state.events.slice(0, MAX_EVENT_ITEMS);
        }
        renderEventFeed();
    }

    function renderEventFeed() {
        const list = document.getElementById('eventFeedList');
        const meta = document.getElementById('eventFeedMeta');
        if (!list) {
            return;
        }

        if (meta) {
            meta.textContent = `事件数 ${state.events.length} / ${MAX_EVENT_ITEMS}`;
        }

        if (state.events.length === 0) {
            if (!renderUnifiedState(list, 'empty', '暂无事件')) {
                list.innerHTML = '<div class="empty">暂无事件</div>';
            }
            return;
        }

        list.innerHTML = state.events.slice(0, 8).map((event) => `
            <div class="timeline-item timeline-${escapeHtml(event.severity)}">
                <div class="timeline-main">
                    <div class="timeline-title">${escapeHtml(event.message)}</div>
                    <div class="timeline-context">
                        <span>${escapeHtml(event.source)}</span>
                        <span>${formatDateTime(event.createdAt)}</span>
                    </div>
                </div>
            </div>
        `).join('');
    }

    function startAutoRefresh() {
        stopAutoRefresh();
        if (!state.autoRefresh) {
            return;
        }
        state.refreshTimer = window.setInterval(() => {
            refreshDashboard({ silent: true, reason: 'auto_refresh' });
        }, Math.max(MIN_REFRESH_INTERVAL_MS, state.refreshIntervalMs));
    }

    function stopAutoRefresh() {
        if (state.refreshTimer) {
            window.clearInterval(state.refreshTimer);
            state.refreshTimer = null;
        }
    }

    function updateAutoRefreshText() {
        const toggleRefreshBtn = document.getElementById('toggleRefreshBtn');
        if (toggleRefreshBtn) {
            toggleRefreshBtn.textContent = state.autoRefresh ? '暂停自动刷新' : '恢复自动刷新';
        }
    }

    function updateClock() {
        const now = new Date();
        setText('nowClock', formatTime(now));
        setText('nowDate', now.toLocaleDateString('zh-CN', {
            year: 'numeric',
            month: '2-digit',
            day: '2-digit',
            weekday: 'short',
        }));
    }

    function setStreamChip(id, stateName, text) {
        const element = document.getElementById(id) || document.getElementById('unifiedStreamChip');
        if (!element) {
            return;
        }
        element.classList.remove('online', 'connecting', 'offline');
        element.classList.add(stateName);
        element.textContent = text;
    }

    function buildFlightStatusCounts(flights) {
        const counts = {};
        for (const flight of flights) {
            const status = normalizeStatus(flight.status);
            counts[status] = (counts[status] || 0) + 1;
        }
        return counts;
    }

    function isActiveFlight(flight) {
        const status = normalizeStatus(flight.status);
        return !['arrived', 'cancelled', 'completed'].includes(status);
    }

    function isDelayedFlight(flight) {
        const status = normalizeStatus(flight.status);
        if (status === 'delayed') {
            return true;
        }
        return calculateDelayMinutes(flight) >= 15;
    }

    function calculateDelayMinutes(flight) {
        const scheduled = pickDate(flight.scheduled_departure);
        const estimated = pickDate(flight.estimated_departure || flight.actual_departure);
        if (!scheduled || !estimated) {
            return 0;
        }
        return Math.max(0, Math.round((estimated.getTime() - scheduled.getTime()) / 60000));
    }

    function pickDepartureTime(flight) {
        return pickDate(flight.estimated_departure) || pickDate(flight.scheduled_departure);
    }

    function getLegPayload(flight, legType) {
        if (!flight || typeof flight !== 'object') {
            return null;
        }
        const key = legType === 'inbound' ? 'inbound_leg' : 'outbound_leg';
        const leg = flight[key];
        if (!leg || typeof leg !== 'object') {
            return null;
        }
        return leg;
    }

    function getLegField(flight, legType, fieldName) {
        const leg = getLegPayload(flight, legType);
        if (!leg) {
            return '';
        }
        return String(leg[fieldName] || '').trim();
    }

    function buildRouteText(flight) {
        const inboundOrigin = getStationListText(flight, 'inbound', 'origin_stations');
        const inboundDestination = getStationListText(flight, 'inbound', 'destination_stations');
        const outboundOrigin = getStationListText(flight, 'outbound', 'origin_stations');
        const outboundDestination = getStationListText(flight, 'outbound', 'destination_stations');
        const inboundNo = getLegField(flight, 'inbound', 'flight_no');
        const outboundNo = getLegField(flight, 'outbound', 'flight_no');
        const airportName = getAirportDisplayName();

        if (inboundNo && outboundNo) {
            return `${inboundOrigin || '-'} → ${airportName} → ${outboundDestination || '-'}`;
        }

        const origin = inboundOrigin || outboundOrigin || '-';
        const destination = outboundDestination || inboundDestination || '-';
        return `${origin} -> ${destination}`;
    }

    function getFlightNo(flight) {
        const inbound = getLegField(flight, 'inbound', 'flight_no');
        const outbound = getLegField(flight, 'outbound', 'flight_no');
        if (inbound && outbound) return `${inbound}|${outbound}`;
        if (inbound) return `${inbound}|-`;
        if (outbound) return `-|${outbound}`;
        return flight.flight_number || flight.flight_id || '-';
    }

    function normalizeStatus(status) {
        const normalized = String(status || 'unknown').trim().toLowerCase();
        return normalized || 'unknown';
    }

    function labelStatus(status) {
        return FLIGHT_STATUS_LABELS[status] || status.toUpperCase();
    }

    function buildFlightRiskScore(flight, delayMinutes, timeToDepartureMinutes) {
        const status = normalizeStatus(flight.status);
        let score = 0;
        if (status === 'delayed') {
            score += 32;
        } else if (status === 'boarding' || status === 'in_progress') {
            score += 14;
        }
        if (delayMinutes >= 45) {
            score += 40;
        } else if (delayMinutes >= 20) {
            score += 24;
        } else if (delayMinutes >= 10) {
            score += 12;
        }
        if (timeToDepartureMinutes <= 30) {
            score += 28;
        } else if (timeToDepartureMinutes <= 60) {
            score += 18;
        } else if (timeToDepartureMinutes <= 120) {
            score += 10;
        }
        return score;
    }

    function getFlightPriorityClass(delayMinutes, status, timeToDepartureMinutes) {
        if (delayMinutes >= 45 || (status === 'delayed' && timeToDepartureMinutes <= 45)) {
            return 'critical';
        }
        if (delayMinutes >= 15 || timeToDepartureMinutes <= 60 || status === 'delayed') {
            return 'warn';
        }
        return 'info';
    }

    function describeAnomalyTarget(item) {
        const context = item && typeof item.context_data === 'object' ? item.context_data : {};
        const candidates = [
            item.flight_id,
            context.flight_id,
            context.resource_value,
            context.stand,
            item.stand_id,
            item.stand,
            context.resource_name,
        ].map((value) => String(value || '').trim()).filter(Boolean);
        return candidates.length ? `影响对象 ${candidates[0]}` : '影响对象待确认';
    }

    function describeAnomalyReason(item) {
        const context = item && typeof item.context_data === 'object' ? item.context_data : {};
        const candidates = [
            context.reason,
            context.message,
            item.anomaly_type,
            item.description,
        ].map((value) => String(value || '').trim()).filter(Boolean);
        return candidates[0] || '需核对异常成因';
    }

    function severityLabel(severity) {
        const normalized = String(severity || '').toLowerCase();
        if (normalized === 'critical') {
            return '严重';
        }
        if (normalized === 'high') {
            return '高';
        }
        if (normalized === 'medium') {
            return '中';
        }
        if (normalized === 'low') {
            return '低';
        }
        return normalized.toUpperCase() || '告警';
    }

    function isHighSeverity(severity) {
        const normalized = String(severity || '').toLowerCase();
        return normalized === 'critical' || normalized === 'high';
    }

    function severityWeight(severity) {
        const normalized = String(severity || '').toLowerCase();
        if (normalized === 'critical') {
            return 4;
        }
        if (normalized === 'high') {
            return 3;
        }
        if (normalized === 'medium') {
            return 2;
        }
        if (normalized === 'low') {
            return 1;
        }
        return 0;
    }

    function normalizeSeverityTag(tag) {
        const normalized = String(tag || '').toLowerCase();
        if (normalized === 'critical') {
            return 'critical';
        }
        if (normalized === 'warn' || normalized === 'warning' || normalized === 'medium') {
            return 'warn';
        }
        return 'info';
    }

    async function safeApi(url, options = {}) {
        let response;
        try {
            response = await Auth.fetch(url, options);
        } catch (error) {
            return {
                ok: false,
                status: 0,
                data: null,
                error: error.message || 'network_error',
            };
        }

        let payload = null;
        try {
            payload = await response.json();
        } catch (_error) {
            payload = null;
        }

        if (!response.ok) {
            let message = `http_${response.status}`;
            if (payload && typeof payload.detail === 'string') {
                message = payload.detail;
            } else if (payload && payload.detail) {
                try {
                    message = JSON.stringify(payload.detail);
                } catch (_error) {
                    message = String(payload.detail);
                }
            } else if (payload && typeof payload.message === 'string') {
                message = payload.message;
            }
            return {
                ok: false,
                status: response.status,
                data: payload,
                error: message,
            };
        }

        return {
            ok: true,
            status: response.status,
            data: payload && Object.prototype.hasOwnProperty.call(payload, 'data') ? payload.data : payload,
            raw: payload,
        };
    }

    function pickDate(value) {
        if (!value) {
            return null;
        }
        const date = value instanceof Date ? value : new Date(value);
        if (Number.isNaN(date.getTime())) {
            return null;
        }
        return date;
    }

    function parseJsonSafe(text) {
        if (!text) {
            return null;
        }
        try {
            return JSON.parse(text);
        } catch (_error) {
            return null;
        }
    }

    function setText(id, value) {
        const element = document.getElementById(id);
        if (!element) {
            return;
        }
        element.textContent = value;
    }

    function formatTime(value) {
        const date = pickDate(value);
        if (!date) {
            return '--:--:--';
        }
        return date.toLocaleTimeString('zh-CN', {
            hour12: false,
            hour: '2-digit',
            minute: '2-digit',
            second: '2-digit',
        });
    }

    function formatDateTime(value) {
        const date = pickDate(value);
        if (!date) {
            return '--';
        }
        const month = String(date.getMonth() + 1).padStart(2, '0');
        const day = String(date.getDate()).padStart(2, '0');
        const hour = String(date.getHours()).padStart(2, '0');
        const minute = String(date.getMinutes()).padStart(2, '0');
        return `${month}-${day} ${hour}:${minute}`;
    }

    function toPercent(value) {
        const number = Number(value);
        if (!Number.isFinite(number)) {
            return '-';
        }
        return `${(number * 100).toFixed(1)}%`;
    }

    function escapeHtml(value) {
        return String(value ?? '')
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;')
            .replace(/'/g, '&#39;');
    }
})();
