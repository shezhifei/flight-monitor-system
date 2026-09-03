function parseFlightChatSSEEventBlock(block) {
    if (!block) {
        return null;
    }
    const lines = String(block).split(/\r?\n/);
    let eventName = 'message';
    const dataLines = [];
    lines.forEach((line) => {
        if (!line) {
            return;
        }
        if (line.startsWith('event:')) {
            eventName = line.slice(6).trim() || 'message';
            return;
        }
        if (line.startsWith('data:')) {
            dataLines.push(line.slice(5).trimStart());
        }
    });
    if (dataLines.length === 0) {
        return null;
    }
    return {
        event: eventName,
        data: dataLines.join('\n'),
    };
}

async function consumeFlightChatSSEStream(response, onEvent) {
    if (!response.body || typeof response.body.getReader !== 'function') {
        throw new Error('浏览器不支持流式读取');
    }
    const reader = response.body.getReader();
    const decoder = new TextDecoder('utf-8');
    let buffer = '';

    while (true) {
        const { value, done } = await reader.read();
        if (done) {
            break;
        }
        buffer += decoder.decode(value, { stream: true });
        buffer = buffer.replace(/\r\n/g, '\n');
        let boundaryIndex = buffer.indexOf('\n\n');
        while (boundaryIndex !== -1) {
            const rawBlock = buffer.slice(0, boundaryIndex);
            buffer = buffer.slice(boundaryIndex + 2);
            const parsedEvent = parseFlightChatSSEEventBlock(rawBlock);
            if (parsedEvent) {
                onEvent(parsedEvent.event, parsedEvent.data);
            }
            boundaryIndex = buffer.indexOf('\n\n');
        }
    }

    buffer += decoder.decode();
    buffer = buffer.replace(/\r\n/g, '\n');
    const tail = buffer.trim();
    if (tail) {
        const parsedEvent = parseFlightChatSSEEventBlock(tail);
        if (parsedEvent) {
            onEvent(parsedEvent.event, parsedEvent.data);
        }
    }
}

function beginNetworkActivity(message = '正在加载数据') {
    globalRequestCount += 1;

    if (globalRequestCount === 1) {
        if (loaderDelayTimer) {
            clearTimeout(loaderDelayTimer);
        }
        loaderDelayTimer = setTimeout(() => {
            setGlobalLoaderVisible(true);
            loaderWasShown = true;
            announce(message);
            loaderDelayTimer = null;
        }, UI_CONSTANTS.loaderDelayMs);
    }
}

function endNetworkActivity(message = '数据加载完成') {
    globalRequestCount = Math.max(0, globalRequestCount - 1);
    if (globalRequestCount > 0) {
        return;
    }

    if (loaderDelayTimer) {
        clearTimeout(loaderDelayTimer);
        loaderDelayTimer = null;
    }
    setGlobalLoaderVisible(false);
    if (loaderWasShown) {
        announce(message);
    }
    loaderWasShown = false;
}

function connectToFlightSSE() {
    // Flight data events — dispatched by SSEHub from the single connection
    var businessTopics = ['flights', 'flight_status_changes', 'anomaly_alerts', 'business_cases'];
    var flightEvents = ['flight_updated', 'flight_status_changed'];
    var businessCaseEvents = ['business_case.created', 'business_case.updated', 'business_case.deleted'];

    businessTopics.forEach(function (topic) {
        SSEHub.on(topic, function (event) {
            try {
                var data = JSON.parse(event.data);
                setConnectionStatus('online');
                if (topic === 'business_cases') {
                    handleBusinessCaseRealtimePayload(data);
                } else {
                    handleFlightRealtimePayload(data);
                }
            } catch (error) {
                console.error('SSE payload parse error for topic ' + topic + ':', error);
            }
        });
    });

    flightEvents.forEach(function (eventName) {
        SSEHub.on(eventName, function (event) {
            try {
                var data = JSON.parse(event.data);
                setConnectionStatus('online');
                handleFlightRealtimePayload(data);
            } catch (error) {
                console.error('SSE payload parse error for event ' + eventName + ':', error);
            }
        });
    });

    SSEHub.on('flight_labels_changed', function (event) {
        try {
            var data = JSON.parse(event.data);
            setConnectionStatus('online');
            handleFlightLabelsChanged(data);
        } catch (error) {
            console.error('SSE payload parse error for flight_labels_changed:', error);
        }
    });

    businessCaseEvents.forEach(function (eventName) {
        SSEHub.on(eventName, function (event) {
            try {
                var data = JSON.parse(event.data);
                setConnectionStatus('online');
                handleBusinessCaseRealtimePayload(data);
            } catch (error) {
                console.error('SSE payload parse error for event ' + eventName + ':', error);
            }
        });
    });

    // Also handle generic (unnamed) messages for backward compatibility
    SSEHub.on('message', function (event) {
        try {
            var data = JSON.parse(event.data);
            // Skip heartbeat / connected payloads
            if (data && (data.type === 'connected' || data.type === 'heartbeat')) {
                return;
            }
            setConnectionStatus('online');
            handleFlightRealtimePayload(data);
        } catch (error) {
            console.error('SSE payload parse error:', error);
        }
    });

    // Connection status tracking via Hub
    SSEHub.onStatusChange(function (newStatus) {
        if (newStatus === 'online') {
            setConnectionStatus('online');
        } else if (newStatus === 'reconnecting') {
            setConnectionStatus('reconnecting');
        }
    });
}

function connectToAnomalySSE(_token) {
    // Anomaly events are already subscribed via SSEHub's topic list.
    // The 'anomaly_alerts' listener was registered in connectToFlightSSE().
    // This function is now a no-op kept for call-site compatibility.
    console.log('[stream.js] connectToAnomalySSE: handled by SSEHub (no-op)');
}

async function fetchWithRetry(url, options = {}, retries = 2, retryDelayMs = 600) {
    let lastError = null;
    for (let attempt = 0; attempt <= retries; attempt += 1) {
        try {
            return await Auth.fetch(url, options);
        } catch (error) {
            lastError = error;
            if (attempt >= retries) break;
            await new Promise((resolve) => setTimeout(resolve, retryDelayMs * (attempt + 1)));
        }
    }
    throw lastError;
}

async function fetchFlightsPageData(page) {
    const url = `${API_BASE}/flights?page=${page}&page_size=${FLIGHT_LIST_PAGE_SIZE}`;
    return runWithRetry(async () => {
        // 移除 Protobuf 传输支持，全面回归 JSON
        const response = await fetchWithRetry(url);
        if (!response.ok) {
            throw new Error(`HTTP error! status: ${response.status}`);
        }
        const result = await response.json();
        return Array.isArray(result.data) ? result.data : [];
    });
}
