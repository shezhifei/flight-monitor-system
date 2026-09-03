let activeModal = null;

let activeModalRestoreTarget = null;

let floatingBadgeLayoutObserver = null;

let floatingBadgeAIFloatObserver = null;

let dispatchNotifyModalState = {
    users: [],
    filteredUsers: [],
    selectedUserIds: new Set(),
    loadingUsers: false,
    sending: false,
    loadedOnce: false,
    receiptGroup: null,
    loadError: '',
    activeTab: 'send',
    pendingReceipts: [],
    pendingReceiptsLoading: false,
    pendingReceiptsLoaded: false,
    pendingReceiptsError: '',
    sentReceiptGroups: [],
    sentReceiptGroupsLoading: false,
    sentReceiptGroupsLoaded: false,
    sentReceiptGroupsError: '',
    selectedSentReceiptGroupId: '',
    sentReceiptGroupDetail: null,
    sentReceiptReminderTimers: new Map(),
    sentReceiptReminderQueue: [],
    sentReceiptReminderRetryTimers: new Map(),
    sentReceiptReminderAttemptCounts: new Map(),
    sentReceiptReminderPresentRetryTimer: null,
};

function escapeHtmlForRender(value) {
    if (typeof escapeHtml === 'function') {
        return escapeHtml(value);
    }
    return String(value ?? '')
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
}

function escapeAttributeForRender(value) {
    return escapeHtmlForRender(value);
}

function jsStringForInlineHandler(value) {
    return escapeAttributeForRender(JSON.stringify(String(value ?? '')));
}

function sanitizeCssToken(value, fallback = 'unknown') {
    const token = String(value ?? '')
        .trim()
        .toLowerCase()
        .replace(/[^a-z0-9_-]+/g, '-')
        .replace(/^-+|-+$/g, '')
        .slice(0, 48);
    return token || fallback;
}

function normalizeHexColor(value, fallback = '#6B7280') {
    const color = String(value ?? '').trim();
    return /^#[0-9a-fA-F]{3}([0-9a-fA-F]{3})?$/.test(color) ? color : fallback;
}

// ── Labels System ──
let _labelDefsCache = null;
let _labelDefsCacheLoading = false;

async function loadLabelDefinitions() {
    if (_labelDefsCache) return _labelDefsCache;
    if (_labelDefsCacheLoading) return {};
    _labelDefsCacheLoading = true;
    try {
        const resp = await (typeof Auth !== 'undefined' && Auth.fetch ? Auth.fetch('/api/v2/labels') : fetch('/api/v2/labels'));
        const json = await resp.json();
        if (json.success && Array.isArray(json.data)) {
            _labelDefsCache = {};
            json.data.forEach(d => { _labelDefsCache[d.code] = d; });
        }
    } catch (e) {
        console.warn('[labels] 加载标签定义失败:', e);
        _labelDefsCache = {};
    }
    _labelDefsCacheLoading = false;
    return _labelDefsCache || {};
}

function renderLabelPills(labels) {
    if (!Array.isArray(labels) || labels.length === 0) return '';
    const defs = _labelDefsCache || {};
    return labels.map(code => {
        const def = defs[code] || { name: code, color: '#6B7280' };
        const color = normalizeHexColor(def.color);
        const bg = color + '20';
        const border = color + '40';
        const name = escapeHtmlForRender(def.name || code || '');
        return `<span class="label-pill" style="background:${bg};color:${color};border-color:${border}" title="${escapeAttributeForRender(def.name || code || '')}">${name}</span>`;
    }).join('');
}

function renderFlightLabelsSection(flight) {
    const flightLabels = Array.isArray(flight.labels) ? flight.labels : [];
    const inboundLabels = Array.isArray(flight.inbound_leg?.labels) ? flight.inbound_leg.labels : [];
    const outboundLabels = Array.isArray(flight.outbound_leg?.labels) ? flight.outbound_leg.labels : [];
    if (flightLabels.length === 0 && inboundLabels.length === 0 && outboundLabels.length === 0) return '';
    let html = '<div class="detail-card labels-card"><div class="labels-section">';
    if (flightLabels.length > 0) {
        html += `<div class="labels-group"><span class="labels-group-title">航班标签</span>${renderLabelPills(flightLabels)}</div>`;
    }
    if (inboundLabels.length > 0) {
        html += `<div class="labels-group"><span class="labels-group-title">进港</span>${renderLabelPills(inboundLabels)}</div>`;
    }
    if (outboundLabels.length > 0) {
        html += `<div class="labels-group"><span class="labels-group-title">出港</span>${renderLabelPills(outboundLabels)}</div>`;
    }
    html += '</div></div>';
    return html;
}

function renderOriginBadge(originType) {
    const normalized = String(originType || 'manual').trim().toLowerCase();
    const label = normalized === 'workflow' ? '流程' : '人工';
    const background = normalized === 'workflow' ? 'rgba(79, 70, 229, 0.14)' : 'rgba(217, 119, 6, 0.14)';
    const color = normalized === 'workflow' ? '#4338ca' : '#b45309';
    return `<span style="display:inline-flex;align-items:center;padding:2px 8px;border-radius:999px;font-size:12px;font-weight:600;background:${background};color:${color};">${label}</span>`;
}

function renderReceiptSummary(summary) {
    const payload = summary && typeof summary === 'object' ? summary : {};
    return `
        <div style="display:flex;gap:8px;flex-wrap:wrap;margin-top:8px;">
            <span class="dispatch-online-user-chip">总数 ${Number(payload.total_count || 0)}</span>
            <span class="dispatch-online-user-chip">待确认 ${Number(payload.pending_count || 0)}</span>
            <span class="dispatch-online-user-chip">已确认 ${Number(payload.acknowledged_count || 0)}</span>
            <span class="dispatch-online-user-chip">已拒绝 ${Number(payload.rejected_count || 0)}</span>
        </div>
    `;
}

function getDispatchNotifyReceiptAccountName(item) {
    const candidates = [
        item?.recipient_username,
        item?.username,
        item?.account_name,
        item?.recipient_display_name,
        item?.display_name,
    ];
    const found = candidates
        .map((value) => String(value || '').trim())
        .find((value) => value);
    return found || String(item?.recipient_user_id || item?.user_id || '-');
}

function buildDispatchNotifyReceiptGroupHtml(payload, options = {}) {
    const {
        emptyMessage = '发送后将在这里显示逐人确认/拒绝情况。',
        heading = '回执情况',
    } = options;
    if (!payload || typeof payload !== 'object') {
        return `<div class="dispatch-notify-tip">${escapeHtml(emptyMessage)}</div>`;
    }
    const items = Array.isArray(payload.items) ? payload.items : [];
    const rows = items.map((item) => {
        const ackStatus = String(item?.ack_status || 'pending').trim();
        const ackText = ackStatus === 'acknowledged'
            ? '已确认'
            : (ackStatus === 'rejected' ? `已拒绝${item?.ack_note ? `：${escapeHtml(String(item.ack_note))}` : ''}` : '待确认');
        return `
            <div style="display:flex;justify-content:space-between;gap:12px;padding:6px 0;border-bottom:1px solid rgba(15,23,42,0.06);">
                <div>
                    <div style="font-weight:600;">${escapeHtml(getDispatchNotifyReceiptAccountName(item))}</div>
                    <div style="font-size:12px;color:#64748b;">${escapeHtml(String(item?.title || payload.title || '-'))}</div>
                    ${item?.ack_at ? `<div style="font-size:11px;color:#94a3b8;margin-top:2px;">${escapeHtml(formatDispatchNotifyDateTime(item.ack_at, ''))}</div>` : ''}
                </div>
                <div style="text-align:right;">
                    ${renderOriginBadge(item?.origin_type || payload.origin_type)}
                    <div style="margin-top:4px;font-size:12px;color:#334155;">${ackText}</div>
                </div>
            </div>
        `;
    }).join('');
    return `
        <div class="dispatch-notify-row" style="margin-top:12px;">
            <label>${escapeHtml(heading)}</label>
            <div>
                <div style="display:flex;align-items:center;gap:8px;flex-wrap:wrap;">
                    ${renderOriginBadge(payload.origin_type)}
                    <span class="dispatch-online-user-chip">批次 ${escapeHtml(String(payload.receipt_group_id || '-'))}</span>
                    <span class="dispatch-online-user-chip">级别 ${escapeHtml(String(payload.severity || 'info').toUpperCase())}</span>
                    ${payload.created_at ? `<span class="dispatch-online-user-chip">发送 ${escapeHtml(formatDispatchNotifyDateTime(payload.created_at, ''))}</span>` : ''}
                    ${payload.is_overdue ? '<span class="dispatch-online-user-chip" style="color:#b91c1c;background:rgba(254,226,226,0.92);border-color:rgba(239,68,68,0.24);">超时未齐</span>' : ''}
                </div>
                ${renderReceiptSummary(payload.summary)}
                <div style="margin-top:10px;max-height:220px;overflow:auto;">${rows || '<div class="dispatch-notify-tip">暂无回执明细。</div>'}</div>
            </div>
        </div>
    `;
}

function renderDispatchNotifyReceiptGroup() {
    const container = document.getElementById('dispatchNotifyReceiptGroup');
    if (!container) {
        return;
    }
    container.innerHTML = buildDispatchNotifyReceiptGroupHtml(dispatchNotifyModalState.receiptGroup, {
        emptyMessage: '发送后将在这里显示逐人确认/拒绝情况。',
        heading: '当前发送回执',
    });
}

function renderTableFlightNumberV2(flight) {
    const inboundFlightNo = getFlightNumberByLegV2(flight, 'inbound');
    const outboundFlightNo = getFlightNumberByLegV2(flight, 'outbound');

    const inboundText = inboundFlightNo || '-';
    const outboundText = outboundFlightNo || '-';
    const inboundClass = inboundFlightNo ? getFlightNumberTextClassV2(flight, 'inbound') : '';
    const outboundClass = outboundFlightNo ? getFlightNumberTextClassV2(flight, 'outbound') : '';

    const formatPart = (text, className) => {
        const safeText = escapeHtml(text);
        return className ? `<span class="${className}">${safeText}</span>` : safeText;
    };

    return `${formatPart(inboundText, inboundClass)}|${formatPart(outboundText, outboundClass)}`;
}

function buildDiagnosisResultText(result) {
    if (result === null || result === undefined) {
        return '';
    }
    if (typeof result === 'string') {
        return compactDiagnosisText(result, 360);
    }
    if (typeof result === 'number' || typeof result === 'boolean') {
        return String(result);
    }
    if (Array.isArray(result)) {
        const summary = result
            .slice(0, 4)
            .map((item, index) => `${index + 1}. ${compactDiagnosisText(typeof item === 'string' ? item : JSON.stringify(item), 96)}`)
            .join('\n');
        return summary || '执行成功';
    }

    const candidates = [
        result.recommendations,
        result.report,
        result.message,
        result.summary,
        result.detail,
    ];
    for (const candidate of candidates) {
        if (typeof candidate === 'string' && candidate.trim()) {
            return compactDiagnosisText(candidate, 360);
        }
    }

    try {
        return compactDiagnosisText(JSON.stringify(result, null, 2), 360);
    } catch (_error) {
        return '执行成功';
    }
}

function buildFlightDiagnosisDescription(flight) {
    const flightNo = getPrimaryFlightNoV2(flight) || '未知航班';
    const routeText = getRouteDisplayTextV2(flight);
    const stand = flight.stand || EMPTY_DISPLAY_TEXT;
    const gate = flight.gate || EMPTY_DISPLAY_TEXT;
    const status = flight.status || '未知状态';
    const delayMinutes = resolveDelayMinutes(flight);
    const delayText = delayMinutes === null
        ? '暂无可用的延误分钟数据'
        : (delayMinutes > 0 ? `预计延误 ${delayMinutes} 分钟` : `预计提前 ${Math.abs(delayMinutes)} 分钟`);

    return [
        '请结合机场运行场景给出处置建议。',
        `航班号: ${flightNo}`,
        `状态: ${status}`,
        `航线: ${routeText}`,
        `机位: ${stand}`,
        `登机口: ${gate}`,
        `运行偏差: ${delayText}`,
        `备注: ${flight.flight_remarks || '无'}`,
    ].join('\n');
}

async function diagnoseSelectedFlight() {
    if (selectedFlightId === null) {
        showToast('请先选择一个航班', 'info');
        return;
    }

    if (!isFlightInsightActionEnabled()) {
        showToast(getAICapabilityHintText() || 'AI 不可用', 'warning');
        return;
    }

    const flight = flights.find((item) => String(item.flight_id) === String(selectedFlightId));
    if (!flight) {
        showToast('未找到当前航班，请刷新后重试', 'error');
        return;
    }

    const diagnosisText = buildFlightDiagnosisDescription(flight);
    const urgency = resolveUrgencyByFlight(flight);
    const flightLabel = getPrimaryFlightNoV2(flight) || String(flight.flight_id || '');

    try {
        setAIDiagnoseButtonLoading(true);
        if (window.FM_AI_BRIDGE && typeof window.FM_AI_BRIDGE.diagnoseSelectedFlight === 'function') {
            await window.FM_AI_BRIDGE.diagnoseSelectedFlight({
                flight_id: String(flight.flight_id || ''),
                flight_number: String(flight.flight_number || flight.flight_id || ''),
                summary: diagnosisText,
                urgency,
            });
            showToast(`航班 ${flightLabel} 诊断完成`, 'success', 4200);
            return;
        }
        const result = await executeAITool('get_handling_recommendation', {
            incident_description: diagnosisText,
            flight_id: String(flight.flight_id),
            urgency,
        });
        if (result.pendingApproval) {
            showToast(
                result.approvalId
                    ? `航班 ${flightLabel} 诊断已进入审批队列（${result.approvalId}）`
                    : `航班 ${flightLabel} 诊断已进入审批队列`,
                'warning',
                4200,
            );
            return;
        }

        const preview = compactDiagnosisText(result.resultText || '处置建议已生成', 120);
        showToast(`航班 ${flightLabel} 诊断完成：${preview}`, 'success', 5200);
    } catch (error) {
        showToast(error?.message || '航班诊断失败，请稍后重试', 'error');
    } finally {
        setAIDiagnoseButtonLoading(false);
    }
}

function isFloatingBadgeVisible(element) {
    if (!(element instanceof HTMLElement)) {
        return false;
    }
    if (element.hidden) {
        return false;
    }
    const computed = window.getComputedStyle(element);
    return computed.display !== 'none' && computed.visibility !== 'hidden';
}

function syncFloatingBadgeLayout() {
    const stack = document.getElementById('floatingBadgeStack');
    if (!(stack instanceof HTMLElement)) {
        return;
    }

    const defaultBaseBottom = 5;
    const gap = 4;
    const reservedAiButtonHeight = window.innerWidth <= 768 ? 48 : 40;
    const aiFloatButtonMetrics = getAiFloatButtonMetrics();
    const baseBottom = aiFloatButtonMetrics
        ? Math.max(
            defaultBaseBottom + reservedAiButtonHeight + gap,
            aiFloatButtonMetrics.bottom + aiFloatButtonMetrics.height + gap,
        )
        : defaultBaseBottom + reservedAiButtonHeight + gap;
    stack.style.right = '';
    stack.style.bottom = `${Math.round(baseBottom)}px`;

    FLOATING_BADGE_STACK_ORDER.forEach((id) => {
        const badge = document.getElementById(id);
        if (!(badge instanceof HTMLElement)) {
            return;
        }
        badge.style.right = '';
        badge.style.bottom = '';
    });
    syncUpdatePanelLayout();
}

function observeFloatingBadgeLayout() {
    if (floatingBadgeLayoutObserver || typeof MutationObserver === 'undefined') {
        return;
    }
    const requestLayoutSync = throttleRAF(syncFloatingBadgeLayout);

    const badges = FLOATING_BADGE_STACK_ORDER
        .map((id) => document.getElementById(id))
        .filter((element) => element instanceof HTMLElement);

    if (badges.length === 0) {
        return;
    }

    floatingBadgeLayoutObserver = new MutationObserver(() => {
        requestLayoutSync();
    });

    badges.forEach((badge) => {
        floatingBadgeLayoutObserver.observe(badge, {
            attributes: true,
            attributeFilter: ['style', 'class', 'hidden'],
        });
    });

    if (document.body instanceof HTMLElement && !floatingBadgeAIFloatObserver) {
        floatingBadgeAIFloatObserver = new MutationObserver(() => {
            if (document.querySelector('.flight-monitor-page .ant-float-btn')) {
                requestLayoutSync();
            }
        });

        floatingBadgeAIFloatObserver.observe(document.body, {
            childList: true,
            subtree: true,
        });
    }
}

function syncDispatchNotifyModalWithContext() {
    const contextEl = document.getElementById('dispatchNotifyContext');
    if (contextEl) {
        const context = resolveDispatchNotifyFlightContext();
        contextEl.innerHTML = `${renderOriginBadge('manual')} <span style="margin-left:8px;">${escapeHtml(context.label)}</span>`;
    }

    const titleInput = document.getElementById('dispatchNotifyTitleInput');
    if (titleInput && titleInput.dataset.edited !== '1') {
        const context = resolveDispatchNotifyFlightContext();
        titleInput.value = context.flightNo ? `航班 ${context.flightNo} 调度通知` : '调度通知';
    }
}

function ensureDispatchNotifyModal() {
    let modal = document.getElementById('dispatchNotifyModal');
    if (modal) {
        return modal;
    }

    modal = document.createElement('div');
    modal.id = 'dispatchNotifyModal';
    modal.className = 'modal-overlay';
    modal.setAttribute('role', 'dialog');
    modal.setAttribute('aria-modal', 'true');
    modal.setAttribute('aria-labelledby', 'dispatchNotifyModalTitle');
    modal.setAttribute('hidden', 'hidden');
    modal.setAttribute('aria-hidden', 'true');
    modal.innerHTML = `
        <div class="modal-container dispatch-notify-dialog" role="document">
            <div class="modal-header ai-chat-header dispatch-notify-header">
                <div class="ai-chat-title-wrap dispatch-notify-title-wrap">
                    <h3 id="dispatchNotifyModalTitle">调度通知中心</h3>
                    <div class="ai-chat-meta">按账号定向下发调度指令，支持多级调度协同</div>
                </div>
                <span class="dispatch-notify-header-badge"><span class="dispatch-online-dot"></span>在线定向推送</span>
                <button type="button" class="close-modal" id="closeDispatchNotifyModalBtn" aria-label="关闭调度通知弹窗">×</button>
            </div>
            <div class="dispatch-notify-tabs" role="tablist" aria-label="调度通知中心分页">
                <button type="button" class="dispatch-notify-tab is-active" id="dispatchNotifyTabSendBtn" data-tab="send" role="tab" aria-selected="true">发送通知</button>
                <button type="button" class="dispatch-notify-tab" id="dispatchNotifyTabPendingBtn" data-tab="pending" role="tab" aria-selected="false">待确认<span class="dispatch-notify-tab-count" id="dispatchNotifyPendingCount">0</span></button>
                <button type="button" class="dispatch-notify-tab" id="dispatchNotifyTabSentBtn" data-tab="sent" role="tab" aria-selected="false">已发回执<span class="dispatch-notify-tab-count" id="dispatchNotifySentCount">0</span></button>
            </div>
            <div class="dispatch-notify-tab-panels">
                <section class="dispatch-notify-tab-panel is-active" id="dispatchNotifyTabPanelSend" data-tab-panel="send" aria-label="发送通知">
                    <div class="dispatch-notify-body">
                        <section class="dispatch-notify-pane" aria-label="在线账号列表">
                            <div class="dispatch-notify-pane-header">
                                <div class="dispatch-notify-pane-heading">
                                    <h4 class="dispatch-notify-pane-title">在线账号</h4>
                                    <div class="dispatch-notify-pane-subtitle">只展示当前在线并可接收通知的账号</div>
                                </div>
                                <span class="dispatch-notify-pane-meta" id="dispatchNotifyUserCount">0 在线</span>
                            </div>
                            <div class="dispatch-notify-toolbar dispatch-notify-toolbar-search">
                                <input id="dispatchNotifySearchInput" class="dispatch-notify-search" type="text" placeholder="按账号、岗位、科室筛选">
                                <button type="button" class="btn btn-secondary" id="dispatchNotifyReloadUsersBtn">刷新</button>
                            </div>
                            <div class="dispatch-notify-toolbar dispatch-notify-toolbar-actions">
                                <button type="button" class="btn btn-secondary" id="dispatchNotifySelectAllBtn">全选可见</button>
                                <button type="button" class="btn btn-secondary" id="dispatchNotifyClearSelectBtn">清空选择</button>
                            </div>
                            <div class="dispatch-online-user-list" id="dispatchOnlineUserList"></div>
                        </section>
                        <section class="dispatch-notify-pane" aria-label="通知编辑器">
                            <div class="dispatch-notify-pane-header">
                                <div class="dispatch-notify-pane-heading">
                                    <h4 class="dispatch-notify-pane-title">通知内容</h4>
                                    <div class="dispatch-notify-pane-subtitle">确认对象后填写指令并立即推送</div>
                                </div>
                                <span class="dispatch-notify-pane-meta" id="dispatchNotifySelectedCount">已选 0 人</span>
                            </div>
                            <div class="dispatch-notify-editor">
                                <div class="dispatch-notify-context" id="dispatchNotifyContext"></div>
                                <div class="dispatch-notify-row">
                                    <label>已选账号</label>
                                    <div class="dispatch-selected-list" id="dispatchNotifySelectedList"></div>
                                </div>
                                <div class="dispatch-notify-grid-two">
                                    <div class="dispatch-notify-row">
                                        <label for="dispatchNotifyTitleInput">标题</label>
                                        <input id="dispatchNotifyTitleInput" type="text" maxlength="120" placeholder="输入通知标题">
                                    </div>
                                    <div class="dispatch-notify-row dispatch-notify-row-compact">
                                        <label for="dispatchNotifySeveritySelect">级别</label>
                                        <select id="dispatchNotifySeveritySelect">
                                            <option value="info">提示</option>
                                            <option value="warning" selected>警告</option>
                                            <option value="critical">紧急</option>
                                        </select>
                                    </div>
                                </div>
                                <div class="dispatch-notify-row">
                                    <label for="dispatchNotifyBodyInput">正文</label>
                                    <textarea id="dispatchNotifyBodyInput" placeholder="例如：请在 10 分钟内到达 102 机位，完成登机前复核。"></textarea>
                                </div>
                                <div class="dispatch-notify-row dispatch-notify-row-compact">
                                    <label for="dispatchNotifyReceiptRequiredInput">回执要求</label>
                                    <label style="display:flex;align-items:center;gap:8px;">
                                        <input id="dispatchNotifyReceiptRequiredInput" type="checkbox" checked>
                                        <span>需要确认收到 / 拒绝并填写理由</span>
                                    </label>
                                </div>
                                <div id="dispatchNotifyReceiptGroup"></div>
                            </div>
                        </section>
                    </div>
                </section>
                <section class="dispatch-notify-tab-panel" id="dispatchNotifyTabPanelPending" data-tab-panel="pending" aria-label="待确认回执" hidden>
                    <div class="dispatch-notify-single-pane">
                        <section class="dispatch-notify-pane" aria-label="待确认回执列表">
                            <div class="dispatch-notify-pane-header">
                                <div class="dispatch-notify-pane-heading">
                                    <h4 class="dispatch-notify-pane-title">待确认回执</h4>
                                    <div class="dispatch-notify-pane-subtitle">所有需回执通知都可在这里统一确认或拒绝</div>
                                </div>
                                <span class="dispatch-notify-pane-meta" id="dispatchNotifyPendingMeta">0 待处理</span>
                            </div>
                            <div class="dispatch-notify-toolbar dispatch-notify-toolbar-actions">
                                <button type="button" class="btn btn-secondary" id="dispatchNotifyReloadPendingBtn">刷新</button>
                            </div>
                            <div class="dispatch-notify-list-panel" id="dispatchNotifyPendingList"></div>
                        </section>
                    </div>
                </section>
                <section class="dispatch-notify-tab-panel" id="dispatchNotifyTabPanelSent" data-tab-panel="sent" aria-label="已发回执" hidden>
                    <div class="dispatch-notify-body">
                        <section class="dispatch-notify-pane" aria-label="已发回执列表">
                            <div class="dispatch-notify-pane-header">
                                <div class="dispatch-notify-pane-heading">
                                    <h4 class="dispatch-notify-pane-title">已发回执</h4>
                                    <div class="dispatch-notify-pane-subtitle">查看本人已发送通知的回执进展与超时状态</div>
                                </div>
                                <span class="dispatch-notify-pane-meta" id="dispatchNotifySentMeta">0 批次</span>
                            </div>
                            <div class="dispatch-notify-toolbar dispatch-notify-toolbar-actions">
                                <button type="button" class="btn btn-secondary" id="dispatchNotifyReloadSentBtn">刷新</button>
                            </div>
                            <div class="dispatch-notify-list-panel" id="dispatchNotifySentList"></div>
                        </section>
                        <section class="dispatch-notify-pane" aria-label="已发回执详情">
                            <div class="dispatch-notify-pane-header">
                                <div class="dispatch-notify-pane-heading">
                                    <h4 class="dispatch-notify-pane-title">回执详情</h4>
                                    <div class="dispatch-notify-pane-subtitle">逐人查看确认、拒绝和超时情况</div>
                                </div>
                            </div>
                            <div class="dispatch-notify-editor">
                                <div id="dispatchNotifySentReceiptDetail" class="dispatch-notify-list-panel"></div>
                            </div>
                        </section>
                    </div>
                </section>
            </div>
            <div class="dispatch-notify-footer">
                <span id="dispatchNotifyStatusHint" class="dispatch-notify-tip dispatch-notify-footer-tip" role="status" aria-live="polite">消息将按账号定向推送到 SSE 统一消息流。</span>
                <button type="button" class="btn btn-secondary" id="dispatchNotifyCancelBtn">取消</button>
                <button type="button" class="btn btn-primary" id="dispatchNotifySendBtn">发送通知</button>
            </div>
        </div>
    `;

    document.body.appendChild(modal);

    const closeBtn = document.getElementById('closeDispatchNotifyModalBtn');
    if (closeBtn) {
        closeBtn.addEventListener('click', () => closeManagedModal(modal));
    }

    const cancelBtn = document.getElementById('dispatchNotifyCancelBtn');
    if (cancelBtn) {
        cancelBtn.addEventListener('click', () => closeManagedModal(modal));
    }

    modal.addEventListener('click', (event) => {
        if (event.target === modal) {
            closeManagedModal(modal);
        }
    });

    modal.querySelectorAll('[data-tab]').forEach((button) => {
        button.addEventListener('click', async () => {
            await setDispatchNotifyActiveTab(button.dataset.tab || 'send');
        });
    });

    const sendTabBtn = document.getElementById('dispatchNotifyTabSendBtn');
    if (sendTabBtn) {
        sendTabBtn.disabled = !canManageDispatchNotifications();
        sendTabBtn.title = canManageDispatchNotifications() ? '发送新的调度通知' : '当前账号缺少 dispatch:manage 权限';
    }

    const searchInput = document.getElementById('dispatchNotifySearchInput');
    if (searchInput) {
        searchInput.addEventListener('input', () => {
            applyDispatchNotifyUserFilter();
        });
    }

    const titleInput = document.getElementById('dispatchNotifyTitleInput');
    if (titleInput) {
        titleInput.addEventListener('input', () => {
            titleInput.dataset.edited = titleInput.value.trim() ? '1' : '';
            updateDispatchNotifySendState();
        });
        titleInput.addEventListener('keydown', async (event) => {
            await handleDispatchNotifyEditorKeydown(event);
        });
    }

    const bodyInput = document.getElementById('dispatchNotifyBodyInput');
    if (bodyInput) {
        bodyInput.addEventListener('input', () => {
            updateDispatchNotifySendState();
        });
        bodyInput.addEventListener('keydown', async (event) => {
            await handleDispatchNotifyEditorKeydown(event);
        });
    }

    const severitySelect = document.getElementById('dispatchNotifySeveritySelect');
    if (severitySelect) {
        severitySelect.addEventListener('change', () => {
            updateDispatchNotifyReceiptControl();
        });
    }

    const reloadBtn = document.getElementById('dispatchNotifyReloadUsersBtn');
    if (reloadBtn) {
        reloadBtn.addEventListener('click', async () => {
            await loadDispatchNotifyOnlineUsers({ preserveSelection: true });
        });
    }

    const reloadPendingBtn = document.getElementById('dispatchNotifyReloadPendingBtn');
    if (reloadPendingBtn) {
        reloadPendingBtn.addEventListener('click', async () => {
            await loadDispatchNotifyPendingReceipts();
        });
    }

    const reloadSentBtn = document.getElementById('dispatchNotifyReloadSentBtn');
    if (reloadSentBtn) {
        reloadSentBtn.addEventListener('click', async () => {
            await loadDispatchNotifySentReceiptGroups({ preserveSelection: true });
        });
    }

    const selectAllBtn = document.getElementById('dispatchNotifySelectAllBtn');
    if (selectAllBtn) {
        selectAllBtn.addEventListener('click', () => {
            dispatchNotifyModalState.filteredUsers.forEach((user) => {
                dispatchNotifyModalState.selectedUserIds.add(user.user_id);
            });
            renderDispatchNotifyUserList();
            renderDispatchNotifySelectedUsers();
            updateDispatchNotifySendState();
        });
    }

    const clearSelectBtn = document.getElementById('dispatchNotifyClearSelectBtn');
    if (clearSelectBtn) {
        clearSelectBtn.addEventListener('click', () => {
            dispatchNotifyModalState.selectedUserIds = new Set();
            renderDispatchNotifyUserList();
            renderDispatchNotifySelectedUsers();
            updateDispatchNotifySendState();
        });
    }

    const sendBtn = document.getElementById('dispatchNotifySendBtn');
    if (sendBtn) {
        sendBtn.addEventListener('click', async () => {
            await sendDispatchNotification();
        });
    }

    syncDispatchNotifyModalWithContext();
    updateDispatchNotifyReceiptControl();
    renderDispatchNotifyUserList();
    renderDispatchNotifySelectedUsers();
    renderDispatchNotifyReceiptGroup();
    renderDispatchNotifyPendingReceipts();
    renderDispatchNotifySentReceiptGroups();
    updateDispatchNotifySendState();
    return modal;
}

function renderDispatchNotifyUserList() {
    const listEl = document.getElementById('dispatchOnlineUserList');
    const countEl = document.getElementById('dispatchNotifyUserCount');
    if (!listEl || !countEl) {
        return;
    }

    countEl.textContent = `${dispatchNotifyModalState.filteredUsers.length} 在线`;
    listEl.innerHTML = '';

    if (dispatchNotifyModalState.loadingUsers) {
        const loadingEl = document.createElement('div');
        loadingEl.className = 'dispatch-notify-empty is-loading';
        loadingEl.textContent = '正在加载在线账号...';
        listEl.appendChild(loadingEl);
        return;
    }

    if (dispatchNotifyModalState.loadError) {
        const errorEl = document.createElement('div');
        errorEl.className = 'dispatch-notify-empty is-error';
        errorEl.style.color = '#b91c1c';
        errorEl.style.background = 'rgba(254, 226, 226, 0.9)';
        errorEl.style.border = '1px solid rgba(248, 113, 113, 0.35)';
        errorEl.textContent = dispatchNotifyModalState.loadError;
        listEl.appendChild(errorEl);
        if (dispatchNotifyModalState.filteredUsers.length <= 0) {
            return;
        }
    }

    if (dispatchNotifyModalState.filteredUsers.length <= 0) {
        const emptyEl = document.createElement('div');
        emptyEl.className = 'dispatch-notify-empty is-empty';
        emptyEl.textContent = '暂无匹配的在线账号';
        listEl.appendChild(emptyEl);
        return;
    }

    dispatchNotifyModalState.filteredUsers.forEach((user) => {
        const isSelected = dispatchNotifyModalState.selectedUserIds.has(user.user_id);
        const itemEl = document.createElement('div');
        itemEl.className = `dispatch-online-user-item${isSelected ? ' active' : ''}`;
        itemEl.setAttribute('role', 'button');
        itemEl.setAttribute('tabindex', '0');

        const mainEl = document.createElement('div');
        mainEl.className = 'dispatch-online-user-main';
        const statusMeta = getDispatchNotifyUserStatusMeta(user.status);

        const checkbox = document.createElement('input');
        checkbox.type = 'checkbox';
        checkbox.checked = isSelected;
        checkbox.setAttribute('aria-label', `选择账号 ${user.username}`);
        checkbox.addEventListener('click', (event) => {
            event.stopPropagation();
            toggleDispatchNotifyUserSelection(user.user_id, checkbox.checked);
        });

        const nameEl = document.createElement('span');
        nameEl.className = 'dispatch-online-user-name';
        nameEl.textContent = user.username;

        const idEl = document.createElement('span');
        idEl.className = 'dispatch-online-user-id';
        idEl.textContent = user.user_id;

        const identityEl = document.createElement('div');
        identityEl.className = 'dispatch-online-user-identity';
        identityEl.appendChild(nameEl);
        identityEl.appendChild(idEl);

        const statusEl = document.createElement('span');
        statusEl.className = `dispatch-online-user-status is-${statusMeta.tone}`;
        statusEl.textContent = statusMeta.label;

        mainEl.appendChild(checkbox);
        mainEl.appendChild(identityEl);
        mainEl.appendChild(statusEl);

        const extraEl = document.createElement('div');
        extraEl.className = 'dispatch-online-user-extra';

        const chips = [];
        if (user.department) {
            chips.push(user.department);
        }
        if (user.job_title) {
            chips.push(user.job_title);
        }
        if (chips.length <= 0) {
            chips.push('未配置组织信息');
        }
        chips.forEach((chipText) => {
            const chipEl = document.createElement('span');
            chipEl.className = 'dispatch-online-user-chip';
            chipEl.textContent = chipText;
            extraEl.appendChild(chipEl);
        });

        const timeText = formatDispatchNotifyUserTime(user.last_heartbeat || user.login_time);
        if (timeText) {
            const timeEl = document.createElement('span');
            timeEl.className = 'dispatch-online-user-time';
            timeEl.textContent = `更新时间 ${timeText}`;
            extraEl.appendChild(timeEl);
        }

        itemEl.appendChild(mainEl);
        itemEl.appendChild(extraEl);
        itemEl.addEventListener('click', () => {
            toggleDispatchNotifyUserSelection(user.user_id, !isSelected);
        });
        itemEl.addEventListener('keydown', (event) => {
            if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                toggleDispatchNotifyUserSelection(user.user_id, !isSelected);
            }
        });

        listEl.appendChild(itemEl);
    });
}

function renderDispatchNotifySelectedUsers() {
    const selectedCountEl = document.getElementById('dispatchNotifySelectedCount');
    const selectedListEl = document.getElementById('dispatchNotifySelectedList');
    if (!selectedCountEl || !selectedListEl) {
        return;
    }

    const selectedIds = Array.from(dispatchNotifyModalState.selectedUserIds);
    selectedCountEl.textContent = `已选 ${selectedIds.length} 人`;
    selectedListEl.innerHTML = '';

    if (selectedIds.length <= 0) {
        const emptyEl = document.createElement('span');
        emptyEl.className = 'dispatch-selected-empty';
        emptyEl.textContent = '尚未选择账号';
        selectedListEl.appendChild(emptyEl);
        return;
    }

    const userMap = new Map(dispatchNotifyModalState.users.map((user) => [user.user_id, user]));
    selectedIds.forEach((userId) => {
        const user = userMap.get(userId);
        const chip = document.createElement('span');
        chip.className = 'dispatch-selected-chip';
        chip.textContent = user ? user.username : userId;
        selectedListEl.appendChild(chip);
    });
}

function updateDispatchNotifyTabBadges() {
    const pendingCountEl = document.getElementById('dispatchNotifyPendingCount');
    const sentCountEl = document.getElementById('dispatchNotifySentCount');
    const pendingMetaEl = document.getElementById('dispatchNotifyPendingMeta');
    const sentMetaEl = document.getElementById('dispatchNotifySentMeta');
    const pendingCount = dispatchNotifyModalState.pendingReceipts.length;
    const sentCount = dispatchNotifyModalState.sentReceiptGroups.length;
    if (pendingCountEl) {
        pendingCountEl.textContent = String(pendingCount);
    }
    if (sentCountEl) {
        sentCountEl.textContent = String(sentCount);
    }
    if (pendingMetaEl) {
        pendingMetaEl.textContent = `${pendingCount} 待处理`;
    }
    if (sentMetaEl) {
        sentMetaEl.textContent = `${sentCount} 批次`;
    }
}

function renderDispatchNotifyPendingReceipts() {
    const container = document.getElementById('dispatchNotifyPendingList');
    if (!container) {
        return;
    }
    updateDispatchNotifyTabBadges();
    if (dispatchNotifyModalState.pendingReceiptsLoading) {
        container.innerHTML = '<div class="dispatch-notify-empty is-loading">正在加载待确认回执...</div>';
        return;
    }
    if (dispatchNotifyModalState.pendingReceiptsError) {
        container.innerHTML = `<div class="dispatch-notify-empty is-error">${escapeHtml(dispatchNotifyModalState.pendingReceiptsError)}</div>`;
        return;
    }
    if (!dispatchNotifyModalState.pendingReceipts.length) {
        container.innerHTML = '<div class="dispatch-notify-empty is-empty">当前没有待确认的回执通知。</div>';
        return;
    }

    container.innerHTML = dispatchNotifyModalState.pendingReceipts.map((item) => {
        const relatedFlightLabel = getNotificationFlightLabel(item);
        return `
            <article class="dispatch-receipt-card" data-notification-id="${escapeHtml(item.notification_id)}">
                <div class="dispatch-receipt-card-head">
                    <div>
                        <div class="dispatch-receipt-card-title">${escapeHtml(item.title)}</div>
                        <div class="dispatch-receipt-card-meta">
                            <span>${escapeHtml(String(item.severity || 'info').toUpperCase())}</span>
                            ${relatedFlightLabel ? `<span>航班 ${escapeHtml(relatedFlightLabel)}</span>` : ''}
                            <span>${escapeHtml(formatDispatchNotifyDateTime(item.created_at, item.timestamp || ''))}</span>
                        </div>
                    </div>
                    ${renderOriginBadge(item.origin_type)}
                </div>
                <div class="dispatch-receipt-card-body">${escapeHtml(item.body || '暂无正文')}</div>
                <textarea class="dispatch-receipt-card-note" data-role="reject-note" placeholder="拒绝时填写原因，确认时可留空"></textarea>
                <div class="dispatch-receipt-card-actions">
                    <button type="button" class="btn btn-secondary" data-role="reject">拒绝并提交理由</button>
                    <button type="button" class="btn btn-primary" data-role="ack">确认收到</button>
                </div>
            </article>
        `;
    }).join('');

    container.querySelectorAll('.dispatch-receipt-card').forEach((card) => {
        const notificationId = card.getAttribute('data-notification-id');
        const noteInput = card.querySelector('[data-role="reject-note"]');
        const ackBtn = card.querySelector('[data-role="ack"]');
        const rejectBtn = card.querySelector('[data-role="reject"]');
        const setBusy = (busy) => {
            if (ackBtn) {
                ackBtn.disabled = busy;
                ackBtn.textContent = busy ? '处理中...' : '确认收到';
            }
            if (rejectBtn) {
                rejectBtn.disabled = busy;
                rejectBtn.textContent = busy ? '处理中...' : '拒绝并提交理由';
            }
            if (noteInput instanceof HTMLTextAreaElement) {
                noteInput.disabled = busy;
            }
        };
        if (ackBtn) {
            ackBtn.addEventListener('click', async () => {
                if (!notificationId) {
                    return;
                }
                setBusy(true);
                try {
                    await acknowledgeNotificationReceipt(notificationId, 'acknowledged');
                    dispatchNotifyModalState.pendingReceipts = dispatchNotifyModalState.pendingReceipts
                        .filter((item) => item.notification_id !== notificationId);
                    renderDispatchNotifyPendingReceipts();
                    showToast('回执已确认', 'success', 2600);
                    updateDispatchNotifyEntryState();
                } catch (error) {
                    setBusy(false);
                    showToast(error?.message || '确认回执失败', 'error', 4200);
                }
            });
        }
        if (rejectBtn) {
            rejectBtn.addEventListener('click', async () => {
                if (!notificationId) {
                    return;
                }
                const note = String(noteInput?.value || '').trim();
                if (!note) {
                    showToast('拒绝回执时必须填写原因', 'warning', 3200);
                    noteInput?.focus();
                    return;
                }
                setBusy(true);
                try {
                    await acknowledgeNotificationReceipt(notificationId, 'rejected', note);
                    dispatchNotifyModalState.pendingReceipts = dispatchNotifyModalState.pendingReceipts
                        .filter((item) => item.notification_id !== notificationId);
                    renderDispatchNotifyPendingReceipts();
                    showToast('回执已拒绝并记录原因', 'success', 2600);
                    updateDispatchNotifyEntryState();
                } catch (error) {
                    setBusy(false);
                    showToast(error?.message || '拒绝回执失败', 'error', 4200);
                }
            });
        }
    });
}

function renderDispatchNotifySentReceiptDetail() {
    const container = document.getElementById('dispatchNotifySentReceiptDetail');
    if (!container) {
        return;
    }
    container.innerHTML = buildDispatchNotifyReceiptGroupHtml(dispatchNotifyModalState.sentReceiptGroupDetail, {
        emptyMessage: '选择左侧批次后查看逐人回执详情。',
        heading: '批次回执详情',
    });
}

function renderDispatchNotifyReminderBody(group) {
    const body = document.getElementById('dispatchNotifyReminderBody');
    if (!body || !group) {
        return;
    }
    const repliedCount = Number(group.total_count || 0) - Number(group.pending_count || 0);
    const relatedFlight = group.flight_id ? getNotificationFlightLabel({ flight_id: group.flight_id }) : '';
    body.innerHTML = `
        <div class="dispatch-notify-row">
            <label>通知标题</label>
            <div>${escapeHtml(group.title || '未命名通知')}</div>
        </div>
        <div class="dispatch-notify-row">
            <label>回执状态</label>
            <div>已回复 ${repliedCount} 人，未回复 ${Number(group.pending_count || 0)} 人</div>
        </div>
        <div class="dispatch-notify-row">
            <label>关联航班</label>
            <div>${escapeHtml(relatedFlight || group.flight_id || '无')}</div>
        </div>
    `;
}

function syncDispatchNotifyReminderModal(group) {
    const modal = document.getElementById('dispatchNotifyReminderModal');
    if (!(modal instanceof HTMLElement) || !group) {
        return;
    }
    const currentReceiptGroupId = String(modal.getAttribute('data-receipt-group-id') || '').trim();
    if (!currentReceiptGroupId || currentReceiptGroupId !== String(group.receipt_group_id || '').trim()) {
        return;
    }
    if (Number(group.pending_count || 0) <= 0) {
        closeManagedModal(modal);
        return;
    }
    renderDispatchNotifyReminderBody(group);
}

function ensureDispatchNotifyReminderModal() {
    let modal = document.getElementById('dispatchNotifyReminderModal');
    if (modal) {
        return modal;
    }
    modal = document.createElement('div');
    modal.id = 'dispatchNotifyReminderModal';
    modal.className = 'modal-overlay';
    modal.setAttribute('role', 'dialog');
    modal.setAttribute('aria-modal', 'true');
    modal.setAttribute('aria-labelledby', 'dispatchNotifyReminderModalTitle');
    modal.setAttribute('hidden', 'hidden');
    modal.setAttribute('aria-hidden', 'true');
    modal.innerHTML = `
        <div class="modal-container dispatch-receipt-reminder-dialog" role="document">
            <div class="modal-header dispatch-notify-header">
                <div class="dispatch-notify-title-wrap">
                    <h3 id="dispatchNotifyReminderModalTitle">回执超时提醒</h3>
                    <div class="ai-chat-meta">仍有接收人未确认，请及时跟进。</div>
                </div>
                <button type="button" class="close-modal" id="dispatchNotifyReminderLaterTopBtn" aria-label="稍后处理">×</button>
            </div>
            <div class="dispatch-receipt-reminder-body" id="dispatchNotifyReminderBody"></div>
            <div class="modal-actions critical-notification-actions">
                <button type="button" class="btn btn-secondary" id="dispatchNotifyReminderLaterBtn">稍后处理</button>
                <button type="button" class="btn btn-primary" id="dispatchNotifyReminderViewBtn">查看已发回执</button>
            </div>
        </div>
    `;
    document.body.appendChild(modal);
    const later = () => closeManagedModal(modal);
    document.getElementById('dispatchNotifyReminderLaterTopBtn')?.addEventListener('click', later);
    document.getElementById('dispatchNotifyReminderLaterBtn')?.addEventListener('click', later);
    document.getElementById('dispatchNotifyReminderViewBtn')?.addEventListener('click', async () => {
        const receiptGroupId = modal.getAttribute('data-receipt-group-id') || '';
        closeManagedModal(modal);
        const notifyModal = ensureDispatchNotifyModal();
        openManagedModal(notifyModal, '#dispatchNotifyTabSentBtn');
        await setDispatchNotifyActiveTab('sent');
        if (receiptGroupId) {
            await selectDispatchNotifySentReceiptGroup(receiptGroupId);
        }
    });
    return modal;
}

function renderDispatchNotifySentReceiptGroups() {
    const container = document.getElementById('dispatchNotifySentList');
    if (!container) {
        return;
    }
    updateDispatchNotifyTabBadges();
    if (dispatchNotifyModalState.sentReceiptGroupsLoading) {
        container.innerHTML = '<div class="dispatch-notify-empty is-loading">正在加载已发回执...</div>';
        renderDispatchNotifySentReceiptDetail();
        return;
    }
    if (dispatchNotifyModalState.sentReceiptGroupsError) {
        container.innerHTML = `<div class="dispatch-notify-empty is-error">${escapeHtml(dispatchNotifyModalState.sentReceiptGroupsError)}</div>`;
        renderDispatchNotifySentReceiptDetail();
        return;
    }
    if (!dispatchNotifyModalState.sentReceiptGroups.length) {
        container.innerHTML = '<div class="dispatch-notify-empty is-empty">当前没有已发回执批次。</div>';
        renderDispatchNotifySentReceiptDetail();
        return;
    }
    container.innerHTML = dispatchNotifyModalState.sentReceiptGroups.map((item) => {
        const active = item.receipt_group_id === dispatchNotifyModalState.selectedSentReceiptGroupId;
        const relatedFlight = item.flight_id ? getNotificationFlightLabel({ flight_id: item.flight_id }) : '';
        return `
            <button type="button" class="dispatch-sent-receipt-item${active ? ' is-active' : ''}" data-receipt-group-id="${escapeHtml(item.receipt_group_id)}">
                <div class="dispatch-sent-receipt-row">
                    <strong>${escapeHtml(item.title)}</strong>
                    <span class="dispatch-online-user-chip">${escapeHtml(String(item.severity || 'info').toUpperCase())}</span>
                </div>
                <div class="dispatch-sent-receipt-meta">
                    <span>${escapeHtml(formatDispatchNotifyDateTime(item.created_at, ''))}</span>
                    ${relatedFlight ? `<span>航班 ${escapeHtml(relatedFlight)}</span>` : ''}
                    <span>${escapeHtml(item.origin_type === 'workflow' ? '流程' : '人工')}</span>
                </div>
                <div class="dispatch-sent-receipt-summary">
                    <span>待回执 ${Number(item.pending_count || 0)}</span>
                    <span>已确认 ${Number(item.acknowledged_count || 0)}</span>
                    <span>已拒绝 ${Number(item.rejected_count || 0)}</span>
                    ${item.is_overdue ? '<span class="is-danger">超时</span>' : ''}
                </div>
            </button>
        `;
    }).join('');
    container.querySelectorAll('[data-receipt-group-id]').forEach((button) => {
        button.addEventListener('click', async () => {
            try {
                await selectDispatchNotifySentReceiptGroup(button.getAttribute('data-receipt-group-id') || '');
            } catch (error) {
                showToast(error?.message || '加载回执详情失败', 'error', 4200);
            }
        });
    });
    renderDispatchNotifySentReceiptDetail();
}

async function openDispatchNotifyModal() {
    const canView = canViewDispatchNotifications();
    const hasPendingReceipts = hasPendingDispatchReceiptNotifications();
    const hasSentReceipts = hasSentDispatchReceiptGroups();
    if (!canView && !hasPendingReceipts && !hasSentReceipts) {
        showToast('当前账号缺少 dispatch:view 权限', 'warning');
        return;
    }

    const modal = ensureDispatchNotifyModal();
    syncDispatchNotifyModalWithContext();
    renderDispatchNotifyReceiptGroup();
    renderDispatchNotifyPendingReceipts();
    renderDispatchNotifySentReceiptGroups();
    const defaultTab = !canManageDispatchNotifications()
        ? (hasPendingReceipts ? 'pending' : 'sent')
        : 'send';
    await setDispatchNotifyActiveTab(defaultTab);
    const initialFocusSelector = defaultTab === 'send'
        ? '#dispatchNotifySearchInput'
        : (defaultTab === 'pending' ? '#dispatchNotifyTabPendingBtn' : '#dispatchNotifyTabSentBtn');
    openManagedModal(modal, initialFocusSelector);

    if (canManageDispatchNotifications() && !dispatchNotifyModalState.loadedOnce) {
        await loadDispatchNotifyOnlineUsers({ preserveSelection: false });
    } else if (canManageDispatchNotifications()) {
        applyDispatchNotifyUserFilter();
        renderDispatchNotifySelectedUsers();
        updateDispatchNotifySendState();
    }
    if (!dispatchNotifyModalState.pendingReceiptsLoaded) {
        await loadDispatchNotifyPendingReceipts();
    }
    if (!dispatchNotifyModalState.sentReceiptGroupsLoaded) {
        await loadDispatchNotifySentReceiptGroups({ preserveSelection: true });
    }
}

function ensureCriticalNotificationModal() {
    let modal = document.getElementById('criticalNotificationModal');
    if (modal) {
        return modal;
    }

    modal = document.createElement('div');
    modal.id = 'criticalNotificationModal';
    modal.className = 'modal-overlay';
    modal.setAttribute('role', 'dialog');
    modal.setAttribute('aria-modal', 'true');
    modal.setAttribute('aria-labelledby', 'criticalNotificationModalTitle');
    modal.setAttribute('hidden', 'hidden');
    modal.setAttribute('aria-hidden', 'true');
    modal.setAttribute('data-blocking', 'true');
    modal.innerHTML = `
        <div class="modal-container critical-notification-dialog" role="document">
            <button type="button" class="close-modal" id="criticalNotificationModalGuardClose" aria-hidden="true" tabindex="-1" style="display:none;">×</button>
            <div class="modal-header critical-notification-header">
                <div>
                    <h3 id="criticalNotificationModalTitle" style="margin:0;font-size:20px;">关键通知待处理</h3>
                    <div style="margin-top:6px;font-size:13px;color:rgba(255,255,255,0.88);">该通知必须确认或拒绝后才能关闭</div>
                </div>
            </div>
            <div class="modal-body critical-notification-body">
                <div id="criticalNotificationMeta" class="critical-notification-meta"></div>
                <div id="criticalNotificationTitle" class="critical-notification-title"></div>
                <div id="criticalNotificationBody" class="critical-notification-copy"></div>
                <label for="criticalNotificationRejectNote" style="font-size:13px;font-weight:600;color:#334155;">拒绝原因（拒绝时必填）</label>
                <textarea id="criticalNotificationRejectNote" rows="4" placeholder="请输入拒绝原因，确认收到时可留空" style="width:100%;border:1px solid rgba(148,163,184,0.4);border-radius:12px;padding:12px;font-size:14px;resize:vertical;"></textarea>
                <div id="criticalNotificationError" hidden style="font-size:13px;color:#b91c1c;"></div>
            </div>
            <div class="modal-actions critical-notification-actions">
                <button type="button" class="btn btn-secondary" id="criticalNotificationRejectBtn">拒绝并提交理由</button>
                <button type="button" class="btn btn-primary" id="criticalNotificationAcknowledgeBtn">确认收到</button>
            </div>
        </div>
    `;

    document.body.appendChild(modal);
    return modal;
}

function setCriticalNotificationModalBusy(isBusy) {
    const acknowledgeBtn = document.getElementById('criticalNotificationAcknowledgeBtn');
    const rejectBtn = document.getElementById('criticalNotificationRejectBtn');
    const noteInput = document.getElementById('criticalNotificationRejectNote');
    if (acknowledgeBtn) {
        acknowledgeBtn.disabled = Boolean(isBusy);
        acknowledgeBtn.textContent = isBusy ? '处理中...' : '确认收到';
    }
    if (rejectBtn) {
        rejectBtn.disabled = Boolean(isBusy);
        rejectBtn.textContent = isBusy ? '处理中...' : '拒绝并提交理由';
    }
    if (noteInput) {
        noteInput.disabled = Boolean(isBusy);
    }
}

function openCriticalNotificationModal(notification) {
    const modal = ensureCriticalNotificationModal();
    const metaEl = document.getElementById('criticalNotificationMeta');
    const titleEl = document.getElementById('criticalNotificationTitle');
    const bodyEl = document.getElementById('criticalNotificationBody');
    const noteInput = document.getElementById('criticalNotificationRejectNote');
    const errorEl = document.getElementById('criticalNotificationError');
    const acknowledgeBtn = document.getElementById('criticalNotificationAcknowledgeBtn');
    const rejectBtn = document.getElementById('criticalNotificationRejectBtn');
    const severityMeta = getNotificationSeverityMeta(notification.severity);
    const relatedFlightLabel = getNotificationFlightLabel(notification);

    activeCriticalNotificationId = notification.notification_id;
    if (metaEl) {
        metaEl.innerHTML = `
            <span style="display:inline-flex;align-items:center;padding:3px 10px;border-radius:999px;font-size:12px;font-weight:700;background:${severityMeta.background};color:${severityMeta.color};">${severityMeta.label}</span>
            ${renderOriginBadge(notification.origin_type)}
            ${relatedFlightLabel ? `<span style="font-size:12px;color:#475569;">航班 ${escapeHtml(relatedFlightLabel)}</span>` : ''}
            <span style="font-size:12px;color:#64748b;">${escapeHtml(notification.timestamp)}</span>
        `;
    }
    if (titleEl) {
        titleEl.textContent = notification.title;
    }
    if (bodyEl) {
        bodyEl.textContent = notification.body || '暂无正文';
    }
    if (noteInput) {
        noteInput.value = '';
    }
    if (errorEl) {
        errorEl.hidden = true;
        errorEl.textContent = '';
    }

    setCriticalNotificationModalBusy(false);

    if (acknowledgeBtn && acknowledgeBtn.dataset.bound !== '1') {
        acknowledgeBtn.dataset.bound = '1';
        acknowledgeBtn.addEventListener('click', async () => {
            const currentId = activeCriticalNotificationId;
            if (!currentId) {
                return;
            }
            setCriticalNotificationModalBusy(true);
            try {
                await acknowledgeNotificationReceipt(currentId, 'acknowledged');
                closeManagedModal(modal);
                activeCriticalNotificationId = null;
                notificationCriticalQueueIds.delete(currentId);

                if (typeof dispatchNotifyModalState !== 'undefined' && Array.isArray(dispatchNotifyModalState.pendingReceipts)) {
                    dispatchNotifyModalState.pendingReceipts = dispatchNotifyModalState.pendingReceipts
                        .filter((item) => item.notification_id !== currentId);
                }
                if (typeof renderDispatchNotifyPendingReceipts === 'function') {
                    renderDispatchNotifyPendingReceipts();
                }
                if (typeof updateDispatchNotifyEntryState === 'function') {
                    updateDispatchNotifyEntryState();
                }

                showToast('关键通知已确认', 'success', 3200);
                presentNextCriticalNotification();
            } catch (error) {
                setCriticalNotificationModalBusy(false);
                if (errorEl) {
                    errorEl.hidden = false;
                    errorEl.textContent = error?.message || '确认通知失败';
                }
            }
        });
    }

    if (rejectBtn && rejectBtn.dataset.bound !== '1') {
        rejectBtn.dataset.bound = '1';
        rejectBtn.addEventListener('click', async () => {
            const currentId = activeCriticalNotificationId;
            if (!currentId) {
                return;
            }
            const note = String(noteInput?.value || '').trim();
            if (!note) {
                if (errorEl) {
                    errorEl.hidden = false;
                    errorEl.textContent = '拒绝关键通知时必须填写原因';
                }
                noteInput?.focus();
                return;
            }
            setCriticalNotificationModalBusy(true);
            try {
                await acknowledgeNotificationReceipt(currentId, 'rejected', note);
                closeManagedModal(modal);
                activeCriticalNotificationId = null;
                notificationCriticalQueueIds.delete(currentId);

                if (typeof dispatchNotifyModalState !== 'undefined' && Array.isArray(dispatchNotifyModalState.pendingReceipts)) {
                    dispatchNotifyModalState.pendingReceipts = dispatchNotifyModalState.pendingReceipts
                        .filter((item) => item.notification_id !== currentId);
                }
                if (typeof renderDispatchNotifyPendingReceipts === 'function') {
                    renderDispatchNotifyPendingReceipts();
                }
                if (typeof updateDispatchNotifyEntryState === 'function') {
                    updateDispatchNotifyEntryState();
                }

                showToast('关键通知已拒绝并记录理由', 'success', 3200);
                presentNextCriticalNotification();
            } catch (error) {
                setCriticalNotificationModalBusy(false);
                if (errorEl) {
                    errorEl.hidden = false;
                    errorEl.textContent = error?.message || '拒绝通知失败';
                }
            }
        });
    }

    openManagedModal(modal, '#criticalNotificationAcknowledgeBtn');
}

function buildInsightFileBaseName(kind, flightNo) {
    const safeFlightNo = String(flightNo || 'UNKNOWN').replace(/[^a-zA-Z0-9_-]/g, '_');
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    return `${kind}-${safeFlightNo}-${timestamp}`;
}

function ensureFlightInsightModal() {
    let modal = document.getElementById('flightInsightModal');
    if (modal) {
        return modal;
    }

    modal = document.createElement('div');
    modal.id = 'flightInsightModal';
    modal.className = 'modal-overlay flight-insight-modal';
    modal.setAttribute('role', 'dialog');
    modal.setAttribute('aria-modal', 'true');
    modal.setAttribute('aria-labelledby', 'flightInsightModalTitle');
    modal.setAttribute('hidden', 'hidden');
    modal.setAttribute('aria-hidden', 'true');
    modal.innerHTML = `
        <div class="modal-container flight-insight-dialog" role="document">
            <div class="modal-header flight-insight-header">
                <h3 id="flightInsightModalTitle">航班洞察结果</h3>
                <button type="button" class="close-modal" id="closeFlightInsightModalBtn" aria-label="关闭洞察结果弹窗">×</button>
            </div>
            <div class="modal-body flight-insight-body">
                <div class="flight-insight-meta" id="flightInsightMeta"></div>
                <pre class="flight-insight-markdown" id="flightInsightMarkdown"></pre>
            </div>
            <div class="modal-actions flight-insight-actions">
                <button type="button" class="btn btn-secondary" id="exportFlightInsightMarkdownBtn">导出 Markdown</button>
                <button type="button" class="btn btn-secondary" id="exportFlightInsightJsonBtn">导出 JSON</button>
                <button type="button" class="btn btn-primary" id="closeFlightInsightModalFooterBtn">关闭</button>
            </div>
        </div>
    `;

    document.body.appendChild(modal);

    const closeButtons = [
        document.getElementById('closeFlightInsightModalBtn'),
        document.getElementById('closeFlightInsightModalFooterBtn'),
    ].filter(Boolean);

    closeButtons.forEach((btn) => {
        btn.addEventListener('click', () => closeManagedModal(modal));
    });

    modal.addEventListener('click', (event) => {
        if (event.target === modal) {
            closeManagedModal(modal);
        }
    });

    const markdownExportBtn = document.getElementById('exportFlightInsightMarkdownBtn');
    if (markdownExportBtn) {
        markdownExportBtn.addEventListener('click', () => {
            if (!currentInsightResultPayload) {
                return;
            }
            const fileBase = buildInsightFileBaseName(
                currentInsightResultPayload.kind,
                currentInsightResultPayload.flightNo,
            );
            downloadInsightContent(
                currentInsightResultPayload.markdown || '',
                `${fileBase}.md`,
                'text/markdown;charset=utf-8',
            );
        });
    }

    const jsonExportBtn = document.getElementById('exportFlightInsightJsonBtn');
    if (jsonExportBtn) {
        jsonExportBtn.addEventListener('click', () => {
            if (!currentInsightResultPayload) {
                return;
            }
            const fileBase = buildInsightFileBaseName(
                currentInsightResultPayload.kind,
                currentInsightResultPayload.flightNo,
            );
            downloadInsightContent(
                JSON.stringify(currentInsightResultPayload.jsonPayload || {}, null, 2),
                `${fileBase}.json`,
                'application/json;charset=utf-8',
            );
        });
    }

    return modal;
}

function openFlightInsightModal(payload) {
    const modal = ensureFlightInsightModal();
    const titleEl = document.getElementById('flightInsightModalTitle');
    const metaEl = document.getElementById('flightInsightMeta');
    const markdownEl = document.getElementById('flightInsightMarkdown');

    currentInsightResultPayload = payload;
    if (titleEl) {
        titleEl.textContent = payload.title || '航班洞察结果';
    }
    if (metaEl) {
        const modelText = payload.model ? `模型: ${payload.model}` : '模型: 未知';
        const generatedAt = payload.generatedAt || '--';
        metaEl.textContent = `航班: ${payload.flightNo || payload.flightId || '--'} | 生成时间: ${generatedAt} | ${modelText}`;
    }
    if (markdownEl) {
        markdownEl.textContent = payload.markdown || '';
    }

    openManagedModal(modal, '#closeFlightInsightModalBtn');
}

function buildFlightChatContextPayload() {
    const payload = {
        source_page: 'flight_monitor',
    };
    if (selectedFlightId) {
        payload.scope_mode = 'selected_or_global';
        payload.selected_flight_id = String(selectedFlightId);
        const flightNo = resolveSelectedFlightNo();
        if (flightNo) {
            payload.selected_flight_no = flightNo;
        }
        return payload;
    }

    payload.scope_mode = 'global';
    return payload;
}

function ensureFlightChatModal() {
    let modal = document.getElementById('flightChatModal');
    if (modal) {
        return modal;
    }

    modal = document.createElement('div');
    modal.id = 'flightChatModal';
    modal.className = 'modal-overlay ai-chat-modal flight-chat-modal';
    modal.setAttribute('role', 'dialog');
    modal.setAttribute('aria-modal', 'true');
    modal.setAttribute('aria-labelledby', 'flightChatModalTitle');
    modal.setAttribute('hidden', 'hidden');
    modal.setAttribute('aria-hidden', 'true');
    modal.innerHTML = `
        <div class="modal-container ai-chat-dialog flight-chat-dialog" role="document">
            <div class="modal-header ai-chat-header">
                <div class="ai-chat-title-wrap">
                    <h3 id="flightChatModalTitle">航班 AI 对话</h3>
                    <div class="ai-chat-meta" id="flightChatConversationMeta">会话: 新会话</div>
                    <div class="ai-chat-meta" id="flightChatScopeMeta">范围: 全局（未选中航班时请提供航班号）</div>
                </div>
                <button type="button" class="close-modal" id="closeFlightChatModalBtn" aria-label="关闭航班 AI 对话">×</button>
            </div>
            <div class="modal-body ai-chat-body">
                <div class="ai-chat-messages" id="flightChatMessages" aria-live="polite"></div>
            </div>
            <div class="ai-chat-input-area">
                <textarea id="flightChatInput" class="ai-chat-input" rows="3" placeholder="例如：生成当前航班事件经过，重点看登机和放行节点。"></textarea>
                <div class="ai-chat-input-actions">
                    <span class="ai-chat-tip">Ctrl/⌘ + Enter 发送</span>
                    <button type="button" class="btn btn-secondary" id="flightChatClearBtn">清空窗口</button>
                    <button type="button" class="btn btn-secondary" id="flightChatEndBtn">结束会话</button>
                    <button type="button" class="btn btn-primary" id="flightChatSendBtn">发送</button>
                </div>
            </div>
        </div>
    `;

    document.body.appendChild(modal);

    const closeBtn = document.getElementById('closeFlightChatModalBtn');
    if (closeBtn) {
        closeBtn.addEventListener('click', () => closeManagedModal(modal));
    }
    modal.addEventListener('click', (event) => {
        if (event.target === modal) {
            closeManagedModal(modal);
        }
    });

    const sendBtn = document.getElementById('flightChatSendBtn');
    if (sendBtn) {
        sendBtn.addEventListener('click', async () => {
            await sendFlightChatMessage();
        });
    }

    const clearBtn = document.getElementById('flightChatClearBtn');
    if (clearBtn) {
        clearBtn.addEventListener('click', () => {
            clearFlightChatMessages();
        });
    }

    const endBtn = document.getElementById('flightChatEndBtn');
    if (endBtn) {
        endBtn.addEventListener('click', async () => {
            await endFlightChatConversation();
        });
    }

    const inputEl = document.getElementById('flightChatInput');
    if (inputEl) {
        inputEl.addEventListener('keydown', async (event) => {
            if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
                event.preventDefault();
                await sendFlightChatMessage();
            }
        });
    }

    clearFlightChatMessages({ silent: true });
    return modal;
}

function renderFlightChatBarChartFallback(container, payload) {
    const groups = payload.group_by_status || payload;
    if (!(groups && typeof groups === 'object' && !Array.isArray(groups))) {
        return false;
    }
    const entries = Object.entries(groups);
    const maxValue = Math.max(1, ...entries.map((entry) => Number(entry[1]) || 0));
    entries.forEach(([label, rawValue]) => {
        const value = Number(rawValue) || 0;
        const row = document.createElement('div');
        row.className = 'ai-chat-bar-row';
        row.innerHTML = `
            <span>${escapeHtml(label)}</span>
            <span class="ai-chat-bar-track"><span class="ai-chat-bar-fill" style="width:${(value / maxValue) * 100}%"></span></span>
            <span>${value}</span>
        `;
        container.appendChild(row);
    });
    return true;
}

function renderFlightChatTimelineFallback(container, payload) {
    const items = Array.isArray(payload?.items) ? payload.items : [];
    if (!items.length) {
        return false;
    }
    const list = document.createElement('div');
    list.className = 'ai-chat-timeline-list';
    items.slice(0, 20).forEach((item) => {
        const block = document.createElement('div');
        block.className = 'ai-chat-timeline-item';
        block.textContent = `${item.flight_number || item.flight_id || '-'} / ${item.scheduled_departure || '-'}`;
        list.appendChild(block);
    });
    container.appendChild(list);
    return true;
}

function renderFlightChatVisualization(hint, payload) {
    if (!payload) {
        return null;
    }

    const container = document.createElement('div');
    container.className = 'ai-chat-viz';

    if (hint === 'table') {
        const rows = Array.isArray(payload)
            ? payload
            : (Array.isArray(payload.items) ? payload.items : null);
        if (rows && rows.length > 0) {
            const headers = Object.keys(rows[0]).slice(0, 6);
            const table = document.createElement('table');
            const thead = document.createElement('thead');
            const tbody = document.createElement('tbody');
            thead.innerHTML = `<tr>${headers.map((key) => `<th>${escapeHtml(key)}</th>`).join('')}</tr>`;
            rows.slice(0, 20).forEach((row) => {
                const tr = document.createElement('tr');
                tr.innerHTML = headers.map((key) => `<td>${escapeHtml(row[key])}</td>`).join('');
                tbody.appendChild(tr);
            });
            table.appendChild(thead);
            table.appendChild(tbody);
            container.appendChild(table);
            return container;
        }
    }

    if (hint === 'bar_chart') {
        if (window.AIQueryCharts && window.AIQueryCharts.renderBarChart(container, payload, { height: 220 })) {
            return container;
        }
        if (renderFlightChatBarChartFallback(container, payload)) {
            return container;
        }
    }

    if (hint === 'timeline') {
        if (window.AIQueryCharts && window.AIQueryCharts.renderTimeseriesChart(container, payload, { height: 240, maxPoints: 120 })) {
            return container;
        }
        if (renderFlightChatTimelineFallback(container, payload)) {
            return container;
        }
    }

    return null;
}

function renderFlightChatInsightCard(insight) {
    const wrapper = document.createElement('div');
    wrapper.className = 'ai-chat-insight-card';

    const header = document.createElement('div');
    header.className = 'ai-chat-insight-header';
    header.innerHTML = `
        <strong>${escapeHtml(insight.title)}</strong>
        <span>${escapeHtml(insight.flightNo || '--')}</span>
    `;
    wrapper.appendChild(header);

    const summaryLine = document.createElement('div');
    summaryLine.className = 'ai-chat-insight-summary';
    summaryLine.textContent = insight.kind === 'history_report'
        ? '已生成航班动态报表，可展开全文并导出。'
        : '已生成航班事件经过，可展开全文并导出。';
    wrapper.appendChild(summaryLine);

    const metaLine = document.createElement('div');
    metaLine.className = 'ai-chat-insight-meta';
    const generatedAt = insight.generatedAt || '--';
    const modelText = insight.model || 'unknown-model';
    metaLine.textContent = `生成时间: ${generatedAt} | 模型: ${modelText}`;
    wrapper.appendChild(metaLine);

    const details = document.createElement('details');
    details.className = 'ai-chat-markdown-details';
    const summary = document.createElement('summary');
    summary.textContent = '展开 Markdown 全文';
    const pre = document.createElement('pre');
    const renderedMarkdown = (window.ReportRenderer && typeof window.ReportRenderer.toMarkdown === 'function')
        ? window.ReportRenderer.toMarkdown(insight.jsonPayload || {}, insight.markdown || '')
        : (insight.markdown || '');
    pre.textContent = renderedMarkdown || '(未返回 Markdown 内容)';
    details.appendChild(summary);
    details.appendChild(pre);
    wrapper.appendChild(details);

    const actions = document.createElement('div');
    actions.className = 'ai-chat-insight-actions';
    const exportMdBtn = document.createElement('button');
    exportMdBtn.type = 'button';
    exportMdBtn.className = 'btn btn-secondary';
    exportMdBtn.textContent = '导出 md';

    const exportJsonBtn = document.createElement('button');
    exportJsonBtn.type = 'button';
    exportJsonBtn.className = 'btn btn-secondary';
    exportJsonBtn.textContent = '导出 json';

    const insightId = `flight-chat-insight-${Date.now()}-${++flightChatInsightSeq}`;
    flightChatInsightPayloads.set(insightId, insight);
    exportMdBtn.dataset.insightId = insightId;
    exportJsonBtn.dataset.insightId = insightId;

    exportMdBtn.addEventListener('click', () => {
        const payload = flightChatInsightPayloads.get(exportMdBtn.dataset.insightId || '');
        if (!payload) {
            return;
        }
        const fileBase = buildInsightFileBaseName(payload.kind, payload.flightNo);
        const markdownContent = (window.ReportRenderer && typeof window.ReportRenderer.toMarkdown === 'function')
            ? window.ReportRenderer.toMarkdown(payload.jsonPayload || {}, payload.markdown || '')
            : (payload.markdown || '');
        downloadInsightContent(
            markdownContent,
            `${fileBase}.md`,
            'text/markdown;charset=utf-8',
        );
    });

    exportJsonBtn.addEventListener('click', () => {
        const payload = flightChatInsightPayloads.get(exportJsonBtn.dataset.insightId || '');
        if (!payload) {
            return;
        }
        const fileBase = buildInsightFileBaseName(payload.kind, payload.flightNo);
        downloadInsightContent(
            JSON.stringify(payload.jsonPayload || {}, null, 2),
            `${fileBase}.json`,
            'application/json;charset=utf-8',
        );
    });

    actions.appendChild(exportMdBtn);
    actions.appendChild(exportJsonBtn);
    wrapper.appendChild(actions);

    return wrapper;
}

function openFlightChatModal() {
    if (!isFlightChatActionEnabled()) {
        showToast(getAIChatCapabilityHintText() || 'AI 对话不可用', 'warning');
        return;
    }

    if (window.FM_AI_BRIDGE && typeof window.FM_AI_BRIDGE.openChat === 'function') {
        window.FM_AI_BRIDGE.openChat();
        return;
    }

    const modal = ensureFlightChatModal();
    updateFlightChatMeta();
    openManagedModal(modal, '#flightChatInput');
}

function showToast(message, type = 'info', timeoutOrOptions = 3200) {
    let timeoutMs = 3200;
    let title = '';
    if (typeof timeoutOrOptions === 'number') {
        timeoutMs = timeoutOrOptions;
    } else if (timeoutOrOptions && typeof timeoutOrOptions === 'object') {
        timeoutMs = Number(timeoutOrOptions.duration) || 3200;
        title = timeoutOrOptions.title || '';
    }

    if (!document.getElementById('globalToastStackStyle')) {
        const style = document.createElement('style');
        style.id = 'globalToastStackStyle';
        style.textContent = `
        .global-toast-container {
            position: fixed;
            left: 50%;
            bottom: 22px;
            transform: translateX(-50%);
            display: flex;
            flex-direction: column-reverse;
            align-items: center;
            gap: 8px;
            z-index: 10010;
            pointer-events: none;
        }
        .global-toast-container .global-toast {
            position: relative !important;
            left: auto !important;
            bottom: auto !important;
            transform: translateY(0) !important;
            max-width: min(560px, calc(100vw - 32px));
            padding: 10px 14px;
            border-radius: 10px;
            font-size: 13px;
            line-height: 1.4;
            box-shadow: 0 8px 20px rgba(0, 0, 0, 0.18);
            color: #ffffff;
            background: rgba(28, 28, 30, 0.92);
            pointer-events: auto;
            transition: opacity 0.3s ease, margin-bottom 0.3s ease, transform 0.3s ease !important;
            opacity: 1;
        }
        .global-toast-container .global-toast.toast-enter {
            opacity: 0 !important;
            transform: translateY(20px) !important;
        }
        .global-toast-container .global-toast.toast-exit {
            opacity: 0 !important;
            transform: translateY(-15px) !important;
        }
        .global-toast-container .global-toast.success { background: rgba(29, 135, 76, 0.95); }
        .global-toast-container .global-toast.warning { background: rgba(245, 158, 11, 0.95); }
        .global-toast-container .global-toast.error { background: rgba(173, 34, 50, 0.95); }
        `;
        document.head.appendChild(style);
    }

    const containerId = 'globalToastContainer';
    let container = document.getElementById(containerId);
    if (!container) {
        container = document.createElement('div');
        container.id = containerId;
        container.className = 'global-toast-container';
        document.body.appendChild(container);
    }

    const toast = document.createElement('div');
    toast.className = `global-toast ${type} toast-enter`;
    toast.setAttribute('role', 'status');
    toast.setAttribute('aria-live', 'polite');
    toast.textContent = String(message || '');
    
    container.appendChild(toast);
    if (typeof announce === 'function') announce(message);

    // Trigger reflow for CSS transitions
    requestAnimationFrame(() => {
        toast.classList.remove('toast-enter');
    });

    setTimeout(() => {
        toast.classList.add('toast-exit');
        // Reliable removal without depending on transitionend (e.g. background tabs)
        setTimeout(() => {
            if (toast.parentNode) {
                toast.parentNode.removeChild(toast);
            }
        }, 350);
    }, timeoutMs);
}

function isBlockingManagedModal(modal) {
    return Boolean(modal) && modal.getAttribute('data-blocking') === 'true';
}

function openManagedModal(modal, initialFocusSelector) {
    if (!modal) return;
    activeModalRestoreTarget = document.activeElement;
    modal.style.display = modal.classList.contains('modal-overlay') ? 'flex' : 'block';
    modal.removeAttribute('hidden');
    modal.setAttribute('aria-hidden', 'false');
    activeModal = modal;

    const initialFocus = initialFocusSelector ? modal.querySelector(initialFocusSelector) : null;
    const fallback = getFocusableElements(modal)[0];
    const target = initialFocus || fallback;
    if (target) target.focus();
}

function closeManagedModal(modal) {
    if (!modal) return;
    modal.style.display = 'none';
    modal.setAttribute('hidden', 'hidden');
    modal.setAttribute('aria-hidden', 'true');
    if (activeModal === modal) {
        activeModal = null;
        if (activeModalRestoreTarget && typeof activeModalRestoreTarget.focus === 'function') {
            activeModalRestoreTarget.focus();
        }
    }
    presentNextCriticalNotification();
    if (typeof presentDispatchNotifyReminderIfNeeded === 'function') {
        window.setTimeout(() => {
            void presentDispatchNotifyReminderIfNeeded();
        }, 0);
    }
}

function handleModalFocusTrap(event) {
    if (!activeModal || event.key !== 'Tab') return;
    const focusables = getFocusableElements(activeModal);
    if (focusables.length === 0) return;

    const first = focusables[0];
    const last = focusables[focusables.length - 1];

    if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
    }
}

function renderInteractiveTimeCell(flight, field) {
    const timeVal = flight[field];
    const displayTime = getCachedTimeForField(flight, field);
    const hasValue = !!timeVal;

    // Store necessary data in attributes for event handling
    return `<div class="interactive-cell time-cell ${hasValue ? 'filled' : 'empty'}" 
                data-flight-id="${flight.flight_id}" 
                data-field="${field}" 
                data-value="${timeVal || ''}"
                data-filler=""
                onclick="handleInteractiveTimeClick(this)"
                oncontextmenu="handleTimeFieldContextMenu(event, '${flight.flight_id}', '${field}', '${timeVal || ''}')">${hasValue ? displayTime : '<span class="cell-placeholder">+</span>'}
            </div>`;
}

function renderInteractiveRemarkCell(flight, field) {
    const val = flight[field] || '';
    return `<div class="interactive-cell remark-cell" 
                data-flight-id="${flight.flight_id}" 
                data-field="${field}" 
                ondblclick="handleInteractiveRemarkDblClick(this)"
                title="${escapeHtml(val)}">${val ? escapeHtml(val) : '<span class="cell-placeholder">...</span>'}
            </div>`;
}

function updateBadge() {
    const badge = document.getElementById('updateBadge');
    const count = document.getElementById('updateCount');
    if (badge && count) {
        count.textContent = unreadCount;
        badge.style.display = 'inline-flex';

        if (unreadCount > 0) {
            // Active state
            badge.style.background = 'linear-gradient(135deg, #007aff, #5856d6)';
            badge.style.boxShadow = '0 4px 16px rgba(0,122,255,0.4)';
        } else {
            // Empty state
            badge.style.background = '#8e8e93';
            badge.style.boxShadow = '0 4px 16px rgba(0,0,0,0.2)';
        }
    }
    syncFloatingBadgeLayout();
}

function renderUpdatePanel() {
    const content = document.getElementById('updatePanelContent');
    if (!content) return;
    if (updateMessages.length === 0) {
        content.innerHTML = '<div class="update-panel-empty">暂无更新消息</div>';
        return;
    }
    content.innerHTML = updateMessages.slice(0, 50).map((message) => {
        if (message.kind === 'user_notification') {
            const severityMeta = getNotificationSeverityMeta(message.severity);
            const suffix = message.relatedFlightLabel ? ` · 航班 ${escapeHtml(message.relatedFlightLabel)}` : '';
            const receiptText = message.receiptRequired ? ' · 需回执' : '';
            const ackText = message.ackStatus && message.ackStatus !== 'pending' ? ` · ${escapeHtml(message.ackStatus)}` : '';
            const readText = message.isRead && !message.receiptRequired ? ' · 已读' : '';
            return `
                <div style="padding:10px 0;border-bottom:1px solid #eee;font-size:13px;">
                    <div style="display:flex;align-items:center;gap:8px;flex-wrap:wrap;margin-bottom:6px;">
                        <span style="color:#888;">${escapeHtml(message.time)}：</span>
                        <span style="display:inline-flex;align-items:center;padding:2px 8px;border-radius:999px;font-size:11px;font-weight:700;background:${severityMeta.background};color:${severityMeta.color};">${severityMeta.label}</span>
                        ${renderOriginBadge(message.originType)}
                    </div>
                    <div style="font-weight:700;color:#0f172a;">${escapeHtml(message.title)}</div>
                    <div style="margin-top:4px;color:#334155;line-height:1.6;white-space:pre-wrap;">${escapeHtml(message.body || '暂无正文')}</div>
                    <div style="margin-top:6px;color:#64748b;">通知提醒${suffix}${receiptText}${ackText}${readText}</div>
                </div>
            `;
        }
        return `
            <div style="padding:8px 0;border-bottom:1px solid #eee;font-size:13px;">
                <span style="color:#888;">${escapeHtml(message.time)}：</span>
                <strong>${escapeHtml(message.flightNo)}</strong>的 <em>${escapeHtml(message.field)}</em>由
                <span style="color:#dc3545;text-decoration:line-through;">${escapeHtml(message.oldValue)}</span>
                变更为 <span style="color:#28a745;font-weight:500;">${escapeHtml(message.newValue)}</span>
            </div>
        `;
    }).join('');
}

function buildAbnormalFlightList(sourceFlights = flights) {
    const items = Array.isArray(sourceFlights) ? sourceFlights : [];
    return items
        .filter((flight) => getAnomalyCountForFlight(flight) > 0)
        .map((flight) => ({
            ...flight,
            anomalies: getAnomaliesForFlight(flight),
        }))
        .sort((left, right) => {
            const weightLeft = getAnomalySeverityWeight(getFlightHighestAnomalySeverity(left));
            const weightRight = getAnomalySeverityWeight(getFlightHighestAnomalySeverity(right));
            return weightRight - weightLeft;
        });
}

function renderFlightList() {
    // If we have a search query and no flights match, show no results message
    const searchInput = document.getElementById('searchInput');
    const query = searchInput ? searchInput.value : '';
    const hasActiveFilters = query.trim() !== '' || hasActiveBusinessFilters();

    // 清理虚拟滚动实例（如果数据量变小或为空）
    if (cardVirtualScroller) {
        cardVirtualScroller.destroy();
        cardVirtualScroller = null;
    }

    if (hasActiveFilters && flights.length === 0) {
        if (cardVirtualScroller) {
            cardVirtualScroller.destroy();
            cardVirtualScroller = null;
        }
        flightListElement.innerHTML = '<div class="no-results" role="status" aria-live="polite">未找到匹配的航班</div>';
        announce('未找到匹配的航班');
        return;
    }

    // 大数据量使用分片渲染，避免固定高度虚拟滚动导致卡片重叠
    if (flights.length > CARD_CHUNK_RENDER_THRESHOLD) {
        renderFlightListChunked(flights, query, 36);
        return;
    }

    // 小数据量使用传统渲染
    flightListElement.innerHTML = '';

    // 使用 DocumentFragment 批量插入 DOM
    const fragment = document.createDocumentFragment();

    flights.forEach((flight) => {
        fragment.appendChild(createFlightCardElement(flight, query));
    });

    // 一次性插入所有元素
    flightListElement.appendChild(fragment);
}

function renderFlightListChunked(flightsToRender, query, chunkSize = 30) {
    // 生成新的渲染任务ID，使旧任务失效
    const taskId = ++currentRenderTaskId;

    flightListElement.innerHTML = '';
    let index = 0;

    function renderChunk() {
        // 检查任务ID，如果不匹配说明有新的渲染任务，停止当前任务
        if (taskId !== currentRenderTaskId) {
            return;
        }

        const fragment = document.createDocumentFragment();
        const end = Math.min(index + chunkSize, flightsToRender.length);

        for (; index < end; index++) {
            const flight = flightsToRender[index];
            const flightElement = createFlightCardElement(flight, query);
            fragment.appendChild(flightElement);
        }

        flightListElement.appendChild(fragment);

        if (index < flightsToRender.length) {
            requestAnimationFrame(renderChunk);
        }
    }

    requestAnimationFrame(renderChunk);
}

function renderFlightListVirtual(query) {
    // 如果虚拟滚动实例不存在，创建它
    if (!cardVirtualScroller) {
        cardVirtualScroller = new VirtualScroller({
            container: flightListElement,
            itemHeight: 180, // 卡片预估高度
            buffer: 3,
            renderFn: (flight, index) => createFlightCardElement(flight, query)
        });
        cardVirtualScroller.init();
    } else {
        cardVirtualScroller.renderFn = (flight, index) => createFlightCardElement(flight, query);
    }

    // 更新数据
    cardVirtualScroller.setData(flights);
}

function renderAlertPoolView() {
    const alertContainer = document.getElementById('flightAlertContainer') || document.getElementById('alertFlightList');
    if (!alertContainer) return;

    const searchInput = document.getElementById('searchInput');
    const query = searchInput ? searchInput.value : '';

    const abnormalFlights = buildAbnormalFlightList(flights);
    const alertCountBadge = document.getElementById('alertCountBadge');
    if (alertCountBadge) {
        alertCountBadge.textContent = String(abnormalFlights.length);
    }

    const criticalCount = abnormalFlights.filter((flight) => getFlightHighestAnomalySeverity(flight) === 'critical').length;
    const highCount = abnormalFlights.filter((flight) => getFlightHighestAnomalySeverity(flight) === 'high').length;
    const mediumCount = abnormalFlights.filter((flight) => getFlightHighestAnomalySeverity(flight) === 'medium').length;
    const alertHeader = document.querySelector('.alert-pool-header');
    if (alertHeader) {
        const summary = alertHeader.querySelector('p');
        if (summary) {
            summary.innerHTML = `当前共发现 <span style="color:var(--system-red); font-weight:bold">${abnormalFlights.length}</span> 个异常航班，已按严重程度排序。`;
        }
        let overview = alertHeader.querySelector('.alert-pool-overview');
        if (!overview) {
            overview = document.createElement('div');
            overview.className = 'alert-pool-overview';
            alertHeader.appendChild(overview);
        }
        overview.innerHTML = `
            <div class="alert-pool-stats">
                <span class="anomaly-severity-badge badge-critical">严重 ${criticalCount}</span>
                <span class="anomaly-severity-badge badge-high">高优 ${highCount}</span>
                <span class="anomaly-severity-badge badge-medium">中优 ${mediumCount}</span>
            </div>
            <button type="button" class="flight-text-btn" id="alertBackToCardBtn">返回航班列表</button>
        `;
        const backBtn = overview.querySelector('#alertBackToCardBtn');
        if (backBtn) {
            backBtn.addEventListener('click', () => toggleView(lastNonAlertView));
        }
    }

    if (abnormalFlights.length === 0) {
        if (window.alertVirtualScroller) {
            window.alertVirtualScroller.destroy();
            window.alertVirtualScroller = null;
        }
        alertContainer.innerHTML = '<div class="no-results">目前没有需要处理的异常航班</div>';
        return;
    }

    const createAlertFlightCard = (flight, query) => {
        // 重用原有的基础构建功能，只替换其 class 和附加 badge
        const el = createFlightCardElement(flight, query);
        // 修改样式使其成为 alert pool 的一种
        el.classList.add('alert-pool-card');

        const highestSeverity = getFlightHighestAnomalySeverity(flight);

        if (highestSeverity === 'critical') el.classList.add('alert-severity-critical');
        else if (highestSeverity === 'high') el.classList.add('alert-severity-high');
        else if (highestSeverity === 'medium') el.classList.add('alert-severity-medium');

        const headerDiv = el.querySelector('.flight-header');
        if (headerDiv) {
            const badgesHtml = (flight.anomalies || []).map((ano) => {
                const s = sanitizeCssToken(ano.severity, 'unknown');
                const type = typeof ano.anomaly_type === 'string' ? ano.anomaly_type : '异常';
                return `<span class="anomaly-severity-badge badge-${s}">${escapeHtmlForRender(type)}</span>`;
            }).join('');
            if (!badgesHtml && flight.anomaly_count) {
                const fallbackHtml = `<span class="anomaly-severity-badge badge-${sanitizeCssToken(highestSeverity, 'unknown')}">含 ${Number(flight.anomaly_count || 0)} 个异常</span>`;
                headerDiv.insertAdjacentHTML('afterbegin', fallbackHtml);
            } else {
                headerDiv.insertAdjacentHTML('afterbegin', badgesHtml);
            }
        }

        return el;
    };

    if (!window.alertVirtualScroller) {
        window.alertVirtualScroller = new VirtualScroller({
            container: alertContainer,
            itemHeight: 180,
            buffer: 3,
            renderFn: (flight, index) => createAlertFlightCard(flight, query)
        });
        window.alertVirtualScroller.init();
    } else {
        window.alertVirtualScroller.renderFn = (flight, index) => createAlertFlightCard(flight, query);
    }

    window.alertVirtualScroller.setData(abnormalFlights);
}

function renderFlightDetail() {
    if (selectedFlightId === null) {
        flightDetailElement.innerHTML = '<div class="no-selection">请选择一个航班查看详细信息</div>';
        return;
    }

    const flight = findFlightById(selectedFlightId);
    if (!flight) {
        flightDetailElement.innerHTML = '<div class="no-selection">未找到选中航班，请刷新后重试</div>';
        return;
    }

    const timelineFlightId = normalizeFlightId(flight.flight_id);
    if (timelineFlightId && !dispatchTimelineCache.has(timelineFlightId)) {
        loadDispatchTimelineForFlight(timelineFlightId)
            .then(() => {
                if (isSameFlightId(selectedFlightId, timelineFlightId)) {
                    renderFlightDetail();
                }
            })
            .catch((error) => {
                console.warn('加载调度时间线失败:', error);
            });
    }

    // Format times - HH:MM only (operation date derived from schedule/estimate)
    const timeOpts = { hour: '2-digit', minute: '2-digit' };
    const scheduledArrival = flight.scheduled_arrival ? new Date(flight.scheduled_arrival).toLocaleTimeString('zh-CN', timeOpts) : EMPTY_DISPLAY_TEXT;
    const scheduledDeparture = flight.scheduled_departure ? new Date(flight.scheduled_departure).toLocaleTimeString('zh-CN', timeOpts) : EMPTY_DISPLAY_TEXT;
    const estimatedArrival = flight.estimated_arrival ? new Date(flight.estimated_arrival).toLocaleTimeString('zh-CN', timeOpts) : EMPTY_DISPLAY_TEXT;
    const estimatedDeparture = flight.estimated_departure ? new Date(flight.estimated_departure).toLocaleTimeString('zh-CN', timeOpts) : EMPTY_DISPLAY_TEXT;
    const actualArrival = flight.actual_arrival ? new Date(flight.actual_arrival).toLocaleTimeString('zh-CN', timeOpts) : EMPTY_DISPLAY_TEXT;
    const actualDeparture = flight.actual_departure ? new Date(flight.actual_departure).toLocaleTimeString('zh-CN', timeOpts) : EMPTY_DISPLAY_TEXT;
    const startBoarding = flight.start_boarding_time ? new Date(flight.start_boarding_time).toLocaleTimeString('zh-CN', timeOpts) : EMPTY_DISPLAY_TEXT;
    const endBoarding = flight.end_boarding_time ? new Date(flight.end_boarding_time).toLocaleTimeString('zh-CN', timeOpts) : EMPTY_DISPLAY_TEXT;

    // Format COBT
    const cobtTime = flight.cobt_time ? new Date(flight.cobt_time).toLocaleTimeString('zh-CN', timeOpts) : EMPTY_DISPLAY_TEXT;

    const inboundFlightNo = getFlightNumberByLegV2(flight, 'inbound');
    const outboundFlightNo = getFlightNumberByLegV2(flight, 'outbound');
    const inboundType = getLegFlightTypeLabelV2(flight, 'inbound') || EMPTY_DISPLAY_TEXT;
    const outboundType = getLegFlightTypeLabelV2(flight, 'outbound') || EMPTY_DISPLAY_TEXT;
    const hasInboundVIP = getLegVipFlagV2(flight, 'inbound');
    const hasOutboundVIP = getLegVipFlagV2(flight, 'outbound');
    const hasAnyVIP = hasInboundVIP || hasOutboundVIP;
    const operationDate = deriveOperationDateLabel(flight);

    // Check if editing mode
    const isEditMode = editMode[flight.flight_id] || false;

    // Helper: Format Time
    const formatTime = (isoString) => {
        if (!isoString) return '--';
        return new Date(isoString).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });
    };

    // Determine flight type components
    const hasInboundFlight = Boolean(inboundFlightNo);
    const hasOutboundFlight = Boolean(outboundFlightNo);

    const originDisplay = getRouteEndpointV2(flight, 'inbound', 'name')
        || getStationListDisplayV2(flight, 'outbound', 'origin_stations', 'name')
        || EMPTY_DISPLAY_TEXT;
    const destinationDisplay = getRouteEndpointV2(flight, 'outbound', 'name')
        || getStationListDisplayV2(flight, 'inbound', 'destination_stations', 'name')
        || EMPTY_DISPLAY_TEXT;
    const airportDisplayName = getAirportDisplayValueV2('name');
    const textHtml = (value, fallback = EMPTY_DISPLAY_TEXT) => escapeHtmlForRender(value || fallback);
    const inboundFlightNoHtml = escapeHtmlForRender(inboundFlightNo || '');
    const outboundFlightNoHtml = escapeHtmlForRender(outboundFlightNo || '');
    const operationDateHtml = escapeHtmlForRender(operationDate);
    const statusHtml = escapeHtmlForRender(flight.status || '计划中');
    const statusClass = sanitizeCssToken(getStatusClass(flight.status), 'status-scheduled');
    const routeDisplayText = hasInboundFlight && hasOutboundFlight
        ? `${originDisplay} → ${airportDisplayName} → ${destinationDisplay}`
        : (hasInboundFlight ? `${originDisplay} → ${airportDisplayName}` : `${airportDisplayName} → ${destinationDisplay}`);
    const routeDisplayHtml = escapeHtmlForRender(routeDisplayText);
    const flightIdJs = jsStringForInlineHandler(flight.flight_id);
    const flightIdToken = sanitizeCssToken(flight.flight_id, 'flight');
    const aircraftCheckRemarksJs = jsStringForInlineHandler(flight.aircraft_check_remarks || '');
    const timeActionAttrs = (fieldName, value) => {
        const fieldJs = jsStringForInlineHandler(fieldName);
        const valueJs = jsStringForInlineHandler(value || '');
        return `onclick="handleTimeFieldClick(event, ${flightIdJs}, ${fieldJs}, ${valueJs})" oncontextmenu="handleTimeFieldContextMenu(event, ${flightIdJs}, ${fieldJs}, ${valueJs})"`;
    };
    const timeContextAttrs = (fieldName, value) => {
        const fieldJs = jsStringForInlineHandler(fieldName);
        const valueJs = jsStringForInlineHandler(value || '');
        return `oncontextmenu="handleTimeFieldContextMenu(event, ${flightIdJs}, ${fieldJs}, ${valueJs})"`;
    };

    // --- Split High Density Layout ---
    flightDetailElement.innerHTML = `
                    <div class="detail-dashboard"><!--1. Combined Header Card(Identity + Primary KPIs)--><div class="detail-card header-combined-card" style="grid-column: 1 / -1; margin-bottom: 0;"><!-- Left Group: Identity --><div class="header-left-group" style="flex: 1; min-width: 0;"><div class="header-id-section" style="padding: 0 32px; display: flex; flex-direction: column; justify-content: center;">${hasInboundFlight ? `<div style="font-size: 32px; font-weight: 800; line-height: 1.1; letter-spacing: -0.5px; white-space: nowrap;">${inboundFlightNoHtml}</div>` : ''}
                        ${hasOutboundFlight ? `<div style="font-size: 32px; font-weight: 800; line-height: 1.1; letter-spacing: -0.5px; white-space: nowrap;">${outboundFlightNoHtml}</div>` : ''}
                    </div><div style="width: 1px; background-color: #eee; margin: 16px 0;"></div><div class="header-info-section" style="padding: 16px 32px; display: flex; flex-direction: column; justify-content: center; overflow: hidden;"><div style="margin-bottom: 8px; display: flex; align-items: center;"><span class="flight-status-badge ${statusClass}"
                                  style="padding: 4px 16px; border-radius: 20px; font-weight: 600; font-size: 14px; white-space: nowrap;">${statusHtml}
                            </span><span style="margin-left: 12px; font-size: 14px; color: var(--text-tertiary); white-space: nowrap;">${operationDateHtml}</span></div><div style="padding-top: 8px; border-top: 1px solid #f0f0f0; color: var(--text-secondary); font-size: 16px; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">${routeDisplayHtml}
                        </div></div></div><!-- Right Group: Primary KPIs (Embedded) --><div class="header-right-group">${hasInboundFlight ? `
                    <div class="header-kpi-item"><span class="header-kpi-label">落地时间</span><span class="header-kpi-value">${textHtml(flight.actual_arrival ? formatTime(flight.actual_arrival) : EMPTY_DISPLAY_TEXT)}</span></div>` : ''}
                    <div class="header-kpi-item"><span class="header-kpi-label">机位</span><span class="header-kpi-value">${textHtml(flight.stand)}</span></div><div class="header-kpi-item"><span class="header-kpi-label">登机口</span><span class="header-kpi-value">${textHtml(flight.gate)}</span></div>${hasOutboundFlight ? `
                    <div class="header-kpi-item"><span class="header-kpi-label">起飞时间</span><span class="header-kpi-value">${textHtml(flight.actual_departure ? formatTime(flight.actual_departure) : EMPTY_DISPLAY_TEXT)}</span></div>` : ''}
                    <div class="header-kpi-item"><span class="header-kpi-label">COBT</span><span class="header-kpi-value">${textHtml(flight.cobt_time ? formatTime(flight.cobt_time) : EMPTY_DISPLAY_TEXT)}</span></div></div></div><!-- 2. Secondary KPI Strip(Timeline Events) - Row 2 --><div class="detail-card secondary-kpi-strip" style="grid-column: 1 / -1; margin-bottom: 0;"><div class="secondary-kpi-item"><span class="secondary-kpi-label">开客舱门</span><span class="secondary-kpi-value clickable-action" ${timeActionAttrs('cabin_door_open_time', flight.cabin_door_open_time)}>${textHtml(flight.cabin_door_open_time ? formatTime(flight.cabin_door_open_time) : '--')}</span></div><div class="secondary-kpi-item"><span class="secondary-kpi-label">清洁开始</span><span class="secondary-kpi-value clickable-action" ${timeActionAttrs('cleaning_start_time', flight.cleaning_start_time)}>${textHtml(flight.cleaning_start_time ? formatTime(flight.cleaning_start_time) : '--')}</span></div><div class="secondary-kpi-item"><span class="secondary-kpi-label">清洁结束</span><span class="secondary-kpi-value clickable-action" ${timeActionAttrs('cleaning_end_time', flight.cleaning_end_time)}>${textHtml(flight.cleaning_end_time ? formatTime(flight.cleaning_end_time) : '--')}</span></div><div class="secondary-kpi-item"><span class="secondary-kpi-label">允许登机</span><span class="secondary-kpi-value clickable-action" ${timeActionAttrs('boarding_allowed_time', flight.boarding_allowed_time)}>${textHtml(flight.boarding_allowed_time ? formatTime(flight.boarding_allowed_time) : '--')}</span></div><div class="secondary-kpi-item"><span class="secondary-kpi-label">人齐</span><span class="secondary-kpi-value clickable-action" ${timeActionAttrs('passenger_ready_time', flight.passenger_ready_time)}>${textHtml(flight.passenger_ready_time ? formatTime(flight.passenger_ready_time) : '--')}</span></div></div><!-- Left Column: Flight Information --><div class="detail-col-left"><!-- Edit Form (if active) -->${isEditMode ? `<div class="detail-card" style="padding:16px;">${createEditForm(flight)}</div>` : ''}

                <!-- Labels Section -->${renderFlightLabelsSection(flight)}
                <!-- 3. Info Grid (Dense Data) -->${!isEditMode ? `
                <div class="detail-card info-grid-card"><div class="info-grid-compact"><div class="info-field"><label>机型</label><span>${textHtml(flight.aircraft_type_detail)}</span></div><div class="info-field"><label>任务类型</label><span>${textHtml(getMissionSummaryV2(flight))}</span></div><div class="info-field"><label>执行日期</label><span>${operationDateHtml}</span></div><div class="info-field"><label>重要旅客</label><span>${hasInboundFlight || hasOutboundFlight ? (hasAnyVIP ? '是' : '否') : EMPTY_DISPLAY_TEXT}</span></div><div class="info-field"><label>快速过站</label><span>${flight.is_quick_turnaround ? '是' : '否'}</span></div>${hasInboundFlight ? `
                        <div class="info-field"><label>进港航班号</label><span>${inboundFlightNoHtml || '--'}</span></div><div class="info-field"><label>进港类别</label><span>${textHtml(inboundType)}</span></div>` : ''}
                        ${hasOutboundFlight ? `
                        <div class="info-field"><label>出港航班号</label><span>${outboundFlightNoHtml || '--'}</span></div><div class="info-field"><label>出港类别</label><span>${textHtml(outboundType)}</span></div><div class="info-field"><label>结束登机</label><span>${textHtml(endBoarding)}</span></div><div class="info-field"><label>登机限制</label><span>${flight.has_boarding_restriction ? '是' : '否'}</span></div><div class="info-field"><label>撤轮挡</label><span class="clickable-action" ${timeContextAttrs('off_blocks_time', flight.off_blocks_time)}>${textHtml(flight.off_blocks_time ? new Date(flight.off_blocks_time).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' }) : EMPTY_DISPLAY_TEXT)}
                            </span></div>` : ''}
                        
                         <div class="info-field"><label>行李转盘</label><span>${textHtml(flight.baggage_carousel)}</span></div><div class="info-field"><label>复核机号</label><span style="cursor: pointer; color: #007bff;" ondblclick="showRemarkEditModal(${flightIdJs}, 'aircraft_check_remarks', ${aircraftCheckRemarksJs})">${textHtml(flight.aircraft_check_remarks, '点击编辑')}
                            </span></div><div class="info-field"><label>登机机号</label><span class="clickable-action" onclick="toggleEdit(${flightIdJs})">${isEditMode ? `<input type="text" id="edit_boarding_id_${escapeAttributeForRender(flightIdToken)}" value="${escapeAttributeForRender(flight.boarding_id || '')}" class="inline-edit-input">` : textHtml(flight.boarding_id, '点击编辑')}</span></div></div></div>` : ''
        }
            </div><!-- Right Column: Operations Card --><div class="detail-col-right"><div class="detail-card ops-card"><!-- Directly invoke renderBusinessEventsSection if available -->${typeof renderBusinessEventsSection === 'function' ? renderBusinessEventsSection(flight) : '<div class="events-section">正在加载...</div>'}
                        </div></div></div>`;
    updateBusinessInsightActionState();

    const selectedIdAtRender = normalizeFlightId(flight.flight_id);
    requestAnimationFrame(function () {
        if (typeof renderGanttChart !== 'function') {
            return;
        }
        if (!isSameFlightId(selectedFlightId, selectedIdAtRender)) {
            return;
        }
        const latestFlight = findFlightById(selectedIdAtRender) || flight;
        renderGanttChart(latestFlight);
    });
}

function renderRouteStationEditorRowsV2(stations, coreDisabled) {
    return stations.map((station) => `
        <div class="route-station-item" style="display:flex; gap:8px; margin-bottom:8px; align-items:center;">
            <input type="text" class="station-code-input" placeholder="代码" value="${escapeAttributeForRender(station.code || '')}" ${coreDisabled}>
            <input type="text" class="station-name-input" placeholder="名称（选填）" value="${escapeAttributeForRender(station.name || '')}" ${coreDisabled}>
            <button type="button" onclick="removeRouteStation(this)" ${coreDisabled}>×</button>
        </div>
    `).join('');
}

function startEdit() {
    if (selectedFlightId === null) return;

    const flight = findFlightById(selectedFlightId);
    if (!flight) return;

    // 保存原始数据快照（深拷贝）
    originalFlightSnapshots[selectedFlightId] = JSON.parse(JSON.stringify(flight));

    editMode[selectedFlightId] = true;
    renderFlightDetail();
    updateEditButtonState();
}

function cancelEdit() {
    if (selectedFlightId === null) return;

    // 清除原始快照
    delete originalFlightSnapshots[selectedFlightId];

    editMode[selectedFlightId] = false;
    renderFlightDetail();
    updateEditButtonState();

    // 清除保存状态
    const saveStatus = document.getElementById('saveStatus');
    if (saveStatus) {
        saveStatus.innerHTML = '';
    }
}

const remarkEditModal = document.getElementById('remarkEditModal');

function showRemarkEditModal(flightId, field, currentValue) {
    remarkTarget = { flightId, field };
    remarkInput.value = currentValue;
    openManagedModal(remarkEditModal, '#remarkInput');
}

function buildDispatchChatEntryMeta(flightId) {
    const normalizedFlightId = normalizeFlightId(flightId);
    if (!normalizedFlightId) {
        return '未指定航班，可查看当前账号可访问的群聊。';
    }

    const flight = findFlightById(normalizedFlightId);
    if (!flight) {
        return `当前航班 ${normalizedFlightId}，正在加载对应群聊。`;
    }

    const flightNo = getPrimaryFlightNoV2(flight) || normalizedFlightId;
    const routeText = getRouteDisplayTextV2(flight) || '--';
    return `当前航班 ${flightNo} · ${routeText}`;
}

const DEFAULT_BUSINESS_CASE_STATUS_OPTIONS = [
    { value: 'INITIAL', label: '初始', manual_transition_enabled: true },
    { value: 'PENDING', label: '待处理', manual_transition_enabled: true },
    { value: 'PROCESSING', label: '处理中', manual_transition_enabled: true },
    { value: 'SUCCESS', label: '成功', manual_transition_enabled: true },
    { value: 'COMPLETED', label: '已完成', manual_transition_enabled: true },
    { value: 'FAILED', label: '失败', manual_transition_enabled: true },
];
let businessCaseStatusOptions = [...DEFAULT_BUSINESS_CASE_STATUS_OPTIONS];
let BUSINESS_CASE_SUPPORTED_STATUSES = businessCaseStatusOptions.map(option => option.value);
const BUSINESS_CASE_BINDING_SELECT_ID = 'boundFlightSelection';

function normalizeBusinessCaseStatusOption(item) {
    const value = String(item?.value || item?.code || item?.status || '').trim().toUpperCase();
    if (!value) {
        return null;
    }
    const fallback = DEFAULT_BUSINESS_CASE_STATUS_OPTIONS.find(option => option.value === value) || {};
    return {
        ...fallback,
        ...item,
        value,
        label: String(item?.label || fallback.label || value).trim(),
        manual_transition_enabled: item?.manual_transition_enabled ?? fallback.manual_transition_enabled ?? true,
    };
}

function populateBusinessCaseStatusSelect() {
    const selectEl = document.getElementById('eventStatus');
    if (!(selectEl instanceof HTMLSelectElement)) {
        return;
    }
    const currentValue = String(selectEl.value || '').trim().toUpperCase();
    const options = businessCaseStatusOptions.filter(option => option.manual_transition_enabled !== false);
    selectEl.innerHTML = '';
    options.forEach(option => {
        const node = document.createElement('option');
        node.value = option.value;
        node.textContent = option.label || option.value;
        selectEl.appendChild(node);
    });
    if (currentValue && options.some(option => option.value === currentValue)) {
        selectEl.value = currentValue;
    }
}

async function loadBusinessCaseStatusOptions() {
    try {
        const fetcher = typeof Auth !== 'undefined' && typeof Auth.fetch === 'function'
            ? Auth.fetch.bind(Auth)
            : fetch;
        const response = await fetcher('/api/v2/reference/business-case-statuses');
        const payload = await response.json().catch(() => ({}));
        if (!response.ok || !payload?.success || !Array.isArray(payload.data)) {
            throw new Error(payload?.detail || payload?.message || `HTTP ${response.status}`);
        }
        const options = payload.data
            .map(normalizeBusinessCaseStatusOption)
            .filter(Boolean);
        if (options.length <= 0) {
            throw new Error('empty status metadata');
        }
        businessCaseStatusOptions = options;
        BUSINESS_CASE_SUPPORTED_STATUSES = options.map(option => option.value);
    } catch (error) {
        console.warn('[flight_monitor] 加载业务事项状态元数据失败，使用本地兜底配置:', error);
        businessCaseStatusOptions = [...DEFAULT_BUSINESS_CASE_STATUS_OPTIONS];
        BUSINESS_CASE_SUPPORTED_STATUSES = businessCaseStatusOptions.map(option => option.value);
    }
    populateBusinessCaseStatusSelect();
}

function getBusinessCaseBindingOptions(flight) {
    if (!flight) {
        return [];
    }

    const options = [];
    const pushOption = (legType, label) => {
        const flightNo = String(getFlightNumberByLegV2(flight, legType) || '').trim().toUpperCase();
        if (!flightNo) {
            return;
        }
        options.push({
            value: `${legType}::${flightNo}`,
            legType,
            flightNo,
            label: `${label} ${flightNo}`,
        });
    };

    pushOption('inbound', '进港');
    pushOption('outbound', '出港');
    return options;
}

function populateBusinessCaseBindingSelect(flight) {
    const selectEl = document.getElementById(BUSINESS_CASE_BINDING_SELECT_ID);
    const submitBtn = document.getElementById('submitEventBtn');
    if (!(selectEl instanceof HTMLSelectElement)) {
        return;
    }

    const options = getBusinessCaseBindingOptions(flight);
    selectEl.innerHTML = '';

    const placeholder = document.createElement('option');
    placeholder.value = '';
    placeholder.textContent = options.length > 0 ? '请选择绑定航班号' : '当前航班无可绑定航班号';
    placeholder.disabled = options.length > 0;
    placeholder.selected = true;
    selectEl.appendChild(placeholder);

    options.forEach((option) => {
        const el = document.createElement('option');
        el.value = option.value;
        el.textContent = option.label;
        el.dataset.legType = option.legType;
        el.dataset.flightNo = option.flightNo;
        selectEl.appendChild(el);
    });

    const singleOption = options.length === 1;
    if (singleOption) {
        selectEl.value = options[0].value;
    }
    selectEl.disabled = options.length <= 1;

    if (submitBtn instanceof HTMLButtonElement) {
        submitBtn.disabled = options.length === 0;
        submitBtn.title = options.length === 0 ? '当前航班没有可绑定的进港或出港航班号' : '';
    }
}

function parseBusinessCaseBindingValue(rawValue) {
    const normalized = String(rawValue || '').trim();
    if (!normalized.includes('::')) {
        return null;
    }
    const [legType, flightNo] = normalized.split('::');
    const safeLegType = legType === 'inbound' || legType === 'outbound' ? legType : '';
    const safeFlightNo = String(flightNo || '').trim().toUpperCase();
    if (!safeLegType || !safeFlightNo) {
        return null;
    }
    return {
        legType: safeLegType,
        flightNo: safeFlightNo,
    };
}

function createNewEvent() {
    const eventCreationModal = document.getElementById('eventCreationModal');
    if (!(eventCreationModal instanceof HTMLElement)) {
        return;
    }
    if (selectedFlightId === null) {
        showToast('请先选择一个航班', 'warning');
        return;
    }

    const flight = findFlightById(selectedFlightId);
    if (!flight) {
        showToast('未找到当前选中航班，请刷新后重试', 'warning');
        return;
    }

    populateBusinessCaseBindingSelect(flight);

    const standInput = document.getElementById('stand_no');
    const gateInput = document.getElementById('gate_no');
    if (standInput) {
        standInput.value = flight.stand || '';
    }
    if (gateInput) {
        gateInput.value = flight.gate || '';
    }

    openManagedModal(eventCreationModal, '#eventType');
}

function setupEventCreationModal() {
    // Event creation modal functionality
    const eventCreationModal = document.getElementById('eventCreationModal');
    if (!eventCreationModal) return;
    const closeBtn = eventCreationModal ? eventCreationModal.querySelector('.close') : null;
    const cancelEventBtn = document.getElementById('cancelEventBtn');
    const eventCreationForm = document.getElementById('eventCreationForm');

    populateBusinessCaseStatusSelect();
    loadBusinessCaseStatusOptions();

    // 从 DB 动态加载业务事项类型，填充 eventType 下拉框
    (async function loadEventTypes() {
        const selectEl = document.getElementById('eventType');
        if (!selectEl) return;
        try {
            const resp = await Auth.fetch('/api/v2/business-case-types');
            const json = await resp.json();
            const items = Array.isArray(json.data) ? json.data : [];
            // 保留占位符
            selectEl.innerHTML = '<option value="">请选择事项类型</option>';
            items.forEach(item => {
                if (item.is_active) {
                    const opt = document.createElement('option');
                    opt.value = item.code;
                    opt.textContent = item.name;
                    selectEl.appendChild(opt);
                }
            });
        } catch (err) {
            console.warn('[flight_monitor] 加载业务事项类型失败:', err);
        }
    })();

    const eventTypeSelect = document.getElementById('eventType');
    const triggerReasonGroup = document.getElementById('triggerReasonGroup');
    const eventDescriptionLabel = document.getElementById('eventDescriptionLabel');
    const gateInput = document.getElementById('gate_no');

    if (eventTypeSelect) {
        eventTypeSelect.addEventListener('change', (e) => {
            if (e.target.value === 'gate_baggage_check') {
                if (triggerReasonGroup) triggerReasonGroup.style.display = 'block';
                if (eventDescriptionLabel) eventDescriptionLabel.textContent = '额外信息补充:';
                if (document.getElementById('eventDescription')) document.getElementById('eventDescription').placeholder = '请输入需要补充给通知对象的额外信息';
                if (document.getElementById('triggerReason')) document.getElementById('triggerReason').required = true;
                if (gateInput) gateInput.required = true;
            } else {
                if (triggerReasonGroup) triggerReasonGroup.style.display = 'none';
                if (eventDescriptionLabel) eventDescriptionLabel.textContent = '事项描述:';
                if (document.getElementById('eventDescription')) document.getElementById('eventDescription').placeholder = '请输入事项描述';
                if (document.getElementById('triggerReason')) document.getElementById('triggerReason').required = false;
                if (gateInput) gateInput.required = false;
            }
        });
    }

    // Close modal when close button is clicked
    if (closeBtn) {
        closeBtn.addEventListener('click', closeModal);
    }
    if (cancelEventBtn) {
        cancelEventBtn.addEventListener('click', closeModal);
    }

    // Close modal when clicking outside of modal content
    window.addEventListener('click', function (event) {
        if (event.target === eventCreationModal) {
            closeModal();
        }
    });

    // Handle form submission
    if (eventCreationForm) {
        eventCreationForm.addEventListener('submit', async function (e) {
            e.preventDefault();

            // Get form values
            const event_type = document.getElementById('eventType').value;
            const eventStatus = String(document.getElementById('eventStatus').value || '').trim().toUpperCase();
            const eventDescription = document.getElementById('eventDescription').value.trim();
            const bindingSelection = document.getElementById(BUSINESS_CASE_BINDING_SELECT_ID).value;
            const stand_no = document.getElementById('stand_no').value.trim();
            const gate_no = document.getElementById('gate_no').value.trim();
            const triggerReasonEl = document.getElementById('triggerReason');
            const triggerReason = triggerReasonEl ? triggerReasonEl.value.trim() : '';
            const binding = parseBusinessCaseBindingValue(bindingSelection);

            if (selectedFlightId === null || selectedFlightId === undefined || String(selectedFlightId).trim() === '') {
                showToast('请先选择一个航班', 'warning');
                return;
            }
            if (!event_type) {
                showToast('请选择事项类型', 'warning');
                return;
            }
            if (!BUSINESS_CASE_SUPPORTED_STATUSES.includes(eventStatus)) {
                showToast('事项状态不合法', 'warning');
                return;
            }
            if (!eventDescription) {
                showToast('请输入事项描述', 'warning');
                return;
            }
            if (!binding) {
                showToast('请选择绑定的航班号', 'warning');
                return;
            }

            // Prepare business case data (使用新的 BusinessCase API)
            let caseData;
            if (event_type === 'gate_baggage_check') {
                caseData = {
                    case_type: event_type,
                    flight_id: selectedFlightId,
                    status: eventStatus,
                    description: triggerReason ? `[${triggerReason}] ${eventDescription}` : eventDescription,
                    context: {
                        bound_leg_type: binding.legType,
                        bound_flight_no: binding.flightNo,
                        gate: gate_no || null,
                        trigger_reason: triggerReason,
                        extra_info: eventDescription || null,
                        stand_no: stand_no || null,
                        gate_no: gate_no || null
                    }
                };
            } else {
                caseData = {
                    case_type: event_type,
                    flight_id: selectedFlightId,
                    status: eventStatus,
                    description: eventDescription,
                    context: {
                        bound_leg_type: binding.legType,
                        bound_flight_no: binding.flightNo,
                        stand_no: stand_no || null,
                        gate_no: gate_no || null
                    }
                };
            }

            try {
                // Show loading state
                const submitBtn = document.getElementById('submitEventBtn');
                submitBtn.textContent = '创建中...';
                submitBtn.disabled = true;

                if (typeof Auth === 'undefined' || typeof Auth.fetch !== 'function') {
                    throw new Error('认证模块不可用');
                }

                // Send request to create business case
                const response = await Auth.fetch(`${API_BASE}/business-cases`, {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json'
                    },
                    body: JSON.stringify(caseData)
                });

                const result = await response.json();

                if (result.success) {
                    showToast('业务事项创建成功', 'success');
                    if (result.data && typeof window.applyBusinessCaseSummaryToCurrentFlight === 'function') {
                        window.applyBusinessCaseSummaryToCurrentFlight(result.data);
                    }
                    closeModal();

                    // Refresh the flight data to show the new case
                    if (selectedFlightId !== null) {
                        renderFlightDetail();
                    } else {
                        loadFlights();
                    }
                } else {
                    showToast(`创建失败: ${result.message || '未知错误'}`, 'error', 4200);
                }
            } catch (error) {
                console.error('Error creating business case:', error);
                showToast('创建业务事项时发生错误: ' + error.message, 'error', 4600);
            } finally {
                // Restore button state
                const submitBtn = document.getElementById('submitEventBtn');
                submitBtn.textContent = '创建业务事项';
                submitBtn.disabled = false;
            }
        });
    }
}

function closeModal() {
    const eventCreationModal = document.getElementById('eventCreationModal');
    if (eventCreationModal) {
        closeManagedModal(eventCreationModal);
    }
    // Reset form
    const eventCreationForm = document.getElementById('eventCreationForm');
    if (eventCreationForm) {
        eventCreationForm.reset();
    }
    const bindingSelect = document.getElementById(BUSINESS_CASE_BINDING_SELECT_ID);
    if (bindingSelect) {
        bindingSelect.innerHTML = '<option value="">请选择绑定航班号</option>';
        bindingSelect.disabled = false;
    }
    
    // Reset dynamic UI changes 
    const triggerReasonGroup = document.getElementById('triggerReasonGroup');
    const eventDescriptionLabel = document.getElementById('eventDescriptionLabel');
    const gateInput = document.getElementById('gate_no');
    if (triggerReasonGroup) triggerReasonGroup.style.display = 'none';
    if (eventDescriptionLabel) eventDescriptionLabel.textContent = '事项描述:';
    if (document.getElementById('eventDescription')) document.getElementById('eventDescription').placeholder = '请输入事项描述';
    if (document.getElementById('triggerReason')) document.getElementById('triggerReason').required = false;
    if (gateInput) gateInput.required = false;
    const submitBtn = document.getElementById('submitEventBtn');
    if (submitBtn instanceof HTMLButtonElement) {
        submitBtn.disabled = false;
        submitBtn.title = '';
    }
}

window.createNewEvent = createNewEvent;

function renderFlights() {
    if (currentView === 'table') {
        renderFlightTable();
    } else if (currentView === 'alert') {
        renderAlertPoolView();
    } else {
        renderFlightList();
    }
    updateAnomalyFloatingButton();
}

function renderFlightTable(options = {}) {
    const { forceHeaderRebuild = false } = options;

    const tableContainer = document.getElementById('flightTableContainer');
    if (!tableContainer || tableContainer.style.display === 'none') return;

    const table = document.getElementById('flightTable');
    if (!table) return;
    table.setAttribute('role', 'grid');

    // Cancel any in-flight chunk render from previous table render
    currentRenderTaskId += 1;

    const columnIds = getRenderableTableColumns();
    ensureTableHeader(table, columnIds, forceHeaderRebuild);

    const tbody = table.querySelector('tbody');
    if (!tbody) return;
    tbody.setAttribute('role', 'rowgroup');

    // Reset body quickly
    tbody.innerHTML = '';

    if (flights.length === 0) {
        tbody.innerHTML = '<tr role="row"><td role="gridcell" colspan="100" style="text-align:center; padding: 20px;">未找到匹配的航班</td></tr>';
        destroyTableVirtualScroller();
        return;
    }

    const wrapper = tableContainer.querySelector('.table-scroll-wrapper');
    const shouldUseVirtual = flights.length > TABLE_SYNC_RENDER_THRESHOLD;

    if (shouldUseVirtual && wrapper) {
        ensureTableVirtualScroller(wrapper, tbody);
        tableVirtualScroller.setData(flights, columnIds);
        return;
    }

    destroyTableVirtualScroller();

    let rowsHtml = '';
    for (let i = 0; i < flights.length; i++) {
        rowsHtml += createTableRowHtml(flights[i], columnIds);
    }
    tbody.innerHTML = rowsHtml;
}

function renderColumnConfigList() {
    const list = document.getElementById('columnConfigList');
    list.innerHTML = '';

    tableConfig.columnOrder.forEach(colId => {
        const colDef = DEFAULT_COLUMNS[colId];
        if (!colDef) return;

        const item = document.createElement('div');
        item.className = 'column-config-item';
        item.dataset.columnId = colId;
        item.draggable = true; // Allow reordering in modal too

        item.innerHTML = `
            <div class="column-handle">⋮⋮</div><input type="checkbox" class="column-checkbox" ${tableConfig.visibleColumns.includes(colId) ? 'checked' : ''}><span class="column-label">${colDef.label}</span>`;

        // Add drag listeners for modal items if needed, or keep simple check for now
        // Let's implement simple Drag & Drop for modal items too
        item.addEventListener('dragstart', handleConfigDragStart);
        item.addEventListener('dragover', handleConfigDragOver);
        item.addEventListener('drop', handleConfigDrop);

        list.appendChild(item);
    });
}

function showTimeEditModal(flightId, field, currentValue) {
    timeEditState = { flightId, field };

    const titleEl = document.getElementById('timeModalTitle');
    if (titleEl) {
        const fieldName = FIELD_NAMES[field] || field;
        titleEl.textContent = `修改时间 - ${fieldName}`;
    }

    const modal = document.getElementById('timeEditModal');
    const input = document.getElementById('timeInput');

    if (!modal || !input) return;

    // Set current value if exists
    if (currentValue && currentValue !== 'null' && currentValue !== '') {
        // Format to YYYY-MM-DDTHH:mm for datetime-local input
        try {
            const date = new Date(currentValue);
            // Adjust to local timezone string manually to fit format
            const offset = date.getTimezoneOffset() * 60000;
            const localISOTime = (new Date(date - offset)).toISOString().slice(0, 16);
            input.value = localISOTime;
        } catch (e) {
            console.error("Date parse error", e);
            input.value = '';
        }
    } else {
        input.value = '';
    }

    openManagedModal(modal, '#timeInput');
}

function closeTimeModal() {
    const modal = document.getElementById('timeEditModal');
    if (modal) closeManagedModal(modal);
    timeEditState = { flightId: null, field: null };
}
