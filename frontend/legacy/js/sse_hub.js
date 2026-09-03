/**
 * SSEHub — Global singleton for SSE connection management.
 *
 * Maintains exactly ONE EventSource connection per browser tab.
 * All business modules register named-event listeners via on()/off()
 * instead of creating their own connections.
 *
 * Usage:
 *   SSEHub.connect();                          // call once at page init
 *   SSEHub.on('flights', handleFlightUpdate);  // register listener
 *   SSEHub.off('flights', handleFlightUpdate); // unregister listener
 *   SSEHub.onStatusChange(cb);                 // monitor connection health
 */
const SSEHub = (function () {
    'use strict';

    // ── Configuration ────────────────────────────────────────────────
    const DEFAULT_TOPICS = [
        'flights',
        'flight_status_changes',
        'anomaly_alerts',
        'ai_execution',
        'kpi_updated',
        'global_status',
        'system_alerts',
        'error_events',
        'smart_monitor',
        'business_cases',
    ];

    const RECONNECT_BASE_MS = 2000;
    const RECONNECT_MAX_MS = 30000;
    const CLIENT_SCOPE = 'sse_hub_global';

    // ── Internal state ───────────────────────────────────────────────
    let source = null;
    let reconnectTimer = null;
    let reconnectAttempt = 0;
    let status = 'offline'; // 'offline' | 'connecting' | 'online' | 'reconnecting'
    let currentTopics = [];
    let intentionalClose = false;
    let pageSuspended = false;
    let lifecycleListenersBound = false;
    let connectSequence = 0;

    // eventName -> Set<Function>
    const listeners = new Map();
    // Set<Function> for status change callbacks
    const statusListeners = new Set();

    // ── Status management ────────────────────────────────────────────
    function setStatus(nextStatus) {
        if (status === nextStatus) {
            return;
        }
        const previous = status;
        status = nextStatus;
        console.log('[SSEHub] Status:', previous, '->', nextStatus);
        statusListeners.forEach(function (cb) {
            try {
                cb(nextStatus, previous);
            } catch (error) {
                console.error('[SSEHub] Status listener error:', error);
            }
        });
    }

    // ── Listener management ──────────────────────────────────────────
    function addListener(eventName, callback) {
        if (typeof callback !== 'function') {
            return;
        }
        var name = String(eventName || 'message').trim();
        if (!listeners.has(name)) {
            listeners.set(name, new Set());
        }
        listeners.get(name).add(callback);

        // If source is already live, bind immediately
        if (source && name !== 'message') {
            source.addEventListener(name, callback);
        }
    }

    function removeListener(eventName, callback) {
        var name = String(eventName || 'message').trim();
        var set = listeners.get(name);
        if (!set) {
            return;
        }
        set.delete(callback);
        if (set.size === 0) {
            listeners.delete(name);
        }

        // Unbind from live source
        if (source && name !== 'message') {
            source.removeEventListener(name, callback);
        }
    }

    function bindAllListeners(eventSource) {
        listeners.forEach(function (callbackSet, eventName) {
            if (eventName === 'message') {
                return; // handled by onmessage
            }
            callbackSet.forEach(function (cb) {
                eventSource.addEventListener(eventName, cb);
            });
        });
    }

    function dispatchToMessageListeners(event) {
        var set = listeners.get('message');
        if (!set) {
            return;
        }
        set.forEach(function (cb) {
            try {
                cb(event);
            } catch (error) {
                console.error('[SSEHub] Message listener error:', error);
            }
        });
    }

    // ── Connection lifecycle ─────────────────────────────────────────
    function clearReconnectTimer() {
        if (reconnectTimer) {
            clearTimeout(reconnectTimer);
            reconnectTimer = null;
        }
    }

    function scheduleReconnect() {
        if (intentionalClose) {
            return;
        }
        clearReconnectTimer();

        var delay = Math.min(
            RECONNECT_BASE_MS * Math.pow(1.5, reconnectAttempt),
            RECONNECT_MAX_MS
        );
        reconnectAttempt += 1;
        setStatus('reconnecting');
        console.log('[SSEHub] Reconnecting in', Math.round(delay), 'ms (attempt', reconnectAttempt, ')');

        reconnectTimer = setTimeout(function () {
            reconnectTimer = null;
            connectInternal();
        }, delay);
    }

    function closeSource() {
        if (source) {
            try {
                source.close();
            } catch (_error) {
                // ignore
            }
            source = null;
        }
    }

    function bindLifecycleListeners() {
        if (lifecycleListenersBound || typeof window === 'undefined') {
            return;
        }

        function handleVisibilityChange() {
            if (typeof document === 'undefined') {
                return;
            }

            if (document.visibilityState === 'hidden') {
                pageSuspended = true;
                clearReconnectTimer();
                closeSource();
                setStatus('offline');
                return;
            }

            if (!pageSuspended || intentionalClose) {
                return;
            }

            pageSuspended = false;
            reconnectAttempt = 0;
            if (window.Auth && typeof Auth.invalidateSSEToken === 'function') {
                Auth.invalidateSSEToken();
            }
            void connectInternal({ forceRefreshToken: true, resetReconnectAttempt: true });
        }

        function handleOnline() {
            if (intentionalClose) {
                return;
            }
            reconnectAttempt = 0;
            if (window.Auth && typeof Auth.invalidateSSEToken === 'function') {
                Auth.invalidateSSEToken();
            }
            void connectInternal({ forceRefreshToken: true, resetReconnectAttempt: true });
        }

        if (typeof document !== 'undefined' && typeof document.addEventListener === 'function') {
            document.addEventListener('visibilitychange', handleVisibilityChange);
        }
        window.addEventListener('online', handleOnline);
        lifecycleListenersBound = true;
    }

    async function connectInternal(options) {
        var opts = options || {};
        if (pageSuspended || (typeof document !== 'undefined' && document.visibilityState === 'hidden')) {
            setStatus('offline');
            return;
        }
        if (!window.Auth || typeof Auth.getEventSource !== 'function') {
            console.warn('[SSEHub] Auth.getEventSource not available, retrying...');
            scheduleReconnect();
            return;
        }

        var mySequence = ++connectSequence;

        if (typeof Auth.requireAuthAsync === 'function') {
            try {
                var isAuthenticated = await Auth.requireAuthAsync();
                if (!isAuthenticated) {
                    setStatus('offline');
                    return;
                }
            } catch (error) {
                console.warn('[SSEHub] Auth.requireAuthAsync failed:', error);
                scheduleReconnect();
                return;
            }
        }

        if (typeof Auth.refreshSSEToken === 'function') {
            try {
                var refreshed = await Auth.refreshSSEToken(opts.forceRefreshToken === true);
                if (!refreshed) {
                    console.warn('[SSEHub] Failed to acquire SSE token before connecting');
                    scheduleReconnect();
                    return;
                }
            } catch (error) {
                console.warn('[SSEHub] Failed to refresh SSE token:', error);
                scheduleReconnect();
                return;
            }
        }

        if (mySequence !== connectSequence) {
            return;
        }

        closeSource();
        setStatus('connecting');
        if (opts.resetReconnectAttempt === true) {
            reconnectAttempt = 0;
        }

        var topicsParam = currentTopics.join(',');
        var url = '/api/v2/sse/stream';
        if (topicsParam) {
            url += '?topics=' + encodeURIComponent(topicsParam);
        }

        try {
            source = Auth.getEventSource(url, {
                clientScope: CLIENT_SCOPE,
                suppressDeprecationWarning: true,
            });
        } catch (error) {
            console.error('[SSEHub] Failed to create EventSource:', error);
            scheduleReconnect();
            return;
        }

        // Bind named event listeners from registry
        bindAllListeners(source);

        // Native EventSource lifecycle hooks
        source.onopen = function () {
            reconnectAttempt = 0;
            setStatus('online');
        };

        source.onmessage = function (event) {
            setStatus('online');
            dispatchToMessageListeners(event);
        };

        source.addEventListener('connected', function () {
            reconnectAttempt = 0;
            setStatus('online');
        });

        source.addEventListener('heartbeat', function () {
            setStatus('online');
        });

        source.onerror = function () {
            closeSource();
            if (!intentionalClose) {
                if (window.Auth && typeof Auth.invalidateSSEToken === 'function') {
                    Auth.invalidateSSEToken();
                }
                scheduleReconnect();
            } else {
                setStatus('offline');
            }
        };
    }

    // ── Public API ───────────────────────────────────────────────────

    /**
     * Connect to the SSE stream. Call once at page initialization.
     * @param {Object} [options]
     * @param {string[]} [options.topics] - Override default topic list
     */
    function connect(options) {
        var opts = options || {};
        currentTopics = Array.isArray(opts.topics) ? opts.topics : DEFAULT_TOPICS;
        intentionalClose = false;
        pageSuspended = typeof document !== 'undefined' && document.visibilityState === 'hidden';
        reconnectAttempt = 0;
        clearReconnectTimer();
        bindLifecycleListeners();
        void connectInternal({ forceRefreshToken: true, resetReconnectAttempt: true });
    }

    /**
     * Disconnect from the SSE stream. Stops all reconnection attempts.
     */
    function disconnect() {
        intentionalClose = true;
        clearReconnectTimer();
        closeSource();
        setStatus('offline');
    }

    /**
     * Register a named-event listener.
     * @param {string} eventName - SSE event name (e.g. 'flights', 'anomaly_alerts', 'heartbeat')
     * @param {Function} callback - Event handler. Receives native MessageEvent.
     * @returns {Function} Unsubscribe function for convenience.
     */
    function on(eventName, callback) {
        addListener(eventName, callback);
        return function () {
            removeListener(eventName, callback);
        };
    }

    /**
     * Remove a previously registered listener.
     * @param {string} eventName
     * @param {Function} callback
     */
    function off(eventName, callback) {
        removeListener(eventName, callback);
    }

    /**
     * Register a connection status change listener.
     * @param {Function} callback - Receives (newStatus, previousStatus)
     * @returns {Function} Unsubscribe function.
     */
    function onStatusChange(callback) {
        if (typeof callback === 'function') {
            statusListeners.add(callback);
        }
        return function () {
            statusListeners.delete(callback);
        };
    }

    /**
     * Get current connection status.
     * @returns {'offline'|'connecting'|'online'|'reconnecting'}
     */
    function getStatus() {
        return status;
    }

    /**
     * Check if currently connected.
     * @returns {boolean}
     */
    function isOnline() {
        return status === 'online';
    }

    return {
        connect: connect,
        disconnect: disconnect,
        on: on,
        off: off,
        onStatusChange: onStatusChange,
        getStatus: getStatus,
        isOnline: isOnline,
    };
})();

if (typeof window !== 'undefined') {
    window.SSEHub = SSEHub;
}
