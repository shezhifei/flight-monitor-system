/**
 * Authentication Utilities for Flight Monitor System
 * 
 * This module handles:
 * - Token storage and retrieval
 * - Automatic token renewal before expiration
 * - Logout functionality
 */

const Auth = (function () {
    'use strict';

    // Configuration
    const CONFIG = {
        RENEWAL_BUFFER_MS: 5 * 60 * 1000, // Renew 5 minutes before expiry
        CHECK_INTERVAL_MS: 60 * 1000,     // Check every minute
        HEARTBEAT_INTERVAL_MS: 60 * 1000, // Keep online session alive every minute
        API_BASE: `${window.location.origin}/api/v2`
    };

    let renewalTimer = null;
    let heartbeatTimer = null;
    let refreshPromise = null; // 用于处理并发刷新令牌的锁
    let heartbeatPromise = null;
    let sseTokenPromise = null;
    let heartbeatLifecycleListenersBound = false;
    let heartbeatSuspendedReason = null;
    const CLIENT_INSTANCE_KEY_PREFIX = 'fm_client_instance_id::';
    const clientInstanceCache = {};
    const IMPLIED_PERMISSION_MAP = {
        'flight:read': ['flight.read', 'business_case.read', 'workflow_run.read'],
        'flight:manage': [
            'flight.read',
            'flight.update',
            'flight.timeline_edit',
            'flight.import_commit',
            'flight.report_generate',
            'business_case.create',
            'business_case.read',
            'business_case.append',
            'business_case.update',
            'business_case.status_transition',
            'workflow_run.start',
            'workflow_run.read',
            'workflow_run.act',
        ],
        'dispatch:view': [
            'dispatch_order.read',
            'dispatch_catalog.read',
            'shift_handover.read',
            'notification.read',
            'notification.receipt_read',
        ],
        'dispatch:manage': [
            'dispatch_order.read',
            'dispatch_order.create',
            'dispatch_order.update',
            'dispatch_order.publish',
            'dispatch_order.cancel',
            'dispatch_catalog.read',
            'dispatch_catalog.edit',
            'shift_handover.read',
            'shift_handover.create',
            'shift_handover.submit',
            'shift_handover.ack',
            'notification.read',
            'notification.send',
            'notification.receipt_read',
            'notification.receipt_manage',
        ],
        'flowable:read': ['workflow_definition.read', 'workflow_run.read'],
        'flowable:manage': [
            'workflow_definition.read',
            'workflow_definition.edit',
            'workflow_definition.publish',
            'workflow_definition.deprecate',
        ],
        'user:read': ['user_admin.read'],
        'user:create': ['user_admin.edit'],
        'user:update': ['user_admin.edit'],
        'user:delete': ['user_admin.edit'],
        'role:read': ['auth_role.read'],
        'role:create': ['auth_role.edit'],
        'role:update': ['auth_role.edit'],
        'role:delete': ['auth_role.edit'],
        'auth:view': ['user_admin.read', 'auth_role.read', 'auth_permission_template.read'],
        'auth:manage': [
            'user_admin.read',
            'user_admin.edit',
            'auth_role.read',
            'auth_role.edit',
            'auth_permission_template.read',
            'auth_permission_template.edit',
        ],
        'system:admin': ['system.config_read', 'system.config_write', 'system.ops_admin'],
    };

    function isPageVisible() {
        if (typeof document === 'undefined') {
            return true;
        }
        return document.visibilityState !== 'hidden';
    }

    function isBrowserOnline() {
        if (typeof navigator === 'undefined' || typeof navigator.onLine !== 'boolean') {
            return true;
        }
        return navigator.onLine;
    }

    function getHeartbeatSuspensionReason() {
        if (!isPageVisible()) {
            return 'page-hidden';
        }
        if (!isBrowserOnline()) {
            return 'browser-offline';
        }
        return null;
    }

    function logHeartbeatStateChange(nextReason) {
        if (heartbeatSuspendedReason === nextReason) {
            return;
        }

        if (nextReason) {
            console.log(`[Auth] Heartbeat paused: ${nextReason}`);
        } else if (heartbeatSuspendedReason) {
            console.log('[Auth] Heartbeat resumed: page visible and browser online');
        }

        heartbeatSuspendedReason = nextReason;
    }

    function canRunHeartbeat() {
        return Boolean(getToken()) && !getHeartbeatSuspensionReason();
    }

    function bindHeartbeatLifecycleListeners() {
        if (heartbeatLifecycleListenersBound || typeof window === 'undefined') {
            return;
        }

        const handleHeartbeatLifecycleChange = () => {
            const previousReason = heartbeatSuspendedReason;
            syncHeartbeatWithPageState();
            const nextReason = getHeartbeatSuspensionReason();
            if (!nextReason && getToken()) {
                if (previousReason || !hasUsableSSEToken()) {
                    invalidateSSEToken();
                    refreshSSEToken(true);
                }
                if (previousReason) {
                    checkAndRenew();
                }
            }
        };

        if (typeof document !== 'undefined' && typeof document.addEventListener === 'function') {
            document.addEventListener('visibilitychange', handleHeartbeatLifecycleChange);
        }

        window.addEventListener('online', handleHeartbeatLifecycleChange);
        window.addEventListener('offline', handleHeartbeatLifecycleChange);
        heartbeatLifecycleListenersBound = true;
    }

    function createClientInstanceId() {
        const nowPart = Date.now().toString(36);
        let randomPart = '';
        if (window.crypto && typeof window.crypto.getRandomValues === 'function') {
            const bytes = new Uint8Array(8);
            window.crypto.getRandomValues(bytes);
            randomPart = Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
        } else {
            randomPart = Math.random().toString(36).slice(2, 14);
        }
        return `${nowPart}-${randomPart}`;
    }

    function getClientInstanceId(scope = 'default') {
        const normalizedScope = String(scope || 'default').trim() || 'default';
        if (clientInstanceCache[normalizedScope]) {
            return clientInstanceCache[normalizedScope];
        }

        const storageKey = `${CLIENT_INSTANCE_KEY_PREFIX}${normalizedScope}`;
        let instanceId = null;

        try {
            instanceId = sessionStorage.getItem(storageKey);
        } catch (_error) {
            instanceId = null;
        }

        if (!instanceId) {
            instanceId = createClientInstanceId();
            try {
                sessionStorage.setItem(storageKey, instanceId);
            } catch (_error) {
                // sessionStorage 不可用时退化为内存缓存
            }
        }

        clientInstanceCache[normalizedScope] = instanceId;
        return instanceId;
    }

    /**
     * Get the storage object (localStorage or sessionStorage)
     */
    function getStorage() {
        // Check if token exists in localStorage first (remember me)
        if (localStorage.getItem('access_token')) {
            return localStorage;
        }
        return sessionStorage;
    }

    /**
     * Get the auth token
     */
    function getToken() {
        return localStorage.getItem('access_token') || sessionStorage.getItem('access_token');
    }

    function readCookieValue(name) {
        const prefix = name + '=';
        const parts = document.cookie.split(';');
        for (let i = 0; i < parts.length; i++) {
            const trimmed = parts[i].trim();
            if (trimmed.startsWith(prefix)) {
                return decodeURIComponent(trimmed.substring(prefix.length));
            }
        }
        return null;
    }

    function getSessionSecret() {
        return localStorage.getItem('session_secret') || sessionStorage.getItem('session_secret') || readCookieValue('session_secret');
    }

    /**
     * Get the refresh token
     */
    function getRefreshToken() {
        return localStorage.getItem('refresh_token') || sessionStorage.getItem('refresh_token');
    }

    /**
     * Get token expiration time (timestamp in ms)
     */
    function getTokenExpiresAt() {
        const expiresAt = localStorage.getItem('token_expires_at') || sessionStorage.getItem('token_expires_at');
        return expiresAt ? parseInt(expiresAt, 10) : null;
    }

    function getSSEToken() {
        return localStorage.getItem('sse_token') || sessionStorage.getItem('sse_token');
    }

    function getSSETokenExpiresAt() {
        const expiresAt = localStorage.getItem('sse_token_expires_at') || sessionStorage.getItem('sse_token_expires_at');
        return expiresAt ? parseInt(expiresAt, 10) : null;
    }

    function hasUsableSSEToken() {
        const token = getSSEToken();
        if (!token) {
            return false;
        }
        const expiresAt = getSSETokenExpiresAt();
        if (!expiresAt) {
            return true;
        }
        return (expiresAt - Date.now()) > 30 * 1000;
    }

    function persistSSEToken(storage, tokenData, options = {}) {
        const keepExistingWhenMissing = options.keepExistingWhenMissing === true;
        if (tokenData && tokenData.sse_token) {
            storage.setItem('sse_token', tokenData.sse_token);
            const expiresInMs = ((tokenData.sse_expires_in || tokenData.expires_in || 3600) * 1000);
            const expiresAt = Date.now() + expiresInMs;
            storage.setItem('sse_token_expires_at', expiresAt.toString());
            return true;
        }

        if (!keepExistingWhenMissing) {
            storage.removeItem('sse_token');
            storage.removeItem('sse_token_expires_at');
        }
        return false;
    }

    function invalidateSSEToken() {
        localStorage.removeItem('sse_token');
        localStorage.removeItem('sse_token_expires_at');
        sessionStorage.removeItem('sse_token');
        sessionStorage.removeItem('sse_token_expires_at');
    }
    /**
     * Decode JWT token
     */
    function decodeToken(token) {
        if (!token) return null;
        try {
            const base64Url = token.split('.')[1];
            const base64 = base64Url.replace(/-/g, '+').replace(/_/g, '/');
            const jsonPayload = decodeURIComponent(atob(base64).split('').map(function (c) {
                return '%' + ('00' + c.charCodeAt(0).toString(16)).slice(-2);
            }).join(''));
            return JSON.parse(jsonPayload);
        } catch (e) {
            console.error('Failed to decode token', e);
            return null;
        }
    }

    /**
     * Get current user info from token
     */
    function getUser() {
        const token = getToken();
        return decodeToken(token);
    }

    function normalizePermissionName(permission) {
        const normalized = String(permission || '').trim();
        return normalized || null;
    }

    function getPermissions(user = getUser()) {
        if (!user || typeof user !== 'object') {
            return [];
        }

        const expandedPermissions = new Set();
        const directPermissions = Array.isArray(user.permissions) ? user.permissions : [];

        directPermissions.forEach((permission) => {
            const normalized = normalizePermissionName(permission);
            if (!normalized) {
                return;
            }

            expandedPermissions.add(normalized);
            const impliedPermissions = IMPLIED_PERMISSION_MAP[normalized] || [];
            impliedPermissions.forEach((impliedPermission) => expandedPermissions.add(impliedPermission));
        });

        return Array.from(expandedPermissions);
    }

    function hasPermission(permission, user = getUser()) {
        if (!user || typeof user !== 'object') {
            return false;
        }
        if (user.is_admin === true || user.role === 'admin') {
            return true;
        }

        const normalized = normalizePermissionName(permission);
        if (!normalized) {
            return false;
        }

        const permissions = new Set(getPermissions(user));
        const wildcard = normalized.includes('.') ? `${normalized.split('.', 1)[0]}.*` : null;
        return permissions.has('*')
            || permissions.has(normalized)
            || (wildcard ? permissions.has(wildcard) : false);
    }

    function hasAnyPermission(requiredPermissions, user = getUser()) {
        if (!Array.isArray(requiredPermissions)) {
            return hasPermission(requiredPermissions, user);
        }

        return requiredPermissions.some((permission) => hasPermission(permission, user));
    }

    /**
     * Check if current user is admin
     */
    function isAdmin() {
        const user = getUser();
        return user && (user.is_admin === true || user.role === 'admin');
    }

    /**
     * Get authorization headers for API requests
     */
    function getAuthHeaders() {
        const token = getToken();
        return {
            'Content-Type': 'application/json',
            'Authorization': token ? `Bearer ${token}` : ''
        };
    }

    /**
     * Save token data to storage
     * @param {Object} tokenData - Token response from API
     * @param {boolean} rememberMe - Whether to use localStorage (persistent) or sessionStorage
     */
    function saveToken(tokenData, rememberMe = null) {
        // Determine storage: use parameter, or check existing preference
        let storage;
        if (rememberMe !== null) {
            storage = rememberMe ? localStorage : sessionStorage;
            // Clear the other storage
            const otherStorage = rememberMe ? sessionStorage : localStorage;
            otherStorage.removeItem('access_token');
            otherStorage.removeItem('refresh_token');
            otherStorage.removeItem('token_type');
            otherStorage.removeItem('token_expires_at');
            otherStorage.removeItem('sse_token');
            otherStorage.removeItem('sse_token_expires_at');
            otherStorage.removeItem('session_secret');
        } else {
            storage = getStorage();
        }

        // Save tokens
        storage.setItem('access_token', tokenData.access_token);
        if (tokenData.refresh_token) {
            storage.setItem('refresh_token', tokenData.refresh_token);
        }
        if (tokenData.session_secret) {
            storage.setItem('session_secret', tokenData.session_secret);
        } else {
            // Web clients receive session_secret via non-HttpOnly cookie; persist it to storage
            const cookieSecret = readCookieValue('session_secret');
            if (cookieSecret) {
                storage.setItem('session_secret', cookieSecret);
            }
        }
        storage.setItem('token_type', tokenData.token_type || 'bearer');

        // Calculate and save expiration time
        // expires_in is in seconds
        const expiresInMs = (tokenData.expires_in || 3600) * 1000;
        const expiresAt = Date.now() + expiresInMs;
        storage.setItem('token_expires_at', expiresAt.toString());
        const hasSSEToken = persistSSEToken(storage, tokenData);

        console.log('[Auth] Token saved. Expires at:', new Date(expiresAt).toLocaleString());

        // Start auto-renewal
        startAutoRenewal();
        if (!hasSSEToken) {
            refreshSSEToken();
        }
    }

    /**
     * Refresh the access token using the refresh token
     */
    async function refreshToken() {
        // 如果已经有一个刷新请求在进行中，直接返回该 Promise
        if (refreshPromise) {
            console.log('[Auth] Token refresh already in progress, waiting...');
            return refreshPromise;
        }

        const refreshTokenValue = getRefreshToken();
        if (!refreshTokenValue) {
            console.warn('[Auth] No refresh token available');
            return false;
        }

        // 创建新的刷新 Promise
        refreshPromise = (async () => {
            try {
                console.log('[Auth] Refreshing token...');
                const response = await fetch(`${CONFIG.API_BASE}/auth/refresh`, {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json'
                    },
                    body: JSON.stringify({ refresh_token: refreshTokenValue }),
                });

                if (!response.ok) {
                    console.error('[Auth] Token refresh failed:', response.status);
                    // If refresh fails with 401, it implies the refresh token is invalid
                    if (response.status === 401) {
                        logout();
                    }
                    return false;
                }

                const tokenData = await response.json();

                const storage = getStorage();
                storage.setItem('access_token', tokenData.access_token);
                if (tokenData.refresh_token) {
                    storage.setItem('refresh_token', tokenData.refresh_token);
                }
                if (tokenData.session_secret) {
                    storage.setItem('session_secret', tokenData.session_secret);
                }
                storage.setItem('token_type', tokenData.token_type || 'bearer');

                // Update expiration time
                const expiresInMs = (tokenData.expires_in || 3600) * 1000;
                const expiresAt = Date.now() + expiresInMs;
                storage.setItem('token_expires_at', expiresAt.toString());
                const hasSSEToken = persistSSEToken(storage, tokenData, { keepExistingWhenMissing: true });
                if (!hasSSEToken) {
                    refreshSSEToken();
                }

                console.log('[Auth] Token refreshed. New expiration:', new Date(expiresAt).toLocaleString());
                return true;
            } catch (error) {
                console.error('[Auth] Token refresh error:', error);
                return false;
            } finally {
                // 请求结束后清除 Promise，允许下一次刷新
                refreshPromise = null;
            }
        })();

        return refreshPromise;
    }

    /**
     * Check if token needs renewal and refresh if necessary
     */
    async function checkAndRenew() {
        const expiresAt = getTokenExpiresAt();
        if (!expiresAt) {
            console.log('[Auth] No expiration time found, skipping renewal check');
            return;
        }

        const now = Date.now();
        const timeUntilExpiry = expiresAt - now;

        console.log(`[Auth] Time until token expiry: ${Math.round(timeUntilExpiry / 1000 / 60)} minutes`);

        // If token expires within buffer time, refresh it
        if (timeUntilExpiry <= CONFIG.RENEWAL_BUFFER_MS) {
            console.log('[Auth] Token expiring soon, initiating refresh...');
            await refreshToken();
        }
    }

    async function sendHeartbeat() {
        if (heartbeatPromise) {
            return heartbeatPromise;
        }

        const token = getToken();
        if (!token) {
            return false;
        }

        heartbeatPromise = (async () => {
            try {
                const response = await authenticatedFetch(`${CONFIG.API_BASE}/auth/heartbeat`, {
                    method: 'POST',
                });

                if (response.ok) {
                    return true;
                }

                return false;
            } catch (error) {
                console.warn('[Auth] Heartbeat request failed:', error);
                return false;
            } finally {
                heartbeatPromise = null;
            }
        })();

        return heartbeatPromise;
    }

    function syncHeartbeatWithPageState(options = {}) {
        const reason = getHeartbeatSuspensionReason();
        logHeartbeatStateChange(reason);

        if (!getToken()) {
            stopHeartbeat();
            return false;
        }

        if (reason) {
            stopHeartbeat();
            return false;
        }

        if (!heartbeatTimer) {
            startHeartbeat({ immediate: options.immediate !== false });
        }

        return true;
    }

    function startHeartbeat(options = {}) {
        if (heartbeatTimer) {
            clearInterval(heartbeatTimer);
            heartbeatTimer = null;
        }

        if (!canRunHeartbeat()) {
            const reason = getHeartbeatSuspensionReason();
            logHeartbeatStateChange(reason);
            return;
        }

        const shouldSendImmediately = options.immediate !== false;
        console.log('[Auth] Heartbeat timer started');
        if (shouldSendImmediately) {
            sendHeartbeat();
        }
        heartbeatTimer = setInterval(() => {
            if (!canRunHeartbeat()) {
                syncHeartbeatWithPageState({ immediate: false });
                return;
            }
            sendHeartbeat();
        }, CONFIG.HEARTBEAT_INTERVAL_MS);
    }

    function stopHeartbeat() {
        if (heartbeatTimer) {
            clearInterval(heartbeatTimer);
            heartbeatTimer = null;
            console.log('[Auth] Heartbeat timer stopped');
        }
    }

    /**
     * Start the automatic token renewal timer
     */
    function startAutoRenewal() {
        // Clear any existing timer
        if (renewalTimer) {
            clearInterval(renewalTimer);
        }

        // Only start if we have a token
        if (!getToken()) {
            stopHeartbeat();
            console.log('[Auth] No token found, auto-renewal not started');
            return;
        }

        console.log('[Auth] Starting auto-renewal timer');
        bindHeartbeatLifecycleListeners();

        // Check immediately
        checkAndRenew();
        syncHeartbeatWithPageState();

        // Set up periodic check
        renewalTimer = setInterval(() => {
            checkAndRenew();
        }, CONFIG.CHECK_INTERVAL_MS);
    }

    /**
     * Stop the automatic token renewal timer
     */
    function stopAutoRenewal() {
        stopHeartbeat();
        if (renewalTimer) {
            clearInterval(renewalTimer);
            renewalTimer = null;
            console.log('[Auth] Auto-renewal timer stopped');
        }
    }

    /**
     * Logout - clear all tokens and redirect to login
     */
    function logout() {
        stopAutoRenewal();
        localStorage.removeItem('access_token');
        localStorage.removeItem('refresh_token');
        localStorage.removeItem('token_type');
        localStorage.removeItem('token_expires_at');
        sessionStorage.removeItem('access_token');
        sessionStorage.removeItem('refresh_token');
        sessionStorage.removeItem('token_type');
        sessionStorage.removeItem('token_expires_at');
        localStorage.removeItem('sse_token');
        localStorage.removeItem('sse_token_expires_at');
        sessionStorage.removeItem('sse_token');
        sessionStorage.removeItem('sse_token_expires_at');
        localStorage.removeItem('session_secret');
        sessionStorage.removeItem('session_secret');
        console.log('[Auth] Logged out');
        window.location.href = '/frontend/html/login.html';
    }

    async function refreshSSEToken(force = false) {
        if (!force && hasUsableSSEToken()) {
            return true;
        }

        if (sseTokenPromise) {
            return sseTokenPromise;
        }

        const token = getToken();
        if (!token) {
            return false;
        }

        sseTokenPromise = (async () => {
            try {
                invalidateSSEToken();
                const response = await authenticatedFetch(`${CONFIG.API_BASE}/auth/sse-token`, {
                    method: 'POST',
                });

                if (response.ok) {
                    const data = await response.json();
                    persistSSEToken(getStorage(), data);
                    return true;
                }

                return false;
            } catch (error) {
                console.warn('[Auth] Failed to refresh SSE token:', error);
                invalidateSSEToken();
                return false;
            } finally {
                sseTokenPromise = null;
            }
        })();

        return sseTokenPromise;
    }

    /**
     * Check if user is authenticated (has valid token)
     */
    function isAuthenticated() {
        const token = getToken();
        const expiresAt = getTokenExpiresAt();

        if (!token) return false;
        if (expiresAt && Date.now() > expiresAt) {
            // Token expired
            return false;
        }
        return true;
    }

    /**
     * Require authentication - redirect to login if not authenticated
     * This is the sync version that checks current token state.
     * For page initialization, use requireAuthAsync() instead.
     */
    function requireAuth() {
        if (!isAuthenticated()) {
            logout();
            return false;
        }
        return true;
    }

    /**
     * Async version of requireAuth that attempts to refresh token if expired.
     * Use this at page initialization to allow token refresh before redirecting.
     */
    async function requireAuthAsync() {
        const token = getToken();
        if (!token) {
            logout();
            return false;
        }

        const expiresAt = getTokenExpiresAt();
        const now = Date.now();

        // Token is still valid
        if (expiresAt && now < expiresAt) {
            return true;
        }

        // Token expired, try to refresh
        console.log('[Auth] Token expired, attempting refresh...');
        const refreshed = await refreshToken();
        if (refreshed) {
            console.log('[Auth] Token refreshed successfully');
            return true;
        }

        // Refresh failed, logout
        console.warn('[Auth] Token refresh failed, redirecting to login');
        logout();
        return false;
    }

    function initAutoRenewal() {
        if (getToken()) {
            console.log('[Auth] Token found, starting auto-renewal');
            startAutoRenewal();
            refreshSSEToken();
        }
    }

    if (typeof document !== 'undefined') {
        if (document.readyState === 'loading') {
            document.addEventListener('DOMContentLoaded', initAutoRenewal);
        } else {
            initAutoRenewal();
        }
    }

    async function sha256Hex(inputBytes) {
        if (!(window.crypto && window.crypto.subtle)) {
            return null;
        }

        const digestBuffer = await window.crypto.subtle.digest('SHA-256', inputBytes);
        return Array.from(new Uint8Array(digestBuffer), (b) => b.toString(16).padStart(2, '0')).join('');
    }

    async function generateAntiReplayHeaders(method, targetPath, bodyHash) {
        const sessionSecret = getSessionSecret();
        if (!sessionSecret) return {};

        const timestamp = Math.floor(Date.now() / 1000).toString();
        const nonce = createClientInstanceId();
        const payload = `${method.toUpperCase()}:${targetPath}:${timestamp}:${nonce}:${bodyHash}`;

        try {
            const encoder = new TextEncoder();
            const keyData = encoder.encode(sessionSecret);
            const msgData = encoder.encode(payload);

            if (window.crypto && window.crypto.subtle) {
                const cryptoKey = await window.crypto.subtle.importKey(
                    "raw",
                    keyData,
                    { name: "HMAC", hash: "SHA-256" },
                    false,
                    ["sign"]
                );

                const signatureBuffer = await window.crypto.subtle.sign(
                    "HMAC",
                    cryptoKey,
                    msgData
                );

                const signatureArray = Array.from(new Uint8Array(signatureBuffer));
                const signatureHex = signatureArray.map(b => b.toString(16).padStart(2, '0')).join('');

                return {
                    'X-Request-Timestamp': timestamp,
                    'X-Request-Nonce': nonce,
                    'X-Request-Signature': signatureHex,
                    'X-Request-Body-SHA256': bodyHash,
                };
            }
        } catch (e) {
            console.warn("[Auth] Failed to generate anti-replay signature", e);
        }
        return {};
    }

    /**
     * Authenticated fetch wrapper
     * Automatically adds authorization header if token exists
     */
    async function authenticatedFetch(url, options = {}) {
        const headers = getAuthHeaders();
        const method = options.method || 'GET';

        let path = url;
        try {
            const urlObj = new URL(url, window.location.origin);
            path = `${urlObj.pathname}${urlObj.search}`;
        } catch (e) {}

        const body = options.body == null ? '' : (typeof options.body === 'string' ? options.body : String(options.body));
        const encoder = new TextEncoder();
        const bodyHash = await sha256Hex(encoder.encode(body));
        const antiReplayHeaders = bodyHash
            ? await generateAntiReplayHeaders(method, path, bodyHash)
            : {};

        const mergedOptions = {
            ...options,
            headers: {
                ...headers,
                ...antiReplayHeaders,
                ...(options.headers || {})
            }
        };

        const response = await fetch(url, mergedOptions);

        // Handle unauthorized response globally
        if (response.status === 401) {
            console.warn('[Auth] Received 401, checking if token needs refresh...');
            // Try to refresh token
            const refreshed = await refreshToken();
            if (refreshed) {
                // Retry the request once with new token
                const retryHeaders = getAuthHeaders();
                const retryBodyHash = await sha256Hex(encoder.encode(body));
                const retryAntiReplayHeaders = retryBodyHash
                    ? await generateAntiReplayHeaders(method, path, retryBodyHash)
                    : {};
                const retryResponse = await fetch(url, {
                    ...mergedOptions,
                    headers: {
                        ...retryHeaders,
                        ...retryAntiReplayHeaders,
                        ...(options.headers || {})
                    }
                });

                if (retryResponse.status === 401) {
                    console.warn('[Auth] Retry after refresh still returned 401, forcing logout');
                    logout();
                }

                return retryResponse;
            } else {
                // Refresh failed or no refresh token, logout
                logout();
            }
        }

        return response;
    }

    /**
     * Get an EventSource instance with authentication token in query params
     * @deprecated Use SSEHub.on() instead for shared connection management.
     */
    function getEventSource(url, options = {}) {
        if (!options.suppressDeprecationWarning) {
            console.warn('[Auth] DEPRECATED: Auth.getEventSource() called directly. Migrate to SSEHub.on() for shared connection.');
        }
        const sseToken = hasUsableSSEToken() ? getSSEToken() : null;
        const fallbackToken = getToken();
        const clientScope = options.clientScope || 'default';
        const clientInstanceId = options.clientInstanceId || getClientInstanceId(clientScope);

        const appendParam = (targetUrl, key, value) => {
            if (!value) {
                return targetUrl;
            }
            const separator = targetUrl.includes('?') ? '&' : '?';
            return `${targetUrl}${separator}${key}=${encodeURIComponent(value)}`;
        };

        let authenticatedUrl = url;
        if (sseToken) {
            authenticatedUrl = appendParam(authenticatedUrl, 'sse_token', sseToken);
        } else {
            if (fallbackToken) {
                authenticatedUrl = appendParam(authenticatedUrl, 'token', fallbackToken);
            }
            console.warn('[Auth] No usable SSE token, using access token fallback and refreshing SSE token in background');
            refreshSSEToken(true);
        }
        authenticatedUrl = appendParam(authenticatedUrl, 'client_instance_id', clientInstanceId);
        return new EventSource(authenticatedUrl);
    }

    // Public API
    return {
        getToken,
        getSSEToken,
        getRefreshToken,
        getTokenExpiresAt,
        getAuthHeaders,
        saveToken,
        refreshToken,
        refreshSSEToken,
        startAutoRenewal,
        stopAutoRenewal,
        logout,
        isAuthenticated,
        requireAuth,
        requireAuthAsync,
        fetch: authenticatedFetch,
        getEventSource,
        invalidateSSEToken,
        getClientInstanceId,
        getUser,
        getPermissions,
        hasPermission,
        hasAnyPermission,
        isAdmin
    };
})();

// For backward compatibility, expose logout globally
if (typeof window !== 'undefined') {
    window.Auth = Auth;

    // 为旧页面提供全局函数兼容
    window.logout = function () {
        Auth.logout();
    };

    window.checkAuth = async function () {
        const isValid = await Auth.requireAuthAsync();
        if (!isValid) return null;
        return Auth.getUser();
    };
}
