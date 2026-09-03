(function (global) {
    'use strict';

    const CHAT_ROLE_SELECTORS = {
        entryMeta: '[data-chat-role="entryMeta"]',
        groupMeta: '[data-chat-role="groupMeta"]',
        groupList: '[data-chat-role="groupList"]',
        activeTitle: '[data-chat-role="activeTitle"]',
        activeSubtitle: '[data-chat-role="activeSubtitle"]',
        archivePill: '[data-chat-role="archivePill"]',
        messageList: '[data-chat-role="messageList"]',
        emptyTip: '[data-chat-role="emptyTip"]',
        composer: '[data-chat-role="composer"]',
        input: '[data-chat-role="input"]',
        inputCount: '[data-chat-role="inputCount"]',
        sendBtn: '[data-chat-role="sendBtn"]',
        atAllToggle: '[data-chat-role="atAllToggle"]',
        readonlyTip: '[data-chat-role="readonlyTip"]',
    };

    function resolveRefs(root) {
        return Object.fromEntries(
            Object.entries(CHAT_ROLE_SELECTORS).map(([key, selector]) => [key, root.querySelector(selector)])
        );
    }

    function defaultGetCurrentUserId() {
        const user = global.Auth && typeof global.Auth.getUser === 'function' ? global.Auth.getUser() : null;
        return String(user?.sub || user?.id || user?.user_id || '').trim();
    }

    async function defaultFetchJson(url, options = {}) {
        if (!global.Auth || typeof global.Auth.fetch !== 'function') {
            throw new Error('认证上下文不可用');
        }

        const headers = new Headers(options.headers || {});
        if (options.body !== undefined && !headers.has('Content-Type')) {
            headers.set('Content-Type', 'application/json');
        }

        const response = await global.Auth.fetch(url, {
            ...options,
            headers,
        });

        if (!response.ok) {
            const error = new Error(`HTTP ${response.status}`);
            error.status = response.status;
            try {
                const payload = await response.json();
                error.message = payload?.detail || payload?.message || error.message;
            } catch (_parseError) {
                const text = await response.text();
                error.message = text || error.message;
            }
            throw error;
        }

        const contentType = String(response.headers.get('content-type') || '');
        if (contentType.includes('application/json')) {
            return await response.json();
        }
        return null;
    }

    function defaultGetEventSource(url, options = {}) {
        if (!global.Auth || typeof global.Auth.getEventSource !== 'function') {
            return null;
        }
        return global.Auth.getEventSource(url, options);
    }

    function defaultRefreshSseToken() {
        if (!global.Auth || typeof global.Auth.refreshSSEToken !== 'function') {
            return null;
        }
        return global.Auth.refreshSSEToken();
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

    function escapeHtml(value) {
        return String(value ?? '')
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;')
            .replace(/'/g, '&#39;');
    }

    function escapeHtmlAttribute(value) {
        return escapeHtml(value).replace(/`/g, '&#96;');
    }

    function create(options = {}) {
        const root = options.root;
        if (!(root instanceof HTMLElement)) {
            throw new Error('DispatchChatPanel root element is required.');
        }

        const refs = resolveRefs(root);
        const state = {
            enabled: true,
            groups: [],
            unreadTotal: 0,
            selectedGroupId: '',
            messages: [],
            messagesHasMore: false,
            messagesNextBeforeSeq: null,
            inputDraft: '',
            atAll: false,
            loadingGroups: false,
            loadingMessages: false,
            sending: false,
            stream: null,
            reconnectTimer: null,
            reconnectDelayMs: Number(options.reconnectDelayMs) > 0 ? Number(options.reconnectDelayMs) : 5000,
            sessionActive: false,
            initialized: false,
        };

        const showToast = typeof options.showToast === 'function' ? options.showToast : (() => { });
        const isOpen = typeof options.isOpen === 'function' ? options.isOpen : (() => true);
        const getCurrentUserId = typeof options.getCurrentUserId === 'function' ? options.getCurrentUserId : defaultGetCurrentUserId;
        const fetchJson = typeof options.fetchJson === 'function' ? options.fetchJson : defaultFetchJson;
        const getEventSource = typeof options.getEventSource === 'function' ? options.getEventSource : defaultGetEventSource;
        const refreshSseToken = typeof options.refreshSseToken === 'function' ? options.refreshSseToken : defaultRefreshSseToken;

        let teardownHandlers = [];

        function initialize() {
            if (state.initialized) {
                return api;
            }

            bindEvents();
            renderGroupList();
            renderMessages();
            renderComposer();
            state.initialized = true;
            return api;
        }

        function bindEvents() {
            addDomListener(refs.groupList, 'click', async (event) => {
                const target = event.target instanceof Element
                    ? event.target.closest('.dispatch-chat-group-item[data-group-id]')
                    : null;
                if (!(target instanceof HTMLElement)) {
                    return;
                }

                const groupId = String(target.dataset.groupId || '').trim();
                if (!groupId) {
                    return;
                }

                await selectGroup(groupId, { refreshMessages: true, markRead: true });
            });

            addDomListener(refs.messageList, 'scroll', async () => {
                if (refs.messageList.scrollTop > 80) {
                    return;
                }
                if (!state.messagesHasMore || state.loadingMessages || !state.selectedGroupId) {
                    return;
                }

                await loadMessages(state.selectedGroupId, {
                    beforeSeq: state.messagesNextBeforeSeq,
                    prepend: true,
                });
            });

            addDomListener(refs.sendBtn, 'click', async () => {
                await sendMessage();
            });

            addDomListener(refs.input, 'input', () => {
                state.inputDraft = String(refs.input.value || '');
                renderComposer();
            });

            addDomListener(refs.input, 'keydown', async (event) => {
                if (event.isComposing) {
                    return;
                }
                if (event.key === 'Enter' && event.ctrlKey) {
                    return;
                }
                if (event.key === 'Enter') {
                    event.preventDefault();
                    await sendMessage();
                }
            });

            addDomListener(refs.atAllToggle, 'change', () => {
                state.atAll = Boolean(refs.atAllToggle.checked);
            });
        }

        function addDomListener(element, eventName, handler) {
            if (!(element instanceof EventTarget)) {
                return;
            }
            element.addEventListener(eventName, handler);
            teardownHandlers.push(() => {
                element.removeEventListener(eventName, handler);
            });
        }

        function setEntryMeta(text) {
            if (!(refs.entryMeta instanceof HTMLElement)) {
                return;
            }
            refs.entryMeta.textContent = String(text || '').trim();
        }

        async function open(openOptions = {}) {
            initialize();

            if (!state.enabled) {
                showToast('群聊功能未启用');
                return false;
            }

            state.sessionActive = true;

            const loaded = await loadGroups({ silent: state.groups.length > 0 });
            if (!loaded && state.groups.length === 0) {
                state.sessionActive = false;
                return false;
            }

            connectStream();

            const flightId = String(openOptions.flightId || '').trim();
            if (flightId) {
                const opened = await openGroupByFlight(flightId, {
                    silentMissingMembership: openOptions.silentMissingMembership === true,
                });

                if (opened) {
                    focusInput();
                    return true;
                }

                const shouldFallbackToFirstGroup = openOptions.fallbackToFirstGroup !== false;
                if (shouldFallbackToFirstGroup && !state.selectedGroupId && state.groups.length > 0) {
                    await selectGroup(state.groups[0].group_id, { refreshMessages: true, markRead: true });
                } else {
                    renderAll();
                }

                focusInput();
                return false;
            }

            if (!state.selectedGroupId && state.groups.length > 0) {
                await selectGroup(state.groups[0].group_id, { refreshMessages: true, markRead: true });
            } else {
                renderAll();
            }

            focusInput();
            return state.groups.length > 0;
        }

        function close() {
            state.sessionActive = false;
            disconnectStream();
        }

        function destroy() {
            close();
            teardownHandlers.forEach((teardown) => teardown());
            teardownHandlers = [];
            state.initialized = false;
        }

        function focusInput() {
            if (!(refs.input instanceof HTMLTextAreaElement) || refs.input.disabled) {
                return;
            }
            refs.input.focus();
        }

        function renderAll() {
            renderGroupList();
            renderMessages();
            renderComposer();
        }

        function setEnabled(enabled) {
            state.enabled = Boolean(enabled);

            if (!state.enabled) {
                state.groups = [];
                state.unreadTotal = 0;
                state.selectedGroupId = '';
                state.messages = [];
                state.messagesHasMore = false;
                state.messagesNextBeforeSeq = null;
                state.inputDraft = '';
                state.atAll = false;
                close();
            }

            renderAll();
        }

        function getSelectedGroup() {
            const groupId = String(state.selectedGroupId || '').trim();
            if (!groupId) {
                return null;
            }
            return state.groups.find((group) => String(group?.group_id || '') === groupId) || null;
        }

        function isGroupArchived(group) {
            return Boolean(group?.read_only) || String(group?.status || '').toLowerCase() === 'archived';
        }

        function sortGroups() {
            state.groups.sort((left, right) => {
                const leftArchived = isGroupArchived(left);
                const rightArchived = isGroupArchived(right);
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

        function syncUnreadTotalFromGroups() {
            state.unreadTotal = state.groups.reduce((sum, group) => {
                return sum + Math.max(0, Number(group?.unread_count || 0));
            }, 0);
        }

        function upsertGroup(group) {
            if (!group || !group.group_id) {
                return;
            }

            const groupId = String(group.group_id);
            const existingIndex = state.groups.findIndex((item) => String(item?.group_id || '') === groupId);
            if (existingIndex >= 0) {
                state.groups[existingIndex] = {
                    ...state.groups[existingIndex],
                    ...group,
                };
            } else {
                state.groups.push({ ...group });
            }
            sortGroups();
        }

        function renderGroupList() {
            if (!(refs.groupList instanceof HTMLElement) || !(refs.groupMeta instanceof HTMLElement)) {
                return;
            }

            if (!state.enabled) {
                refs.groupMeta.textContent = '未启用';
                refs.groupList.innerHTML = '<div class="dispatch-chat-group-empty">群聊功能未开启</div>';
                return;
            }

            refs.groupMeta.textContent = `${state.groups.length} 个群`;

            if (state.loadingGroups && state.groups.length === 0) {
                refs.groupList.innerHTML = '<div class="dispatch-chat-group-empty">群列表加载中...</div>';
                return;
            }

            if (state.groups.length === 0) {
                refs.groupList.innerHTML = '<div class="dispatch-chat-group-empty">当前暂无可见群聊</div>';
                return;
            }

            refs.groupList.innerHTML = state.groups.map((group) => {
                const groupId = String(group?.group_id || '');
                const unreadCount = Math.max(0, Number(group?.unread_count || 0));
                const archived = isGroupArchived(group);
                const preview = truncateText(group?.last_message_preview || '暂无消息', 40);
                const timeText = group?.last_message_at ? formatDateTime(group.last_message_at) : '-';

                return `
                    <button class="dispatch-chat-group-item ${groupId === state.selectedGroupId ? 'is-selected' : ''}" type="button" data-group-id="${escapeHtmlAttribute(groupId)}" aria-label="打开群组 ${escapeHtml(group?.group_name || groupId)}">
                        <div class="dispatch-chat-group-main">
                            <span class="dispatch-chat-group-title">${escapeHtml(group?.group_name || groupId)}</span>
                            ${archived ? '<span class="dispatch-chat-group-status">已归档</span>' : ''}
                        </div>
                        <div class="dispatch-chat-group-sub">${escapeHtml(preview)}</div>
                        <div class="dispatch-chat-group-meta-row">
                            <span>${escapeHtml(timeText)}</span>
                            ${unreadCount > 0 ? `<span class="dispatch-chat-group-unread">${unreadCount > 99 ? '99+' : unreadCount}</span>` : ''}
                        </div>
                    </button>
                `;
            }).join('');
        }

        function getMessageKey(message) {
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

        function dedupeAndSortMessages(messages) {
            const seen = new Set();
            const deduped = [];
            for (const message of messages || []) {
                const key = getMessageKey(message);
                if (key && seen.has(key)) {
                    continue;
                }
                if (key) {
                    seen.add(key);
                }
                deduped.push(message);
            }

            deduped.sort((left, right) => Number(left?.seq_no || 0) - Number(right?.seq_no || 0));
            return deduped;
        }

        function appendMessage(message) {
            if (!message || typeof message !== 'object') {
                return;
            }
            state.messages = dedupeAndSortMessages([...state.messages, message]);
        }

        function scrollToBottom() {
            if (!(refs.messageList instanceof HTMLElement)) {
                return;
            }
            refs.messageList.scrollTop = refs.messageList.scrollHeight;
        }

        function renderMessages() {
            if (!(refs.messageList instanceof HTMLElement)
                || !(refs.emptyTip instanceof HTMLElement)
                || !(refs.activeTitle instanceof HTMLElement)
                || !(refs.activeSubtitle instanceof HTMLElement)
                || !(refs.archivePill instanceof HTMLElement)) {
                return;
            }

            const selectedGroup = getSelectedGroup();
            if (!selectedGroup) {
                refs.activeTitle.textContent = '请选择群组';
                refs.activeSubtitle.textContent = '仅成员可见';
                refs.archivePill.hidden = true;
                refs.messageList.innerHTML = '';
                refs.emptyTip.hidden = false;
                refs.emptyTip.textContent = state.enabled ? '选择左侧群组开始沟通' : '群聊功能未开启';
                return;
            }

            refs.activeTitle.textContent = selectedGroup.group_name || selectedGroup.group_id || '-';
            refs.activeSubtitle.textContent = `航班 ${selectedGroup.flight_id || '-'} | 成员 ${Number(selectedGroup.member_count || 0)}`;
            refs.archivePill.hidden = !isGroupArchived(selectedGroup);

            if (state.loadingMessages && state.messages.length === 0) {
                refs.messageList.innerHTML = '<div class="dispatch-chat-message-loading">消息加载中...</div>';
                refs.emptyTip.hidden = true;
                return;
            }

            if (state.messages.length === 0) {
                refs.messageList.innerHTML = '';
                refs.emptyTip.hidden = false;
                refs.emptyTip.textContent = '暂无消息，发送第一条沟通信息';
                return;
            }

            const currentUserId = getCurrentUserId();
            refs.messageList.innerHTML = state.messages.map((message) => {
                const messageType = String(message?.message_type || 'text').toLowerCase();
                const senderId = String(message?.sender_user_id || '').trim();
                const senderName = String(message?.sender_username || senderId || '系统').trim() || '系统';
                const isMine = Boolean(senderId && currentUserId && senderId === currentUserId);
                const contentHtml = escapeHtml(String(message?.content || '')).replace(/\n/g, '<br>');
                const timeText = formatDateTime(message?.sent_at);
                const atAllTag = message?.is_at_all ? '<span class="dispatch-chat-message-atall">@全体</span>' : '';

                if (messageType === 'system') {
                    return `
                        <div class="dispatch-chat-system-message">
                            <span>${contentHtml}</span>
                        </div>
                    `;
                }

                return `
                    <div class="dispatch-chat-message-row ${isMine ? 'is-mine' : ''}">
                        <div class="dispatch-chat-message-meta">
                            <span>${escapeHtml(isMine ? '我' : senderName)}</span>
                            <span>${escapeHtml(timeText)}</span>
                        </div>
                        <div class="dispatch-chat-message-bubble">
                            ${atAllTag}
                            <div class="dispatch-chat-message-content">${contentHtml}</div>
                        </div>
                    </div>
                `;
            }).join('');

            refs.emptyTip.hidden = true;
        }

        function renderComposer() {
            if (!(refs.composer instanceof HTMLElement)
                || !(refs.input instanceof HTMLTextAreaElement)
                || !(refs.inputCount instanceof HTMLElement)
                || !(refs.sendBtn instanceof HTMLButtonElement)
                || !(refs.atAllToggle instanceof HTMLInputElement)
                || !(refs.readonlyTip instanceof HTMLElement)) {
                return;
            }

            const selectedGroup = getSelectedGroup();
            const archived = isGroupArchived(selectedGroup);
            const disabled = !selectedGroup || archived || state.sending;

            refs.input.value = state.inputDraft || '';
            refs.atAllToggle.checked = Boolean(state.atAll);
            refs.atAllToggle.disabled = disabled;
            refs.input.disabled = disabled;
            refs.sendBtn.disabled = disabled || !String(state.inputDraft || '').trim();
            refs.inputCount.textContent = `${String(state.inputDraft || '').length}/2000`;
            refs.readonlyTip.hidden = !selectedGroup || !archived;
        }

        async function loadGroups(options = {}) {
            if (!state.enabled) {
                return false;
            }

            state.loadingGroups = true;
            renderGroupList();

            try {
                const payload = await fetchJson('/api/v2/dispatch/collaboration/groups?status=all&limit=120&offset=0');
                const items = Array.isArray(payload?.items) ? payload.items : [];
                const selectedGroupId = state.selectedGroupId;

                state.groups = items.map((item) => ({ ...item }));
                sortGroups();

                if (Number.isFinite(Number(payload?.unread_total))) {
                    state.unreadTotal = Math.max(0, Number(payload.unread_total));
                } else {
                    syncUnreadTotalFromGroups();
                }

                if (selectedGroupId && !state.groups.some((group) => String(group?.group_id || '') === selectedGroupId)) {
                    state.selectedGroupId = '';
                    state.messages = [];
                    state.messagesHasMore = false;
                    state.messagesNextBeforeSeq = null;
                }

                renderAll();
                return true;
            } catch (error) {
                if (Number(error?.status) === 503) {
                    setEnabled(false);
                    return false;
                }
                if (!options.silent) {
                    showToast(error?.message || '加载群聊列表失败');
                }
                return false;
            } finally {
                state.loadingGroups = false;
                renderGroupList();
            }
        }

        async function openGroupByFlight(flightId, options = {}) {
            const normalizedFlightId = String(flightId || '').trim();
            if (!normalizedFlightId) {
                return false;
            }

            try {
                const group = await fetchJson(`/api/v2/dispatch/collaboration/groups/by-flight/${encodeURIComponent(normalizedFlightId)}`);
                upsertGroup(group);
                syncUnreadTotalFromGroups();
                renderGroupList();
                await selectGroup(group.group_id, { refreshMessages: true, markRead: true });
                return true;
            } catch (error) {
                if (Number(error?.status) === 404) {
                    if (!options.silentMissingMembership) {
                        showToast('你不在该航班群聊中');
                    }
                    return false;
                }
                showToast(error?.message || '打开航班群聊失败');
                return false;
            }
        }

        async function selectGroup(groupId, options = {}) {
            const normalizedGroupId = String(groupId || '').trim();
            if (!normalizedGroupId) {
                return;
            }

            const changed = state.selectedGroupId !== normalizedGroupId;
            state.selectedGroupId = normalizedGroupId;
            if (changed) {
                state.messages = [];
                state.messagesHasMore = false;
                state.messagesNextBeforeSeq = null;
                state.inputDraft = '';
                state.atAll = false;
            }

            renderAll();

            if (options.refreshMessages !== false) {
                await loadMessages(normalizedGroupId, { prepend: false });
            }

            if (options.markRead !== false) {
                await markGroupRead(normalizedGroupId, null, { silent: true });
            }
        }

        async function loadMessages(groupId, options = {}) {
            const normalizedGroupId = String(groupId || '').trim();
            if (!normalizedGroupId || state.loadingMessages) {
                return;
            }

            const prepend = options.prepend === true;
            const beforeSeq = Number.isFinite(Number(options.beforeSeq)) ? Number(options.beforeSeq) : null;
            state.loadingMessages = true;
            renderMessages();

            const previousHeight = refs.messageList instanceof HTMLElement ? refs.messageList.scrollHeight : 0;
            const previousTop = refs.messageList instanceof HTMLElement ? refs.messageList.scrollTop : 0;

            try {
                const params = new URLSearchParams();
                params.set('limit', prepend ? '40' : '50');
                if (beforeSeq && beforeSeq > 0) {
                    params.set('before_seq', String(beforeSeq));
                }

                const payload = await fetchJson(`/api/v2/dispatch/collaboration/groups/${encodeURIComponent(normalizedGroupId)}/messages?${params.toString()}`);
                const items = Array.isArray(payload?.items) ? payload.items : [];

                if (state.selectedGroupId !== normalizedGroupId) {
                    return;
                }

                if (prepend) {
                    state.messages = dedupeAndSortMessages([...items, ...state.messages]);
                } else {
                    state.messages = dedupeAndSortMessages(items);
                }

                state.messagesHasMore = Boolean(payload?.has_more);
                state.messagesNextBeforeSeq = Number(payload?.next_before_seq || 0) || null;

                renderMessages();
                renderComposer();

                if (prepend && refs.messageList instanceof HTMLElement) {
                    const nextHeight = refs.messageList.scrollHeight;
                    refs.messageList.scrollTop = Math.max(0, nextHeight - previousHeight + previousTop);
                } else {
                    scrollToBottom();
                }
            } catch (error) {
                showToast(error?.message || '加载群消息失败');
            } finally {
                state.loadingMessages = false;
                renderMessages();
                renderComposer();
            }
        }

        async function markGroupRead(groupId, readSeq, options = {}) {
            const normalizedGroupId = String(groupId || '').trim();
            if (!normalizedGroupId || !state.enabled) {
                return;
            }

            const body = {};
            if (Number.isFinite(Number(readSeq)) && Number(readSeq) > 0) {
                body.read_seq = Number(readSeq);
            }

            try {
                const payload = await fetchJson(`/api/v2/dispatch/collaboration/groups/${encodeURIComponent(normalizedGroupId)}/read`, {
                    method: 'POST',
                    body: JSON.stringify(body),
                });

                const group = state.groups.find((item) => String(item?.group_id || '') === normalizedGroupId);
                if (group) {
                    group.unread_count = Math.max(0, Number(payload?.unread_count || 0));
                }

                if (Number.isFinite(Number(payload?.unread_total))) {
                    state.unreadTotal = Math.max(0, Number(payload.unread_total));
                } else {
                    syncUnreadTotalFromGroups();
                }

                renderGroupList();
            } catch (error) {
                if (!options.silent) {
                    showToast(error?.message || '标记已读失败');
                }
            }
        }

        async function sendMessage() {
            if (!state.enabled || state.sending) {
                return;
            }

            const selectedGroup = getSelectedGroup();
            if (!selectedGroup) {
                return;
            }

            if (isGroupArchived(selectedGroup)) {
                showToast('群聊已归档，只读不可发送');
                return;
            }

            const content = String(state.inputDraft || '').trim();
            if (!content) {
                return;
            }

            state.sending = true;
            renderComposer();

            try {
                const message = await fetchJson(`/api/v2/dispatch/collaboration/groups/${encodeURIComponent(selectedGroup.group_id)}/messages`, {
                    method: 'POST',
                    body: JSON.stringify({
                        content,
                        at_all: Boolean(state.atAll),
                    }),
                });

                appendMessage(message);
                state.inputDraft = '';
                state.atAll = false;

                const group = getSelectedGroup();
                if (group) {
                    group.last_message_preview = String(message?.content || '');
                    group.last_message_at = message?.sent_at || new Date().toISOString();
                    group.last_message_seq = Number(message?.seq_no || group.last_message_seq || 0);
                    group.unread_count = 0;
                }

                syncUnreadTotalFromGroups();
                renderMessages();
                renderComposer();
                renderGroupList();
                scrollToBottom();

                await markGroupRead(selectedGroup.group_id, Number(message?.seq_no || 0), { silent: true });
            } catch (error) {
                showToast(error?.message || '发送消息失败');
            } finally {
                state.sending = false;
                renderComposer();
            }
        }

        function applyMessageEvent(payload) {
            if (!payload || typeof payload !== 'object') {
                return;
            }

            const groupId = String(payload.group_id || '').trim();
            const message = payload.message;
            if (!groupId || !message) {
                return;
            }

            let group = state.groups.find((item) => String(item?.group_id || '') === groupId);
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
                state.groups.push(group);
            }

            group.last_message_preview = String(message?.content || '');
            group.last_message_at = message?.sent_at || new Date().toISOString();
            group.last_message_seq = Number(message?.seq_no || group.last_message_seq || 0);
            group.unread_count = Math.max(0, Number(payload.unread_count || 0));

            if (Number.isFinite(Number(payload.unread_total))) {
                state.unreadTotal = Math.max(0, Number(payload.unread_total));
            } else {
                syncUnreadTotalFromGroups();
            }

            sortGroups();

            if (state.selectedGroupId === groupId) {
                appendMessage(message);
                renderMessages();
                scrollToBottom();

                const messageSender = String(message?.sender_user_id || '').trim();
                const currentUserId = getCurrentUserId();
                if (isOpen() && messageSender && currentUserId && messageSender !== currentUserId) {
                    markGroupRead(groupId, Number(message?.seq_no || 0), { silent: true });
                }
            }

            renderGroupList();
            renderComposer();
        }

        function applyGroupUpsertEvent(payload) {
            if (!payload || typeof payload !== 'object' || !payload.group || !payload.group.group_id) {
                return;
            }

            upsertGroup(payload.group);
            if (!state.selectedGroupId) {
                state.selectedGroupId = String(payload.group.group_id);
            }
            syncUnreadTotalFromGroups();
            renderAll();
        }

        function applyGroupArchivedEvent(payload) {
            if (!payload || typeof payload !== 'object') {
                return;
            }

            const groupId = String(payload.group_id || '').trim();
            if (!groupId) {
                return;
            }

            const group = state.groups.find((item) => String(item?.group_id || '') === groupId);
            if (!group) {
                return;
            }

            group.status = 'archived';
            group.read_only = true;
            group.archived_at = payload.archived_at || group.archived_at || null;
            sortGroups();
            renderAll();
        }

        function applyReadSyncedEvent(payload) {
            if (!payload || typeof payload !== 'object') {
                return;
            }

            const groupId = String(payload.group_id || '').trim();
            if (!groupId) {
                return;
            }

            const group = state.groups.find((item) => String(item?.group_id || '') === groupId);
            if (group) {
                group.unread_count = Math.max(0, Number(payload.unread_count || 0));
            }

            if (Number.isFinite(Number(payload?.unread_total))) {
                state.unreadTotal = Math.max(0, Number(payload.unread_total));
            } else {
                syncUnreadTotalFromGroups();
            }

            renderGroupList();
        }

        function handlePayload(payload, explicitEvent) {
            if (!payload || typeof payload !== 'object') {
                return;
            }

            const eventName = String(explicitEvent || '').trim();
            const payloadType = String(payload.type || '').trim().toLowerCase();

            if (eventName === 'initial' || payloadType === 'dispatch_chat_initial') {
                const items = Array.isArray(payload.items) ? payload.items : [];
                state.groups = items.map((item) => ({ ...item }));
                sortGroups();

                if (Number.isFinite(Number(payload?.unread_total))) {
                    state.unreadTotal = Math.max(0, Number(payload.unread_total));
                } else {
                    syncUnreadTotalFromGroups();
                }

                if (state.selectedGroupId && !state.groups.some((group) => String(group?.group_id || '') === state.selectedGroupId)) {
                    state.selectedGroupId = '';
                    state.messages = [];
                    state.messagesHasMore = false;
                    state.messagesNextBeforeSeq = null;
                }

                renderAll();
                return;
            }

            if (eventName === 'chat_message' || payloadType === 'dispatch_chat_message') {
                applyMessageEvent(payload);
                return;
            }

            if (eventName === 'chat_group_upserted' || payloadType === 'dispatch_chat_group_upserted') {
                applyGroupUpsertEvent(payload);
                return;
            }

            if (eventName === 'chat_group_archived' || payloadType === 'dispatch_chat_group_archived') {
                applyGroupArchivedEvent(payload);
                return;
            }

            if (eventName === 'chat_read_synced' || payloadType === 'dispatch_chat_read_synced') {
                applyReadSyncedEvent(payload);
            }
        }

        function disconnectStream(clearReconnect) {
            // Unregister SSEHub listeners
            if (state._sseHubHandlers) {
                Object.keys(state._sseHubHandlers).forEach(function (eventName) {
                    var key = eventName === '_message' ? 'message' : eventName;
                    SSEHub.off(key, state._sseHubHandlers[eventName]);
                });
                state._sseHubHandlers = null;
            }
            state.stream = null;

            if (clearReconnect !== false && state.reconnectTimer) {
                global.clearTimeout(state.reconnectTimer);
                state.reconnectTimer = null;
            }
        }

        function scheduleReconnect() {
            // No-op: SSEHub handles reconnection globally
        }

        function connectStream() {
            if (!state.sessionActive || !state.enabled) {
                return;
            }

            disconnectStream(false);

            var parsePayload = function (raw) {
                if (!raw) { return null; }
                if (typeof raw === 'object') { return raw; }
                try { return JSON.parse(raw); } catch (_e) { return null; }
            };

            var makeHandler = function (eventName) {
                return function (event) {
                    var payload = parsePayload(event && event.data);
                    if (!payload) { return; }
                    handlePayload(payload, eventName);
                };
            };

            // Register named event listeners on the shared SSEHub
            var chatEvents = ['dispatch_chat_initial', 'chat_message', 'chat_group_upserted', 'chat_group_archived', 'chat_read_synced'];
            state._sseHubHandlers = {};
            chatEvents.forEach(function (eventName) {
                var handler = makeHandler(eventName);
                state._sseHubHandlers[eventName] = handler;
                SSEHub.on(eventName, handler);
            });

            // Also listen on generic message for backward compat
            var genericHandler = function (event) {
                var payload = parsePayload(event && event.data);
                if (!payload) { return; }
                handlePayload(payload, '');
            };
            state._sseHubHandlers['_message'] = genericHandler;
            SSEHub.on('message', genericHandler);

            state.stream = { _sseHub: true }; // sentinel to indicate connected
        }

        const api = {
            initialize,
            open,
            close,
            destroy,
            focusInput,
            setEntryMeta,
        };

        return api;
    }

    global.DispatchChatPanel = {
        create,
    };
})(window);

