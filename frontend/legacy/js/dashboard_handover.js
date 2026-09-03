const DashboardHandover = (function () {
    'use strict';

    const API_BASE = `${window.location.origin}/api/v2`;
    const WEB_CONTEXT_TYPE = 'web_client';
    const WEB_CLIENT_STORAGE_KEY = 'fm_web_client_id';
    const STATUS_LABELS = { draft: '草稿', pending: '待签收', sign_off: '待整单签收', completed: '已完成' };
    const RISK_LABELS = { low: '低风险', medium: '中风险', high: '高风险', critical: '关键风险' };
    const ITEM_TYPE_LABELS = { pending_task: '待办任务', open_anomaly: '未关闭异常', risk_note: '风险备注', other: '其他' };
    const state = {
        currentUser: null,
        currentUserId: null,
        webClientId: null,
        candidates: [],
        handovers: { incoming: [], outgoing: [] },
        stats: { incomingPending: 0, outgoingOpen: 0 },
        systemDraftPreview: { generated_item_count: 0, mandatory_count: 0, titles: [], items: [] },
        mounted: false,
        drawerOpen: false,
        loadPromise: null,
        onUserUpdated: null,
        cardElement: null,
        elements: {},
    };

    function hasPermission(user, permission) {
        return Boolean(user && Array.isArray(user.permissions) && user.permissions.includes(permission));
    }

    function canManage(user) {
        return Boolean(user && (user.is_admin || hasPermission(user, 'dispatch:manage')));
    }

    function canAccess(user) {
        return Boolean(user && (user.is_admin || hasPermission(user, 'dispatch:view') || hasPermission(user, 'dispatch:manage')));
    }

    function createIdentifier() {
        const timePart = Date.now().toString(36);
        if (window.crypto && typeof window.crypto.getRandomValues === 'function') {
            const bytes = new Uint8Array(8);
            window.crypto.getRandomValues(bytes);
            return `${timePart}-${Array.from(bytes, (value) => value.toString(16).padStart(2, '0')).join('')}`;
        }
        return `${timePart}-${Math.random().toString(36).slice(2, 18)}`;
    }

    function getPersistentStorage() {
        try {
            const probeKey = `${WEB_CLIENT_STORAGE_KEY}::probe`;
            localStorage.setItem(probeKey, '1');
            localStorage.removeItem(probeKey);
            return localStorage;
        } catch (_error) {
            try {
                const probeKey = `${WEB_CLIENT_STORAGE_KEY}::probe`;
                sessionStorage.setItem(probeKey, '1');
                sessionStorage.removeItem(probeKey);
                return sessionStorage;
            } catch (_innerError) {
                return null;
            }
        }
    }

    function getWebClientId() {
        if (state.webClientId) {
            return state.webClientId;
        }

        const storage = getPersistentStorage();
        const storedValue = storage ? storage.getItem(WEB_CLIENT_STORAGE_KEY) : null;
        state.webClientId = storedValue || createIdentifier();
        if (storage && !storedValue) {
            storage.setItem(WEB_CLIENT_STORAGE_KEY, state.webClientId);
        }
        return state.webClientId;
    }

    function getOperatorContextHeaders() {
        return {
            'X-Operator-Context-Type': WEB_CONTEXT_TYPE,
            'X-Operator-Context-Id': getWebClientId(),
        };
    }

    function escapeHtml(value) {
        return String(value ?? '')
            .replaceAll('&', '&amp;')
            .replaceAll('<', '&lt;')
            .replaceAll('>', '&gt;')
            .replaceAll('"', '&quot;')
            .replaceAll("'", '&#39;');
    }

    function formatDate(value) {
        if (!value) {
            return '未设置';
        }
        const dateValue = new Date(value);
        return Number.isNaN(dateValue.getTime()) ? String(value) : dateValue.toLocaleDateString('zh-CN');
    }

    function formatDateTime(value) {
        if (!value) {
            return '未记录';
        }
        const dateValue = new Date(value);
        return Number.isNaN(dateValue.getTime()) ? String(value) : dateValue.toLocaleString('zh-CN', { hour12: false });
    }

    function formatShiftCode(value) {
        return String(value || '').trim() || '未命名班次';
    }

    function getStatusLabel(status) {
        return STATUS_LABELS[String(status || '').trim()] || '未知状态';
    }

    function getRiskLabel(riskLevel) {
        return RISK_LABELS[String(riskLevel || '').trim()] || '未设风险';
    }

    function getItemTypeLabel(itemType) {
        return ITEM_TYPE_LABELS[String(itemType || '').trim()] || '其他';
    }

    function getCurrentUserId(user) {
        return user && user.id ? String(user.id) : null;
    }

    function getCurrentOperatorLabel(user) {
        return user ? (user.effective_operator_label || user.username || '未命名') : '未登录';
    }

    function getAccountLabel(user) {
        return user ? `岗位账号：${user.username} · web_client：${getWebClientId().slice(0, 8)}` : '';
    }

    async function parseResponse(response) {
        let payload = null;
        const contentType = response.headers.get('content-type') || '';
        if (contentType.includes('application/json')) {
            payload = await response.json();
        }
        if (!response.ok) {
            throw new Error((payload && (payload.detail || payload.message || payload.error)) || `请求失败（${response.status}）`);
        }
        return payload;
    }

    async function request(path, options = {}) {
        const response = await Auth.fetch(`${API_BASE}${path}`, {
            ...options,
            headers: {
                ...(options.headers || {}),
                ...getOperatorContextHeaders(),
            },
        });
        return parseResponse(response);
    }

    function showMessage(message, type = 'info') {
        const element = state.elements.message;
        if (!element) {
            return;
        }
        element.textContent = message;
        element.classList.add('visible');
        element.classList.remove('info', 'success', 'error');
        element.classList.add(type);
    }

    function hideMessage() {
        const element = state.elements.message;
        if (!element) {
            return;
        }
        element.textContent = '';
        element.classList.remove('visible', 'info', 'success', 'error');
    }

    function createSectionSkeleton() {
        return `
            <div class="dashboard-handover-overlay" data-role="overlay"></div>
            <aside class="dashboard-handover-drawer" aria-hidden="true" aria-label="Dashboard 交接班抽屉">
                <div class="dashboard-handover-drawer-header">
                    <div>
                        <h2 class="dashboard-handover-drawer-title">交接班</h2>
                        <p class="dashboard-handover-drawer-subtitle">岗位账号负责认证与权限，当前浏览器独立保存当前值班人姓名并用于留痕。</p>
                    </div>
                    <button type="button" class="dashboard-handover-close-btn" data-action="close" aria-label="关闭交接班抽屉">×</button>
                </div>
                <div class="dashboard-handover-body">
                    <div class="dashboard-handover-message" data-role="message"></div>
                    <section class="dashboard-handover-section">
                        <div class="dashboard-handover-section-header">
                            <div>
                                <h3 class="dashboard-handover-section-title">当前值班人</h3>
                                <div class="dashboard-handover-note">仅影响当前浏览器的 web_client 上下文，不会覆盖其他终端。</div>
                            </div>
                            <button type="button" class="dashboard-handover-button-secondary dashboard-handover-mini-btn" data-action="refresh-all">刷新</button>
                        </div>
                        <div class="dashboard-handover-operator-panel">
                            <div>
                                <div class="dashboard-handover-operator-label" data-role="operator-label">加载中...</div>
                                <div class="dashboard-handover-account-label" data-role="account-label"></div>
                            </div>
                        </div>
                        <form data-role="operator-form">
                            <div class="dashboard-handover-form-grid single">
                                <div class="dashboard-handover-field">
                                    <label for="dashboardHandoverOperatorName">当前值班人姓名</label>
                                    <input id="dashboardHandoverOperatorName" name="operator_name" maxlength="100" autocomplete="off" placeholder="例如：王五">
                                </div>
                            </div>
                            <div class="dashboard-handover-actions" style="margin-top: 12px;">
                                <button type="submit" class="dashboard-handover-button">切换当前值班人</button>
                                <button type="button" class="dashboard-handover-button-secondary" data-action="reset-operator">恢复账号默认姓名</button>
                            </div>
                        </form>
                    </section>
                    <section class="dashboard-handover-section">
                        <div class="dashboard-handover-section-header">
                            <div>
                                <h3 class="dashboard-handover-section-title">待我签收</h3>
                                <div class="dashboard-handover-note">条目确认完成后，可在此整单签收。</div>
                            </div>
                            <div class="dashboard-handover-pill dashboard-handover-status-pending" data-role="pending-count">0</div>
                        </div>
                        <div class="dashboard-handover-list" data-role="pending-list"></div>
                    </section>
                    <section class="dashboard-handover-section">
                        <div class="dashboard-handover-section-header">
                            <div>
                                <h3 class="dashboard-handover-section-title">我发起的交接</h3>
                                <div class="dashboard-handover-note">草稿可提交，已完成记录保留快照展示。</div>
                            </div>
                            <div class="dashboard-handover-pill dashboard-handover-status-draft" data-role="outgoing-count">0</div>
                        </div>
                        <div class="dashboard-handover-list" data-role="outgoing-list"></div>
                    </section>
                    <section class="dashboard-handover-section">
                        <div class="dashboard-handover-section-header">
                            <div>
                                <h3 class="dashboard-handover-section-title">发起新交接</h3>
                                <div class="dashboard-handover-note">先创建草稿，再在“我发起的交接”中提交交接。</div>
                            </div>
                        </div>
                        <div class="dashboard-handover-guard" data-role="create-guard" hidden>当前账号没有发起交接权限，需要 dispatch:manage。</div>
                        <form data-role="create-form">
                            <div class="dashboard-handover-form-grid">
                                <div class="dashboard-handover-field"><label for="dashboardHandoverShiftDate">班次日期</label><input id="dashboardHandoverShiftDate" name="shift_date" type="date" required></div>
                                <div class="dashboard-handover-field"><label for="dashboardHandoverShiftCode">班次代码</label><input id="dashboardHandoverShiftCode" name="shift_code" maxlength="32" placeholder="例如：DAY / NIGHT" required></div>
                                <div class="dashboard-handover-field"><label for="dashboardHandoverCandidate">接班岗位账号</label><select id="dashboardHandoverCandidate" name="to_user_id" required></select></div>
                                <div class="dashboard-handover-field"><label for="dashboardHandoverRisk">风险等级</label><select id="dashboardHandoverRisk" name="risk_level"><option value="low">低风险</option><option value="medium" selected>中风险</option><option value="high">高风险</option><option value="critical">关键风险</option></select></div>
                                <div class="dashboard-handover-field full"><label for="dashboardHandoverSummary">风险说明</label><textarea id="dashboardHandoverSummary" name="summary" maxlength="200" placeholder="仅补充系统草稿未覆盖的风险说明，限 200 字。"></textarea></div>
                            </div>
                            <div class="dashboard-handover-section-header" style="margin-top: 18px;">
                                <div>
                                    <h4 class="dashboard-handover-section-title" style="font-size: 15px;">系统草稿</h4>
                                    <div class="dashboard-handover-note">系统会先汇总未完工工单、未关闭异常、补录待办和高风险通知，人工只补增量。</div>
                                </div>
                                <div class="dashboard-handover-pill dashboard-handover-status-draft" data-role="draft-count">0</div>
                            </div>
                            <div class="dashboard-handover-list" data-role="draft-list"></div>
                            <div class="dashboard-handover-section-header" style="margin-top: 18px;">
                                <div><h4 class="dashboard-handover-section-title" style="font-size: 15px;">人工补充条目</h4><div class="dashboard-handover-note">只补系统草稿未覆盖的额外风险或说明；空白条目会在提交前忽略。</div></div>
                                <button type="button" class="dashboard-handover-button-secondary dashboard-handover-mini-btn" data-action="add-item">新增条目</button>
                            </div>
                            <div data-role="item-list"></div>
                            <div class="dashboard-handover-actions" style="margin-top: 12px;">
                                <button type="submit" class="dashboard-handover-button">创建交接草稿</button>
                                <button type="button" class="dashboard-handover-button-ghost" data-action="reset-form">重置表单</button>
                            </div>
                        </form>
                    </section>
                </div>
            </aside>
        `;
    }

    function ensureMounted() {
        if (state.mounted) {
            return;
        }

        const root = document.createElement('div');
        root.id = 'dashboardHandoverRoot';
        root.innerHTML = createSectionSkeleton();
        document.body.appendChild(root);

        state.elements.root = root;
        state.elements.overlay = root.querySelector('[data-role="overlay"]');
        state.elements.drawer = root.querySelector('.dashboard-handover-drawer');
        state.elements.message = root.querySelector('[data-role="message"]');
        state.elements.operatorForm = root.querySelector('[data-role="operator-form"]');
        state.elements.operatorInput = root.querySelector('#dashboardHandoverOperatorName');
        state.elements.operatorLabel = root.querySelector('[data-role="operator-label"]');
        state.elements.accountLabel = root.querySelector('[data-role="account-label"]');
        state.elements.pendingList = root.querySelector('[data-role="pending-list"]');
        state.elements.outgoingList = root.querySelector('[data-role="outgoing-list"]');
        state.elements.pendingCount = root.querySelector('[data-role="pending-count"]');
        state.elements.outgoingCount = root.querySelector('[data-role="outgoing-count"]');
        state.elements.createGuard = root.querySelector('[data-role="create-guard"]');
        state.elements.createForm = root.querySelector('[data-role="create-form"]');
        state.elements.candidateSelect = root.querySelector('#dashboardHandoverCandidate');
        state.elements.draftCount = root.querySelector('[data-role="draft-count"]');
        state.elements.draftList = root.querySelector('[data-role="draft-list"]');
        state.elements.itemList = root.querySelector('[data-role="item-list"]');
        state.elements.shiftDateInput = root.querySelector('#dashboardHandoverShiftDate');
        state.elements.shiftCodeInput = root.querySelector('#dashboardHandoverShiftCode');
        state.elements.summaryInput = root.querySelector('#dashboardHandoverSummary');
        state.elements.riskSelect = root.querySelector('#dashboardHandoverRisk');
        state.elements.overlay.addEventListener('click', closeDrawer);
        state.elements.root.addEventListener('click', handleRootClick);
        state.elements.operatorForm.addEventListener('submit', handleOperatorSubmit);
        state.elements.createForm.addEventListener('submit', handleCreateSubmit);
        state.elements.candidateSelect.addEventListener('change', () => {
            void loadSystemDraftPreview();
        });
        document.addEventListener('keydown', handleDocumentKeyDown);
        state.mounted = true;
    }

    function handleDocumentKeyDown(event) {
        if (event.key === 'Escape' && state.drawerOpen) {
            closeDrawer();
        }
    }

    function openDrawer() {
        ensureMounted();
        resetCreateForm();
        renderOperatorPanel();
        renderPendingList();
        renderOutgoingList();
        renderCreateSection();
        state.drawerOpen = true;
        document.body.classList.add('dashboard-handover-open');
        state.elements.drawer.setAttribute('aria-hidden', 'false');
        loadAllData({ showLoadingMessage: true });
    }

    function closeDrawer() {
        if (!state.mounted) {
            return;
        }
        state.drawerOpen = false;
        document.body.classList.remove('dashboard-handover-open');
        state.elements.drawer.setAttribute('aria-hidden', 'true');
        hideMessage();
    }

    function createCard(user) {
        ensureMounted();
        state.currentUser = user;
        state.currentUserId = getCurrentUserId(user);
        const button = document.createElement('button');
        button.type = 'button';
        button.className = 'bento-card theme-purple card-wide dashboard-handover-card';
        button.innerHTML = `
            <div class="card-icon-container"><img src="/frontend/icons/refresh.svg" class="svg-icon"></div>
            <div class="card-content">
                <div class="card-title">交接班</div>
                <div class="card-desc">在 Dashboard 内完成交班、条目确认、整单签收与当前值班人切换。</div>
                <div class="dashboard-handover-stats">
                    <div class="dashboard-handover-stat"><span class="dashboard-handover-stat-label">待我签收</span><span class="dashboard-handover-stat-value" data-role="card-pending-count">0</span></div>
                    <div class="dashboard-handover-stat"><span class="dashboard-handover-stat-label">我发起未完成</span><span class="dashboard-handover-stat-value" data-role="card-outgoing-count">0</span></div>
                    <div class="dashboard-handover-stat dashboard-handover-stat--operator"><span class="dashboard-handover-stat-label">当前值班人</span><span class="dashboard-handover-stat-value dashboard-handover-stat-value--operator" data-role="card-operator-short">--</span></div>
                </div>
                <div class="dashboard-handover-operator-caption">当前身份<strong data-role="card-operator-label">${escapeHtml(getCurrentOperatorLabel(user))}</strong></div>
            </div>
            <div class="card-action">打开交接班抽屉 <span>→</span></div>
        `;
        button.addEventListener('click', openDrawer);
        state.cardElement = button;
        renderCardState();
        return button;
    }

    function renderCardState() {
        if (!state.cardElement) {
            return;
        }
        const pendingCount = state.cardElement.querySelector('[data-role="card-pending-count"]');
        const outgoingCount = state.cardElement.querySelector('[data-role="card-outgoing-count"]');
        const operatorShort = state.cardElement.querySelector('[data-role="card-operator-short"]');
        const operatorLabel = state.cardElement.querySelector('[data-role="card-operator-label"]');
        if (pendingCount) {
            pendingCount.textContent = String(state.stats.incomingPending);
        }
        if (outgoingCount) {
            outgoingCount.textContent = String(state.stats.outgoingOpen);
        }
        if (operatorShort) {
            operatorShort.textContent = getCurrentOperatorLabel(state.currentUser) || '--';
        }
        if (operatorLabel) {
            operatorLabel.textContent = getCurrentOperatorLabel(state.currentUser);
        }
    }

    function setCurrentUser(user) {
        state.currentUser = user;
        state.currentUserId = getCurrentUserId(user);
        renderCardState();
        renderOperatorPanel();
    }

    function afterRenderModules(user, options = {}) {
        ensureMounted();
        state.currentUser = user;
        state.currentUserId = getCurrentUserId(user);
        state.onUserUpdated = typeof options.onUserUpdated === 'function' ? options.onUserUpdated : null;
        resetCreateForm();
        renderCandidateOptions();
        renderOperatorPanel();
        renderCreateSection();
        loadAllData();
    }

    async function loadAllData(options = {}) {
        if (!state.currentUser || !canAccess(state.currentUser)) {
            return;
        }
        if (state.loadPromise) {
            return state.loadPromise;
        }
        if (options.showLoadingMessage) {
            showMessage('正在同步交接班数据...', 'info');
        }
        state.loadPromise = (async () => {
            try {
                const requests = [loadHandovers()];
                if (canManage(state.currentUser)) {
                    requests.push(loadCandidates());
                    requests.push(loadSystemDraftPreview());
                } else {
                    state.candidates = [];
                }
                await Promise.all(requests);
                computeStats();
                renderCandidateOptions();
                renderPendingList();
                renderOutgoingList();
                renderCreateSection();
                renderOperatorPanel();
                renderCardState();
                if (options.showLoadingMessage) {
                    hideMessage();
                }
            } catch (error) {
                console.error('[DashboardHandover] load failed', error);
                showMessage(error.message || '交接班数据加载失败', 'error');
            } finally {
                state.loadPromise = null;
            }
        })();
        return state.loadPromise;
    }

    async function loadHandovers() {
        if (!state.currentUserId) {
            state.handovers.incoming = [];
            state.handovers.outgoing = [];
            computeStats();
            return;
        }
        const [incoming, outgoing] = await Promise.all([
            request(`/shift-handovers?to_user_id=${encodeURIComponent(state.currentUserId)}&limit=20&offset=0`, { method: 'GET' }),
            request(`/shift-handovers?from_user_id=${encodeURIComponent(state.currentUserId)}&limit=20&offset=0`, { method: 'GET' }),
        ]);
        state.handovers.incoming = Array.isArray(incoming) ? incoming : [];
        state.handovers.outgoing = Array.isArray(outgoing) ? outgoing : [];
        computeStats();
    }

    async function loadCandidates() {
        const payload = await request('/shift-handovers/candidates', { method: 'GET' });
        state.candidates = Array.isArray(payload) ? payload.slice().sort((left, right) => {
            const leftLabel = String(left.display_label || left.username || '');
            const rightLabel = String(right.display_label || right.username || '');
            return leftLabel.localeCompare(rightLabel, 'zh-CN');
        }) : [];
    }

    async function loadSystemDraftPreview() {
        if (!state.currentUserId || !canManage(state.currentUser)) {
            state.systemDraftPreview = { generated_item_count: 0, mandatory_count: 0, titles: [], items: [] };
            renderSystemDraftPreview();
            return;
        }
        const candidateUserId = String(state.elements.candidateSelect?.value || '').trim();
        const query = candidateUserId ? `?to_user_id=${encodeURIComponent(candidateUserId)}` : '';
        const payload = await request(`/shift-handovers/system-draft-preview${query}`, { method: 'GET' });
        state.systemDraftPreview = payload && typeof payload === 'object'
            ? payload
            : { generated_item_count: 0, mandatory_count: 0, titles: [], items: [] };
        renderSystemDraftPreview();
    }

    function computeStats() {
        state.stats.incomingPending = state.handovers.incoming.filter((item) => item.status !== 'completed').length;
        state.stats.outgoingOpen = state.handovers.outgoing.filter((item) => item.status !== 'completed').length;
    }

    function renderOperatorPanel() {
        if (!state.mounted) {
            return;
        }
        state.elements.operatorLabel.textContent = getCurrentOperatorLabel(state.currentUser);
        state.elements.accountLabel.textContent = getAccountLabel(state.currentUser);
        state.elements.operatorInput.value = state.currentUser ? (state.currentUser.effective_operator_name || state.currentUser.display_name || '') : '';
    }

    function renderCandidateOptions() {
        if (!state.mounted) {
            return;
        }
        state.elements.candidateSelect.innerHTML = state.candidates.length
            ? ['<option value="">请选择接班岗位账号</option>', ...state.candidates.map((candidate) => `<option value="${escapeHtml(candidate.user_id)}">${escapeHtml(candidate.display_label || candidate.username)}</option>`)].join('')
            : '<option value="">暂无可选岗位账号</option>';
    }

    function renderCreateSection() {
        if (!state.mounted) {
            return;
        }
        const available = canManage(state.currentUser);
        state.elements.createGuard.hidden = available;
        state.elements.createForm.style.display = available ? '' : 'none';
        renderSystemDraftPreview();
    }

    function renderSystemDraftPreview() {
        if (!state.mounted || !state.elements.draftCount || !state.elements.draftList) {
            return;
        }
        const preview = state.systemDraftPreview && typeof state.systemDraftPreview === 'object'
            ? state.systemDraftPreview
            : { generated_item_count: 0, mandatory_count: 0, titles: [], items: [] };
        const items = Array.isArray(preview.items) ? preview.items : [];
        state.elements.draftCount.textContent = String(Number(preview.generated_item_count || 0));
        state.elements.draftList.innerHTML = items.length
            ? items.map((item) => `
                <div class="dashboard-handover-panel" style="margin-bottom:10px;">
                    <div class="dashboard-handover-panel-meta">
                        <span class="dashboard-handover-pill dashboard-handover-status-pending">${escapeHtml(getItemTypeLabel(item.item_type))}</span>
                        <span class="dashboard-handover-pill ${item.is_mandatory === false ? 'dashboard-handover-status-draft' : 'dashboard-handover-risk-high'}">${item.is_mandatory === false ? '选确认' : '必确认'}</span>
                    </div>
                    <div class="dashboard-handover-panel-text">
                        ${escapeHtml(item.title || '未命名系统草稿')}
                        ${item.detail ? `<br>${escapeHtml(item.detail)}` : ''}
                    </div>
                </div>
            `).join('')
            : '<div class="dashboard-handover-empty">选择接班岗位后，系统会在这里自动生成交接草稿。</div>';
    }

    function renderPendingList() {
        if (!state.mounted) {
            return;
        }
        state.elements.pendingCount.textContent = String(state.stats.incomingPending);
        const items = state.handovers.incoming.filter((handover) => handover.status !== 'completed');
        state.elements.pendingList.innerHTML = items.length
            ? items.map((handover) => renderHandoverPanel(handover, 'incoming')).join('')
            : '<div class="dashboard-handover-empty">当前没有待你签收的交接班。</div>';
    }

    function renderOutgoingList() {
        if (!state.mounted) {
            return;
        }
        state.elements.outgoingCount.textContent = String(state.stats.outgoingOpen);
        state.elements.outgoingList.innerHTML = state.handovers.outgoing.length
            ? state.handovers.outgoing.map((handover) => renderHandoverPanel(handover, 'outgoing')).join('')
            : '<div class="dashboard-handover-empty">你尚未发起交接班，可在下方创建。</div>';
    }

    function renderHandoverPanel(handover, panelType) {
        const fromLabel = escapeHtml(handover.from_operator_label || handover.from_operator_name || handover.from_user_id || '未记录');
        const toLabel = escapeHtml(handover.to_operator_label || handover.to_operator_name || handover.to_user_id || '未记录');
        const statusClass = `dashboard-handover-status-${escapeHtml(handover.status || 'draft')}`;
        const riskClass = `dashboard-handover-risk-${escapeHtml(handover.risk_level || 'medium')}`;
        return `
            <div class="dashboard-handover-panel">
                <div class="dashboard-handover-panel-header">
                    <div class="dashboard-handover-panel-title">${escapeHtml(formatShiftCode(handover.shift_code))} · ${escapeHtml(formatDate(handover.shift_date))}</div>
                    <div class="dashboard-handover-actions">${renderPanelActions(handover, panelType)}</div>
                </div>
                <div class="dashboard-handover-panel-meta">
                    <span class="dashboard-handover-pill ${statusClass}">${escapeHtml(getStatusLabel(handover.status))}</span>
                    <span class="dashboard-handover-pill ${riskClass}">${escapeHtml(getRiskLabel(handover.risk_level))}</span>
                </div>
                <div class="dashboard-handover-panel-text">
                    交班：${fromLabel}<br>
                    接班：${toLabel}<br>
                    提交时间：${escapeHtml(formatDateTime(handover.submitted_at || handover.created_at))}<br>
                    ${escapeHtml(handover.summary || '暂无摘要')}
                </div>
                ${renderItems(handover, panelType)}
            </div>
        `;
    }

    function renderPanelActions(handover, panelType) {
        const actions = [];
        if (panelType === 'outgoing' && handover.status === 'draft' && canManage(state.currentUser)) {
            actions.push(`<button type="button" class="dashboard-handover-button dashboard-handover-mini-btn" data-action="submit-handover" data-handover-id="${escapeHtml(handover.handover_id)}">提交交接</button>`);
        }
        if (panelType === 'incoming' && canCompleteHandover(handover)) {
            actions.push(`<button type="button" class="dashboard-handover-button dashboard-handover-mini-btn" data-action="complete-handover" data-handover-id="${escapeHtml(handover.handover_id)}">整单签收</button>`);
        }
        return actions.join('');
    }

    function canCompleteHandover(handover) {
        return Boolean(handover && ['pending', 'sign_off'].includes(String(handover.status || '')) && (handover.items || []).every((item) => !item.is_mandatory || item.acknowledged));
    }

    function renderItems(handover, panelType) {
        const items = Array.isArray(handover.items) ? handover.items : [];
        if (!items.length) {
            return '<div class="dashboard-handover-empty" style="margin-top: 14px;">暂无交接条目。</div>';
        }
        const canToggle = panelType === 'incoming' && ['pending', 'sign_off'].includes(String(handover.status || ''));
        return `
            <ul class="dashboard-handover-items">
                ${items.map((item) => `
                    <li class="dashboard-handover-item">
                        <div class="dashboard-handover-item-head">
                            <div>
                                <div class="dashboard-handover-item-title">${escapeHtml(item.title)}</div>
                                <div class="dashboard-handover-item-detail">${escapeHtml(item.detail || '无补充说明')}</div>
                            </div>
                            ${canToggle ? `<button type="button" class="dashboard-handover-button-secondary dashboard-handover-mini-btn" data-action="toggle-item-ack" data-handover-id="${escapeHtml(handover.handover_id)}" data-item-id="${escapeHtml(item.item_id)}" data-next-ack="${item.acknowledged ? 'false' : 'true'}">${item.acknowledged ? '取消确认' : '确认条目'}</button>` : ''}
                        </div>
                        <div class="dashboard-handover-item-meta">${escapeHtml(getItemTypeLabel(item.item_type))} · ${item.is_mandatory ? '必确认' : '选确认'} · ${item.acknowledged ? `已确认（${escapeHtml(formatDateTime(item.acknowledged_at))}）` : '未确认'}</div>
                    </li>
                `).join('')}
            </ul>
        `;
    }

    function addItemRow(initialValue = {}) {
        ensureMounted();
        const row = document.createElement('div');
        row.className = 'dashboard-handover-item-row';
        row.dataset.itemKey = createIdentifier();
        row.innerHTML = `
            <div class="dashboard-handover-item-row-header">
                <div class="dashboard-handover-item-row-title">交接条目</div>
                <button type="button" class="dashboard-handover-button-danger dashboard-handover-mini-btn" data-action="remove-item">删除</button>
            </div>
            <div class="dashboard-handover-form-grid">
                <div class="dashboard-handover-field"><label>条目类型</label><select name="item_type"><option value="pending_task" ${initialValue.item_type === 'pending_task' ? 'selected' : ''}>待办任务</option><option value="open_anomaly" ${initialValue.item_type === 'open_anomaly' ? 'selected' : ''}>未关闭异常</option><option value="risk_note" ${initialValue.item_type === 'risk_note' ? 'selected' : ''}>风险备注</option><option value="other" ${!initialValue.item_type || initialValue.item_type === 'other' ? 'selected' : ''}>其他</option></select></div>
                <div class="dashboard-handover-field"><label>标题</label><input name="title" maxlength="255" autocomplete="off" value="${escapeHtml(initialValue.title || '')}" placeholder="例如：关闭 A-17 异常"></div>
                <div class="dashboard-handover-field"><label>是否必确认</label><select name="is_mandatory"><option value="true" ${initialValue.is_mandatory === false ? '' : 'selected'}>必确认</option><option value="false" ${initialValue.is_mandatory === false ? 'selected' : ''}>选确认</option></select></div>
                <div class="dashboard-handover-field full"><label>详情</label><textarea name="detail" maxlength="2000" placeholder="填写上下文、风险或补充说明。">${escapeHtml(initialValue.detail || '')}</textarea></div>
            </div>
        `;
        state.elements.itemList.appendChild(row);
    }

    function resetCreateForm() {
        if (!state.mounted) {
            return;
        }
        state.elements.shiftDateInput.value = new Date().toISOString().slice(0, 10);
        state.elements.shiftCodeInput.value = '';
        state.elements.summaryInput.value = '';
        state.elements.riskSelect.value = 'medium';
        state.elements.itemList.innerHTML = '';
        renderCandidateOptions();
        renderSystemDraftPreview();
    }

    function collectCreatePayload() {
        const shiftDate = state.elements.shiftDateInput.value;
        const shiftCode = state.elements.shiftCodeInput.value.trim();
        const toUserId = state.elements.candidateSelect.value;
        if (!shiftDate) {
            throw new Error('请选择班次日期');
        }
        if (!shiftCode) {
            throw new Error('请填写班次代码');
        }
        if (!toUserId) {
            throw new Error('请选择接班岗位账号');
        }
        if (!state.currentUserId) {
            throw new Error('当前用户信息未就绪');
        }
        const itemRows = Array.from(state.elements.itemList.querySelectorAll('[data-item-key]'));
        const items = itemRows.reduce((result, row, index) => {
            const title = row.querySelector('[name="title"]').value.trim();
            const detail = row.querySelector('[name="detail"]').value.trim();
            if (!title && !detail) {
                return result;
            }
            if (!title) {
                throw new Error(`第 ${index + 1} 条交接条目缺少标题`);
            }
            result.push({
                item_type: row.querySelector('[name="item_type"]').value,
                title,
                detail: detail || null,
                is_mandatory: row.querySelector('[name="is_mandatory"]').value === 'true',
            });
            return result;
        }, []);
        return {
            shift_date: shiftDate,
            shift_code: shiftCode,
            from_user_id: state.currentUserId,
            to_user_id: toUserId,
            summary: state.elements.summaryInput.value.trim() || null,
            risk_level: state.elements.riskSelect.value,
            items,
        };
    }

    async function handleOperatorSubmit(event) {
        event.preventDefault();
        if (!state.currentUser) {
            return;
        }
        try {
            const user = await request('/auth/me/operator-context', {
                method: 'PUT',
                body: JSON.stringify({ operator_name: state.elements.operatorInput.value.trim() || null }),
            });
            setCurrentUser(user);
            if (state.onUserUpdated) {
                state.onUserUpdated(user);
            }
            showMessage('当前值班人已更新。', 'success');
        } catch (error) {
            console.error('[DashboardHandover] operator update failed', error);
            showMessage(error.message || '当前值班人更新失败', 'error');
        }
    }

    async function resetOperatorContext() {
        try {
            const user = await request('/auth/me/operator-context', {
                method: 'PUT',
                body: JSON.stringify({ operator_name: null }),
            });
            setCurrentUser(user);
            if (state.onUserUpdated) {
                state.onUserUpdated(user);
            }
            showMessage('已恢复账号默认姓名。', 'success');
        } catch (error) {
            console.error('[DashboardHandover] operator reset failed', error);
            showMessage(error.message || '恢复默认姓名失败', 'error');
        }
    }

    async function handleCreateSubmit(event) {
        event.preventDefault();
        if (!canManage(state.currentUser)) {
            showMessage('当前账号没有发起交接权限。', 'error');
            return;
        }
        try {
            await request('/shift-handovers', {
                method: 'POST',
                body: JSON.stringify(collectCreatePayload()),
            });
            showMessage('交接草稿已创建，可在“我发起的交接”中提交。', 'success');
            resetCreateForm();
            await loadHandovers();
            renderPendingList();
            renderOutgoingList();
            renderCardState();
        } catch (error) {
            console.error('[DashboardHandover] create failed', error);
            showMessage(error.message || '交接草稿创建失败', 'error');
        }
    }

    async function submitHandover(handoverId) {
        await request(`/shift-handovers/${encodeURIComponent(handoverId)}/submit`, { method: 'POST' });
        showMessage('交接已提交，等待接班方签收。', 'success');
        await loadHandovers();
        renderPendingList();
        renderOutgoingList();
        renderCardState();
    }

    async function toggleItemAck(handoverId, itemId, nextAck) {
        await request(`/shift-handovers/${encodeURIComponent(handoverId)}/items/${encodeURIComponent(itemId)}/ack`, {
            method: 'POST',
            body: JSON.stringify({ acknowledged: nextAck }),
        });
        showMessage(nextAck ? '条目已确认。' : '条目确认已撤销。', 'success');
        await loadHandovers();
        renderPendingList();
        renderOutgoingList();
        renderCardState();
    }

    async function completeHandover(handoverId) {
        await request(`/shift-handovers/${encodeURIComponent(handoverId)}/ack`, { method: 'POST' });
        showMessage('交接班已整单签收。', 'success');
        await loadHandovers();
        renderPendingList();
        renderOutgoingList();
        renderCardState();
    }

    async function handleRootClick(event) {
        const actionTarget = event.target.closest('[data-action]');
        if (!actionTarget) {
            return;
        }

        const action = actionTarget.dataset.action;
        if (action === 'close') {
            closeDrawer();
            return;
        }
        if (action === 'refresh-all') {
            await loadAllData({ showLoadingMessage: true });
            return;
        }
        if (action === 'reset-operator') {
            await resetOperatorContext();
            return;
        }
        if (action === 'add-item') {
            addItemRow();
            return;
        }
        if (action === 'remove-item') {
            const row = actionTarget.closest('[data-item-key]');
            if (row) {
                row.remove();
            }
            if (!state.elements.itemList.children.length) {
                addItemRow();
            }
            return;
        }
        if (action === 'reset-form') {
            resetCreateForm();
            hideMessage();
            return;
        }

        try {
            if (action === 'submit-handover') {
                await submitHandover(actionTarget.dataset.handoverId);
                return;
            }
            if (action === 'toggle-item-ack') {
                await toggleItemAck(actionTarget.dataset.handoverId, actionTarget.dataset.itemId, actionTarget.dataset.nextAck === 'true');
                return;
            }
            if (action === 'complete-handover') {
                await completeHandover(actionTarget.dataset.handoverId);
            }
        } catch (error) {
            console.error('[DashboardHandover] action failed', error);
            showMessage(error.message || '交接班操作失败', 'error');
        }
    }

    return {
        ensureMounted,
        getOperatorContextHeaders,
        canAccess,
        createCard,
        openDrawer,
        afterRenderModules,
        setCurrentUser,
    };
})();

if (typeof window !== 'undefined') {
    window.DashboardHandover = DashboardHandover;
}
