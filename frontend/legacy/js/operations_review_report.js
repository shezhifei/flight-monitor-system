(function () {
    'use strict';

    const KPI_METRIC_LABELS = {
        avg_turnaround_minutes: '平均过站(分钟)',
        p90_turnaround_minutes: 'P90 过站(分钟)',
        on_time_departure_rate: '出港准点率',
        on_time_arrival_rate: '到港准点率',
        service_node_compliance_rate: '服务节点达标率',
        abnormal_ratio: '异常航班占比',
    };

    const state = {
        events: [],
        replayIndex: -1,
        playbackSpeed: 1,
        playing: false,
        playTimer: null,
        windowStart: null,
        windowEnd: null,
        flightUpdates: [],
        anomalyItems: [],
        dispatchConflicts: [],
        replayLoadResult: {
            flight: null,
            anomaly: null,
            dispatch: null,
        },
        kpiCompare: null,
        trendOverlay: [],
        reportMarkdown: '',
        reportJson: null,
    };

    document.addEventListener('DOMContentLoaded', init);

    async function init() {
        const authenticated = await Auth.requireAuthAsync();
        if (!authenticated) {
            return;
        }

        bindControls();
        initDateDefaults();
        await refreshAll({ silent: true });
    }

    function bindControls() {
        const refreshAllBtn = document.getElementById('refreshAllBtn');
        if (refreshAllBtn) {
            refreshAllBtn.addEventListener('click', async () => {
                await refreshAll({ silent: false });
            });
        }

        const logoutBtn = document.getElementById('logoutBtn');
        if (logoutBtn) {
            logoutBtn.addEventListener('click', () => Auth.logout());
        }

        const lookbackHoursSelect = document.getElementById('lookbackHoursSelect');
        if (lookbackHoursSelect) {
            lookbackHoursSelect.addEventListener('change', async () => {
                await loadReplayData();
                renderReplay();
            });
        }

        const playbackSpeedSelect = document.getElementById('playbackSpeedSelect');
        if (playbackSpeedSelect) {
            playbackSpeedSelect.addEventListener('change', () => {
                const nextSpeed = Number(playbackSpeedSelect.value);
                state.playbackSpeed = Number.isFinite(nextSpeed) && nextSpeed > 0 ? nextSpeed : 1;
                if (state.playing) {
                    restartPlaybackTimer();
                }
            });
        }

        const reloadReplayBtn = document.getElementById('reloadReplayBtn');
        if (reloadReplayBtn) {
            reloadReplayBtn.addEventListener('click', async () => {
                await loadReplayData();
                renderReplay();
            });
        }

        const playBtn = document.getElementById('playBtn');
        if (playBtn) {
            playBtn.addEventListener('click', startPlayback);
        }

        const pauseBtn = document.getElementById('pauseBtn');
        if (pauseBtn) {
            pauseBtn.addEventListener('click', pausePlayback);
        }

        const stepBtn = document.getElementById('stepBtn');
        if (stepBtn) {
            stepBtn.addEventListener('click', () => {
                pausePlayback();
                stepReplay(1);
            });
        }

        const replaySlider = document.getElementById('replaySlider');
        if (replaySlider) {
            replaySlider.addEventListener('input', () => {
                const index = Number(replaySlider.value);
                if (Number.isFinite(index)) {
                    state.replayIndex = index;
                    pausePlayback();
                    renderReplay();
                }
            });
        }

        const replayEventList = document.getElementById('replayEventList');
        if (replayEventList) {
            replayEventList.addEventListener('click', (event) => {
                const row = event.target.closest('.row[data-index]');
                if (!row) {
                    return;
                }
                const index = Number(row.dataset.index);
                if (!Number.isFinite(index)) {
                    return;
                }
                state.replayIndex = index;
                pausePlayback();
                renderReplay();
            });
        }

        const loadKpiCompareBtn = document.getElementById('loadKpiCompareBtn');
        if (loadKpiCompareBtn) {
            loadKpiCompareBtn.addEventListener('click', async () => {
                await loadKpiAnalytics();
                renderKpiSection();
            });
        }

        const loadBaselineBtn = document.getElementById('loadBaselineBtn');
        if (loadBaselineBtn) {
            loadBaselineBtn.addEventListener('click', async () => {
                await loadBaselineCompare();
            });
        }

        const generateReportBtn = document.getElementById('generateReportBtn');
        if (generateReportBtn) {
            generateReportBtn.addEventListener('click', () => {
                generateReport();
                renderReportSection();
            });
        }

        const exportMarkdownBtn = document.getElementById('exportMarkdownBtn');
        if (exportMarkdownBtn) {
            exportMarkdownBtn.addEventListener('click', () => {
                if (!state.reportMarkdown) {
                    generateReport();
                }
                downloadFile(
                    `operations-review-${formatDateForFile(new Date())}.md`,
                    state.reportMarkdown,
                    'text/markdown;charset=utf-8'
                );
            });
        }

        const exportJsonBtn = document.getElementById('exportJsonBtn');
        if (exportJsonBtn) {
            exportJsonBtn.addEventListener('click', () => {
                if (!state.reportJson) {
                    generateReport();
                }
                downloadFile(
                    `operations-review-${formatDateForFile(new Date())}.json`,
                    JSON.stringify(state.reportJson, null, 2),
                    'application/json;charset=utf-8'
                );
            });
        }

        window.addEventListener('beforeunload', pausePlayback);
    }

    async function refreshAll(options = {}) {
        await Promise.all([
            loadReplayData(),
            loadKpiAnalytics(),
        ]);

        renderReplay();
        renderKpiSection();
        if (!options.silent) {
            generateReport();
            renderReportSection();
        }
    }

    function initDateDefaults() {
        const today = new Date();
        const end = formatDateInput(today);

        const baseStart = new Date(today);
        baseStart.setDate(baseStart.getDate() - 6);

        const compareEnd = new Date(baseStart);
        compareEnd.setDate(compareEnd.getDate() - 1);

        const compareStart = new Date(compareEnd);
        compareStart.setDate(compareStart.getDate() - 6);

        setInputValue('baseStartDate', formatDateInput(baseStart));
        setInputValue('baseEndDate', end);
        setInputValue('compareStartDate', formatDateInput(compareStart));
        setInputValue('compareEndDate', formatDateInput(compareEnd));
        setInputValue('baselineDate', end);
    }

    async function loadReplayData() {
        const lookbackHours = Number(getInputValue('lookbackHoursSelect') || 6);
        const now = new Date();
        const start = new Date(now.getTime() - Math.max(1, lookbackHours) * 60 * 60 * 1000);
        const minutes = Math.max(30, Math.min(1440, Math.round(lookbackHours * 60)));

        state.windowStart = start;
        state.windowEnd = now;

        const anomalyParams = new URLSearchParams({
            start_date: start.toISOString(),
            end_date: now.toISOString(),
            limit: '300',
            offset: '0',
        });
        const conflictParams = new URLSearchParams({
            window_start: start.toISOString(),
            window_end: now.toISOString(),
            limit: '200',
        });

        const [flightUpdatesResult, anomalyListResult, dispatchConflictsResult] = await Promise.all([
            safeApi(`/api/v2/flights/updates/recent?minutes=${minutes}&limit=500`),
            safeApi(`/api/v2/anomalies?${anomalyParams.toString()}`),
            safeApi(`/api/v2/dispatch-orders/conflicts?${conflictParams.toString()}`),
        ]);

        state.replayLoadResult = {
            flight: flightUpdatesResult,
            anomaly: anomalyListResult,
            dispatch: dispatchConflictsResult,
        };

        state.flightUpdates = flightUpdatesResult.ok && Array.isArray(flightUpdatesResult.data)
            ? flightUpdatesResult.data
            : [];
        state.anomalyItems = anomalyListResult.ok && anomalyListResult.data
            ? (Array.isArray(anomalyListResult.data.items) ? anomalyListResult.data.items : [])
            : [];
        state.dispatchConflicts = dispatchConflictsResult.ok && dispatchConflictsResult.data
            ? (Array.isArray(dispatchConflictsResult.data.conflicts) ? dispatchConflictsResult.data.conflicts : [])
            : [];

        state.events = buildReplayEvents(state.flightUpdates, state.anomalyItems, state.windowStart, state.windowEnd);
        state.replayIndex = state.events.length > 0 ? 0 : -1;
        updateReplaySlider();
    }

    async function loadKpiAnalytics() {
        const baseStartDate = getInputValue('baseStartDate');
        const baseEndDate = getInputValue('baseEndDate');
        const compareStartDate = getInputValue('compareStartDate');
        const compareEndDate = getInputValue('compareEndDate');

        if (!baseStartDate || !baseEndDate || !compareStartDate || !compareEndDate) {
            state.kpiCompare = null;
            state.trendOverlay = [];
            return;
        }

        const compareParams = new URLSearchParams({
            base_start_date: baseStartDate,
            base_end_date: baseEndDate,
            compare_start_date: compareStartDate,
            compare_end_date: compareEndDate,
        });

        const [kpiCompareResult, trendResult] = await Promise.all([
            safeApi(`/api/v2/kpi/compare?${compareParams.toString()}`),
            safeApi('/api/v2/kpi/trend-with-anomalies?metric=on_time_rate&days=14'),
        ]);

        state.kpiCompare = kpiCompareResult.ok ? (kpiCompareResult.data || null) : null;
        state.trendOverlay = trendResult.ok && trendResult.data
            ? (Array.isArray(trendResult.data.items) ? trendResult.data.items : [])
            : [];

        setText('kpiCompareMeta', state.kpiCompare
            ? `${baseStartDate} - ${baseEndDate} 对比 ${compareStartDate} - ${compareEndDate}`
            : 'KPI 对比数据不可用');
        setText('trendMeta', state.trendOverlay.length > 0
            ? `异常总数 ${Number(trendResult.data.anomaly_total || 0)}`
            : '暂无趋势数据');
    }

    function buildReplayEvents(flightUpdates, anomalies, windowStart, windowEnd) {
        const events = [];

        for (const item of flightUpdates || []) {
            const eventTime = pickDate(item.timestamp || item.created_at);
            if (!eventTime) {
                continue;
            }
            events.push({
                id: `flight-${item.id || Math.random().toString(16).slice(2)}`,
                timestamp: eventTime,
                kind: 'flight_update',
                severity: inferFlightUpdateSeverity(item),
                title: `航班 ${item.entity_id || '-'} ${formatOperation(item.action || item.operation)}`,
                subtitle: summarizeChanges(item.changes),
                flightId: item.entity_id || '',
                payload: item,
            });
        }

        for (const anomaly of anomalies || []) {
            const detectedAt = pickDate(anomaly.detected_at);
            if (detectedAt) {
                events.push({
                    id: `anomaly-open-${anomaly.anomaly_id}`,
                    timestamp: detectedAt,
                    kind: 'anomaly_open',
                    severity: inferAnomalySeverity(anomaly.severity),
                    title: anomaly.title || '异常告警',
                    subtitle: `${anomaly.flight_id || '-'} | ${anomaly.anomaly_type || '-'}`,
                    flightId: anomaly.flight_id || '',
                    payload: anomaly,
                });
            }

            const resolvedAt = pickDate(anomaly.resolved_at);
            if (resolvedAt && inWindow(resolvedAt, windowStart, windowEnd)) {
                events.push({
                    id: `anomaly-resolved-${anomaly.anomaly_id}`,
                    timestamp: resolvedAt,
                    kind: 'anomaly_resolved',
                    severity: 'info',
                    title: `异常已关闭: ${anomaly.title || anomaly.anomaly_id}`,
                    subtitle: `${anomaly.flight_id || '-'} | ${anomaly.anomaly_type || '-'}`,
                    flightId: anomaly.flight_id || '',
                    payload: anomaly,
                });
            }
        }

        events.sort((left, right) => left.timestamp.getTime() - right.timestamp.getTime());
        return events;
    }

    function renderReplay() {
        renderReplayMeta();
        renderReplaySummary();
        renderReplayList();
        renderCurrentEvent();
        updateReplaySlider();
    }

    function renderReplayMeta() {
        setText(
            'replayMetaText',
            `时间窗: ${formatDateTime(state.windowStart)} - ${formatDateTime(state.windowEnd)}`
        );
    }

    function renderReplaySummary() {
        const flightEventCount = state.events.filter((event) => event.kind === 'flight_update').length;
        const anomalyEventCount = state.events.filter((event) => event.kind.startsWith('anomaly_')).length;
        const conflictCount = Array.isArray(state.dispatchConflicts) ? state.dispatchConflicts.length : 0;

        setText('summaryEventTotal', String(state.events.length));
        setText('summaryFlightEvents', String(flightEventCount));
        setText('summaryAnomalyEvents', String(anomalyEventCount));
        setText('summaryDispatchConflicts', String(conflictCount));
    }

    function renderReplayList() {
        const list = document.getElementById('replayEventList');
        if (!list) {
            return;
        }

        if (!state.events || state.events.length === 0) {
            list.innerHTML = '<div class="empty">当前窗口暂无可回放事件</div>';
            return;
        }

        list.innerHTML = state.events.map((event, index) => {
            const activeClass = index === state.replayIndex ? ' active' : '';
            return `
                <div class="row${activeClass}" data-index="${index}">
                    <div class="row-head">
                        <span class="row-title">${escapeHtml(event.title)}</span>
                        <span class="chip ${escapeHtml(event.severity)}">${escapeHtml(event.severity.toUpperCase())}</span>
                    </div>
                    <div class="row-sub">${escapeHtml(formatDateTime(event.timestamp))} | ${escapeHtml(event.subtitle || '-')}</div>
                </div>
            `;
        }).join('');
    }

    function renderCurrentEvent() {
        if (state.replayIndex < 0 || state.replayIndex >= state.events.length) {
            setText('eventDetailTitle', '当前事件');
            setText('eventDetailSub', '暂无事件');
            setText('eventDetailJson', '{}');
            setEventDetailTag('info');
            return;
        }

        const event = state.events[state.replayIndex];
        setText('eventDetailTitle', event.title);
        setText('eventDetailSub', `${formatDateTime(event.timestamp)} | ${event.kind} | ${event.subtitle || '-'}`);
        setText('eventDetailJson', JSON.stringify(event.payload || {}, null, 2));
        setEventDetailTag(event.severity);
    }

    function setEventDetailTag(severity) {
        const tag = document.getElementById('eventDetailTag');
        if (!tag) {
            return;
        }
        tag.classList.remove('info', 'warn', 'critical');
        const normalized = normalizeSeverityTag(severity);
        tag.classList.add(normalized);
        tag.textContent = normalized.toUpperCase();
    }

    function updateReplaySlider() {
        const slider = document.getElementById('replaySlider');
        if (!slider) {
            return;
        }

        const max = Math.max(0, state.events.length - 1);
        slider.max = String(max);
        slider.value = String(Math.max(0, Math.min(max, state.replayIndex)));
    }

    function startPlayback() {
        if (!state.events || state.events.length === 0) {
            return;
        }

        if (state.replayIndex < 0) {
            state.replayIndex = 0;
        }

        state.playing = true;
        restartPlaybackTimer();
    }

    function pausePlayback() {
        state.playing = false;
        if (state.playTimer) {
            window.clearInterval(state.playTimer);
            state.playTimer = null;
        }
    }

    function restartPlaybackTimer() {
        pausePlayback();
        state.playing = true;
        const intervalMs = Math.max(120, Math.round(900 / Math.max(1, state.playbackSpeed)));
        state.playTimer = window.setInterval(() => {
            const moved = stepReplay(1);
            if (!moved) {
                pausePlayback();
            }
        }, intervalMs);
    }

    function stepReplay(delta) {
        if (!state.events || state.events.length === 0) {
            return false;
        }

        const nextIndex = state.replayIndex + delta;
        if (nextIndex < 0 || nextIndex >= state.events.length) {
            return false;
        }

        state.replayIndex = nextIndex;
        renderReplay();
        return true;
    }

    function renderKpiSection() {
        renderKpiCompareTable();
        renderTrendOverlay();
    }

    function renderKpiCompareTable() {
        const tableBody = document.getElementById('kpiCompareRows');
        if (!tableBody) {
            return;
        }

        if (!state.kpiCompare || !state.kpiCompare.metrics) {
            tableBody.innerHTML = '<tr><td colspan="4" class="empty">KPI 对比数据不可用</td></tr>';
            return;
        }

        const rows = Object.entries(state.kpiCompare.metrics);
        if (rows.length === 0) {
            tableBody.innerHTML = '<tr><td colspan="4" class="empty">暂无 KPI 指标</td></tr>';
            return;
        }

        tableBody.innerHTML = rows.map(([key, payload]) => {
            const base = Number(payload.base || 0);
            const compare = Number(payload.compare || 0);
            const delta = Number(payload.delta || 0);
            const changeRate = payload.change_rate;
            const isRate = key.includes('rate') || key.includes('ratio');
            const deltaClass = delta > 0 ? 'info' : (delta < 0 ? 'critical' : 'warn');
            const deltaText = isRate
                ? `${(delta * 100).toFixed(2)}%`
                : delta.toFixed(2);
            const changeText = changeRate === null || changeRate === undefined
                ? '-'
                : `${(Number(changeRate) * 100).toFixed(1)}%`;
            return `
                <tr>
                    <td>${escapeHtml(KPI_METRIC_LABELS[key] || key)}</td>
                    <td>${escapeHtml(formatMetricValue(base, isRate))}</td>
                    <td>${escapeHtml(formatMetricValue(compare, isRate))}</td>
                    <td><span class="chip ${deltaClass}">${escapeHtml(deltaText)} (${escapeHtml(changeText)})</span></td>
                </tr>
            `;
        }).join('');
    }

    function renderTrendOverlay() {
        const container = document.getElementById('trendOverlayList');
        if (!container) {
            return;
        }

        if (!Array.isArray(state.trendOverlay) || state.trendOverlay.length === 0) {
            container.innerHTML = '<div class="empty">暂无趋势叠加数据</div>';
            return;
        }

        const maxValue = Math.max(0.0001, ...state.trendOverlay.map((item) => Number(item.value || 0)));
        container.innerHTML = state.trendOverlay.map((item) => {
            const value = Number(item.value || 0);
            const anomalyCount = Number(item.anomaly_count || 0);
            const normalized = Math.max(0, Math.min(1, value / maxValue));
            const widthPercent = value <= 0 ? 0 : Math.max(2, Math.round(normalized * 100));
            return `
                <div class="trend-row">
                    <span>${escapeHtml(String(item.date || '-'))}</span>
                    <span class="track"><span class="fill" style="width:${widthPercent}%"></span></span>
                    <span>${escapeHtml((value * 100).toFixed(1))}%</span>
                    <span>${escapeHtml(String(anomalyCount))} 条</span>
                </div>
            `;
        }).join('');
    }

    function generateReport() {
        const summary = buildReportSummary();
        const markdown = buildReportMarkdown(summary);
        const payload = {
            generated_at: new Date().toISOString(),
            replay_window: {
                start: state.windowStart ? state.windowStart.toISOString() : null,
                end: state.windowEnd ? state.windowEnd.toISOString() : null,
            },
            summary,
            kpi_compare: state.kpiCompare,
            trend_overlay: state.trendOverlay,
            top_events: summary.topEvents,
            dispatch_conflicts: state.dispatchConflicts,
        };

        state.reportMarkdown = markdown;
        state.reportJson = payload;
    }

    function renderReportSection() {
        setText('reportOutput', state.reportMarkdown || '点击“生成复盘报告”开始输出。');
        setText('reportMeta', state.reportMarkdown ? `已生成 ${formatDateTime(new Date())}` : '尚未生成');
    }

    function buildReportSummary() {
        const flightEvents = state.events.filter((event) => event.kind === 'flight_update');
        const anomalyEvents = state.events.filter((event) => event.kind.startsWith('anomaly_'));
        const criticalEvents = state.events.filter((event) => event.severity === 'critical');

        const affectedFlights = new Set();
        for (const event of state.events) {
            if (event.flightId) {
                affectedFlights.add(event.flightId);
            }
        }

        const topEvents = [...state.events]
            .sort((left, right) => {
                const weightGap = severityWeight(right.severity) - severityWeight(left.severity);
                if (weightGap !== 0) {
                    return weightGap;
                }
                return right.timestamp.getTime() - left.timestamp.getTime();
            })
            .slice(0, 8)
            .map((event) => ({
                time: event.timestamp.toISOString(),
                severity: event.severity,
                title: event.title,
                subtitle: event.subtitle,
                kind: event.kind,
            }));

        return {
            totalEvents: state.events.length,
            flightEventCount: flightEvents.length,
            anomalyEventCount: anomalyEvents.length,
            criticalEventCount: criticalEvents.length,
            dispatchConflictCount: Array.isArray(state.dispatchConflicts) ? state.dispatchConflicts.length : 0,
            affectedFlightCount: affectedFlights.size,
            topEvents,
        };
    }

    function buildReportMarkdown(summary) {
        const lines = [];
        lines.push('# 运行复盘报告');
        lines.push('');
        lines.push(`- 生成时间: ${formatDateTime(new Date())}`);
        lines.push(`- 回放窗口: ${formatDateTime(state.windowStart)} - ${formatDateTime(state.windowEnd)}`);
        lines.push('');
        lines.push('## 1. 事件概览');
        lines.push(`- 事件总数: ${summary.totalEvents}`);
        lines.push(`- 航班更新事件: ${summary.flightEventCount}`);
        lines.push(`- 异常事件: ${summary.anomalyEventCount}`);
        lines.push(`- 高优先级事件: ${summary.criticalEventCount}`);
        lines.push(`- 影响航班数: ${summary.affectedFlightCount}`);
        lines.push(`- 调度冲突数: ${summary.dispatchConflictCount}`);
        lines.push('');

        lines.push('## 2. 关键事件（Top 8）');
        if (!summary.topEvents || summary.topEvents.length === 0) {
            lines.push('- 无关键事件');
        } else {
            summary.topEvents.forEach((event, index) => {
                lines.push(`${index + 1}. [${formatDateTime(event.time)}] [${event.severity.toUpperCase()}] ${event.title} (${event.kind})`);
                if (event.subtitle) {
                    lines.push(`   - ${event.subtitle}`);
                }
            });
        }
        lines.push('');

        lines.push('## 3. KPI 区间对比');
        if (!state.kpiCompare || !state.kpiCompare.metrics) {
            lines.push('- KPI 对比数据不可用');
        } else {
            lines.push('| 指标 | 基线 | 对比 | 变化 |');
            lines.push('| --- | ---: | ---: | ---: |');
            Object.entries(state.kpiCompare.metrics).forEach(([key, payload]) => {
                const isRate = key.includes('rate') || key.includes('ratio');
                const base = formatMetricValue(Number(payload.base || 0), isRate);
                const compare = formatMetricValue(Number(payload.compare || 0), isRate);
                const delta = isRate
                    ? `${(Number(payload.delta || 0) * 100).toFixed(2)}%`
                    : Number(payload.delta || 0).toFixed(2);
                lines.push(`| ${KPI_METRIC_LABELS[key] || key} | ${base} | ${compare} | ${delta} |`);
            });
        }
        lines.push('');

        lines.push('## 4. 调度冲突摘要');
        if (!Array.isArray(state.dispatchConflicts) || state.dispatchConflicts.length === 0) {
            lines.push('- 当前窗口未发现调度冲突');
        } else {
            state.dispatchConflicts.slice(0, 10).forEach((conflict, index) => {
                const severity = String(conflict.severity || 'medium').toUpperCase();
                const conflictType = String(conflict.conflict_type || 'unknown');
                const message = String(conflict.message || '-');
                lines.push(`${index + 1}. [${severity}] ${conflictType}: ${message}`);
            });
        }
        lines.push('');

        lines.push('## 5. 建议');
        lines.push('- 对高优先级异常建立值班闭环，优先处置 critical/high。');
        lines.push('- 对冲突高发时段执行预排班复核，减少同资源并发重叠。');
        lines.push('- 持续跟踪准点率与异常叠加曲线，验证策略调整效果。');

        return lines.join('\n');
    }

    function inWindow(date, start, end) {
        if (!date || !start || !end) {
            return false;
        }
        const ms = date.getTime();
        return ms >= start.getTime() && ms <= end.getTime();
    }

    function inferFlightUpdateSeverity(item) {
        const operation = String(item.action || item.operation || '').toLowerCase();
        if (operation.includes('cancel') || operation.includes('delay')) {
            return 'critical';
        }
        const text = JSON.stringify(item.changes || {});
        if (text.includes('delayed') || text.includes('cancelled')) {
            return 'critical';
        }
        if (operation.includes('status') || operation.includes('gate') || operation.includes('stand')) {
            return 'warn';
        }
        return 'info';
    }

    function inferAnomalySeverity(severity) {
        const normalized = String(severity || '').toLowerCase();
        if (normalized === 'critical' || normalized === 'high') {
            return 'critical';
        }
        if (normalized === 'medium') {
            return 'warn';
        }
        return 'info';
    }

    function formatOperation(operation) {
        const normalized = String(operation || '').toLowerCase();
        if (!normalized) {
            return '更新';
        }
        if (normalized.includes('create')) {
            return '创建';
        }
        if (normalized.includes('update')) {
            return '更新';
        }
        if (normalized.includes('status')) {
            return '状态变更';
        }
        if (normalized.includes('cancel')) {
            return '取消';
        }
        return normalized;
    }

    function summarizeChanges(changes) {
        if (!changes || typeof changes !== 'object') {
            return '无字段差异';
        }
        const keys = Object.keys(changes);
        if (keys.length === 0) {
            return '无字段差异';
        }
        return `变更字段: ${keys.slice(0, 4).join(', ')}${keys.length > 4 ? ` +${keys.length - 4}` : ''}`;
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
            return {
                ok: false,
                status: response.status,
                data: payload,
                error: extractError(payload, response.status),
            };
        }

        return {
            ok: true,
            status: response.status,
            data: payload && Object.prototype.hasOwnProperty.call(payload, 'data') ? payload.data : payload,
            raw: payload,
        };
    }

    function extractError(payload, status) {
        if (payload && typeof payload.detail === 'string') {
            return payload.detail;
        }
        if (payload && payload.detail) {
            try {
                return JSON.stringify(payload.detail);
            } catch (_error) {
                return String(payload.detail);
            }
        }
        if (payload && typeof payload.message === 'string') {
            return payload.message;
        }
        return `http_${status}`;
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

    function severityWeight(severity) {
        const normalized = normalizeSeverityTag(severity);
        if (normalized === 'critical') {
            return 3;
        }
        if (normalized === 'warn') {
            return 2;
        }
        return 1;
    }

    function formatMetricValue(value, isRate) {
        if (!Number.isFinite(value)) {
            return '-';
        }
        if (isRate) {
            return `${(value * 100).toFixed(2)}%`;
        }
        return value.toFixed(2);
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

    function formatDateInput(date) {
        const year = date.getFullYear();
        const month = String(date.getMonth() + 1).padStart(2, '0');
        const day = String(date.getDate()).padStart(2, '0');
        return `${year}-${month}-${day}`;
    }

    function formatDateForFile(date) {
        const year = date.getFullYear();
        const month = String(date.getMonth() + 1).padStart(2, '0');
        const day = String(date.getDate()).padStart(2, '0');
        const hour = String(date.getHours()).padStart(2, '0');
        const minute = String(date.getMinutes()).padStart(2, '0');
        return `${year}${month}${day}-${hour}${minute}`;
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
        const second = String(date.getSeconds()).padStart(2, '0');
        return `${month}-${day} ${hour}:${minute}:${second}`;
    }

    function setText(id, value) {
        const element = document.getElementById(id);
        if (!element) {
            return;
        }
        element.textContent = value;
    }

    function getInputValue(id) {
        const element = document.getElementById(id);
        return element ? element.value : '';
    }

    function setInputValue(id, value) {
        const element = document.getElementById(id);
        if (element) {
            element.value = value;
        }
    }

    function escapeHtml(value) {
        return String(value ?? '')
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;')
            .replace(/'/g, '&#39;');
    }

    function downloadFile(filename, content, contentType) {
        const blob = new Blob([content], { type: contentType });
        const url = URL.createObjectURL(blob);
        const anchor = document.createElement('a');
        anchor.href = url;
        anchor.download = filename;
        document.body.appendChild(anchor);
        anchor.click();
        anchor.remove();
        URL.revokeObjectURL(url);
    }

    /* ========== EP-06: Baseline Compare ========== */

    let baselineChartInstance = null;

    async function loadBaselineCompare() {
        const targetDate = getInputValue('baselineDate');
        const weather = getInputValue('weatherCategory') || 'normal';
        if (!targetDate) return;

        setText('baselineCompareMeta', '加载中…');
        const result = await safeApi(`/api/v2/kpi/baseline-compare?date=${targetDate}&weather=${weather}`);
        if (!result.ok || !result.data || !Array.isArray(result.data.items)) {
            setText('baselineCompareMeta', '基线数据不可用');
            return;
        }
        renderBaselineChart(result.data);
    }

    function renderBaselineChart(data) {
        const chartDom = document.getElementById('baselineChart');
        if (!chartDom) return;

        if (!baselineChartInstance) {
            baselineChartInstance = echarts.init(chartDom);
            window.addEventListener('resize', () => baselineChartInstance.resize());
        }

        const hours = data.items.map(i => i.hour);
        const actualVol = data.items.map(i => i.actual_volume);
        const baselineVol = data.items.map(i => i.baseline_volume);
        const actualRate = data.items.map(i => Math.round((i.actual_on_time_rate || 0) * 100));
        const baselineRate = data.items.map(i => Math.round((i.baseline_on_time_rate || 0) * 100));

        const abnormalHours = data.items.filter(i => i.is_abnormal).map(i => i.hour);

        const option = {
            tooltip: {
                trigger: 'axis',
                axisPointer: { type: 'cross' },
            },
            legend: {
                data: ['实际航班量', '基线航班量', '实际准点率', '基线准点率'],
                bottom: 0,
                textStyle: { fontSize: 11, color: '#5f7082' },
            },
            grid: { left: 50, right: 50, top: 20, bottom: 40 },
            xAxis: {
                type: 'category',
                data: hours,
                axisLabel: { fontSize: 10, color: '#5f7082' },
            },
            yAxis: [
                {
                    type: 'value',
                    name: '航班量',
                    nameTextStyle: { fontSize: 10, color: '#8a97a8' },
                    axisLabel: { fontSize: 10, color: '#8a97a8' },
                    splitLine: { lineStyle: { type: 'dashed', color: 'rgba(0,0,0,0.06)' } },
                },
                {
                    type: 'value',
                    name: '准点率 %',
                    min: 0,
                    max: 100,
                    nameTextStyle: { fontSize: 10, color: '#8a97a8' },
                    axisLabel: { fontSize: 10, color: '#8a97a8', formatter: '{value}%' },
                    splitLine: { show: false },
                },
            ],
            series: [
                {
                    name: '实际航班量',
                    type: 'bar',
                    data: actualVol,
                    itemStyle: { color: 'rgba(0,122,255,0.65)', borderRadius: [3, 3, 0, 0] },
                    barGap: '10%',
                },
                {
                    name: '基线航班量',
                    type: 'bar',
                    data: baselineVol,
                    itemStyle: { color: 'rgba(142,142,147,0.35)', borderRadius: [3, 3, 0, 0] },
                },
                {
                    name: '实际准点率',
                    type: 'line',
                    yAxisIndex: 1,
                    data: actualRate,
                    smooth: true,
                    symbol: 'circle',
                    symbolSize: 5,
                    lineStyle: { width: 2, color: '#34C759' },
                    itemStyle: { color: '#34C759' },
                },
                {
                    name: '基线准点率',
                    type: 'line',
                    yAxisIndex: 1,
                    data: baselineRate,
                    smooth: true,
                    symbol: 'diamond',
                    symbolSize: 5,
                    lineStyle: { width: 2, type: 'dashed', color: '#FF9500' },
                    itemStyle: { color: '#FF9500' },
                },
            ],
        };

        baselineChartInstance.setOption(option, true);

        // Update meta and alert banner
        const weatherLabels = { normal: '晴好', rain: '雨天', storm: '暴风雨', snow: '雪天' };
        setText('baselineCompareMeta', `${data.target_date} | 天气: ${weatherLabels[data.weather_category] || data.weather_category}`);

        const alertsEl = document.getElementById('baselineAlerts');
        if (alertsEl) {
            if (abnormalHours.length > 0) {
                alertsEl.style.display = 'block';
                alertsEl.textContent = `⚠️ 偏离基线时段: ${abnormalHours.join(', ')} — 实际准点率显著低于基线阈值`;
            } else {
                alertsEl.style.display = 'none';
            }
        }
    }
})();
