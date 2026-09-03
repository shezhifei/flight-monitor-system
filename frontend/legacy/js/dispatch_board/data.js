(function (window) {
    'use strict';

    function apiCall(url, options) {
        const requestOptions = options || {};
        const headers = {
            ...(requestOptions.headers || {})
        };

        if (!(requestOptions.body instanceof FormData)) {
            headers['Content-Type'] = 'application/json';
        }

        if (!window.Auth || typeof window.Auth.fetch !== 'function') {
            return Promise.reject(new Error('认证请求能力不可用'));
        }

        return window.Auth.fetch(url, {
            ...requestOptions,
            headers,
        }).then(async (response) => {
            if (!response.ok) {
                let message = `请求失败 (${response.status})`;
                let detailPayload = null;
                try {
                    const payload = await response.json();
                    if (typeof payload?.detail === 'string') {
                        message = payload.detail;
                    } else if (payload?.detail) {
                        detailPayload = payload.detail;
                        if (typeof payload.detail.message === 'string') {
                            message = payload.detail.message;
                        } else {
                            message = JSON.stringify(payload.detail);
                        }
                    } else if (typeof payload?.message === 'string') {
                        message = payload.message;
                    }
                } catch (_error) {
                    // ignore parse error
                }
                const requestError = new Error(message);
                requestError.status = response.status;
                if (detailPayload !== null) {
                    requestError.detailPayload = detailPayload;
                }
                throw requestError;
            }

            if (response.status === 204) {
                return null;
            }

            return response.json();
        });
    }

    function unwrapApiData(payload) {
        return payload?.data !== undefined ? payload.data : payload;
    }

    function normalizeTerminalList(rawTerminals) {
        const seen = new Set();
        const terminals = [];

        for (const terminal of rawTerminals || []) {
            const value = String(terminal || '').trim();
            if (!value) {
                continue;
            }
            const key = value.toLowerCase();
            if (seen.has(key)) {
                continue;
            }
            seen.add(key);
            terminals.push(value);
        }

        terminals.sort((a, b) => a.localeCompare(b, 'zh-CN', { numeric: true, sensitivity: 'base' }));
        return terminals;
    }

    function collectTerminalsFromTimeline(timeline) {
        if (!timeline || !Array.isArray(timeline.items)) {
            return [];
        }

        return timeline.items
            .map((item) => String(item?.terminal || '').trim())
            .filter(Boolean);
    }

    async function loadConfiguredTerminals(options) {
        const settings = options || {};
        let fetchedTerminals = [];

        try {
            const stands = unwrapApiData(await apiCall('/api/v2/stands?include_inactive=false'));
            if (Array.isArray(stands)) {
                fetchedTerminals = stands
                    .map((stand) => String(stand?.terminal || '').trim())
                    .filter(Boolean);
            }
        } catch (error) {
            if (typeof settings.onFetchError === 'function') {
                settings.onFetchError(error);
            }
        }

        if (fetchedTerminals.length === 0) {
            fetchedTerminals = collectTerminalsFromTimeline(settings.timeline);
        }

        return normalizeTerminalList(fetchedTerminals);
    }

    async function fetchTimeline(options) {
        const settings = options || {};
        const params = new URLSearchParams();
        params.set('view_mode', settings.viewMode || 'flight');
        params.set('window_start', new Date(settings.windowStartMs).toISOString());
        params.set('window_end', new Date(settings.windowEndMs).toISOString());
        if (settings.terminal && settings.terminal !== 'all') {
            params.set('terminal', settings.terminal);
        }

        const payload = unwrapApiData(await apiCall(`/api/v2/dispatch-orders/timeline?${params.toString()}`));
        return payload && typeof payload === 'object' ? payload : null;
    }

    async function fetchTimelineSafetyProgress(timeline) {
        const requestItems = [];
        const uniqueByOrder = new Map();
        const timelineItems = Array.isArray(timeline?.items) ? timeline.items : [];

        for (const item of timelineItems) {
            if (!item || item.is_flight_summary) {
                continue;
            }

            const orderId = String(item.order_id || '').trim();
            const stepCode = String(item.task_type || '').trim();
            if (!orderId || !stepCode) {
                continue;
            }

            if (!uniqueByOrder.has(orderId)) {
                uniqueByOrder.set(orderId, stepCode);
            }
        }

        uniqueByOrder.forEach((stepCode, orderId) => {
            requestItems.push({
                dispatch_order_id: orderId,
                task_type: stepCode
            });
        });

        if (requestItems.length === 0) {
            return {};
        }

        const payload = unwrapApiData(await apiCall('/api/v2/dispatch-orders/safety-checklist/progress', {
            method: 'POST',
            body: JSON.stringify({ orders: requestItems })
        }));

        const items = Array.isArray(payload?.items) ? payload.items : [];
        const nextMap = {};
        for (const item of items) {
            const orderId = String(item?.dispatch_order_id || '').trim();
            if (!orderId) {
                continue;
            }
            nextMap[orderId] = {
                dispatch_order_id: orderId,
                task_type: String(item.task_type || '').trim(),
                enforced: Boolean(item.enforced),
                ready: Boolean(item.ready),
                required_total: Number(item.required_total || 0),
                completed_required: Number(item.completed_required || 0),
                pending_required_count: Number(item.pending_required_count || 0),
                failed_required_count: Number(item.failed_required_count || 0),
                template_version: item.template_version || null
            };
        }

        return nextMap;
    }

    async function fetchAnalytics(options) {
        const settings = options || {};
        const params = new URLSearchParams();
        params.set('window_start', new Date(settings.windowStartMs).toISOString());
        params.set('window_end', new Date(settings.windowEndMs).toISOString());

        const [summaryPayload, breakdownPayload, trendPayload] = await Promise.all([
            apiCall(`/api/v2/dispatch/analytics/summary?${params.toString()}`),
            apiCall(`/api/v2/dispatch/analytics/breakdown?group_by=team&${params.toString()}`),
            apiCall(`/api/v2/dispatch/analytics/trend?bucket=hour&${params.toString()}`)
        ]);

        const summary = unwrapApiData(summaryPayload);
        const breakdown = unwrapApiData(breakdownPayload);
        const trend = unwrapApiData(trendPayload);
        return {
            summary: summary && typeof summary === 'object' ? summary : null,
            breakdown: Array.isArray(breakdown) ? breakdown : [],
            trend: Array.isArray(trend) ? trend : []
        };
    }

    function fetchOrder(orderId) {
        return apiCall(`/api/v2/dispatch-orders/${encodeURIComponent(orderId)}`).then(unwrapApiData);
    }

    async function fetchOrdersByFlight(flightId) {
        const payload = unwrapApiData(await apiCall(`/api/v2/dispatch-orders?flight_id=${encodeURIComponent(flightId)}&page=1&page_size=100`));
        return Array.isArray(payload) ? payload : [];
    }

    function fetchOrderSafetyChecklist(orderId) {
        return apiCall(`/api/v2/dispatch-orders/${encodeURIComponent(orderId)}/safety-checklist`).then(unwrapApiData);
    }

    window.DispatchBoardData = {
        apiCall,
        unwrapApiData,
        collectTerminalsFromTimeline,
        normalizeTerminalList,
        loadConfiguredTerminals,
        fetchTimeline,
        fetchTimelineSafetyProgress,
        fetchAnalytics,
        fetchOrder,
        fetchOrdersByFlight,
        fetchOrderSafetyChecklist
    };
}(window));
