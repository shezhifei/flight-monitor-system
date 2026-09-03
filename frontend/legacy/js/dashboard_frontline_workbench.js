const DashboardFrontlineWorkbench = (function () {
    'use strict';

    const API_BASE = `${window.location.origin}/api/v2`;
    const WORKBENCH_CACHE_KEY = 'fm_frontline_workbench_cache_v1';
    const BUSY_MODE_STORAGE_KEY = 'fm_frontline_busy_mode_v1';
    const QUEUE_STORAGE_KEY = 'fm_frontline_action_queue_v1';
    const ACCEPTED_STORAGE_PREFIX = 'fm_frontline_accepted_orders_v1';
    const ATTACHMENT_DB_NAME = 'fm_frontline_offline_attachments_v1';
    const ATTACHMENT_STORE_NAME = 'attachments';
    const MAX_ORDERS = 8;
    const WORKBENCH_CORE_FIELDS = Object.freeze([
        'user_id',
        'generated_at',
        'my_orders',
        'order_counts',
        'pending_shift_handover_count',
    ]);
    const WORKBENCH_DEFERRED_FIELDS = Object.freeze([
        'notification_unread_count',
        'chat_unread_total',
        'pending_sync_action_count',
        'channel_recommendation',
        'next_primary_action',
        'arrival_verification_needed',
        'soft_followups_count',
        'critical_alerts',
        'handover_draft_summary',
    ]);
    const FEEDBACK_COMPONENT_URLS = Object.freeze({
        toast: '/frontend/js/components/toast.js',
        loading: '/frontend/js/components/loading.js',
        emptyError: '/frontend/js/components/empty_error.js',
    });
    const state = {
        currentUser: null,
        currentUserId: null,
        cardElement: null,
        burdenCardElement: null,
        modalRoot: null,
        workbench: null,
        burdenMetrics: null,
        followupQueue: null,
        busyMode: loadBusyMode(),
        acceptedOrders: new Set(),
        queueFlushPromise: null,
        loadPromise: null,
        activeModal: null,
        lastMessage: null,
        attachmentDbPromise: null,
        feedbackComponentsPromise: null,
    };

    function hasPermission(user, permission) {
        return Boolean(user && Array.isArray(user.permissions) && user.permissions.includes(permission));
    }

    function canAccess(user) {
        return Boolean(user && (user.is_admin || hasPermission(user, 'dispatch:view') || hasPermission(user, 'dispatch:manage')));
    }

    function canViewBurdenMetrics(user) {
        return Boolean(user && (user.is_admin || hasPermission(user, 'dispatch:manage')));
    }

    function getStorage() {
        try {
            const probeKey = `${QUEUE_STORAGE_KEY}::probe`;
            localStorage.setItem(probeKey, '1');
            localStorage.removeItem(probeKey);
            return localStorage;
        } catch (_error) {
            try {
                const probeKey = `${QUEUE_STORAGE_KEY}::probe`;
                sessionStorage.setItem(probeKey, '1');
                sessionStorage.removeItem(probeKey);
                return sessionStorage;
            } catch (_innerError) {
                return null;
            }
        }
    }

    function supportsOfflineAttachments() {
        return typeof window !== 'undefined' && 'indexedDB' in window;
    }

    function idbRequestToPromise(request) {
        return new Promise((resolve, reject) => {
            request.onsuccess = () => resolve(request.result);
            request.onerror = () => reject(request.error || new Error('IndexedDB request failed'));
        });
    }

    async function openAttachmentDb() {
        if (!supportsOfflineAttachments()) {
            throw new Error('当前浏览器不支持离线附件缓存');
        }
        if (state.attachmentDbPromise) {
            return state.attachmentDbPromise;
        }
        state.attachmentDbPromise = new Promise((resolve, reject) => {
            const request = window.indexedDB.open(ATTACHMENT_DB_NAME, 1);
            request.onupgradeneeded = () => {
                const database = request.result;
                if (!database.objectStoreNames.contains(ATTACHMENT_STORE_NAME)) {
                    database.createObjectStore(ATTACHMENT_STORE_NAME, { keyPath: 'id' });
                }
            };
            request.onsuccess = () => resolve(request.result);
            request.onerror = () => reject(request.error || new Error('无法打开离线附件数据库'));
        });
        try {
            return await state.attachmentDbPromise;
        } catch (error) {
            state.attachmentDbPromise = null;
            throw error;
        }
    }

    async function putAttachmentRecord(record) {
        const database = await openAttachmentDb();
        const transaction = database.transaction(ATTACHMENT_STORE_NAME, 'readwrite');
        const store = transaction.objectStore(ATTACHMENT_STORE_NAME);
        await idbRequestToPromise(store.put(record));
        await new Promise((resolve, reject) => {
            transaction.oncomplete = () => resolve();
            transaction.onerror = () => reject(transaction.error || new Error('离线附件写入失败'));
            transaction.onabort = () => reject(transaction.error || new Error('离线附件写入中断'));
        });
    }

    async function getAttachmentRecord(id) {
        const database = await openAttachmentDb();
        const transaction = database.transaction(ATTACHMENT_STORE_NAME, 'readonly');
        const store = transaction.objectStore(ATTACHMENT_STORE_NAME);
        return await idbRequestToPromise(store.get(id));
    }

    async function deleteAttachmentRecord(id) {
        const database = await openAttachmentDb();
        const transaction = database.transaction(ATTACHMENT_STORE_NAME, 'readwrite');
        const store = transaction.objectStore(ATTACHMENT_STORE_NAME);
        await idbRequestToPromise(store.delete(id));
        await new Promise((resolve, reject) => {
            transaction.oncomplete = () => resolve();
            transaction.onerror = () => reject(transaction.error || new Error('离线附件删除失败'));
            transaction.onabort = () => reject(transaction.error || new Error('离线附件删除中断'));
        });
    }

    async function saveOfflineAttachment(file) {
        if (!(file instanceof Blob)) {
            throw new Error('缺少可缓存的附件');
        }
        const attachmentId = createIdentifier('offline-attachment');
        await putAttachmentRecord({
            id: attachmentId,
            blob: file,
            name: String(file.name || attachmentId),
            type: String(file.type || 'application/octet-stream'),
            size: Number(file.size || 0),
            last_modified: Number(file.lastModified || Date.now()),
            created_at: new Date().toISOString(),
        });
        return attachmentId;
    }

    function escapeHtml(value) {
        return String(value ?? '')
            .replaceAll('&', '&amp;')
            .replaceAll('<', '&lt;')
            .replaceAll('>', '&gt;')
            .replaceAll('"', '&quot;')
            .replaceAll("'", '&#39;');
    }

    function formatFileSize(bytes) {
        if (bytes < 1024) return `${bytes} B`;
        if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
        return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    }

    function createIdentifier(prefix) {
        const head = `${prefix}-${Date.now().toString(36)}`;
        if (window.crypto && typeof window.crypto.getRandomValues === 'function') {
            const bytes = new Uint8Array(6);
            window.crypto.getRandomValues(bytes);
            return `${head}-${Array.from(bytes, (value) => value.toString(16).padStart(2, '0')).join('')}`;
        }
        return `${head}-${Math.random().toString(36).slice(2, 12)}`;
    }

    function getAcceptedStorageKey(userId) {
        return `${ACCEPTED_STORAGE_PREFIX}:${String(userId || '').trim() || 'anonymous'}`;
    }

    function loadBusyMode() {
        const storage = getStorage();
        return storage ? storage.getItem(BUSY_MODE_STORAGE_KEY) === '1' : false;
    }

    function persistBusyMode(value) {
        const storage = getStorage();
        if (!storage) {
            return;
        }
        if (value) {
            storage.setItem(BUSY_MODE_STORAGE_KEY, '1');
        } else {
            storage.removeItem(BUSY_MODE_STORAGE_KEY);
        }
    }

    function loadQueue() {
        const storage = getStorage();
        if (!storage) {
            return [];
        }
        try {
            const raw = storage.getItem(QUEUE_STORAGE_KEY);
            const parsed = raw ? JSON.parse(raw) : [];
            return Array.isArray(parsed) ? parsed : [];
        } catch (_error) {
            return [];
        }
    }

    function saveQueue(queue) {
        const storage = getStorage();
        if (!storage) {
            return;
        }
        storage.setItem(QUEUE_STORAGE_KEY, JSON.stringify(Array.isArray(queue) ? queue : []));
    }

    function getQueueCount() {
        return loadQueue().length;
    }

    function loadCachedWorkbench() {
        const storage = getStorage();
        if (!storage) {
            return null;
        }
        try {
            const raw = storage.getItem(WORKBENCH_CACHE_KEY);
            return raw ? JSON.parse(raw) : null;
        } catch (_error) {
            return null;
        }
    }

    function saveCachedWorkbench(payload) {
        const storage = getStorage();
        if (!storage) {
            return;
        }
        storage.setItem(WORKBENCH_CACHE_KEY, JSON.stringify(payload || null));
    }

    function loadAcceptedOrders(userId) {
        const storage = getStorage();
        if (!storage) {
            return new Set();
        }
        try {
            const raw = storage.getItem(getAcceptedStorageKey(userId));
            const parsed = raw ? JSON.parse(raw) : [];
            return new Set(Array.isArray(parsed) ? parsed.map((item) => String(item || '').trim()).filter(Boolean) : []);
        } catch (_error) {
            return new Set();
        }
    }

    function saveAcceptedOrders() {
        if (!state.currentUserId) {
            return;
        }
        const storage = getStorage();
        if (!storage) {
            return;
        }
        storage.setItem(getAcceptedStorageKey(state.currentUserId), JSON.stringify(Array.from(state.acceptedOrders)));
    }

    function pruneAcceptedOrders() {
        if (!state.workbench || !Array.isArray(state.workbench.my_orders)) {
            return;
        }
        const activeIds = new Set(
            state.workbench.my_orders
                .map((item) => String(item.order_id || '').trim())
                .filter(Boolean),
        );
        state.acceptedOrders = new Set(Array.from(state.acceptedOrders).filter((orderId) => activeIds.has(orderId)));
        saveAcceptedOrders();
    }

    function getContextHeaders() {
        if (window.DashboardHandover && typeof window.DashboardHandover.getOperatorContextHeaders === 'function') {
            return window.DashboardHandover.getOperatorContextHeaders();
        }
        return {};
    }

    async function parseJsonResponse(response) {
        const contentType = response.headers.get('content-type') || '';
        if (!contentType.includes('application/json')) {
            if (!response.ok) {
                throw new Error(`请求失败（${response.status}）`);
            }
            return null;
        }
        const payload = await response.json().catch(() => null);
        if (!response.ok) {
            const detail = payload && (payload.detail || payload.message || payload.error);
            if (detail && typeof detail === 'object') {
                const joined = [
                    detail.message,
                    ...(Array.isArray(detail.blocking_issues) ? detail.blocking_issues : []),
                ].filter(Boolean).join(' / ');
                throw new Error(joined || `请求失败（${response.status}）`);
            }
            throw new Error(detail || `请求失败（${response.status}）`);
        }
        if (payload && payload.success === false) {
            throw new Error(payload.message || '请求失败');
        }
        return payload;
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

    function normalizeToastType(type) {
        return type === 'error'
            ? 'error'
            : (type === 'success' ? 'success' : (type === 'warning' ? 'warning' : 'info'));
    }

    function clearContainerLoading(container) {
        if (container && window.Loading && typeof window.Loading.hide === 'function') {
            window.Loading.hide(container);
        }
    }

    function showContainerLoading(container, message, options = {}) {
        if (!container || !window.Loading || typeof window.Loading.show !== 'function') {
            return false;
        }
        container.replaceChildren();
        window.Loading.show({
            mode: 'skeleton',
            target: container,
            message: message || '正在加载内容',
            lines: options.lines || 3,
            minHeight: options.minHeight || '120px',
        });
        return true;
    }

    function renderUnifiedState(container, type, message, onRetry) {
        if (!container) {
            return false;
        }
        clearContainerLoading(container);
        if (window.EmptyError && typeof window.EmptyError.show === 'function') {
            window.EmptyError.show(container, type, message, onRetry);
            return true;
        }
        return false;
    }

    function showWorkbenchToast(message, type = 'info', options = {}) {
        const normalizedMessage = String(message || '').trim();
        if (!normalizedMessage) {
            return false;
        }
        if (window.Toast && typeof window.Toast.show === 'function') {
            window.Toast.show(normalizeToastType(type), normalizedMessage, options);
            return true;
        }
        return false;
    }

    function setWorkbenchLoading(isLoading) {
        const orders = state.cardElement && state.cardElement.querySelector('[data-role="orders"]');
        const followupList = state.burdenCardElement && state.burdenCardElement.querySelector('[data-role="followup-list"]');

        if (isLoading) {
            showContainerLoading(orders, '正在加载任务卡...', { lines: 4, minHeight: '220px' });
            showContainerLoading(followupList, '正在加载补核队列...', { lines: 3, minHeight: '120px' });
            return;
        }

        clearContainerLoading(orders);
        clearContainerLoading(followupList);
    }

    async function showUploadToast(type, message, options = {}) {
        try {
            await ensureFeedbackComponents();
        } catch (_error) {
            // ignore component boot failure and fall back below
        }
        if (window.Toast && typeof window.Toast.show === 'function') {
            window.Toast.show(type, message, options);
            return;
        }
        showCardMessage(message, type === 'error' ? 'error' : 'info');
    }

    function createAbortError(message) {
        const error = new Error(message || '已取消上传');
        error.name = 'AbortError';
        return error;
    }

    function getUploadRequestHeaders() {
        const headers = {
            ...getContextHeaders(),
        };
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
                throw new Error(joined || `请求失败（${status}）`);
            }
            throw new Error(detail || `请求失败（${status}）`);
        }
        if (payload && payload.success === false) {
            throw new Error(payload.message || '请求失败');
        }
        return payload;
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
                    const payload = parseUploadResponse(xhr.status, xhr.responseText);
                    resolve(payload && Object.prototype.hasOwnProperty.call(payload, 'data') ? payload.data : payload);
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

    async function request(path, options = {}) {
        const headers = {
            ...getContextHeaders(),
            ...(options.headers || {}),
        };
        const init = {
            ...options,
            headers,
        };
        if (init.body && !(init.body instanceof FormData) && !headers['Content-Type']) {
            headers['Content-Type'] = 'application/json';
        }
        if (init.body && headers['Content-Type'] === 'application/json' && typeof init.body !== 'string') {
            init.body = JSON.stringify(init.body);
        }
        const response = await Auth.fetch(`${API_BASE}${path}`, init);
        const payload = await parseJsonResponse(response);
        return payload && Object.prototype.hasOwnProperty.call(payload, 'data') ? payload.data : payload;
    }

    function buildWorkbenchRequestPath(fields, pendingSyncActionCount) {
        const params = new URLSearchParams();
        params.set('pending_sync_action_count', String(pendingSyncActionCount));
        params.set('max_orders', String(MAX_ORDERS));
        if (Array.isArray(fields) && fields.length > 0) {
            params.set('fields', fields.join(','));
        }
        return `/mobile/workbench?${params.toString()}`;
    }

    async function requestWorkbench(fields, pendingSyncActionCount) {
        return await request(buildWorkbenchRequestPath(fields, pendingSyncActionCount));
    }

    function mergeWorkbenchData(base, patch) {
        return {
            ...(base || {}),
            ...(patch || {}),
        };
    }

    function ensureModalRoot() {
        if (state.modalRoot) {
            return;
        }
        const root = document.createElement('div');
        root.className = 'dashboard-frontline-modal-root';
        root.hidden = true;
        root.addEventListener('click', handleModalClick);
        root.addEventListener('change', handleModalChange);
        root.addEventListener('input', handleModalInput);
        document.body.appendChild(root);
        state.modalRoot = root;
    }

    function createCard(user) {
        ensureModalRoot();
        state.currentUser = user;
        state.currentUserId = user && user.id ? String(user.id) : null;
        state.acceptedOrders = loadAcceptedOrders(state.currentUserId);

        const section = document.createElement('section');
        section.className = `bento-card theme-blue dashboard-frontline-card ${state.busyMode ? 'is-busy-mode' : ''}`;
        section.addEventListener('click', handleCardClick);
        section.innerHTML = `
            <div class="dashboard-frontline-header">
                <div class="dashboard-frontline-title-group">
                    <div class="dashboard-frontline-eyebrow">Frontline Workbench</div>
                    <div class="card-title">一线任务卡</div>
                    <div class="dashboard-frontline-subtitle">只保留当前动作、关键风险、异常首报和完工确认。</div>
                </div>
                <label class="dashboard-frontline-toggle">
                    <input type="checkbox" data-action="toggle-busy-mode" ${state.busyMode ? 'checked' : ''}>
                    <span>忙时模式</span>
                </label>
            </div>
            <div class="dashboard-frontline-overview" data-role="overview"></div>
            <div class="dashboard-frontline-syncbar">
                <div class="dashboard-frontline-sync-meta">
                    <span class="dashboard-frontline-status ${navigator.onLine ? 'is-online' : 'is-offline'}" data-role="network-status">${navigator.onLine ? '在线' : '离线'}</span>
                    <span class="dashboard-frontline-pill" data-role="sync-pill">待补传 0</span>
                    <span class="dashboard-frontline-sync-message" data-role="sync-message">自动补传未开始</span>
                </div>
                <button type="button" class="dashboard-frontline-btn-ghost" data-action="flush-queue">立即补传</button>
            </div>
            <div class="dashboard-frontline-alerts" data-role="alerts"></div>
            <div class="dashboard-frontline-task-list" data-role="orders"></div>
            <div class="dashboard-frontline-card-head">
                <div class="dashboard-frontline-caption">默认一单只呈现一个主按钮；安全检查仅在完工前集中确认。</div>
                <a class="dashboard-frontline-card-link" href="/frontend/html/dispatch_board.html">打开派工中心 <span>→</span></a>
            </div>
            <div class="dashboard-frontline-message" data-role="message"></div>
        `;

        state.cardElement = section;
        renderWorkbenchCard();
        return section;
    }

    function createBurdenCard(user) {
        ensureModalRoot();
        state.currentUser = user;
        state.currentUserId = user && user.id ? String(user.id) : null;

        const section = document.createElement('section');
        section.className = 'bento-card theme-orange dashboard-frontline-burden-card';
        section.innerHTML = `
            <div class="dashboard-frontline-burden-head">
                <div>
                    <div class="card-title">系统增加负担</div>
                    <div class="dashboard-frontline-burden-subtitle">管理侧只看趋势、补核和软闭环，不反压一线补字段。</div>
                </div>
                <a class="dashboard-frontline-burden-link" href="/frontend/html/dispatch_board.html">进入派工中心 <span>→</span></a>
            </div>
            <div class="dashboard-frontline-burden-grid" data-role="burden-grid"></div>
            <div class="dashboard-frontline-queue-summary" data-role="followup-summary"></div>
            <div class="dashboard-frontline-queue-list" data-role="followup-list"></div>
            <div class="dashboard-frontline-message" data-role="burden-message"></div>
        `;

        state.burdenCardElement = section;
        renderBurdenCard();
        return section;
    }

    function afterRenderModules(user) {
        ensureModalRoot();
        state.currentUser = user;
        state.currentUserId = user && user.id ? String(user.id) : null;
        state.acceptedOrders = loadAcceptedOrders(state.currentUserId);
        renderWorkbenchCard();
        renderBurdenCard();
        window.removeEventListener('online', handleOnline);
        window.removeEventListener('offline', handleOffline);
        window.addEventListener('online', handleOnline);
        window.addEventListener('offline', handleOffline);
        void loadAllData();
    }

    function handleOnline() {
        renderWorkbenchCard();
        void flushQueuedActions({ showMessage: true }).then(() => loadAllData());
    }

    function handleOffline() {
        renderWorkbenchCard();
        showCardMessage('当前离线，可继续记录动作，联网后自动补传。', 'info');
    }

    function showCardMessage(message, type) {
        if (showWorkbenchToast(message, type, { duration: type === 'error' ? 4200 : 3200 })) {
            state.lastMessage = null;
            renderCardMessage();
            return;
        }
        state.lastMessage = { message, type };
        renderCardMessage();
    }

    function clearCardMessage() {
        state.lastMessage = null;
        renderCardMessage();
    }

    function renderCardMessage() {
        if (!state.cardElement) {
            return;
        }
        const messageElement = state.cardElement.querySelector('[data-role="message"]');
        if (!messageElement) {
            return;
        }
        const current = state.lastMessage;
        if (!current || !current.message) {
            messageElement.className = 'dashboard-frontline-message';
            messageElement.textContent = '';
            return;
        }
        messageElement.className = `dashboard-frontline-message is-visible is-${current.type || 'info'}`;
        messageElement.textContent = current.message;
    }

    function formatDateTime(value) {
        if (!value) {
            return '未记录';
        }
        const dateValue = new Date(value);
        if (Number.isNaN(dateValue.getTime())) {
            return String(value);
        }
        return dateValue.toLocaleString('zh-CN', { hour12: false, month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' });
    }

    function formatTimeRange(startValue, endValue) {
        const start = startValue ? formatDateTime(startValue) : '未设开始';
        const end = endValue ? formatDateTime(endValue) : '未设结束';
        return `${start} - ${end}`;
    }

    function translateStatus(status) {
        const mapping = {
            pending: '待执行',
            assigned: '待到场',
            in_progress: '进行中',
            completed: '已完成',
            cancelled: '已取消',
        };
        return mapping[String(status || '').trim().toLowerCase()] || '未知状态';
    }

    function translatePrimaryAction(action) {
        const mapping = {
            arrive: '到场',
            complete: '关键确认后完工',
            review_followup: '查看补录',
            review_handover: '查看交接',
            view: '查看详情',
        };
        return mapping[String(action || '').trim().toLowerCase()] || '查看详情';
    }

    function actionButtonText(order) {
        const action = String(order && order.next_primary_action || 'view').trim().toLowerCase();
        if (action === 'complete') {
            return '关键确认后完工';
        }
        return translatePrimaryAction(action);
    }

    function getOrderQueueActions(orderId) {
        const normalizedOrderId = String(orderId || '').trim();
        return loadQueue().filter((item) => String(item.dispatch_order_id || '').trim() === normalizedOrderId);
    }

    function hasPendingAccept(orderId) {
        return getOrderQueueActions(orderId).some((item) => String(item.action_type || '').trim() === 'accept');
    }

    function shouldShowAccept(order) {
        const orderId = String(order && order.order_id || '').trim();
        if (!orderId) {
            return false;
        }
        const status = String(order.status || '').trim().toLowerCase();
        if (status !== 'assigned') {
            return false;
        }
        if (state.acceptedOrders.has(orderId)) {
            return false;
        }
        return !hasPendingAccept(orderId);
    }

    function buildOverviewMetrics() {
        const counts = state.workbench && state.workbench.order_counts ? state.workbench.order_counts : {};
        const criticalAlerts = Array.isArray(state.workbench && state.workbench.critical_alerts) ? state.workbench.critical_alerts : [];
        const pendingSyncCount = getQueueCount();
        return [
            { label: '待执行', value: Number(counts.pending || 0) + Number(counts.assigned || 0) },
            { label: '进行中', value: Number(counts.in_progress || 0) },
            { label: '关键告警', value: criticalAlerts.length },
            { label: '待补传', value: pendingSyncCount },
        ];
    }

    function renderOverview() {
        if (!state.cardElement) {
            return;
        }
        const container = state.cardElement.querySelector('[data-role="overview"]');
        if (!container) {
            return;
        }
        const metrics = buildOverviewMetrics();
        container.innerHTML = metrics.map((item) => `
            <div class="dashboard-frontline-metric">
                <div class="dashboard-frontline-metric-label">${escapeHtml(item.label)}</div>
                <div class="dashboard-frontline-metric-value">${escapeHtml(String(item.value))}</div>
            </div>
        `).join('');
    }

    function renderSyncBar() {
        if (!state.cardElement) {
            return;
        }
        const statusElement = state.cardElement.querySelector('[data-role="network-status"]');
        const pillElement = state.cardElement.querySelector('[data-role="sync-pill"]');
        const messageElement = state.cardElement.querySelector('[data-role="sync-message"]');
        if (statusElement) {
            statusElement.className = `dashboard-frontline-status ${navigator.onLine ? 'is-online' : 'is-offline'}`;
            statusElement.textContent = navigator.onLine ? '在线' : '离线';
        }
        if (pillElement) {
            pillElement.textContent = `待补传 ${getQueueCount()}`;
        }
        if (messageElement) {
            const generatedAt = state.workbench && state.workbench.generated_at ? `最近刷新 ${formatDateTime(state.workbench.generated_at)}` : '尚未加载任务卡';
            messageElement.textContent = navigator.onLine ? generatedAt : '当前离线，动作将进入补传队列';
        }
    }

    function renderAlerts() {
        if (!state.cardElement) {
            return;
        }
        const container = state.cardElement.querySelector('[data-role="alerts"]');
        if (!container) {
            return;
        }
        const criticalAlerts = Array.isArray(state.workbench && state.workbench.critical_alerts) ? state.workbench.critical_alerts : [];
        const summary = state.workbench && state.workbench.handover_draft_summary ? state.workbench.handover_draft_summary : {};
        const pendingHandoverCount = Number(state.workbench && state.workbench.pending_shift_handover_count || 0);
        if (criticalAlerts.length <= 0 && !summary.generated_item_count && !summary.mandatory_count && pendingHandoverCount <= 0) {
            container.hidden = true;
            container.innerHTML = '';
            return;
        }
        container.hidden = false;
        const alertItems = criticalAlerts.slice(0, 3).map((item) => `
            <div class="dashboard-frontline-alert-item">
                <div class="dashboard-frontline-card-head">
                    <div>
                        <div class="dashboard-frontline-task-title">${escapeHtml(item.title || '关键通知')}</div>
                        <div class="dashboard-frontline-task-note">${escapeHtml(item.category || 'critical')}</div>
                    </div>
                    <span class="dashboard-frontline-alert-badge is-critical">critical</span>
                </div>
            </div>
        `);
        const draftSummary = Number(summary.generated_item_count || 0) > 0 ? `
            <div class="dashboard-frontline-alert-item">
                <div class="dashboard-frontline-card-head">
                    <div>
                        <div class="dashboard-frontline-task-title">交接班草稿已生成</div>
                        <div class="dashboard-frontline-task-note">系统已汇总 ${escapeHtml(String(Number(summary.generated_item_count || 0)))} 项，强制项 ${escapeHtml(String(Number(summary.mandatory_count || 0)))} 项。</div>
                    </div>
                    <span class="dashboard-frontline-alert-badge">${escapeHtml(String(Number(summary.generated_item_count || 0)))} 项</span>
                </div>
            </div>
        ` : '';
        const handoverNotice = pendingHandoverCount > 0 ? `
            <div class="dashboard-frontline-alert-item">
                <div class="dashboard-frontline-card-head">
                    <div>
                        <div class="dashboard-frontline-task-title">待签收交接班</div>
                        <div class="dashboard-frontline-task-note">当前有 ${escapeHtml(String(pendingHandoverCount))} 份交接待你确认。</div>
                    </div>
                    <button type="button" class="dashboard-frontline-btn-secondary" data-action="open-handover">打开交接</button>
                </div>
            </div>
        ` : '';
        container.innerHTML = `
            <div class="dashboard-frontline-card-head">
                <div class="dashboard-frontline-task-title">当前关键提醒</div>
                <div class="dashboard-frontline-note">普通通知不打断作业，关键通知和交接强制项保留显式提示。</div>
            </div>
            <div class="dashboard-frontline-alert-list">
                ${handoverNotice}
                ${alertItems.join('')}
                ${draftSummary}
            </div>
        `;
    }

    function renderOrderQueueBadges(orderId) {
        const items = getOrderQueueActions(orderId);
        if (items.length <= 0) {
            return '';
        }
        return `
            <span class="dashboard-frontline-queue-pill">待补传 ${escapeHtml(items.map((item) => translateActionType(item.action_type)).join(' / '))}</span>
        `;
    }

    function translateActionType(actionType) {
        const mapping = {
            accept: '接收任务',
            checkin: '到场',
            complete: '完工',
            report_issue: '异常首报',
            safety_batch_submit: '安全批量确认',
        };
        return mapping[String(actionType || '').trim()] || '补传动作';
    }

    function buildOrderLocation(order) {
        const parts = [
            order.terminal ? `航站楼 ${order.terminal}` : '',
            order.gate ? `登机口 ${order.gate}` : '',
            order.stand_id ? `机位 ${order.stand_id}` : '',
        ].filter(Boolean);
        return parts.join(' · ') || '位置待补充';
    }

    function renderOrders() {
        if (!state.cardElement) {
            return;
        }
        const container = state.cardElement.querySelector('[data-role="orders"]');
        if (!container) {
            return;
        }
        const orders = Array.isArray(state.workbench && state.workbench.my_orders) ? state.workbench.my_orders : [];
        if (orders.length <= 0) {
            const handoverAction = Number(state.workbench && state.workbench.pending_shift_handover_count || 0) > 0
                ? (() => {
                    const button = document.createElement('button');
                    button.type = 'button';
                    button.className = 'dashboard-frontline-btn-secondary';
                    button.dataset.action = 'open-handover';
                    button.textContent = '打开交接班';
                    return button;
                })()
                : null;

            if (!renderUnifiedState(container, 'empty', '当前没有待处理工单。系统仍会持续缓存离线动作，并在联网后补传。')) {
                container.innerHTML = '<div class="dashboard-frontline-empty">当前没有待处理工单。系统仍会持续缓存离线动作，并在联网后补传。</div>';
            }

            if (handoverAction) {
                container.appendChild(handoverAction);
            }
            return;
        }

        clearContainerLoading(container);

        container.innerHTML = orders.map((order) => {
            const orderId = String(order.order_id || '').trim();
            const queuedActions = getOrderQueueActions(orderId);
            const hasPendingQueue = queuedActions.length > 0;
            const primaryAction = String(order.next_primary_action || 'view').trim().toLowerCase();
            const showAccept = shouldShowAccept(order);
            const location = buildOrderLocation(order);
            const riskBadges = [];
            if (String(order.verification_status || '').trim() === 'pending_verification') {
                riskBadges.push('<span class="dashboard-frontline-risk-badge is-warning">到场待补核</span>');
            } else if (order.arrival_verification_needed) {
                riskBadges.push('<span class="dashboard-frontline-risk-badge is-warning">待到场</span>');
            }
            if (order.soft_followup_required) {
                riskBadges.push('<span class="dashboard-frontline-risk-badge is-muted">待班组长补录</span>');
            }
            if (hasPendingQueue) {
                riskBadges.push('<span class="dashboard-frontline-risk-badge is-critical">存在待补传动作</span>');
            }
            if (order.supervisor_notified) {
                riskBadges.push('<span class="dashboard-frontline-risk-badge is-muted">主管已关注</span>');
            }

            return `
                <article class="dashboard-frontline-task ${hasPendingQueue ? 'is-pending-sync' : ''}">
                    <div class="dashboard-frontline-task-head">
                        <div>
                            <div class="dashboard-frontline-task-title">${escapeHtml(order.task_type || '现场任务')}</div>
                            <div class="dashboard-frontline-task-subtitle">${escapeHtml(order.flight_id || '无航班号')} · ${escapeHtml(location)}</div>
                        </div>
                        <span class="dashboard-frontline-pill">${escapeHtml(translateStatus(order.status))}</span>
                    </div>
                    <div class="dashboard-frontline-risk-list">
                        ${riskBadges.join('')}
                        ${renderOrderQueueBadges(orderId)}
                    </div>
                    <div class="dashboard-frontline-task-window">计划窗口：${escapeHtml(formatTimeRange(order.planned_start_time, order.planned_end_time))}</div>
                    <div class="dashboard-frontline-task-meta">
                        <span>截止：${escapeHtml(formatDateTime(order.assignment_deadline))}</span>
                        <span>实际开工：${escapeHtml(formatDateTime(order.actual_start_time))}</span>
                    </div>
                    <div class="dashboard-frontline-task-actions">
                        ${showAccept ? `<button type="button" class="dashboard-frontline-btn-secondary" data-action="accept-order" data-order-id="${escapeHtml(orderId)}">接收任务</button>` : ''}
                        <button type="button" class="dashboard-frontline-btn-ghost" data-action="report-issue" data-order-id="${escapeHtml(orderId)}">异常首报</button>
                        <button type="button" class="dashboard-frontline-btn" data-action="primary" data-order-id="${escapeHtml(orderId)}" data-primary-action="${escapeHtml(primaryAction)}">${escapeHtml(actionButtonText(order))}</button>
                    </div>
                </article>
            `;
        }).join('');
    }

    function renderWorkbenchCard() {
        if (!state.cardElement) {
            return;
        }
        state.cardElement.classList.toggle('is-busy-mode', state.busyMode);
        renderOverview();
        renderSyncBar();
        renderAlerts();
        renderOrders();
        renderCardMessage();
    }

    function renderBurdenCard() {
        if (!state.burdenCardElement) {
            return;
        }
        const grid = state.burdenCardElement.querySelector('[data-role="burden-grid"]');
        const summary = state.burdenCardElement.querySelector('[data-role="followup-summary"]');
        const list = state.burdenCardElement.querySelector('[data-role="followup-list"]');
        const message = state.burdenCardElement.querySelector('[data-role="burden-message"]');
        if (!grid || !summary || !list || !message) {
            return;
        }

        const metrics = state.burdenMetrics || {};
        const followupQueue = state.followupQueue || {};
        const items = [
            { label: '完工被阻断', value: Number(metrics.blocked_completion_count || 0) },
            { label: '软闭环完工', value: Number(metrics.soft_completion_count || 0) },
            { label: '到场待补核', value: Number(metrics.open_arrival_verifications || metrics.pending_arrival_verification_count || 0) },
            { label: '待补录', value: Number(metrics.open_soft_followups || 0) },
            {
                label: '异常首报',
                value: `${Number(metrics.issue_reported_counts && metrics.issue_reported_counts.text || 0)}/${Number(metrics.issue_reported_counts && metrics.issue_reported_counts.photo || 0)}/${Number(metrics.issue_reported_counts && metrics.issue_reported_counts.voice || 0)}`,
            },
            { label: '我的补核队列', value: Number(followupQueue.total || 0) },
        ];

        grid.innerHTML = items.map((item) => `
            <div class="dashboard-frontline-burden-metric">
                <div class="dashboard-frontline-burden-label">${escapeHtml(item.label)}</div>
                <div class="dashboard-frontline-burden-value">${escapeHtml(String(item.value))}</div>
            </div>
        `).join('');

        if (Number(followupQueue.total || 0) > 0) {
            clearContainerLoading(list);
            summary.innerHTML = `
                <span class="dashboard-frontline-risk-badge is-warning">待补核 ${escapeHtml(String(Number(followupQueue.pending_verification_count || 0)))}</span>
                <span class="dashboard-frontline-risk-badge is-muted">待补录 ${escapeHtml(String(Number(followupQueue.soft_followup_count || 0)))}</span>
            `;
            list.innerHTML = (Array.isArray(followupQueue.items) ? followupQueue.items.slice(0, 4) : []).map((item) => `
                <div class="dashboard-frontline-queue-item">
                    <div class="dashboard-frontline-card-head">
                        <div>
                            <div class="dashboard-frontline-task-title">${escapeHtml(item.title || '待办')}</div>
                            <div class="dashboard-frontline-burden-meta">${escapeHtml(item.followup_kind || 'followup')} · 截止 ${escapeHtml(formatDateTime(item.due_date))}</div>
                        </div>
                        <span class="dashboard-frontline-pill">${escapeHtml(item.dispatch_order_id || item.source_id || '-')}</span>
                    </div>
                </div>
            `).join('');
            message.className = 'dashboard-frontline-message';
            message.textContent = '';
        } else {
            summary.innerHTML = '';
            if (!renderUnifiedState(list, 'empty', '当前没有需要班组长补核或补录的积压。')) {
                list.innerHTML = '<div class="dashboard-frontline-queue-empty">当前没有需要班组长补核或补录的积压。</div>';
            }
        }
    }

    async function loadAllData() {
        if (!state.currentUser || !canAccess(state.currentUser)) {
            return;
        }
        if (state.loadPromise) {
            return state.loadPromise;
        }
        state.loadPromise = (async () => {
            try {
                setWorkbenchLoading(true);
                await flushQueuedActions({ showMessage: false });
                const pendingSyncActionCount = getQueueCount();
                const burdenRequests = [];
                if (canViewBurdenMetrics(state.currentUser)) {
                    burdenRequests.push(request('/dispatch-orders/burden-metrics'));
                    burdenRequests.push(request(`/dispatch-orders/followup-queue?assignee=${encodeURIComponent(state.currentUserId || '')}&limit=20`));
                }
                const coreWorkbenchPromise = requestWorkbench(WORKBENCH_CORE_FIELDS, pendingSyncActionCount);
                const deferredWorkbenchPromise = requestWorkbench(WORKBENCH_DEFERRED_FIELDS, pendingSyncActionCount)
                    .catch((error) => {
                        console.warn('[DashboardFrontlineWorkbench] deferred workbench load failed', error);
                        return null;
                    });
                const burdenPromise = burdenRequests.length > 0 ? Promise.all(burdenRequests) : Promise.resolve([]);

                const coreWorkbench = await coreWorkbenchPromise;
                state.workbench = mergeWorkbenchData(null, coreWorkbench || null);
                saveCachedWorkbench(state.workbench);
                pruneAcceptedOrders();
                renderWorkbenchCard();

                const [deferredWorkbench, burdenResults] = await Promise.all([
                    deferredWorkbenchPromise,
                    burdenPromise,
                ]);
                state.workbench = mergeWorkbenchData(state.workbench, deferredWorkbench || null);
                saveCachedWorkbench(state.workbench);
                pruneAcceptedOrders();
                if (canViewBurdenMetrics(state.currentUser)) {
                    state.burdenMetrics = burdenResults[0] || null;
                    state.followupQueue = burdenResults[1] || null;
                }
                clearCardMessage();
            } catch (error) {
                if (!navigator.onLine) {
                    state.workbench = loadCachedWorkbench();
                    showCardMessage('当前离线，已回退到最近一次缓存任务卡。', 'info');
                } else {
                    console.error('[DashboardFrontlineWorkbench] load failed', error);
                    showCardMessage(error.message || '工作台加载失败', 'error');
                }
            } finally {
                setWorkbenchLoading(false);
                renderWorkbenchCard();
                renderBurdenCard();
                state.loadPromise = null;
            }
        })();
        return state.loadPromise;
    }

    function appendQueueItem(item) {
        const queue = loadQueue();
        queue.push(item);
        saveQueue(queue);
        renderWorkbenchCard();
    }

    function removeQueueItemById(id) {
        saveQueue(loadQueue().filter((item) => String(item.id || '') !== String(id || '')));
    }

    function updateQueueItem(id, updater) {
        const queue = loadQueue();
        const nextQueue = queue.map((item) => {
            if (String(item.id || '') !== String(id || '')) {
                return item;
            }
            const nextItem = typeof updater === 'function' ? updater(item) : updater;
            return nextItem || item;
        });
        saveQueue(nextQueue);
    }

    async function flushQueuedActions(options = {}) {
        if (state.queueFlushPromise) {
            return state.queueFlushPromise;
        }
        if (!navigator.onLine) {
            return null;
        }
        state.queueFlushPromise = (async () => {
            const queue = loadQueue();
            if (queue.length <= 0) {
                if (options.showMessage) {
                    showCardMessage('当前没有待补传动作。', 'success');
                }
                renderWorkbenchCard();
                return;
            }
            for (const item of queue) {
                try {
                    if (String(item.queue_type || '') === 'request') {
                        await request(String(item.path || ''), {
                            method: String(item.method || 'POST'),
                            body: item.body || {},
                        });
                        removeQueueItemById(item.id);
                        continue;
                    }

                    if (String(item.action_type || '') === 'report_issue') {
                        const payload = { ...(item.payload || {}) };
                        const offlineAttachmentId = String(payload._offline_attachment_id || '').trim();
                        let uploadedAttachmentId = '';
                        if (offlineAttachmentId) {
                            const uploaded = await uploadOfflineAttachmentReference(offlineAttachmentId);
                            if (uploaded && uploaded.upload_id) {
                                uploadedAttachmentId = String(uploaded.upload_id || '');
                                payload.attachments = uploadedAttachmentId ? [uploadedAttachmentId] : [];
                                if (String(payload.input_mode || '').trim() === 'voice') {
                                    payload.voice_attachment_id = uploadedAttachmentId;
                                }
                            }
                            delete payload._offline_attachment_id;
                            updateQueueItem(item.id, (current) => ({
                                ...current,
                                payload,
                            }));
                        }
                        await request('/dispatch-orders/mobile/sync/actions', {
                            method: 'POST',
                            body: {
                                actions: [
                                    {
                                        client_action_id: String(item.client_action_id || item.id || ''),
                                        action_type: String(item.action_type || ''),
                                        dispatch_order_id: String(item.dispatch_order_id || ''),
                                        payload,
                                },
                            ],
                        },
                    });
                        if (offlineAttachmentId) {
                            await deleteAttachmentRecord(offlineAttachmentId).catch(() => null);
                        }
                        removeQueueItemById(item.id);
                        continue;
                    }

                    await request('/dispatch-orders/mobile/sync/actions', {
                        method: 'POST',
                        body: {
                            actions: [
                                {
                                    client_action_id: String(item.client_action_id || item.id || ''),
                                    action_type: String(item.action_type || ''),
                                    dispatch_order_id: String(item.dispatch_order_id || ''),
                                    payload: item.payload || {},
                                },
                            ],
                        },
                    });
                    removeQueueItemById(item.id);
                } catch (error) {
                    console.error('[DashboardFrontlineWorkbench] flush queue failed', error);
                    if (options.showMessage) {
                        showCardMessage(`补传中断：${error.message || '未知错误'}`, 'error');
                    }
                    break;
                }
            }
            renderWorkbenchCard();
            if (options.showMessage && getQueueCount() === 0) {
                showCardMessage('离线动作已全部补传。', 'success');
            }
        })();
        try {
            return await state.queueFlushPromise;
        } finally {
            state.queueFlushPromise = null;
        }
    }

    function queueSyncAction(orderId, actionType, payload, successMessage) {
        const clientActionId = String((payload && payload.client_action_id) || createIdentifier(actionType));
        appendQueueItem({
            id: clientActionId,
            queue_type: 'sync_action',
            client_action_id: clientActionId,
            dispatch_order_id: String(orderId || ''),
            action_type: String(actionType || ''),
            payload: payload || {},
            enqueued_at: new Date().toISOString(),
        });
        if (actionType === 'accept') {
            state.acceptedOrders.add(String(orderId || ''));
            saveAcceptedOrders();
        }
        showCardMessage(successMessage, 'info');
    }

    function queueRequest(path, method, body, orderId, actionType, successMessage) {
        appendQueueItem({
            id: createIdentifier(actionType || 'request'),
            queue_type: 'request',
            dispatch_order_id: String(orderId || ''),
            action_type: String(actionType || 'request'),
            path: String(path || ''),
            method: String(method || 'POST'),
            body: body || {},
            enqueued_at: new Date().toISOString(),
        });
        showCardMessage(successMessage, 'info');
    }

    function findOrderById(orderId) {
        const orders = Array.isArray(state.workbench && state.workbench.my_orders) ? state.workbench.my_orders : [];
        return orders.find((item) => String(item.order_id || '') === String(orderId || '')) || null;
    }

    async function acceptOrder(orderId) {
        const clientActionId = createIdentifier('accept');
        const payload = { note: null, client_action_id: clientActionId };
        if (!navigator.onLine) {
            queueSyncAction(orderId, 'accept', payload, '接收任务已进入补传队列。');
            renderWorkbenchCard();
            return;
        }
        await request(`/dispatch-orders/${encodeURIComponent(orderId)}/accept`, {
            method: 'POST',
            body: payload,
        });
        state.acceptedOrders.add(String(orderId || ''));
        saveAcceptedOrders();
        showCardMessage('任务已接收。', 'success');
        await loadAllData();
    }

    async function resolvePositionPayload() {
        if (!navigator.geolocation) {
            return { lat: null, lng: null, accuracy_m: null };
        }
        return await new Promise((resolve) => {
            navigator.geolocation.getCurrentPosition(
                (position) => resolve({
                    lat: Number(position.coords.latitude || 0),
                    lng: Number(position.coords.longitude || 0),
                    accuracy_m: Number(position.coords.accuracy || 0),
                }),
                () => resolve({ lat: null, lng: null, accuracy_m: null }),
                { enableHighAccuracy: true, timeout: 6000, maximumAge: 30000 },
            );
        });
    }

    async function arriveOrder(orderId) {
        const position = await resolvePositionPayload();
        const payload = {
            ...position,
            note: null,
            client_action_id: createIdentifier('checkin'),
        };
        if (!navigator.onLine) {
            queueSyncAction(orderId, 'checkin', payload, '到场动作已缓存，联网后自动补传。');
            return;
        }
        const result = await request(`/dispatch-orders/${encodeURIComponent(orderId)}/checkin`, {
            method: 'POST',
            body: payload,
        });
        const verificationStatus = String(result && result.verification_status || '');
        showCardMessage(verificationStatus === 'pending_verification' ? '到场已记录，待班组长补核。' : '到场成功。', 'success');
        await loadAllData();
    }

    function getChecklistLevel(item) {
        return String(item && item.level || 'critical').trim().toLowerCase() === 'routine' ? 'routine' : 'critical';
    }

    async function openIssueModal(orderId) {
        state.activeModal = {
            type: 'issue',
            orderId: String(orderId || ''),
            mode: 'text',
            note: '',
            severity: 'medium',
            error: '',
            uploadFile: null,
            uploadState: null,
        };
        void ensureFeedbackComponents();
        renderModal();
    }

    async function openCompletionModal(orderId) {
        state.activeModal = {
            type: 'complete',
            orderId: String(orderId || ''),
            loading: true,
            checklist: null,
            routineConfirmed: true,
            criticalResults: {},
            completionNotes: '',
            error: '',
            submitting: false,
        };
        renderModal();
        try {
            const checklist = await request(`/dispatch-orders/${encodeURIComponent(orderId)}/safety-checklist`);
            const criticalResults = {};
            const items = Array.isArray(checklist && checklist.items) ? checklist.items : [];
            items
                .filter((item) => getChecklistLevel(item) === 'critical')
                .forEach((item) => {
                    const itemCode = String(item.item_code || '').trim();
                    const result = String(item.result || item.status || '').trim().toLowerCase();
                    criticalResults[itemCode] = {
                        result: result === 'pending' ? '' : result,
                        note: String(item.note || ''),
                        handled_on_site: false,
                    };
                });
            state.activeModal = {
                ...state.activeModal,
                loading: false,
                checklist,
                criticalResults,
            };
        } catch (error) {
            state.activeModal = {
                ...state.activeModal,
                loading: false,
                error: error.message || '安全清单加载失败',
            };
        }
        renderModal();
    }

    function closeModal() {
        if (state.activeModal && state.activeModal.type === 'issue') {
            const abortController = state.activeModal.uploadState && state.activeModal.uploadState.abortController;
            if (abortController && typeof abortController.abort === 'function') {
                abortController.abort();
            }
        }
        state.activeModal = null;
        renderModal();
    }

    function renderIssueUploadState(modal) {
        const uploadState = modal && modal.uploadState ? modal.uploadState : null;
        if (!uploadState) {
            return '';
        }
        const percent = Math.max(0, Math.min(100, Number(uploadState.progress || 0)));
        const status = String(uploadState.status || 'uploading').trim().toLowerCase();
        const fileName = modal.uploadFile ? String(modal.uploadFile.name || '').trim() : '';
        const message = String(uploadState.message || '').trim() || (status === 'error' ? '上传失败，请重试。' : '正在上传附件...');
        // T20: cancel button during uploading and retrying; manual retry button when exhausted
        const cancelHtml = (status === 'uploading' || status === 'retrying')
            ? '<button type="button" class="mobile-upload-progress__button mobile-upload-progress__button--secondary" data-action="cancel-issue-upload">取消上传</button>'
            : '';
        const retryHtml = status === 'error' && uploadState.retryExhausted
            ? '<button type="button" class="mobile-upload-progress__button" data-action="submit-issue" data-order-id="' + escapeHtml(String(modal.orderId || '')) + '">手动重试</button>'
            : '';
        const errorHost = status === 'error' && !uploadState.retryExhausted
            ? '<div class="mobile-upload-progress__error-host" data-role="issue-upload-error"></div>'
            : '';
        // T19: compression info display
        const compressionInfo = uploadState.compressionInfo;
        const compressionHtml = compressionInfo && compressionInfo.originalSize !== compressionInfo.compressedSize
            ? `<div class="mobile-upload-progress__compression">已压缩：${formatFileSize(compressionInfo.originalSize)} → ${formatFileSize(compressionInfo.compressedSize)}（节省 ${compressionInfo.ratio}%）</div>`
            : '';
        return `
            <div class="mobile-upload-progress" data-upload-status="${escapeHtml(status)}">
                <div class="mobile-upload-progress__header">
                    <div>
                        <div class="mobile-upload-progress__title">附件上传进度</div>
                        <div class="mobile-upload-progress__subtitle">${escapeHtml(fileName || '待上传附件')}</div>
                    </div>
                    <div class="mobile-upload-progress__percent">${escapeHtml(String(percent))}%</div>
                </div>
                <div class="mobile-upload-progress__track" role="progressbar" aria-label="附件上传进度" aria-valuemin="0" aria-valuemax="100" aria-valuenow="${escapeHtml(String(percent))}">
                    <span class="mobile-upload-progress__bar" style="width:${escapeHtml(String(percent))}%"></span>
                </div>
                <div class="mobile-upload-progress__meta">${escapeHtml(message)}</div>
                ${compressionHtml}
                <div class="mobile-upload-progress__actions">${cancelHtml}${retryHtml}</div>
                ${errorHost}
            </div>
        `;
    }

    function renderModalMessage(modal) {
        const error = modal && modal.error ? `<div class="dashboard-frontline-message is-visible is-error">${escapeHtml(modal.error)}</div>` : '';
        return error;
    }

    function renderIssueModal(modal) {
        const order = findOrderById(modal.orderId);
        const canCacheOfflineAttachment = supportsOfflineAttachments();
        const offlineAttachmentHint = !navigator.onLine
            ? `<div class="dashboard-frontline-modal-note">${canCacheOfflineAttachment ? '离线状态下会先把附件缓存到本机，联网后自动上传并补传首报。' : '当前浏览器不支持离线附件缓存，请改用文本首报。'}</div>`
            : '';
        return `
            <div class="dashboard-frontline-modal-overlay" data-action="close-modal"></div>
            <div class="dashboard-frontline-modal" role="dialog" aria-modal="true" aria-label="异常首报">
                <div class="dashboard-frontline-modal-head">
                    <div>
                        <div class="card-title">异常首报</div>
                        <div class="dashboard-frontline-modal-note">${escapeHtml((order && order.task_type) || '现场任务')} · ${escapeHtml((order && order.flight_id) || modal.orderId)}</div>
                    </div>
                    <button type="button" class="dashboard-frontline-btn-ghost" data-action="close-modal">关闭</button>
                </div>
                <div class="dashboard-frontline-modal-body">
                    ${renderModalMessage(modal)}
                    <div class="dashboard-frontline-field-list">
                        <div class="dashboard-frontline-field">
                            <label for="frontlineIssueMode">首报方式</label>
                            <select id="frontlineIssueMode" data-field="issue-mode">
                                <option value="text" ${modal.mode === 'text' ? 'selected' : ''}>一句话文本</option>
                                <option value="photo" ${modal.mode === 'photo' ? 'selected' : ''}>拍照首报</option>
                                <option value="voice" ${modal.mode === 'voice' ? 'selected' : ''}>语音首报</option>
                            </select>
                        </div>
                        <div class="dashboard-frontline-field">
                            <label for="frontlineIssueSeverity">严重等级</label>
                            <select id="frontlineIssueSeverity" data-field="issue-severity">
                                <option value="low" ${modal.severity === 'low' ? 'selected' : ''}>low</option>
                                <option value="medium" ${modal.severity === 'medium' ? 'selected' : ''}>medium</option>
                                <option value="high" ${modal.severity === 'high' ? 'selected' : ''}>high</option>
                                <option value="critical" ${modal.severity === 'critical' ? 'selected' : ''}>critical</option>
                            </select>
                        </div>
                        <div class="dashboard-frontline-field">
                            <label for="frontlineIssueNote">现场说明</label>
                            <textarea id="frontlineIssueNote" data-field="issue-note" placeholder="拍照、语音或一句话均可，系统会自动带出订单、人员、位置和时间。">${escapeHtml(modal.note || '')}</textarea>
                        </div>
                        <div class="dashboard-frontline-field">
                            <label for="frontlineIssueFile">附件</label>
                            <input id="frontlineIssueFile" data-field="issue-file" type="file" ${modal.mode === 'photo' ? 'accept="image/*"' : ''} ${modal.mode === 'voice' ? 'accept="audio/*"' : ''} ${modal.mode === 'text' || (!navigator.onLine && !canCacheOfflineAttachment) ? 'disabled' : ''}>
                            ${modal.uploadFile ? `<div class="dashboard-frontline-modal-note">已选附件：${escapeHtml(String(modal.uploadFile.name || '未命名附件'))}</div>` : ''}
                            ${renderIssueUploadState(modal)}
                            ${offlineAttachmentHint}
                        </div>
                    </div>
                    <div class="dashboard-frontline-modal-footer">
                        <div class="dashboard-frontline-modal-note">目标：首报在 15 秒内完成。复杂分类由班组长/调度补录。</div>
                        <div class="dashboard-frontline-modal-actions">
                            <button type="button" class="dashboard-frontline-btn-secondary" data-action="close-modal">取消</button>
                            <button type="button" class="dashboard-frontline-btn" data-action="submit-issue" data-order-id="${escapeHtml(modal.orderId)}" ${modal.uploadState && modal.uploadState.status === 'uploading' ? 'disabled' : ''}>提交首报</button>
                        </div>
                    </div>
                </div>
            </div>
        `;
    }

    function renderCompletionModal(modal) {
        const order = findOrderById(modal.orderId);
        if (modal.loading) {
            return `
                <div class="dashboard-frontline-modal-overlay" data-action="close-modal"></div>
                <div class="dashboard-frontline-modal" role="dialog" aria-modal="true" aria-label="完工确认">
                    <div class="dashboard-frontline-modal-head">
                        <div class="card-title">完工确认</div>
                        <button type="button" class="dashboard-frontline-btn-ghost" data-action="close-modal">关闭</button>
                    </div>
                    <div class="dashboard-frontline-modal-body">
                        <div class="dashboard-frontline-message is-visible is-info">正在加载关键安全项...</div>
                    </div>
                </div>
            `;
        }

        const checklist = modal.checklist || {};
        const items = Array.isArray(checklist.items) ? checklist.items : [];
        const criticalItems = items.filter((item) => getChecklistLevel(item) === 'critical');
        const routineItems = items.filter((item) => getChecklistLevel(item) === 'routine');
        const pendingRoutineItems = routineItems.filter((item) => {
            const result = String(item.result || item.status || '').trim().toLowerCase();
            return !result || result === 'pending';
        });
        const completionLabel = pendingRoutineItems.length > 0 && !modal.routineConfirmed ? '软闭环完工' : '完工确认';

        const criticalHtml = criticalItems.length > 0
            ? criticalItems.map((item) => {
                const itemCode = String(item.item_code || '').trim();
                const selected = modal.criticalResults[itemCode] || { result: '', note: '', handled_on_site: false };
                const currentResult = String(item.result || item.status || '').trim().toLowerCase();
                return `
                    <div class="dashboard-frontline-critical-item">
                        <div class="dashboard-frontline-card-head">
                            <div>
                                <div class="dashboard-frontline-critical-title">${escapeHtml(item.title || itemCode)}</div>
                                <div class="dashboard-frontline-note">${escapeHtml(itemCode)} · 当前状态 ${escapeHtml(currentResult || 'pending')}</div>
                            </div>
                            <span class="dashboard-frontline-risk-badge is-critical">critical</span>
                        </div>
                        <div class="dashboard-frontline-critical-controls">
                            <select data-field="critical-result" data-item-code="${escapeHtml(itemCode)}">
                                <option value="" ${selected.result ? '' : 'selected'}>请选择结果</option>
                                <option value="pass" ${selected.result === 'pass' ? 'selected' : ''}>通过</option>
                                <option value="fail" ${selected.result === 'fail' ? 'selected' : ''}>不通过</option>
                                ${item.allow_na ? `<option value="na" ${selected.result === 'na' ? 'selected' : ''}>不适用</option>` : ''}
                            </select>
                            <input type="text" value="${escapeHtml(selected.note || '')}" placeholder="失败时填写处置说明，或标记已现场处置" data-field="critical-note" data-item-code="${escapeHtml(itemCode)}">
                        </div>
                        <label class="dashboard-frontline-checkbox">
                            <input type="checkbox" data-field="critical-handled" data-item-code="${escapeHtml(itemCode)}" ${selected.handled_on_site ? 'checked' : ''}>
                            <span>已现场处置</span>
                        </label>
                    </div>
                `;
            }).join('')
            : '<div class="dashboard-frontline-empty">当前任务未配置关键安全项。</div>';

        const routineHtml = routineItems.length > 0
            ? `
                <div class="dashboard-frontline-checklist-box">
                    <label class="dashboard-frontline-checkbox">
                        <input type="checkbox" data-field="routine-confirmed" ${modal.routineConfirmed ? 'checked' : ''}>
                        <span>常规项已检查，提交时批量确认剩余 ${escapeHtml(String(pendingRoutineItems.length))} 项</span>
                    </label>
                    <div class="dashboard-frontline-note">常规项不逐条点击；如赶工可先软闭环完工，系统自动转给班组长补录。</div>
                </div>
            `
            : '<div class="dashboard-frontline-note">当前模板没有常规安全项。</div>';

        return `
            <div class="dashboard-frontline-modal-overlay" data-action="close-modal"></div>
            <div class="dashboard-frontline-modal" role="dialog" aria-modal="true" aria-label="完工确认">
                <div class="dashboard-frontline-modal-head">
                    <div>
                        <div class="card-title">完工确认</div>
                        <div class="dashboard-frontline-modal-note">${escapeHtml((order && order.task_type) || '现场任务')} · ${escapeHtml((order && order.flight_id) || modal.orderId)}</div>
                    </div>
                    <button type="button" class="dashboard-frontline-btn-ghost" data-action="close-modal">关闭</button>
                </div>
                <div class="dashboard-frontline-modal-body">
                    ${renderModalMessage(modal)}
                    <div class="dashboard-frontline-checklist-summary">
                        <div class="dashboard-frontline-risk-list">
                            <span class="dashboard-frontline-risk-badge is-critical">关键项 ${escapeHtml(String(Number(checklist.completed_required || 0)))} / ${escapeHtml(String(Number(checklist.required_total || 0)))}</span>
                            <span class="dashboard-frontline-risk-badge is-muted">常规项 ${escapeHtml(String(Number(checklist.completed_routine || 0)))} / ${escapeHtml(String(Number(checklist.routine_total || 0)))}</span>
                        </div>
                        <div class="dashboard-frontline-note">仅关键项失败或重大异常未首报时阻断完工。其余缺项可软闭环。</div>
                    </div>
                    <div class="dashboard-frontline-critical-list">${criticalHtml}</div>
                    ${routineHtml}
                    <div class="dashboard-frontline-field">
                        <label for="frontlineCompleteNote">完工说明</label>
                        <textarea id="frontlineCompleteNote" data-field="completion-note" placeholder="可选，仅填写现场需要补充的说明。">${escapeHtml(modal.completionNotes || '')}</textarea>
                    </div>
                    <div class="dashboard-frontline-modal-footer">
                        <div class="dashboard-frontline-modal-note">真实作业完成优先于系统闭环；非关键缺项允许先完工后补录。</div>
                        <div class="dashboard-frontline-modal-actions">
                            <button type="button" class="dashboard-frontline-btn-secondary" data-action="close-modal">取消</button>
                            <button type="button" class="dashboard-frontline-btn" data-action="submit-complete" data-order-id="${escapeHtml(modal.orderId)}" ${modal.submitting ? 'disabled' : ''}>${escapeHtml(completionLabel)}</button>
                        </div>
                    </div>
                </div>
            </div>
        `;
    }

    function renderModal() {
        ensureModalRoot();
        if (!state.activeModal) {
            state.modalRoot.hidden = true;
            state.modalRoot.innerHTML = '';
            return;
        }
        state.modalRoot.hidden = false;
        if (state.activeModal.type === 'issue') {
            state.modalRoot.innerHTML = renderIssueModal(state.activeModal);
            hydrateIssueUploadFeedback();
            return;
        }
        if (state.activeModal.type === 'complete') {
            state.modalRoot.innerHTML = renderCompletionModal(state.activeModal);
        }
    }

    function updateActiveModal(patch) {
        if (!state.activeModal) {
            return;
        }
        state.activeModal = { ...state.activeModal, ...patch };
        renderModal();
    }

    function handleModalClick(event) {
        const target = event.target.closest('[data-action]');
        if (!target) {
            return;
        }
        const action = target.dataset.action;
        if (action === 'close-modal') {
            closeModal();
            return;
        }
        if (action === 'submit-issue') {
            void submitIssueReport(target.dataset.orderId);
            return;
        }
        if (action === 'cancel-issue-upload') {
            cancelActiveIssueUpload();
            return;
        }
        if (action === 'submit-complete') {
            void submitCompletion(target.dataset.orderId);
        }
    }

    function handleModalChange(event) {
        if (!state.activeModal) {
            return;
        }
        const field = event.target.dataset.field;
        if (!field) {
            return;
        }
        if (state.activeModal.type === 'issue') {
            if (field === 'issue-mode') {
                updateActiveModal({ mode: event.target.value, error: '', uploadFile: null, uploadState: null });
            } else if (field === 'issue-severity') {
                state.activeModal.severity = event.target.value;
            } else if (field === 'issue-file') {
                const file = event.target.files && event.target.files[0] ? event.target.files[0] : null;
                updateActiveModal({ uploadFile: file, uploadState: null, error: '' });
            }
            return;
        }
        if (state.activeModal.type === 'complete') {
            if (field === 'routine-confirmed') {
                state.activeModal.routineConfirmed = Boolean(event.target.checked);
                renderModal();
                return;
            }
            const itemCode = String(event.target.dataset.itemCode || '').trim();
            if (!itemCode) {
                return;
            }
            const current = state.activeModal.criticalResults[itemCode] || { result: '', note: '', handled_on_site: false };
            if (field === 'critical-result') {
                state.activeModal.criticalResults[itemCode] = { ...current, result: event.target.value };
            } else if (field === 'critical-handled') {
                state.activeModal.criticalResults[itemCode] = { ...current, handled_on_site: Boolean(event.target.checked) };
            }
        }
    }

    function handleModalInput(event) {
        if (!state.activeModal) {
            return;
        }
        const field = event.target.dataset.field;
        if (!field) {
            return;
        }
        if (state.activeModal.type === 'issue' && field === 'issue-note') {
            state.activeModal.note = event.target.value;
            return;
        }
        if (state.activeModal.type === 'complete') {
            if (field === 'completion-note') {
                state.activeModal.completionNotes = event.target.value;
                return;
            }
            const itemCode = String(event.target.dataset.itemCode || '').trim();
            if (!itemCode) {
                return;
            }
            const current = state.activeModal.criticalResults[itemCode] || { result: '', note: '', handled_on_site: false };
            if (field === 'critical-note') {
                state.activeModal.criticalResults[itemCode] = { ...current, note: event.target.value };
            }
        }
    }

    function hydrateIssueUploadFeedback() {
        if (!state.activeModal || state.activeModal.type !== 'issue' || !state.modalRoot) {
            return;
        }
        const host = state.modalRoot.querySelector('[data-role="issue-upload-error"]');
        const uploadState = state.activeModal.uploadState;
        if (!host || !uploadState || uploadState.status !== 'error') {
            return;
        }
        const message = String(uploadState.message || '上传失败，请重试。').trim();
        const retryHandler = () => submitIssueReport(state.activeModal.orderId, { retryUploadOnly: true });
        if (window.EmptyError && typeof window.EmptyError.show === 'function') {
            window.EmptyError.show(host, 'error', message, retryHandler);
            return;
        }
        host.innerHTML = `<button type="button" class="mobile-upload-progress__button" data-action="submit-issue" data-order-id="${escapeHtml(state.activeModal.orderId)}">重试上传</button>`;
    }

    function cancelActiveIssueUpload() {
        if (!state.activeModal || state.activeModal.type !== 'issue') {
            return;
        }
        const abortController = state.activeModal.uploadState && state.activeModal.uploadState.abortController;
        if (abortController && typeof abortController.abort === 'function') {
            abortController.abort();
        }
    }

    async function uploadIssueAttachment(file, options = {}) {
        const formData = new FormData();
        formData.append('file', file);
        formData.append('category', 'dispatch_issue');
        return await uploadWithProgress(`${API_BASE}/mobile/uploads`, {
            formData,
            signal: options.signal,
            onProgress: options.onProgress,
        });
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

    async function uploadIssueAttachmentWithFeedback(file) {
        if (!state.activeModal || state.activeModal.type !== 'issue') {
            return null;
        }

        // Image compression step (T19)
        let uploadFile = file;
        let compressionInfo = null;
        if (file && file.type.startsWith('image/')) {
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

        const abortController = new AbortController();
        const MAX_RETRIES = 3;
        let attempt = 0;
        let lastError = null;

        // T20: Exponential backoff retry loop
        while (attempt <= MAX_RETRIES) {
            if (abortController.signal.aborted) {
                updateActiveModal({ uploadFile: null, uploadState: null });
                await showUploadToast('info', '已取消附件上传');
                return null;
            }

            if (attempt > 0) {
                // Exponential backoff: 1s, 2s, 4s
                const delayMs = Math.pow(2, attempt - 1) * 1000;
                updateActiveModal({
                    uploadFile: uploadFile,
                    uploadState: {
                        status: 'retrying',
                        progress: 0,
                        message: `重试中 (${attempt}/${MAX_RETRIES})，${delayMs / 1000}s 后自动重试...`,
                        abortController,
                        compressionInfo,
                        retryAttempt: attempt,
                    },
                });
                // Cancellable delay
                try {
                    await new Promise((resolve, reject) => {
                        const timer = setTimeout(resolve, delayMs);
                        const onAbort = () => { clearTimeout(timer); reject(createAbortError('已取消重试')); };
                        abortController.signal.addEventListener('abort', onAbort, { once: true });
                    });
                } catch (abortErr) {
                    if (abortErr && abortErr.name === 'AbortError') {
                        updateActiveModal({ uploadFile: null, uploadState: null });
                        await showUploadToast('info', '已取消附件上传');
                        return null;
                    }
                    throw abortErr;
                }
            }

            updateActiveModal({
                error: '',
                uploadFile: uploadFile,
                uploadState: {
                    status: 'uploading',
                    progress: 0,
                    message: attempt > 0 ? `重试中 (${attempt}/${MAX_RETRIES})，正在上传...` : '正在上传附件（0%）',
                    abortController,
                    compressionInfo,
                    retryAttempt: attempt,
                },
            });

            try {
                const upload = await uploadIssueAttachment(uploadFile, {
                    signal: abortController.signal,
                    onProgress(progress) {
                        if (!state.activeModal || state.activeModal.type !== 'issue') {
                            return;
                        }
                        updateActiveModal({
                            uploadFile: uploadFile,
                            uploadState: {
                                ...(state.activeModal.uploadState || {}),
                                status: 'uploading',
                                progress,
                                message: attempt > 0
                                    ? `重试中 (${attempt}/${MAX_RETRIES})，上传进度 ${progress}%`
                                    : `正在上传附件（${progress}%）`,
                                abortController,
                                compressionInfo,
                                retryAttempt: attempt,
                            },
                        });
                    },
                });
                // Success
                updateActiveModal({
                    uploadFile: uploadFile,
                    uploadState: {
                        status: 'success',
                        progress: 100,
                        message: '附件上传完成（100%）',
                        compressionInfo,
                    },
                });
                if (compressionInfo) {
                    await showUploadToast('success', `附件上传完成（已压缩 ${compressionInfo.ratio}%）`);
                } else if (attempt > 0) {
                    await showUploadToast('success', `附件上传完成（重试 ${attempt} 次后成功）`);
                } else {
                    await showUploadToast('success', '附件上传完成');
                }
                return upload;
            } catch (error) {
                if (error && error.name === 'AbortError') {
                    updateActiveModal({ uploadFile: null, uploadState: null });
                    await showUploadToast('info', '已取消附件上传');
                    return null;
                }
                lastError = error;
                attempt++;
                if (attempt > MAX_RETRIES) {
                    break;
                }
                // Continue to next retry iteration
            }
        }

        // All retries exhausted
        updateActiveModal({
            uploadFile: uploadFile,
            uploadState: {
                status: 'error',
                progress: 0,
                message: lastError ? lastError.message : `上传失败，已重试 ${MAX_RETRIES} 次`,
                compressionInfo,
                retryExhausted: true,
            },
        });
        await showUploadToast('error', lastError ? lastError.message : `上传失败，已重试 ${MAX_RETRIES} 次`);
        return null;
    }

    async function uploadOfflineAttachmentReference(attachmentId) {
        if (!attachmentId) {
            return null;
        }
        const record = await getAttachmentRecord(attachmentId);
        if (!record || !(record.blob instanceof Blob)) {
            throw new Error('离线附件不存在或已损坏');
        }
        const file = new File([record.blob], String(record.name || attachmentId), {
            type: String(record.type || 'application/octet-stream'),
            lastModified: Number(record.last_modified || Date.now()),
        });
        return await uploadIssueAttachment(file);
    }

    async function submitIssueReport(orderId, options = {}) {
        if (!state.activeModal || state.activeModal.type !== 'issue') {
            return;
        }
        const note = String(state.activeModal.note || '').trim();
        const mode = String(state.activeModal.mode || 'text').trim();
        const severity = String(state.activeModal.severity || 'medium').trim();
        const fileInput = state.modalRoot.querySelector('#frontlineIssueFile');
        const file = state.activeModal.uploadFile || (fileInput && fileInput.files ? fileInput.files[0] : null);

        if (!note && mode === 'text') {
            updateActiveModal({ error: '文本首报至少需要一句说明。' });
            return;
        }
        if ((mode === 'photo' || mode === 'voice') && !navigator.onLine && !supportsOfflineAttachments()) {
            updateActiveModal({ error: '当前浏览器不支持离线附件缓存，请改用文本首报。' });
            return;
        }
        if ((mode === 'photo' || mode === 'voice') && !file) {
            updateActiveModal({ error: '请选择要上传的附件。' });
            return;
        }

        const payload = {
            title: note || (mode === 'photo' ? '现场图片异常首报' : mode === 'voice' ? '现场语音异常首报' : '现场异常首报'),
            note: note || null,
            severity,
            issue_type: 'dispatch_issue',
            input_mode: mode,
            client_action_id: createIdentifier('report-issue'),
        };
        try {
            if ((mode === 'photo' || mode === 'voice') && !navigator.onLine) {
                const offlineAttachmentId = await saveOfflineAttachment(file);
                payload._offline_attachment_id = offlineAttachmentId;
                queueSyncAction(orderId, 'report_issue', payload, '异常首报与附件已缓存，联网后自动补传。');
                closeModal();
                return;
            }
            if (file) {
                const upload = await uploadIssueAttachmentWithFeedback(file);
                if (!upload || !upload.upload_id) {
                    return;
                }
                if (mode === 'voice') {
                    payload.voice_attachment_id = upload && upload.upload_id ? upload.upload_id : null;
                }
                payload.attachments = upload && upload.upload_id ? [upload.upload_id] : [];
            }
            if (!navigator.onLine) {
                queueSyncAction(orderId, 'report_issue', payload, '异常首报已缓存，联网后自动补传。');
                closeModal();
                return;
            }
            await request(`/dispatch-orders/${encodeURIComponent(orderId)}/report-issue`, {
                method: 'POST',
                body: payload,
            });
            closeModal();
            showCardMessage('异常首报已提交。', 'success');
            await loadAllData();
        } catch (error) {
            updateActiveModal({ error: error.message || '异常首报提交失败' });
        }
    }

    function buildCompletionBatch(orderId, modal) {
        const checklist = modal.checklist || {};
        const items = Array.isArray(checklist.items) ? checklist.items : [];
        const batchItems = [];
        const pendingCriticalTitles = [];
        items
            .filter((item) => getChecklistLevel(item) === 'critical')
            .forEach((item) => {
                const itemCode = String(item.item_code || '').trim();
                const currentRecord = modal.criticalResults[itemCode] || { result: '', note: '', handled_on_site: false };
                const existingResult = String(item.result || item.status || '').trim().toLowerCase();
                if (!currentRecord.result && (!existingResult || existingResult === 'pending')) {
                    pendingCriticalTitles.push(item.title || itemCode);
                    return;
                }
                if (!currentRecord.result) {
                    return;
                }
                if (currentRecord.result === 'fail' && !currentRecord.handled_on_site && !String(currentRecord.note || '').trim()) {
                    throw new Error(`${item.title || itemCode} 标记失败时必须填写说明或勾选已现场处置`);
                }
                batchItems.push({
                    item_code: itemCode,
                    result: currentRecord.result,
                    note: String(currentRecord.note || '').trim() || null,
                    handled_on_site: Boolean(currentRecord.handled_on_site),
                });
            });

        if (pendingCriticalTitles.length > 0) {
            throw new Error(`请先确认关键安全项：${pendingCriticalTitles.join(' / ')}`);
        }

        if (modal.routineConfirmed) {
            items
                .filter((item) => getChecklistLevel(item) === 'routine')
                .filter((item) => {
                    const status = String(item.result || item.status || '').trim().toLowerCase();
                    return !status || status === 'pending';
                })
                .forEach((item) => {
                    batchItems.push({
                        item_code: String(item.item_code || '').trim(),
                        result: 'pass',
                        note: null,
                        handled_on_site: false,
                    });
                });
        }

        return batchItems.filter((item) => item.item_code);
    }

    async function submitCompletion(orderId) {
        if (!state.activeModal || state.activeModal.type !== 'complete') {
            return;
        }
        try {
            const batchItems = buildCompletionBatch(orderId, state.activeModal);
            const completionPayload = {
                completion_notes: String(state.activeModal.completionNotes || '').trim() || null,
                client_action_id: createIdentifier('complete'),
            };
            if (!navigator.onLine) {
                if (batchItems.length > 0) {
                    queueRequest(
                        `/dispatch-orders/${encodeURIComponent(orderId)}/safety-checklist/batch-submit`,
                        'POST',
                        { items: batchItems },
                        orderId,
                        'safety_batch_submit',
                        '安全确认已进入补传队列。',
                    );
                }
                queueSyncAction(orderId, 'complete', completionPayload, '完工动作已缓存，联网后自动补传。');
                closeModal();
                return;
            }

            if (batchItems.length > 0) {
                await request(`/dispatch-orders/${encodeURIComponent(orderId)}/safety-checklist/batch-submit`, {
                    method: 'POST',
                    body: { items: batchItems },
                });
            }

            const result = await request(`/dispatch-orders/${encodeURIComponent(orderId)}/complete`, {
                method: 'POST',
                body: completionPayload,
            });
            closeModal();
            const completionMode = String(result && result.completion_mode || '').trim();
            showCardMessage(completionMode === 'soft_complete' ? '已软闭环完工，补录任务已转交班组长。' : '工单已完工。', 'success');
            await loadAllData();
        } catch (error) {
            updateActiveModal({ error: error.message || '完工失败' });
        }
    }

    function openDetail(orderId) {
        window.location.href = `/frontend/html/dispatch_board.html?dispatch_order_id=${encodeURIComponent(orderId)}`;
    }

    function openHandoverDrawer() {
        if (window.DashboardHandover && typeof window.DashboardHandover.openDrawer === 'function') {
            window.DashboardHandover.openDrawer();
            return;
        }
        const handoverCard = document.querySelector('.dashboard-handover-card');
        if (handoverCard instanceof HTMLElement) {
            handoverCard.click();
        }
    }

    async function handlePrimaryAction(orderId, primaryAction) {
        switch (String(primaryAction || 'view').trim().toLowerCase()) {
            case 'arrive':
                await arriveOrder(orderId);
                break;
            case 'complete':
                await openCompletionModal(orderId);
                break;
            case 'review_followup':
            case 'view':
            default:
                openDetail(orderId);
                break;
        }
    }

    async function handleCardClick(event) {
        const target = event.target.closest('[data-action]');
        if (!target) {
            return;
        }
        const action = target.dataset.action;
        if (action === 'toggle-busy-mode') {
            state.busyMode = Boolean(target.checked);
            persistBusyMode(state.busyMode);
            renderWorkbenchCard();
            return;
        }
        if (action === 'flush-queue') {
            await flushQueuedActions({ showMessage: true });
            await loadAllData();
            return;
        }
        if (action === 'open-handover') {
            openHandoverDrawer();
            return;
        }

        const orderId = target.dataset.orderId;
        if (!orderId) {
            return;
        }
        try {
            if (action === 'accept-order') {
                await acceptOrder(orderId);
                return;
            }
            if (action === 'report-issue') {
                await openIssueModal(orderId);
                return;
            }
            if (action === 'primary') {
                await handlePrimaryAction(orderId, target.dataset.primaryAction);
            }
        } catch (error) {
            console.error('[DashboardFrontlineWorkbench] card action failed', error);
            showCardMessage(error.message || '操作失败', 'error');
        }
    }

    return {
        canAccess,
        canViewBurdenMetrics,
        createCard,
        createBurdenCard,
        afterRenderModules,
    };
})();

if (typeof window !== 'undefined') {
    window.DashboardFrontlineWorkbench = DashboardFrontlineWorkbench;
}
