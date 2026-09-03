(function () {
    'use strict';

    const state = {
        user: null,
        departments: [],
        task_types: [],
        equipmentTypes: [],
        qualifications: [],
        qualificationLevels: [],
        teams: [],
        equipment: [],
        teamMembersByTeam: {},
        generationRules: [],
        adjustmentRules: [],
        requirementVersions: [],
        temporaryTaskTemplates: [],
        selectedDepartmentId: '',
        activeTab: 'task-driven',
        generationDraft: {
            conditions: {},
        },
        adjustmentDraft: {
            actions: [],
            conditions: {},
        },
        taskTypeDraft: {
            crew_requirements: [],
            equipment_requirements: [],
            turnaround_continuity_rules: [],
        },
        previewResult: null,
        manualDraft: {
            mode: 'task_type',
        },
        generationValidationTimer: null,
        generationValidationValid: true,
        _tdToolTab: 'preview',
        _tdContextTaskCode: '',
        _tdDraftTaskType: '',
        _tdRequirementNotes: null,
        _tdPreviewTaskType: '',
    };

    const ACTION_TEMPLATES = {
        increase_slot_count: '人员槽位数量 +1',
        add_slot: '新增人员槽位',
        upgrade_min_level: '提升最低资质等级',
        extend_duration: '任务时长延长',
        advance_publish_offset: '发布时间提前',
        delay_publish_offset: '发布时间延后',
        increase_equipment_count: '设备数量 +1',
        add_equipment_type_requirement: '新增设备类型需求',
        require_driver_for_equipment: '设备强制配司机',
    };

    const FLIGHT_NATURE_OPTIONS = [
        { value: 'domestic', label: '国内' },
        { value: 'intl', label: '国际' },
        { value: 'region', label: '地区' },
    ];

    const FLIGHT_STATUS_OPTIONS = [
        { value: 'SCHEDULED', label: '计划中' },
        { value: 'PREV_DEPARTED', label: '前站起飞' },
        { value: 'ARRIVED', label: '到达本站' },
        { value: 'CHECK_IN_END', label: '值机结束' },
        { value: 'BOARDING', label: '登机' },
        { value: 'BOARDING_URGE', label: '催促登机' },
        { value: 'BOARDING_END', label: '结束登机' },
        { value: 'DEPARTED', label: '已起飞' },
        { value: 'NEXT_ARRIVED', label: '到下站' },
        { value: 'CANCELLED', label: '取消' },
        { value: 'DELAYED', label: '延误' },
    ];

    document.addEventListener('DOMContentLoaded', bootstrap);

    async function bootstrap() {
        const user = await checkAuth();
        if (!user) {
            window.location.href = '/frontend/html/login.html';
            return;
        }

        state.user = user;
        state.isAdmin = user.is_admin === true || user.role === 'admin';
        state.userDepartment = String(user.department || '').trim();

        const userNode = document.getElementById('ruleCenterUser');
        if (userNode) {
            userNode.textContent = user.username || '当前用户';
        }

        bindBaseEvents();
        await loadBaseOptions();

        // 科室权限控制
        if (!state.isAdmin && state.userDepartment) {
            const departmentSelect = document.getElementById('departmentSelect');
            if (departmentSelect) {
                departmentSelect.value = state.userDepartment;
                departmentSelect.disabled = true;
            }
            state.selectedDepartmentId = state.userDepartment;
            await loadDepartmentData(state.userDepartment);
        } else if (!state.isAdmin && !state.userDepartment) {
            setGlobalStatus('您未归属任何科室，无法编辑规则。请联系管理员设置您的科室。', 'warn');
        }

        renderAllPanels();
    }

    function bindBaseEvents() {
        const backBtn = document.getElementById('backToBoardBtn');
        if (backBtn) {
            backBtn.addEventListener('click', () => {
                window.location.href = '/frontend/html/dispatch_board.html';
            });
        }

        const reloadBtn = document.getElementById('reloadDepartmentBtn');
        if (reloadBtn) {
            reloadBtn.addEventListener('click', async () => {
                const select = document.getElementById('departmentSelect');
                state.selectedDepartmentId = String(select && select.value || '').trim();
                if (!state.selectedDepartmentId) {
                    setGlobalStatus('请选择科室后再加载。', 'warn');
                    return;
                }
                await loadBaseOptions();
                await loadDepartmentData(state.selectedDepartmentId);
                renderAllPanels();
            });
        }

        // Hide step-nav — task-driven view is the only view
        const stepNav = document.getElementById('stepNav');
        if (stepNav) stepNav.style.display = 'none';
    }

    async function loadBaseOptions() {
        setGlobalStatus('正在加载基础主数据...', '');
        const [departments, task_types, equipmentTypes] = await Promise.all([
            apiGet('/api/v2/reference/departments'),
            apiGet('/api/v2/dispatch/task-types'),
            apiGet('/api/v2/dispatch/resources/equipment-types'),
        ]);
        state.departments = Array.isArray(departments) ? departments : [];
        state.task_types = Array.isArray(task_types) ? task_types : [];
        state.equipmentTypes = Array.isArray(equipmentTypes) ? equipmentTypes : [];

        const departmentSelect = document.getElementById('departmentSelect');
        if (departmentSelect) {
            const allOption = state.isAdmin ? '<option value="__all__">全部科室</option>' : '';
            departmentSelect.innerHTML = '<option value="">请选择科室</option>' + allOption + state.departments.map((item) => {
                return `<option value="${escapeHtml(item.id)}">${escapeHtml(item.name || item.department_name || item.id)}</option>`;
            }).join('');
            if (!state.selectedDepartmentId && state.departments.length) {
                state.selectedDepartmentId = state.isAdmin ? '__all__' : String(state.departments[0].id || '');
                departmentSelect.value = state.selectedDepartmentId;
                await loadDepartmentData(state.selectedDepartmentId);
            } else {
                syncDepartmentSelectValue();
            }
        }
        setGlobalStatus('基础主数据已加载。', 'success');
    }

    async function loadDepartmentData(departmentId) {
        if (departmentId === '__all__') {
            return await loadAllDepartmentsData();
        }
        setGlobalStatus('正在加载科室规则、资质和模板...', '');
        const base = `/api/v2/dispatch/rules/departments/${encodeURIComponent(departmentId)}`;
        const department = state.departments.find((item) => String(item.id) === String(departmentId));
        const terminal = department && department.terminal ? `&terminal=${encodeURIComponent(department.terminal)}` : '';
        const [qualifications, levels, generationRules, adjustmentRules, requirementVersions, templates, teams, equipment] = await Promise.all([
            apiGet(`${base}/qualifications`),
            apiGet(`${base}/qualification-levels`),
            apiGet(`${base}/flight-generation-rules`),
            apiGet(`${base}/generation-adjustment-rules`),
            apiGet(`${base}/task-type-requirements/versions`),
            apiGet(`${base}/temporary-task-templates`),
            apiGet(`/api/v2/dispatch/resources/teams?page_size=200${terminal}`),
            apiGet(`/api/v2/dispatch/resources/equipment?page_size=200${terminal}`),
        ]);
        state.qualifications = Array.isArray(qualifications) ? qualifications : [];
        state.qualificationLevels = Array.isArray(levels) ? levels : [];
        state.teams = Array.isArray(teams) ? teams : [];
        state.equipment = Array.isArray(equipment) ? equipment : [];
        state.generationRules = Array.isArray(generationRules) ? generationRules : [];
        state.adjustmentRules = Array.isArray(adjustmentRules) ? adjustmentRules : [];
        state.requirementVersions = Array.isArray(requirementVersions) ? requirementVersions : [];
        state.temporaryTaskTemplates = Array.isArray(templates) ? templates : [];
        state.teamMembersByTeam = {};
        setGlobalStatus('科室规则已加载。', 'success');
    }

    async function loadAllDepartmentsData() {
        setGlobalStatus('正在加载全部科室规则...', '');
        state.generationRules = [];
        state.adjustmentRules = [];
        state.requirementVersions = [];
        state.qualifications = [];
        state.qualificationLevels = [];
        state.temporaryTaskTemplates = [];
        state.teams = [];
        state.equipment = [];
        state.teamMembersByTeam = {};

        const results = await Promise.allSettled(
            state.departments.map(async (dept) => {
                const base = `/api/v2/dispatch/rules/departments/${encodeURIComponent(dept.id)}`;
                const terminal = dept.terminal ? `&terminal=${encodeURIComponent(dept.terminal)}` : '';
                const [gen, adj, req, qual, levels, tmpl, teams, equip] = await Promise.all([
                    apiGet(`${base}/flight-generation-rules`),
                    apiGet(`${base}/generation-adjustment-rules`),
                    apiGet(`${base}/task-type-requirements/versions`),
                    apiGet(`${base}/qualifications`),
                    apiGet(`${base}/qualification-levels`),
                    apiGet(`${base}/temporary-task-templates`),
                    apiGet(`/api/v2/dispatch/resources/teams?page_size=200${terminal}`),
                    apiGet(`/api/v2/dispatch/resources/equipment?page_size=200${terminal}`),
                ]);
                return { gen, adj, req, qual, levels, tmpl, teams, equip, deptId: dept.id };
            })
        );

        for (const r of results) {
            if (r.status !== 'fulfilled') continue;
            const d = r.value;
            state.generationRules.push(...(Array.isArray(d.gen) ? d.gen : []));
            state.adjustmentRules.push(...(Array.isArray(d.adj) ? d.adj : []));
            state.requirementVersions.push(...(Array.isArray(d.req) ? d.req : []));
            state.qualifications.push(...(Array.isArray(d.qual) ? d.qual : []));
            state.qualificationLevels.push(...(Array.isArray(d.levels) ? d.levels : []));
            state.temporaryTaskTemplates.push(...(Array.isArray(d.tmpl) ? d.tmpl : []));
            state.teams.push(...(Array.isArray(d.teams) ? d.teams : []));
            state.equipment.push(...(Array.isArray(d.equip) ? d.equip : []));
        }
        setGlobalStatus(`已加载全部 ${results.filter(r => r.status === 'fulfilled').length} 个科室的规则。`, 'success');
    }

    function renderAllPanels() {
        renderTaskDrivenPanel();
        for (const id of ['panel-generation', 'panel-adjustment', 'panel-requirements', 'panel-turnaround', 'panel-preview', 'panel-manual']) {
            const panel = document.getElementById(id);
            if (panel) {
                panel.innerHTML = '';
                panel.classList.remove('active');
            }
        }
    }

    function renderTaskDrivenPanel() {
        const panel = document.getElementById('panel-task-driven');
        if (!panel) return;

        const taskTypes = getVisibleTaskTypes();
        const canEdit = state.isAdmin || (state.userDepartment && state.selectedDepartmentId === state.userDepartment);

        if (!state.selectedDepartmentId) {
            panel.innerHTML = `
                <div class="section-title">任务视图</div>
                <div class="info-banner">请先选择科室并点击「加载」按钮。</div>
            `;
            return;
        }

        // ---- build per-task-type data ----
        const taskTypeData = taskTypes.map((tt) => {
            const typeCode = String(tt.code || tt.task_type || tt.id || '').trim();
            const typeName = String(tt.name || tt.task_type_name || tt.label || typeCode);
            const genRules = (state.generationRules || []).filter(r =>
                String(r.task_type || '').trim() === typeCode && String(r.status || '').trim() !== 'archived'
            );
            const adjRules = (state.adjustmentRules || []).filter(r => {
                const targets = Array.isArray(r.target_task_types) ? r.target_task_types : [r.target_task_type || r.task_type];
                return targets.some(t => String(t || '').trim() === typeCode);
            });
            const reqVersions = (state.requirementVersions || []).filter(r => String(r.task_type || '').trim() === typeCode);
            return { code: typeCode, name: typeName, genRules, adjRules, reqVersions };
        });

        // ---- persist selected code ----
        if (!state._tdSelectedCode) state._tdSelectedCode = (taskTypeData[0] || {}).code || '';
        if (!state._tdActiveTab) state._tdActiveTab = 'gen';
        if (!state._tdSearchQuery) state._tdSearchQuery = '';

        const query = state._tdSearchQuery.toLowerCase();
        const filtered = query
            ? taskTypeData.filter(t => t.name.toLowerCase().includes(query) || t.code.toLowerCase().includes(query))
            : taskTypeData;

        const selectedTT = taskTypeData.find(t => t.code === state._tdSelectedCode) || filtered[0] || null;
        if (!taskTypeData.length) {
            state._tdSelectedCode = '';
        }
        if (selectedTT) state._tdSelectedCode = selectedTT.code;

        // ---- left list ----
        const taskListHtml = filtered.length
            ? filtered.map((tt) => {
                const ruleCount = tt.genRules.length + tt.adjRules.length;
                const reqCount = tt.reqVersions.length;
                const isActive = selectedTT && tt.code === selectedTT.code;
                return `
                    <button class="task-type-item ${isActive ? 'active' : ''}" data-task-code="${escapeHtml(tt.code)}">
                        <span class="task-type-name">${escapeHtml(tt.name)}</span>
                        <span class="task-type-badge">
                            ${ruleCount > 0 ? `<span title="规则数">${ruleCount}</span>` : ''}
                            ${reqCount > 0 ? `<span title="资质版本">${reqCount}</span>` : ''}
                            ${(ruleCount + reqCount) === 0 ? '<span class="empty">○</span>' : ''}
                        </span>
                    </button>
                `;
            }).join('')
            : `<div class="empty-hint">${
                taskTypeData.length
                    ? '没有匹配的任务类型，请调整搜索条件。'
                    : (state.isAdmin
                        ? '当前科室暂无任务类型。请先点击“＋ 新增”，并归属到当前科室。'
                        : '当前科室暂无任务类型，请联系管理员先新增并归属任务类型。')
            }</div>`;

        const taskDetailHtml = taskTypeData.length
            ? renderTaskTypeDetail(selectedTT, canEdit)
            : `<div class="empty-hint">${
                state.isAdmin
                    ? '当前科室暂无任务类型。请先在左侧新增并归属到当前科室，再配置基础规则、增量规则和资质要求。'
                    : '当前科室暂无任务类型，请联系管理员先新增并归属任务类型后再配置规则。'
            }</div>`;

        panel.innerHTML = `
            ${!canEdit ? '<div class="info-banner warning">当前科室非您的归属科室，仅可查看，无法编辑。</div>' : ''}
            <div class="task-driven-layout">
                <div class="task-type-list">
                    <div class="task-type-toolbar">
                        <input type="text" id="tdSearchInput" class="td-search-input" placeholder="搜索任务类型…" value="${escapeHtml(state._tdSearchQuery)}" />
                        ${state.isAdmin ? '<button id="tdAddTaskTypeBtn" class="btn btn-sm btn-accent task-type-add-btn" title="新增任务类型">＋ 新增</button>' : ''}
                    </div>
                    <div id="tdCreateTaskTypeForm" style="display:none;padding:10px;margin-bottom:8px;border:1px solid var(--rc-border);border-radius:8px;background:var(--rc-card-bg)">
                        <div style="font-weight:600;font-size:13px;margin-bottom:8px">新建任务类型</div>
                        <label style="display:block;font-size:12px;margin-bottom:4px">编码 <span style="color:#e53e3e">*</span></label>
                        <input type="text" id="ttCreateCode" placeholder="如 cabin_clean" style="width:100%;margin-bottom:6px;padding:5px 8px;border:1px solid var(--rc-border);border-radius:6px;font-size:12px" />
                        <label style="display:block;font-size:12px;margin-bottom:4px">名称 <span style="color:#e53e3e">*</span></label>
                        <input type="text" id="ttCreateName" placeholder="如 客舱清洁" style="width:100%;margin-bottom:6px;padding:5px 8px;border:1px solid var(--rc-border);border-radius:6px;font-size:12px" />
                        <label style="display:block;font-size:12px;margin-bottom:4px">归属科室</label>
                        <select id="ttCreateDept" style="width:100%;margin-bottom:8px;padding:5px 8px;border:1px solid var(--rc-border);border-radius:6px;font-size:12px">
                            <option value="">不指定</option>
                            ${(state.departments || []).map(d => `<option value="${escapeHtml(d.id)}"${String(d.id) === getDefaultTaskTypeDepartmentId() ? ' selected' : ''}>${escapeHtml(d.name || d.id)}</option>`).join('')}
                        </select>
                        <div style="display:flex;gap:6px;justify-content:flex-end">
                            <button id="ttCreateCancel" class="btn btn-sm" style="font-size:12px;padding:5px 12px">取消</button>
                            <button id="ttCreateSubmit" class="btn btn-sm btn-accent" style="font-size:12px;padding:5px 12px">创建</button>
                        </div>
                    </div>
                    <div id="taskTypeListInner">${taskListHtml}</div>
                </div>
                <div class="task-type-detail" id="taskTypeDetail">
                    ${taskDetailHtml}
                </div>
            </div>
        `;

        // ---- bind search ----
        const searchInput = document.getElementById('tdSearchInput');
        if (searchInput) {
            searchInput.addEventListener('input', () => {
                state._tdSearchQuery = searchInput.value;
                renderTaskDrivenPanel();
            });
        }

        // ---- bind add task type ----
        const addBtn = document.getElementById('tdAddTaskTypeBtn');
        const createForm = document.getElementById('tdCreateTaskTypeForm');
        if (addBtn && createForm) {
            addBtn.addEventListener('click', () => {
                createForm.style.display = createForm.style.display === 'none' ? '' : 'none';
            });
            const cancelBtn = document.getElementById('ttCreateCancel');
            if (cancelBtn) cancelBtn.addEventListener('click', () => { createForm.style.display = 'none'; });

            const submitBtn = document.getElementById('ttCreateSubmit');
            if (submitBtn) {
                submitBtn.addEventListener('click', async () => {
                    const code = (document.getElementById('ttCreateCode').value || '').trim();
                    const name = (document.getElementById('ttCreateName').value || '').trim();
                    const deptId = (document.getElementById('ttCreateDept').value || '').trim();
                    if (!code || !name) {
                        setGlobalStatus('编码和名称不能为空。', 'warn');
                        return;
                    }
                    try {
                        submitBtn.disabled = true;
                        submitBtn.textContent = '创建中…';
                        await apiPost('/api/v2/dispatch/task-types', {
                            code,
                            name,
                            default_department_id: deptId || null,
                        });
                        setGlobalStatus(`任务类型「${name}」已创建。`, 'success');
                        const nextDepartmentId = deptId || String(state.selectedDepartmentId || '').trim();
                        if (nextDepartmentId) {
                            state.selectedDepartmentId = nextDepartmentId;
                        }
                        syncDepartmentSelectValue();
                        await loadBaseOptions();
                        if (hasSelectedDepartment()) {
                            await loadDepartmentData(state.selectedDepartmentId);
                        }
                        state._tdSelectedCode = code;
                        document.getElementById('ttCreateCode').value = '';
                        document.getElementById('ttCreateName').value = '';
                        document.getElementById('ttCreateDept').value = getDefaultTaskTypeDepartmentId();
                        createForm.style.display = 'none';
                        renderAllPanels();
                    } catch (err) {
                        setGlobalStatus('创建任务类型失败: ' + (err.message || err), 'error');
                    } finally {
                        submitBtn.disabled = false;
                        submitBtn.textContent = '创建';
                    }
                });
            }
        }


        // ---- bind task type clicks ----
        const listInner = document.getElementById('taskTypeListInner');
        if (listInner) {
            listInner.addEventListener('click', (e) => {
                const item = e.target.closest('.task-type-item[data-task-code]');
                if (!item) return;
                state._tdSelectedCode = item.dataset.taskCode;
                state._tdEditingGen = null;
                state._tdEditingAdj = null;
                renderTaskDrivenPanel();
            });
        }
    }

    function renderTaskTypeDetail(tt, canEdit) {
        if (!tt) return '<div class="empty-hint">请从左侧选择一个任务类型。</div>';

        syncTaskDetailContext(tt);

        const activeTab = state._tdActiveTab || 'gen';
        const genCount = tt.genRules.length;
        const adjCount = tt.adjRules.length;
        const reqCount = tt.reqVersions.length;
        const turnaroundCount = state.taskTypeDraft.turnaround_continuity_rules.length;

        const tabBar = `
            <div class="td-tab-bar">
                <button class="td-tab ${activeTab === 'gen' ? 'active' : ''}" data-td-tab="gen">触发规则 (${genCount})</button>
                <button class="td-tab ${activeTab === 'adj' ? 'active' : ''}" data-td-tab="adj">调整规则 (${adjCount})</button>
                <button class="td-tab ${activeTab === 'req' ? 'active' : ''}" data-td-tab="req">资质与设备 (${reqCount})</button>
                <button class="td-tab ${activeTab === 'turn' ? 'active' : ''}" data-td-tab="turn">过站约束 (${turnaroundCount})</button>
                <button class="td-tab ${activeTab === 'tools' ? 'active' : ''}" data-td-tab="tools">工具</button>
            </div>
        `;

        let content = '';
        if (activeTab === 'gen') {
            content = renderGenRulesTab(tt, canEdit);
        } else if (activeTab === 'adj') {
            content = renderAdjRulesTab(tt, canEdit);
        } else if (activeTab === 'req') {
            content = renderReqTab(tt, canEdit);
        } else if (activeTab === 'turn') {
            content = renderTurnaroundTab(tt, canEdit);
        } else if (activeTab === 'tools') {
            content = renderToolsTab(tt, canEdit);
        }

        const html = `
            <div class="td-detail-header" style="display:flex; justify-content:space-between; align-items:flex-start;">
                <div>
                    <h3 style="margin:0; font-size:18px; font-weight:700">${escapeHtml(tt.name)} <span class="muted" style="font-size:12px;font-weight:400">${escapeHtml(tt.code)}</span></h3>
                    <div class="muted">当前科室：${escapeHtml(selectedDepartmentName())}</div>
                </div>
                ${canEdit && state.isAdmin ? `
                <div>
                    <button class="btn btn-sm" id="tdDelTaskTypeBtn" style="color:#e53e3e; border:1px solid #fc8181; background:transparent; padding: 4px 8px; border-radius: 4px; cursor: pointer;" title="删除该任务类型">删除作业类型</button>
                </div>
                ` : ''}
            </div>
            ${tabBar}
            <div class="td-tab-content" id="tdTabContent">
                ${content}
            </div>
        `;

        setTimeout(() => bindTaskDetailEvents(tt, canEdit), 0);
        return html;
    }

    function syncTaskDetailContext(tt) {
        if (!tt || !tt.code) {
            return;
        }
        if (state._tdContextTaskCode === tt.code) {
            if (!state._tdToolTab) {
                state._tdToolTab = 'preview';
            }
            return;
        }
        state._tdContextTaskCode = tt.code;
        state._tdPreviewTaskType = tt.code;
        state._tdToolTab = state._tdToolTab || 'preview';
        state._tdEditingGen = null;
        state._tdEditingAdj = null;
        state.adjustmentDraft.actions = [];
        state.adjustmentDraft.conditions = {};
        ensureTaskTypeDraftFor(tt.code);
    }

    function ensureTaskTypeDraftFor(taskType) {
        const normalized = String(taskType || '').trim();
        if (!normalized || state._tdDraftTaskType === normalized) {
            return;
        }
        const version = findLatestPublishedRequirement(normalized);
        if (!version) {
            state.taskTypeDraft.crew_requirements = [defaultCrewRequirement()];
            state.taskTypeDraft.equipment_requirements = [defaultEquipmentRequirement()];
            state.taskTypeDraft.turnaround_continuity_rules = [];
            state._tdRequirementNotes = null;
            state._tdDraftTaskType = normalized;
            return;
        }
        state.taskTypeDraft.crew_requirements = (version.crew_requirements || []).map((item) => ({ ...item }));
        state.taskTypeDraft.equipment_requirements = (version.equipment_requirements || []).map((item) => ({ ...item }));
        state.taskTypeDraft.turnaround_continuity_rules = (version.turnaround_continuity_rules || []).map((item) => ({ ...item }));
        state._tdRequirementNotes = version.notes || null;
        state._tdDraftTaskType = normalized;
        ensureRequirementRows();
    }

    // ---------- Sub-tab: Generation Rules ----------
    function renderGenRulesTab(tt, canEdit) {
        const cards = tt.genRules.map((r, idx) => {
            const condStr = summarizeConditions(r.conditions);
            const anchor = r.generation_anchor_type || r.anchor_type || '';
            const offset = r.start_offset_minutes ?? r.offset_minutes ?? '';
            const dur = r.duration_minutes ?? '';
            return `
                <div class="task-rule-card" data-rule-idx="${idx}">
                    <div class="task-rule-header">
                        <span class="task-rule-tag gen">触发</span>
                        <strong>${escapeHtml(r.rule_name || r.name || '未命名')}</strong>
                        <span class="task-rule-status ${(r.status || '') === 'published' ? 'active' : 'inactive'}">${(r.status || '') === 'published' ? '已发布' : '草稿'}</span>
                        ${canEdit ? `<span class="task-rule-actions">
                            <button class="tiny-btn td-edit-gen" data-idx="${idx}" title="编辑">✎</button>
                            <button class="tiny-btn danger td-del-gen" data-idx="${idx}" title="删除">✕</button>
                        </span>` : ''}
                    </div>
                    <div class="task-rule-meta">
                        ${condStr ? `<span title="条件">📋 ${escapeHtml(condStr)}</span>` : ''}
                        ${anchor ? `<span>锚点: ${escapeHtml(anchor)}</span>` : ''}
                        ${offset !== '' ? `<span>偏移: ${offset}分钟</span>` : ''}
                        ${dur !== '' ? `<span>时长: ${dur}分钟</span>` : ''}
                    </div>
                </div>
            `;
        }).join('');

        const addBtn = canEdit ? `<button class="section-btn primary td-add-gen" style="margin-top:12px">+ 新增触发规则</button>` : '';

        // Inline editing form (if active)
        const editForm = (state._tdEditingGen != null) ? renderGenEditForm(tt) : '';

        return `
            ${cards || '<div class="empty-hint">暂无该任务类型的触发规则</div>'}
            ${editForm}
            ${!state._tdEditingGen && state._tdEditingGen !== 0 ? addBtn : ''}
            <div id="tdGenStatus" class="status-line" style="margin-top:8px"></div>
        `;
    }

    function renderGenEditForm(tt) {
        const editIdx = state._tdEditingGen;
        const isNew = editIdx === -1;
        const rule = isNew ? {} : (tt.genRules[editIdx] || {});

        return `
            <div class="task-edit-form" id="tdGenForm">
                <h4>${isNew ? '新建触发规则' : '编辑触发规则'} <span class="muted">任务: ${escapeHtml(tt.name)}</span></h4>
                <div class="form-grid">
                    <label>规则名称<input id="tdGenRuleName" value="${escapeHtml(rule.rule_name || rule.name || '')}" placeholder="例：国内航班触发"></label>
                    <label>作业类型<input id="tdGenTaskType" value="${escapeHtml(tt.code)}" disabled></label>
                    <label>进出港<select id="tdGenLegScope">
                        <option value="inbound"${(rule.leg_scope || 'inbound') === 'inbound' ? ' selected' : ''}>进港</option>
                        <option value="outbound"${rule.leg_scope === 'outbound' ? ' selected' : ''}>出港</option>
                        <option value="both"${rule.leg_scope === 'both' ? ' selected' : ''}>进出港</option>
                    </select></label>
                    <label>状态<select id="tdGenStatus">
                        <option value="draft"${(rule.status || 'draft') === 'draft' ? ' selected' : ''}>草稿</option>
                        <option value="published"${rule.status === 'published' ? ' selected' : ''}>发布</option>
                    </select></label>
                </div>
                <div class="form-grid">
                    <label>锚点类型<select id="tdGenAnchor">
                        <option value="scheduled_time"${(rule.generation_anchor_type || 'scheduled_time') === 'scheduled_time' ? ' selected' : ''}>计划时间</option>
                        <option value="estimated_time"${rule.generation_anchor_type === 'estimated_time' ? ' selected' : ''}>预计时间</option>
                        <option value="actual_time"${rule.generation_anchor_type === 'actual_time' ? ' selected' : ''}>实际时间</option>
                    </select></label>
                    <label>偏移(分钟)<input id="tdGenOffset" type="number" value="${rule.start_offset_minutes ?? rule.offset_minutes ?? 0}"></label>
                    <label>时长(分钟)<input id="tdGenDuration" type="number" value="${rule.duration_minutes ?? ''}"></label>
                </div>
                <div class="form-grid">
                    <label>发布状态<select id="tdGenPubState">
                        <option value="prepublished"${(rule.publication_state || 'prepublished') === 'prepublished' ? ' selected' : ''}>预发布</option>
                        <option value="published"${rule.publication_state === 'published' ? ' selected' : ''}>已发布</option>
                    </select></label>
                    <label>发布触发<select id="tdGenPubMode">
                        <option value="time"${(rule.publish_trigger_mode || 'time') === 'time' ? ' selected' : ''}>时间触发</option>
                        <option value="event"${rule.publish_trigger_mode === 'event' ? ' selected' : ''}>事件触发</option>
                    </select></label>
                    <label>发布偏移(分钟)<input id="tdGenPubOffset" type="number" value="${rule.publish_offset_minutes ?? ''}"></label>
                </div>
                <div style="display:flex; gap:12px; margin-top:16px">
                    <button class="section-btn primary" id="tdGenSaveBtn">${isNew ? '创建' : '保存'}</button>
                    <button class="section-btn" id="tdGenCancelBtn">取消</button>
                </div>
            </div>
        `;
    }

    // ---------- Sub-tab: Adjustment Rules ----------
    function renderAdjRulesTab(tt, canEdit) {
        const cards = tt.adjRules.map((r, idx) => {
            const actions = (r.actions || []).map(a => escapeHtml(ACTION_TEMPLATES[a.action_type] || a.action_type || '')).join(', ');
            const condStr = summarizeConditions(r.conditions);
            return `
                <div class="task-rule-card" data-rule-idx="${idx}">
                    <div class="task-rule-header">
                        <span class="task-rule-tag adj">调整</span>
                        <strong>${escapeHtml(r.rule_name || r.name || '未命名')}</strong>
                        <span class="task-rule-status ${(r.status || '') === 'published' ? 'active' : 'inactive'}">${(r.status || '') === 'published' ? '已发布' : '草稿'}</span>
                        ${canEdit ? `<span class="task-rule-actions"><button class="tiny-btn td-edit-adj" data-idx="${idx}" title="编辑">✎</button></span>` : ''}
                    </div>
                    <div class="task-rule-meta">
                        ${condStr ? `<span>📋 ${escapeHtml(condStr)}</span>` : ''}
                        ${actions ? `<span>动作: ${actions}</span>` : ''}
                    </div>
                </div>
            `;
        }).join('');

        const addBtn = canEdit ? '<button class="section-btn primary td-add-adj" style="margin-top:12px">+ 新增调整规则</button>' : '';
        const editForm = (state._tdEditingAdj != null) ? renderAdjEditForm(tt, canEdit) : '';

        return `
            ${cards || '<div class="empty-hint">暂无该任务类型的调整规则</div>'}
            ${editForm}
            ${state._tdEditingAdj == null ? addBtn : ''}
            <div id="adjustmentRuleStatus" class="status-line" style="margin-top:8px"></div>
        `;
    }

    function renderAdjEditForm(tt, canEdit) {
        const editIdx = state._tdEditingAdj;
        const isNew = editIdx === -1;
        const rule = isNew ? {} : (tt.adjRules[editIdx] || {});
        const disabledAttr = canEdit ? '' : ' disabled';

        return `
            <div class="task-edit-form">
                <h4>${isNew ? '新建调整规则' : '编辑调整规则'} <span class="muted">任务: ${escapeHtml(tt.name)}</span></h4>
                <input type="hidden" id="adjustmentRuleId" value="${escapeHtml(rule.id || '')}">
                <input type="hidden" id="adjustmentTaskType" value="${escapeHtml(tt.code)}">
                <div class="form-grid">
                    <label>规则名称<input id="adjustmentRuleName" value="${escapeHtml(rule.rule_name || '')}" placeholder="例：VIP 加派规则"${disabledAttr}></label>
                    <label>作业类型<input value="${escapeHtml(tt.code)}" disabled></label>
                    <label>规则状态
                        <select id="adjustmentStatus"${disabledAttr}>
                            <option value="draft"${(rule.status || 'draft') === 'draft' ? ' selected' : ''}>草稿</option>
                            <option value="published"${rule.status === 'published' ? ' selected' : ''}>发布</option>
                        </select>
                    </label>
                </div>

                <h4 class="form-group-title">附加触发条件</h4>
                <p class="muted">当航班额外满足以下条件时，对已生成的基础任务执行调整动作。</p>
                ${renderConditionBuilder('adjustment', state.adjustmentDraft.conditions)}

                <h4 class="form-group-title">调整动作配置</h4>
                <div class="form-grid columns-3">
                    <label>触发动作类型
                        <select id="adjustmentActionType"${disabledAttr}>
                            ${Object.entries(ACTION_TEMPLATES).map(([value, label]) => `<option value="${escapeHtml(value)}">${escapeHtml(label)}</option>`).join('')}
                        </select>
                    </label>
                    <label>资源槽位编码<input id="adjustmentActionSlotCode" placeholder="如: cleaner_2"${disabledAttr}></label>
                    <label>增减数量/调整时长(分)<input id="adjustmentActionDelta" type="number" value="1"${disabledAttr}></label>
                    <label>最低等级要求<select id="adjustmentActionLevel"${disabledAttr}>${getLevelOptions('')}</select></label>
                    <label>指定资质类型<select id="adjustmentActionQualification"${disabledAttr}>${getQualificationOptions('')}</select></label>
                    <label>指定新增设备类型<select id="adjustmentActionEquipmentType"${disabledAttr}>${getEquipmentTypeOptions('')}</select></label>
                </div>
                ${canEdit ? `
                    <div class="inline-actions" style="margin-top:12px;">
                        <button class="section-btn" id="addAdjustmentActionBtn">+ 将动作加入当前调整列表</button>
                    </div>
                ` : ''}

                <h4 class="form-group-title">当前已添加的动作列表</h4>
                <div class="collection" id="adjustmentActionsList" style="margin-bottom: 20px;"></div>
                ${canEdit ? `
                    <div class="inline-actions">
                        <button class="section-btn primary" id="saveAdjustmentRuleBtn">${isNew ? '创建调整规则' : '保存调整规则'}</button>
                        <button class="section-btn" id="tdAdjCancelBtn">取消</button>
                    </div>
                ` : ''}
            </div>
        `;
    }

    // ---------- Sub-tab: Requirement Versions ----------
    function renderReqTab(tt, canEdit) {
        ensureTaskTypeDraftFor(tt.code);
        const versions = [...tt.reqVersions].sort((left, right) => Number(right.version_no || 0) - Number(left.version_no || 0));
        const listHtml = versions.length
            ? versions.map((item) => `
                <div class="item-card">
                    <strong>v${escapeHtml(item.version_no || 1)}</strong>
                    <div>状态: ${escapeHtml(item.status || '')}</div>
                    <div>人员槽位: ${(item.crew_requirements || []).length} / 设备槽位: ${(item.equipment_requirements || []).length}</div>
                    <div class="muted">${escapeHtml(summarizeRequirementVersion(item))}</div>
                </div>
            `).join('')
            : '<div class="empty-hint">当前任务类型还没有资质与设备要求版本。</div>';
        const disabledAttr = canEdit ? '' : ' disabled';

        return `
            <div class="panel-grid">
                <div>
                    <div class="info-banner">当前任务类型：${escapeHtml(tt.name)} (${escapeHtml(tt.code)})</div>
                    <div class="list-card">
                        <h3>编辑作业类型规则</h3>
                        <div class="form-grid">
                            <label>作业类型<input id="requirementTaskType" value="${escapeHtml(tt.code)}" readonly disabled></label>
                            <label>版本备注<textarea id="requirementNotes" placeholder="说明本次调整的原因"${disabledAttr}>${escapeHtml(state._tdRequirementNotes || '')}</textarea></label>
                        </div>

                        <h4 class="form-group-title">人员资质槽位</h4>
                        <p class="muted">每个槽位代表一个需要分配的人员角色，指定所需资质和最低等级。</p>
                        <table class="builder-table">
                            <thead>
                                <tr><th>槽位名称</th><th>资质要求</th><th>最低等级</th><th>数量</th><th>要求不同人</th><th>操作</th></tr>
                            </thead>
                            <tbody id="crewRequirementRows"></tbody>
                        </table>
                        ${canEdit ? '<button class="section-btn" id="addCrewRequirementBtn" style="margin-top: 8px;">+ 添加人员槽位</button>' : ''}

                        <h4 class="form-group-title">设备类型槽位</h4>
                        <p class="muted">每个槽位代表一类需要调度的设备，可指定是否需要配司机。</p>
                        <table class="builder-table">
                            <thead>
                                <tr><th>槽位名称</th><th>设备类型</th><th>数量</th><th>不同设备</th><th>司机资质</th><th>司机等级</th><th>操作</th></tr>
                            </thead>
                            <tbody id="equipmentRequirementRows"></tbody>
                        </table>
                        ${canEdit ? '<button class="section-btn" id="addEquipmentRequirementBtn" style="margin-top: 8px;">+ 添加设备槽位</button>' : ''}

                        <div class="inline-actions" style="margin-top: 16px;">
                            <button class="section-btn" id="loadLatestRequirementBtn">载入线上最新版本</button>
                            ${canEdit ? '<button class="section-btn" id="saveRequirementDraftBtn">保存草稿</button>' : ''}
                            ${canEdit ? '<button class="section-btn primary" id="publishRequirementBtn">发布为新版本</button>' : ''}
                        </div>
                        <div class="status-line" id="requirementStatus"></div>
                    </div>
                </div>
                <div>
                    <div class="list-card">
                        <h3>版本历史</h3>
                        <div class="collection">${listHtml}</div>
                    </div>
                    <div class="list-card">
                        <h3>版本差异对比</h3>
                        <div class="form-grid">
                            <label>左侧版本<select id="requirementCompareLeft">${buildRequirementVersionOptions(tt.code, 0)}</select></label>
                            <label>右侧版本<select id="requirementCompareRight">${buildRequirementVersionOptions(tt.code, 1)}</select></label>
                        </div>
                        <table class="builder-table">
                            <thead><tr><th>字段</th><th>左侧</th><th>右侧</th></tr></thead>
                            <tbody id="requirementDiffRows"></tbody>
                        </table>
                    </div>
                </div>
            </div>
        `;
    }

    function renderTurnaroundTab(tt, canEdit) {
        ensureTaskTypeDraftFor(tt.code);
        const disabledAttr = canEdit ? '' : ' disabled';

        return `
            <div class="panel-grid">
                <div>
                    <div class="info-banner">过站约束属于当前任务类型的资源配置，将随作业类型规则草稿一并保存。</div>
                    <div class="list-card">
                        <h3>新建过站约束</h3>
                        <input type="hidden" id="requirementTaskType" value="${escapeHtml(tt.code)}">
                        <div class="form-grid">
                            <label>本作业类型<input id="turnaroundTaskType" value="${escapeHtml(tt.code)}" readonly disabled></label>
                            <label>关联作业类型<select id="turnaroundCounterpartTaskType"${disabledAttr}>${getTaskTypeOptions('')}</select></label>
                            <label>关联方向
                                <select id="turnaroundCounterpartLegScope"${disabledAttr}>
                                    <option value="inbound">进港</option>
                                    <option value="outbound" selected>出港</option>
                                </select>
                            </label>
                        </div>

                        <h4 class="form-group-title">人员关联模式</h4>
                        <div class="info-banner warning">模式说明："强制同一人"要求进出港同槽位必须同一人；"偏好同一人"为软约束，排班器会优先但不强制。</div>
                        <div class="form-grid">
                            <label>关联模式
                                <select id="turnaroundConstraintMode"${disabledAttr}>
                                    <option value="same_person">强制同一人</option>
                                    <option value="soft_prefer_same_person">偏好同一人</option>
                                    <option value="handover_required">必须交接</option>
                                    <option value="disabled">无绑定</option>
                                </select>
                            </label>
                            <label>本端槽位<input id="turnaroundInboundSlot" placeholder="如 cleaner_1"${disabledAttr}></label>
                            <label>关联端槽位<input id="turnaroundOutboundSlot" placeholder="如 cleaner_1"${disabledAttr}></label>
                        </div>

                        <details class="advanced-options">
                            <summary>时间门槛与机型过滤</summary>
                            <div class="advanced-content">
                                <div class="form-grid">
                                    <label>紧凑生效最小间隔(分钟)<input id="turnaroundTight" type="number" value="20"${disabledAttr}></label>
                                    <label>放松判定门槛(分钟)<input id="turnaroundRelax" type="number" value="45"${disabledAttr}></label>
                                    <label>机型白名单<input id="turnaroundAircraftFilters" placeholder="A330,B787（留空=全部适用）"${disabledAttr}></label>
                                </div>
                            </div>
                        </details>

                        <h4 class="form-group-title">附加航班条件</h4>
                        <p class="muted">仅当航班满足以下条件时，此过站约束才生效。</p>
                        ${renderConditionBuilder('turnaround', {})}

                        <div class="inline-actions" style="margin-top: 12px;">
                            ${canEdit ? '<button class="section-btn" id="addTurnaroundRuleBtn">+ 添加到约束列表</button>' : ''}
                            ${canEdit ? '<button class="section-btn primary" id="saveTurnaroundRequirementBtn">保存草稿</button>' : ''}
                        </div>
                        <div class="status-line" id="turnaroundStatus"></div>
                    </div>
                </div>
                <div>
                    <div class="list-card">
                        <h3>已添加的过站约束</h3>
                        <div class="collection" id="turnaroundRuleList"></div>
                    </div>
                </div>
            </div>
        `;
    }

    function renderToolsTab(tt, canEdit) {
        const toolTab = state._tdToolTab || 'preview';
        const subTabBar = `
            <div class="td-subtab-bar">
                <button class="td-subtab ${toolTab === 'preview' ? 'active' : ''}" data-td-tool-tab="preview">仿真验证</button>
                <button class="td-subtab ${toolTab === 'manual' ? 'active' : ''}" data-td-tool-tab="manual">临时加单</button>
            </div>
        `;
        const content = toolTab === 'manual'
            ? renderManualToolTab(tt, canEdit)
            : renderPreviewToolTab(tt);
        return `
            ${subTabBar}
            <div class="td-tool-content">${content}</div>
        `;
    }

    function renderPreviewToolTab(tt) {
        const selectedTaskType = String(state._tdPreviewTaskType || tt.code || '').trim();
        const resultText = state.previewResult
            ? formatPreviewResultForDisplay(state.previewResult, selectedTaskType)
            : '点击“执行仿真”查看引擎输出…';
        const disabledAttr = isAllDepartmentsView() ? ' disabled' : '';
        return `
            <div class="panel-grid">
                <div>
                    <div class="info-banner">模拟模式不会产生实际派工单。当前任务类型默认预填为 ${escapeHtml(tt.name)}。</div>
                    ${isAllDepartmentsView() ? '<div class="info-banner warning" style="margin-top:8px;">“全部科室”模式仅支持汇总查看，请切换到具体科室后执行单科室规则预演。</div>' : ''}
                    <div class="list-card">
                        <h3>模拟航班参数</h3>
                        <div class="form-grid columns-3">
                            <label>当前任务类型<select id="previewTaskType">${getTaskTypeOptions(selectedTaskType)}</select></label>
                            <label>航班号<input id="previewFlightId" placeholder="选填"></label>
                            <label>航班方向
                                <select id="previewLegScope">
                                    <option value="inbound">进港</option>
                                    <option value="outbound">出港</option>
                                </select>
                            </label>
                            <label>航班性质<select id="previewFlightNature">${getFlightNatureOptions('domestic', '未指定')}</select></label>
                            <label>航班状态<select id="previewFlightStatus">${getFlightStatusOptions('', '未指定')}</select></label>
                            <label>航站楼<input id="previewTerminal" placeholder="T1"></label>
                            <label>机型<input id="previewAircraftType" placeholder="A320"></label>
                            <label>机位类型<input id="previewStandType" placeholder="remote"></label>
                        </div>
                        <div class="checkbox-row" style="margin-top: 8px;">
                            <label><input type="checkbox" id="previewVip"> VIP</label>
                            <label><input type="checkbox" id="previewTurnaround" checked> 过站</label>
                            <label><input type="checkbox" id="previewBoardingRestriction"> 限制登机</label>
                            <label><input type="checkbox" id="previewQuickTurnaround"> 快速过站</label>
                            <label><input type="checkbox" id="previewCommercialSigned"> 商务签署</label>
                        </div>
                        <details class="advanced-options">
                            <summary>时间参数</summary>
                            <div class="advanced-content">
                                <div class="form-grid">
                                    <label>提早时间(分)<input id="previewDeltaT" type="number" value="60"></label>
                                    <label>最小过站时长(分)<input id="previewMinTurnaround" type="number" value="40"></label>
                                </div>
                            </div>
                        </details>
                        <div class="inline-actions" style="margin-top: 12px;">
                            <button class="section-btn primary" id="runPreviewBtn"${disabledAttr}>执行仿真</button>
                        </div>
                        <div class="status-line" id="previewStatus"></div>
                    </div>
                </div>
                <div>
                    <div class="list-card">
                        <h3>仿真结果</h3>
                        <pre class="preview-box" id="previewResultBox">${escapeHtml(resultText)}</pre>
                    </div>
                </div>
            </div>
        `;
    }

    function renderManualToolTab(tt, canEdit) {
        const mode = state.manualDraft.mode || 'task_type';
        const disabledCreateAttr = isAllDepartmentsView() || !canEdit ? ' disabled' : '';
        return `
            <div class="panel-grid">
                <div>
                    <div class="info-banner">临时加单默认带入当前任务类型 ${escapeHtml(tt.name)}，你仍可手动改成其他任务类型或模板。</div>
                    ${isAllDepartmentsView() ? '<div class="info-banner warning" style="margin-top:8px;">“全部科室”模式不支持直接创建临时工单，请切换到具体科室后再提交。</div>' : ''}
                    <div class="list-card">
                        <h3>创建临时工单</h3>
                        <div class="form-grid">
                            <label>来源方式
                                <select id="manualMode">
                                    <option value="task_type"${mode === 'task_type' ? ' selected' : ''}>基于作业类型</option>
                                    <option value="template"${mode === 'template' ? ' selected' : ''}>基于任务模板</option>
                                </select>
                            </label>
                            <label>航班号(可选)<input id="manualFlightId" placeholder="留空即不绑定"></label>
                            <label>作业类型<select id="manualTaskType">${getTaskTypeOptions(tt.code)}</select></label>
                            <label>任务模板<select id="manualTemplateCode">${getTemplateOptions('')}</select></label>
                        </div>

                        <h4 class="form-group-title">时间与位置</h4>
                        <div class="form-grid">
                            <label>开始时间<input id="manualStartTime" type="datetime-local"></label>
                            <label>结束时间<input id="manualEndTime" type="datetime-local"></label>
                            <label>机位<input id="manualStandId" placeholder="101"></label>
                            <label>位置描述<input id="manualLocation" placeholder="辅助定位"></label>
                        </div>

                        <details class="advanced-options">
                            <summary>优先级与发布策略</summary>
                            <div class="advanced-content">
                                <div class="form-grid">
                                    <label>优先级<input id="manualPriority" type="number" value="50"></label>
                                    <label>发布状态
                                        <select id="manualPublicationState">
                                            <option value="published">立即生效</option>
                                            <option value="prepublished">预备状态</option>
                                        </select>
                                    </label>
                                </div>
                            </div>
                        </details>

                        <h4 class="form-group-title">人员与设备指定</h4>
                        <div class="form-grid">
                            <label>资源锁定
                                <select id="manualLockMode">
                                    <option value="none">系统自动安排</option>
                                    <option value="team">指定班组</option>
                                    <option value="members">指定人员</option>
                                </select>
                            </label>
                            <label>班组<select id="manualTeamId">${getTeamOptions('')}</select></label>
                        </div>
                        <div class="checkbox-row" style="margin-top: 6px;">
                            <label><input type="checkbox" id="manualLock"> 锁定此单，禁止排班器调配</label>
                        </div>
                        <div class="summary-card" style="margin-top:8px;">
                            <h3>选择人员</h3>
                            <div class="collection" id="manualMemberChoices"><div class="empty-hint">先选择班组后加载成员。</div></div>
                        </div>
                        <div class="summary-card">
                            <h3>固定设备</h3>
                            <div class="collection" id="manualEquipmentChoices">${getEquipmentOptions([])}</div>
                        </div>

                        <div style="margin: 12px 0;">
                            <label>加单事由<textarea id="manualRemarks" placeholder="说明理由，如：值机要求补件…"></textarea></label>
                        </div>
                        <div class="inline-actions">
                            <button class="section-btn" id="loadManualSnapshotBtn">预览数据包</button>
                            <button class="section-btn primary" id="createManualOrderBtn"${disabledCreateAttr}>推送生成工单</button>
                        </div>
                        <div class="status-line" id="manualStatus"></div>
                    </div>
                </div>
                <div>
                    <div class="list-card">
                        <h3>数据包快照</h3>
                        <pre class="preview-box" id="manualSnapshotBox">点击“预览数据包”查看实际发送内容…</pre>
                    </div>
                </div>
            </div>
        `;
    }

    function buildRequirementVersionOptions(taskType, defaultIndex) {
        const versions = state.requirementVersions
            .filter((item) => String(item.task_type || '') === String(taskType || ''))
            .sort((left, right) => Number(right.version_no || 0) - Number(left.version_no || 0));
        return versions.map((item, index) => {
            const selected = index === defaultIndex || (versions.length === 1 && index === 0);
            return `<option value="${index}"${selected ? ' selected' : ''}>v${escapeHtml(item.version_no || 1)} / ${escapeHtml(item.status || '')}</option>`;
        }).join('') || '<option value="">暂无版本</option>';
    }

    // ---------- Event binding for detail ----------
    function bindTaskDetailEvents(tt, canEdit) {
        const delBtn = document.getElementById('tdDelTaskTypeBtn');
        if (delBtn && canEdit && state.isAdmin) {
            delBtn.addEventListener('click', async () => {
                if (!confirm(`确认删除作业类型「${tt.name}」吗？`)) return;
                try {
                    delBtn.disabled = true;
                    delBtn.textContent = '删除中…';
                    await apiDelete(`/api/v2/dispatch/task-types/${encodeURIComponent(tt.code)}`);
                    setGlobalStatus(`作业类型「${tt.name}」已删除。`, 'success');
                    await loadBaseOptions();
                    if (hasSelectedDepartment()) {
                        await loadDepartmentData(state.selectedDepartmentId);
                    }
                    state._tdSelectedCode = '';
                    renderAllPanels();
                } catch (err) {
                    setGlobalStatus('删除作业类型失败: ' + (err.message || err), 'error');
                } finally {
                    if (delBtn) {
                        delBtn.disabled = false;
                        delBtn.textContent = '删除作业类型';
                    }
                }
            });
        }

        const tabBar = document.querySelector('.td-tab-bar');
        if (tabBar) {
            tabBar.addEventListener('click', (e) => {
                const btn = e.target.closest('.td-tab[data-td-tab]');
                if (!btn) return;
                state._tdActiveTab = btn.dataset.tdTab;
                state._tdEditingGen = null;
                state._tdEditingAdj = null;
                state.adjustmentDraft.actions = [];
                state.adjustmentDraft.conditions = {};
                renderTaskDrivenPanel();
            });
        }

        const toolTabBar = document.querySelector('.td-subtab-bar');
        if (toolTabBar) {
            toolTabBar.addEventListener('click', (e) => {
                const btn = e.target.closest('.td-subtab[data-td-tool-tab]');
                if (!btn) return;
                state._tdToolTab = btn.dataset.tdToolTab || 'preview';
                renderTaskDrivenPanel();
            });
        }

        const activeTab = state._tdActiveTab || 'gen';
        if (activeTab === 'gen') {
            bindGenerationDetailEvents(tt, canEdit);
            return;
        }
        if (activeTab === 'adj') {
            bindAdjustmentDetailEvents(tt, canEdit);
            return;
        }
        if (activeTab === 'req') {
            bindRequirementsPanelEvents();
            ensureRequirementRows();
            renderRequirementRows();
            renderRequirementDiffSelectors();
            renderRequirementDiff();
            return;
        }
        if (activeTab === 'turn') {
            bindTurnaroundPanelEvents();
            cbBindEvents('turnaround');
            renderTurnaroundRuleList();
            return;
        }
        if (activeTab === 'tools') {
            bindToolsDetailEvents(tt, canEdit);
        }
    }

    function bindGenerationDetailEvents(tt, canEdit) {
        const addGenBtn = document.querySelector('.td-add-gen');
        if (addGenBtn && canEdit) {
            addGenBtn.addEventListener('click', () => {
                state._tdEditingGen = -1;
                renderTaskDrivenPanel();
            });
        }

        for (const btn of document.querySelectorAll('.td-edit-gen')) {
            btn.addEventListener('click', () => {
                if (!canEdit) {
                    return;
                }
                state._tdEditingGen = parseInt(btn.dataset.idx, 10);
                renderTaskDrivenPanel();
            });
        }

        for (const btn of document.querySelectorAll('.td-del-gen')) {
            btn.addEventListener('click', async () => {
                if (!canEdit) {
                    return;
                }
                if (!confirm('确认删除此触发规则？')) return;
                const idx = parseInt(btn.dataset.idx, 10);
                const rule = tt.genRules[idx];
                if (!rule || !rule.id) return;
                try {
                    await apiPost(
                        `/api/v2/dispatch/rules/departments/${encodeURIComponent(state.selectedDepartmentId)}/flight-generation-rules/${encodeURIComponent(rule.id)}/delete`,
                        {}
                    );
                    await loadDepartmentData(state.selectedDepartmentId);
                    renderTaskDrivenPanel();
                } catch (err) {
                    alert('删除失败: ' + err.message);
                }
            });
        }

        const saveGenBtn = document.getElementById('tdGenSaveBtn');
        if (saveGenBtn && canEdit) {
            saveGenBtn.addEventListener('click', async () => {
                const payload = {
                    rule_name: readValue('tdGenRuleName'),
                    task_type: tt.code,
                    leg_scope: readValue('tdGenLegScope') || 'inbound',
                    status: readValue('tdGenStatus') || 'draft',
                    generation_anchor_type: readValue('tdGenAnchor') || 'scheduled_time',
                    start_offset_minutes: Number(readValue('tdGenOffset') || 0),
                    duration_minutes: nullableNumber(readValue('tdGenDuration')),
                    publication_state: readValue('tdGenPubState') || 'prepublished',
                    publish_trigger_mode: readValue('tdGenPubMode') || 'time',
                    publish_offset_minutes: nullableNumber(readValue('tdGenPubOffset')),
                    conditions: {},
                };

                const editIdx = state._tdEditingGen;
                const editRule = editIdx >= 0 ? tt.genRules[editIdx] : null;
                if (editRule && (editRule.rule_id || editRule.id)) {
                    payload.rule_id = editRule.rule_id || editRule.id;
                }

                const statusNode = document.getElementById('tdGenStatus');
                try {
                    await apiPost(
                        `/api/v2/dispatch/rules/departments/${encodeURIComponent(state.selectedDepartmentId)}/flight-generation-rules`,
                        payload
                    );
                    state._tdEditingGen = null;
                    await loadDepartmentData(state.selectedDepartmentId);
                    renderTaskDrivenPanel();
                } catch (err) {
                    if (statusNode) {
                        statusNode.textContent = '保存失败: ' + err.message;
                        statusNode.className = 'status-line error';
                    }
                }
            });
        }

        const cancelGenBtn = document.getElementById('tdGenCancelBtn');
        if (cancelGenBtn) {
            cancelGenBtn.addEventListener('click', () => {
                state._tdEditingGen = null;
                renderTaskDrivenPanel();
            });
        }
    }

    function bindAdjustmentDetailEvents(tt, canEdit) {
        const cloneDraft = (value, fallback) => {
            if (value == null) {
                return fallback;
            }
            return JSON.parse(JSON.stringify(value));
        };

        const addAdjBtn = document.querySelector('.td-add-adj');
        if (addAdjBtn && canEdit) {
            addAdjBtn.addEventListener('click', () => {
                state._tdEditingAdj = -1;
                state.adjustmentDraft.actions = [];
                state.adjustmentDraft.conditions = {};
                renderTaskDrivenPanel();
            });
        }

        for (const btn of document.querySelectorAll('.td-edit-adj')) {
            btn.addEventListener('click', () => {
                if (!canEdit) {
                    return;
                }
                const index = Number(btn.dataset.idx || -1);
                const rule = index >= 0 ? tt.adjRules[index] : null;
                state._tdEditingAdj = index;
                state.adjustmentDraft.actions = cloneDraft(rule?.actions, []);
                state.adjustmentDraft.conditions = cloneDraft(rule?.conditions, {});
                renderTaskDrivenPanel();
            });
        }

        if (state._tdEditingAdj != null) {
            bindAdjustmentPanelEvents();
            cbBindEvents('adjustment');
            renderAdjustmentActions();
        }

        const cancelAdjBtn = document.getElementById('tdAdjCancelBtn');
        if (cancelAdjBtn) {
            cancelAdjBtn.addEventListener('click', () => {
                state._tdEditingAdj = null;
                state.adjustmentDraft.actions = [];
                state.adjustmentDraft.conditions = {};
                renderTaskDrivenPanel();
            });
        }
    }

    function bindToolsDetailEvents(tt, canEdit) {
        const toolTab = state._tdToolTab || 'preview';
        if (toolTab === 'preview') {
            const previewTaskType = document.getElementById('previewTaskType');
            if (previewTaskType) {
                previewTaskType.addEventListener('change', () => {
                    state._tdPreviewTaskType = String(previewTaskType.value || '').trim();
                    const box = document.getElementById('previewResultBox');
                    if (box && state.previewResult) {
                        box.textContent = formatPreviewResultForDisplay(state.previewResult, state._tdPreviewTaskType);
                    }
                });
            }
            const runBtn = document.getElementById('runPreviewBtn');
            if (runBtn) {
                runBtn.addEventListener('click', async () => {
                    await runPreview();
                });
            }
            return;
        }

        if (toolTab === 'manual') {
            const modeSelect = document.getElementById('manualMode');
            if (modeSelect) {
                modeSelect.addEventListener('change', () => {
                    state.manualDraft.mode = readValue('manualMode') || 'task_type';
                    syncManualModeFieldState();
                });
            }
            bindManualPanelEvents();
            syncManualModeFieldState();
        }
    }

    function syncManualModeFieldState() {
        const mode = readValue('manualMode') || state.manualDraft.mode || 'task_type';
        state.manualDraft.mode = mode;
        const taskTypeSelect = document.getElementById('manualTaskType');
        const templateSelect = document.getElementById('manualTemplateCode');
        if (taskTypeSelect) {
            taskTypeSelect.disabled = mode === 'template';
        }
        if (templateSelect) {
            templateSelect.disabled = mode !== 'template';
        }
    }



    function setGlobalStatus(message, kind) {
        const node = document.getElementById('globalStatus');
        if (!node) {
            return;
        }
        node.textContent = message || '';
        node.className = `status-line${kind ? ` ${kind}` : ''}`;
    }

    function selectedDepartmentName() {
        const current = state.departments.find((item) => String(item.id) === String(state.selectedDepartmentId));
        return current ? String(current.name || current.department_name || current.id) : '未选择科室';
    }

    function getVisibleTaskTypes() {
        const allTaskTypes = Array.isArray(state.task_types) ? state.task_types : [];
        if (isAllDepartmentsView() || !hasSelectedDepartment()) {
            return allTaskTypes;
        }
        const selectedDepartmentId = String(state.selectedDepartmentId || '').trim();
        return allTaskTypes.filter((item) => {
            const defaultDepartmentId = String(item.default_department_id || '').trim();
            return defaultDepartmentId === selectedDepartmentId;
        });
    }

    function hasVisibleTaskTypes() {
        return getVisibleTaskTypes().length > 0;
    }

    function shouldShowDepartmentTaskTypeEmptyState() {
        return hasSelectedDepartment() && !isAllDepartmentsView() && !hasVisibleTaskTypes();
    }

    function getDefaultTaskTypeDepartmentId() {
        if (!hasSelectedDepartment() || isAllDepartmentsView()) {
            return '';
        }
        return String(state.selectedDepartmentId || '').trim();
    }

    function syncDepartmentSelectValue() {
        const departmentSelect = document.getElementById('departmentSelect');
        if (departmentSelect) {
            departmentSelect.value = String(state.selectedDepartmentId || '').trim();
        }
    }

    function renderDepartmentTaskTypeEmptyPanel(panel, title, intro, message) {
        if (!panel) {
            return;
        }
        panel.innerHTML = `
            <div>
                <h2 class="section-title">${escapeHtml(title)}</h2>
                <p class="muted panel-intro">${escapeHtml(intro)}</p>
                <div class="info-banner">当前科室：${escapeHtml(selectedDepartmentName())}</div>
                <div class="list-card">
                    <h3>暂不可配置</h3>
                    <div class="empty-hint">${escapeHtml(message)}</div>
                </div>
            </div>
        `;
    }

    function getTaskTypeOptions(selectedValue) {
        return '<option value="">请选择作业类型</option>' + getVisibleTaskTypes().map((item) => {
            const value = String(item.task_type || item.code || item.id || '');
            const label = String(item.task_type_name || item.name || value);
            return `<option value="${escapeHtml(value)}"${value === String(selectedValue || '') ? ' selected' : ''}>${escapeHtml(label)} (${escapeHtml(value)})</option>`;
        }).join('');
    }

    function getEquipmentTypeOptions(selectedValue) {
        return '<option value="">请选择设备类型</option>' + state.equipmentTypes.map((item) => {
            const value = String(item.type_code || item.code || item.id || '');
            const label = String(item.type_name || item.name || value);
            return `<option value="${escapeHtml(value)}"${value === String(selectedValue || '') ? ' selected' : ''}>${escapeHtml(label)}</option>`;
        }).join('');
    }

    function getQualificationOptions(selectedValue) {
        return '<option value="">请选择资质</option>' + state.qualifications.map((item) => {
            const value = String(item.qualification_code || '');
            const label = String(item.qualification_name || value);
            return `<option value="${escapeHtml(value)}"${value === String(selectedValue || '') ? ' selected' : ''}>${escapeHtml(label)}</option>`;
        }).join('');
    }

    function getLevelOptions(selectedValue) {
        return '<option value="">不限等级</option>' + state.qualificationLevels.map((item) => {
            const value = String(item.level_code || '');
            const label = String(item.level_name || value);
            return `<option value="${escapeHtml(value)}"${value === String(selectedValue || '') ? ' selected' : ''}>${escapeHtml(label)}</option>`;
        }).join('');
    }

    function buildSelectOptions(options, selectedValue, emptyLabel) {
        const placeholder = emptyLabel ? `<option value="">${escapeHtml(emptyLabel)}</option>` : '';
        return placeholder + options.map((item) => {
            const value = String(item.value || '');
            const label = String(item.label || value);
            return `<option value="${escapeHtml(value)}"${value === String(selectedValue || '') ? ' selected' : ''}>${escapeHtml(label)}</option>`;
        }).join('');
    }

    function getFlightNatureOptions(selectedValue, emptyLabel = '不限') {
        return buildSelectOptions(FLIGHT_NATURE_OPTIONS, selectedValue, emptyLabel);
    }

    function getFlightStatusOptions(selectedValue, emptyLabel = '不限') {
        return buildSelectOptions(FLIGHT_STATUS_OPTIONS, selectedValue, emptyLabel);
    }

    function formatFlightNatureLabel(value) {
        const normalized = String(value || '').trim().toLowerCase();
        const matched = FLIGHT_NATURE_OPTIONS.find((item) => item.value === normalized);
        return matched ? matched.label : normalized;
    }

    function formatFlightStatusLabel(value) {
        const normalized = String(value || '').trim().toUpperCase();
        const matched = FLIGHT_STATUS_OPTIONS.find((item) => item.value === normalized);
        return matched ? matched.label : normalized;
    }

    // ───────────────── Condition Builder Engine ─────────────────

    const CB_FIELDS = [
        { key: 'is_vip', label: 'VIP 航班', type: 'bool' },
        { key: 'is_turnaround', label: '过站航班', type: 'bool' },
        { key: 'has_boarding_restriction', label: '限制登机', type: 'bool' },
        { key: 'is_quick_turnaround', label: '快速过站', type: 'bool' },
        { key: 'is_commercial_signed', label: '商务签署', type: 'bool' },
        { key: 'flight_nature', label: '航班性质', type: 'enum', options: FLIGHT_NATURE_OPTIONS },
        { key: 'flight_status', label: '航班状态', type: 'enum', options: FLIGHT_STATUS_OPTIONS },
        { key: 'terminal', label: '航站楼', type: 'text' },
        { key: 'aircraft_type', label: '机型', type: 'text' },
        { key: 'stand_type', label: '机位类型', type: 'text' },
    ];

    const CB_OPS = {
        bool: [{ value: 'eq', label: '是' }],
        enum: [
            { value: 'eq', label: '等于' },
            { value: 'neq', label: '不等于' },
            { value: 'in', label: '属于' },
        ],
        text: [
            { value: 'eq', label: '等于' },
            { value: 'neq', label: '不等于' },
            { value: 'contains', label: '包含' },
        ],
    };

    let cbIdCounter = 0;
    function cbNextId() { return `cb_${++cbIdCounter}`; }

    function cbCreateLeaf(field, op, value) {
        return { _id: cbNextId(), field: field || '', op: op || 'eq', value: value ?? '' };
    }
    function cbCreateGroup(operator, children) {
        return { _id: cbNextId(), operator: operator || 'AND', children: children || [cbCreateLeaf()] };
    }

    function cbFromTree(tree) {
        if (!tree || (!tree.operator && !tree.field)) {
            return cbCreateGroup('AND', [cbCreateLeaf()]);
        }
        if (tree.field) {
            const leaf = cbCreateLeaf(tree.field, tree.op, tree.value);
            return cbCreateGroup('AND', [leaf]);
        }
        const group = cbCreateGroup(tree.operator || 'AND', []);
        for (const child of (tree.children || [])) {
            if (child.field) {
                group.children.push(cbCreateLeaf(child.field, child.op, child.value));
            } else if (child.operator) {
                group.children.push(cbFromTree(child));
            }
        }
        if (!group.children.length) {
            group.children.push(cbCreateLeaf());
        }
        return group;
    }

    function cbToTree(group) {
        if (!group) return { operator: 'AND', children: [] };
        if (group.field !== undefined) {
            if (!group.field) return null;
            const leaf = { field: group.field, op: group.op || 'eq', value: group.value };
            return leaf;
        }
        const children = [];
        for (const child of (group.children || [])) {
            const node = cbToTree(child);
            if (node) children.push(node);
        }
        return { operator: group.operator || 'AND', children };
    }

    function cbLegacyToTree(flat) {
        if (!flat || typeof flat !== 'object') return { operator: 'AND', children: [] };
        if (flat.operator && flat.children) return flat;
        const children = [];
        for (const [key, val] of Object.entries(flat)) {
            if (val == null || val === '' || (Array.isArray(val) && !val.length)) continue;
            if (Array.isArray(val)) {
                children.push({ field: key, op: 'in', value: val });
            } else {
                children.push({ field: key, op: 'eq', value: val });
            }
        }
        return { operator: 'AND', children };
    }

    function cbRenderGroup(group, depth) {
        depth = depth || 0;
        const maxDepth = 3;
        const opAttr = `data-operator="${escapeHtml(group.operator || 'AND')}"`;
        const isAnd = (group.operator || 'AND').toUpperCase() === 'AND';

        let childrenHtml = '';
        for (const child of (group.children || [])) {
            if (child.operator !== undefined && child.children !== undefined) {
                childrenHtml += cbRenderGroup(child, depth + 1);
            } else {
                childrenHtml += cbRenderLeaf(child);
            }
        }

        const andActiveClass = isAnd ? 'active-and' : '';
        const orActiveClass = !isAnd ? 'active-or' : '';
        const addSubGroupBtn = depth < maxDepth
            ? `<button class="cb-add-btn" data-action="add-group" data-id="${group._id}">+ 添加条件组</button>`
            : '';

        return `
            <div class="cb-group" ${opAttr} data-id="${group._id}">
                <div class="cb-group-header">
                    <div class="cb-operator-toggle" data-id="${group._id}">
                        <button class="${andActiveClass}" data-op="AND">全部满足</button>
                        <button class="${orActiveClass}" data-op="OR">任一满足</button>
                    </div>
                    ${depth > 0 ? `<button class="cb-remove-btn" data-action="remove" data-id="${group._id}" title="删除此条件组">×</button>` : ''}
                </div>
                ${childrenHtml}
                <div class="cb-actions">
                    <button class="cb-add-btn" data-action="add-condition" data-id="${group._id}">+ 添加条件</button>
                    ${addSubGroupBtn}
                </div>
            </div>
        `;
    }

    function cbRenderLeaf(leaf) {
        const fieldDef = CB_FIELDS.find(f => f.key === leaf.field);
        const fieldType = fieldDef ? fieldDef.type : 'text';
        const ops = CB_OPS[fieldType] || CB_OPS.text;

        const fieldOptions = CB_FIELDS.map(f =>
            `<option value="${escapeHtml(f.key)}"${f.key === leaf.field ? ' selected' : ''}>${escapeHtml(f.label)}</option>`
        ).join('');

        const opOptions = ops.map(o =>
            `<option value="${escapeHtml(o.value)}"${o.value === leaf.op ? ' selected' : ''}>${escapeHtml(o.label)}</option>`
        ).join('');

        let valueHtml = '';
        if (fieldType === 'bool') {
            const checked = leaf.value === true || leaf.value === 'true' ? ' checked' : '';
            valueHtml = `<input type="checkbox" class="cb-value-bool" data-role="value"${checked}>`;
        } else if (fieldType === 'enum' && fieldDef) {
            const valOptions = (fieldDef.options || []).map(o => {
                const selected = (Array.isArray(leaf.value) ? leaf.value.includes(o.value) : o.value === leaf.value) ? ' selected' : '';
                return `<option value="${escapeHtml(o.value)}"${selected}>${escapeHtml(o.label)}</option>`;
            }).join('');
            const isMulti = leaf.op === 'in' || leaf.op === 'nin';
            valueHtml = `<select class="cb-value-select" data-role="value"${isMulti ? ' multiple' : ''}><option value="">请选择</option>${valOptions}</select>`;
        } else {
            valueHtml = `<input class="cb-value-input" data-role="value" value="${escapeHtml(String(leaf.value || ''))}" placeholder="输入值">`;
        }

        return `
            <div class="cb-row" data-id="${leaf._id}">
                <select class="cb-field-select" data-role="field"><option value="">选择字段</option>${fieldOptions}</select>
                <select class="cb-op-select" data-role="op">${opOptions}</select>
                ${valueHtml}
                <button class="cb-remove-btn" data-action="remove" data-id="${leaf._id}" title="删除">×</button>
            </div>
        `;
    }

    // ── Condition Builder instances per prefix ──
    const cbInstances = {};

    function renderConditionBuilder(prefix, existingConditions) {
        const tree = cbLegacyToTree(existingConditions);
        const model = cbFromTree(tree);
        cbInstances[prefix] = model;
        return `<div class="cb-root" id="cb-${prefix}">${cbRenderGroup(model, 0)}</div>`;
    }

    function cbBindEvents(prefix) {
        const root = document.getElementById(`cb-${prefix}`);
        if (!root) return;

        root.addEventListener('click', (e) => {
            const btn = e.target.closest('[data-action]');
            if (!btn) return;
            const action = btn.dataset.action;
            const targetId = btn.dataset.id;
            const model = cbInstances[prefix];
            if (!model) return;

            if (action === 'add-condition') {
                const group = cbFindNode(model, targetId);
                if (group && group.children) {
                    group.children.push(cbCreateLeaf());
                    cbRerender(prefix);
                }
            } else if (action === 'add-group') {
                const group = cbFindNode(model, targetId);
                if (group && group.children) {
                    group.children.push(cbCreateGroup('AND', [cbCreateLeaf()]));
                    cbRerender(prefix);
                }
            } else if (action === 'remove') {
                cbRemoveNode(model, targetId);
                if (!model.children.length) {
                    model.children.push(cbCreateLeaf());
                }
                cbRerender(prefix);
            }
        });

        root.addEventListener('click', (e) => {
            const opBtn = e.target.closest('.cb-operator-toggle button[data-op]');
            if (!opBtn) return;
            const toggleDiv = opBtn.closest('.cb-operator-toggle');
            const groupId = toggleDiv.dataset.id;
            const model = cbInstances[prefix];
            const group = cbFindNode(model, groupId);
            if (group) {
                group.operator = opBtn.dataset.op;
                cbRerender(prefix);
            }
        });

        root.addEventListener('change', (e) => {
            const row = e.target.closest('.cb-row[data-id]');
            if (!row) return;
            const leafId = row.dataset.id;
            const model = cbInstances[prefix];
            const leaf = cbFindNode(model, leafId);
            if (!leaf) return;
            const role = e.target.dataset.role;
            if (role === 'field') {
                leaf.field = e.target.value;
                const newDef = CB_FIELDS.find(f => f.key === leaf.field);
                const newType = newDef ? newDef.type : 'text';
                const availOps = CB_OPS[newType] || CB_OPS.text;
                if (!availOps.find(o => o.value === leaf.op)) {
                    leaf.op = availOps[0].value;
                }
                leaf.value = newType === 'bool' ? true : '';
                cbRerender(prefix);
            } else if (role === 'op') {
                leaf.op = e.target.value;
                cbRerender(prefix);
            } else if (role === 'value') {
                if (e.target.type === 'checkbox') {
                    leaf.value = e.target.checked;
                } else if (e.target.multiple) {
                    leaf.value = Array.from(e.target.selectedOptions).map(o => o.value);
                } else {
                    leaf.value = e.target.value;
                }
            }
        });
    }

    function cbRerender(prefix) {
        const root = document.getElementById(`cb-${prefix}`);
        const model = cbInstances[prefix];
        if (root && model) {
            root.innerHTML = cbRenderGroup(model, 0);
        }
    }

    function cbFindNode(node, id) {
        if (node._id === id) return node;
        if (node.children) {
            for (const child of node.children) {
                const found = cbFindNode(child, id);
                if (found) return found;
            }
        }
        return null;
    }

    function cbRemoveNode(parent, id) {
        if (!parent.children) return false;
        const idx = parent.children.findIndex(c => c._id === id);
        if (idx >= 0) {
            parent.children.splice(idx, 1);
            return true;
        }
        for (const child of parent.children) {
            if (cbRemoveNode(child, id)) return true;
        }
        return false;
    }

    function collectConditionTree(prefix) {
        const model = cbInstances[prefix];
        if (!model) return { operator: 'AND', children: [] };
        return cbToTree(model);
    }

    // Legacy compat wrappers
    function renderConditionFields(prefix) {
        return renderConditionBuilder(prefix, {});
    }
    function collectConditionFields(prefix) {
        return collectConditionTree(prefix);
    }

    function summarizeConditions(conditions) {
        if (!conditions || (typeof conditions === 'object' && !Object.keys(conditions).length)) {
            return '无附加过滤条件';
        }
        // New tree format
        if (conditions.operator && conditions.children) {
            return _summarizeTree(conditions);
        }
        // Legacy flat dict
        return _summarizeFlatConditions(conditions);
    }

    function _summarizeTree(node) {
        if (node.field) {
            return _summarizeLeaf(node);
        }
        const parts = [];
        for (const child of (node.children || [])) {
            const text = child.field ? _summarizeLeaf(child) : _summarizeTree(child);
            if (text) parts.push(text);
        }
        if (!parts.length) return '无附加条件';
        const joiner = (node.operator || 'AND').toUpperCase() === 'OR' ? ' 或 ' : '、';
        const joined = parts.join(joiner);
        if ((node.operator || 'AND').toUpperCase() === 'OR' && parts.length > 1) {
            return `(${joined})`;
        }
        return joined;
    }

    function _summarizeLeaf(leaf) {
        const fieldDef = CB_FIELDS.find(f => f.key === leaf.field);
        const label = fieldDef ? fieldDef.label : leaf.field;
        const op = leaf.op || 'eq';
        const val = leaf.value;
        if (fieldDef && fieldDef.type === 'bool') {
            return val ? label : '';
        }
        if (fieldDef && fieldDef.type === 'enum') {
            const vals = Array.isArray(val) ? val : [val];
            const labels = vals.map(v => {
                const optDef = (fieldDef.options || []).find(o => o.value === v);
                return optDef ? optDef.label : v;
            }).filter(Boolean);
            if (op === 'in') return `${label}∈${labels.join('/')}`;
            if (op === 'neq') return `${label}≠${labels.join('/')}`;
            return `${label}=${labels.join('/')}`;
        }
        if (op === 'contains') return `${label}含"${val}"`;
        if (op === 'neq') return `${label}≠${val}`;
        return `${label}=${val}`;
    }

    function _summarizeFlatConditions(conditions) {
        const parts = [];
        if (conditions.is_vip) parts.push('VIP 航班');
        if (conditions.is_turnaround) parts.push('过站航班');
        if (conditions.has_boarding_restriction) parts.push('限制登机');
        if (conditions.is_quick_turnaround) parts.push('快速过站');
        if (conditions.is_commercial_signed) parts.push('商务签署');
        if (conditions.flight_nature) {
            const vals = Array.isArray(conditions.flight_nature) ? conditions.flight_nature : [conditions.flight_nature];
            const lbls = vals.map(v => formatFlightNatureLabel(v)).filter(Boolean);
            if (lbls.length) parts.push(`航班性质:${lbls.join('/')}`);
        }
        if (conditions.flight_status) {
            const vals = Array.isArray(conditions.flight_status) ? conditions.flight_status : [conditions.flight_status];
            const lbls = vals.map(v => formatFlightStatusLabel(v)).filter(Boolean);
            if (lbls.length) parts.push(`航班状态:${lbls.join('/')}`);
        }
        if (conditions.terminal) parts.push(`${conditions.terminal} 航站楼`);
        if (conditions.aircraft_type) parts.push(`${conditions.aircraft_type} 机型`);
        if (conditions.stand_type) parts.push(`${conditions.stand_type} 机位类型`);
        return parts.length ? parts.join('、') : '无附加过滤条件';
    }

    function summarizeGenerationRule(rule) {
        return `${rule.leg_scope || 'none'} leg 命中 ${summarizeConditions(rule.conditions || {})} 时，生成作业类型 ${rule.task_type || '未选择'}，按 ${rule.generation_anchor_type || 'scheduled_time'} 锚点偏移 ${rule.start_offset_minutes || 0} 分钟启动，默认时长 ${rule.duration_minutes || '未设'} 分钟，以 ${rule.publish_trigger_mode || 'time'} 模式发布。`;
    }

    function summarizeAdjustmentAction(action) {
        const type = action.action_type || '';
        if (type === 'increase_slot_count') {
            return `人员槽位 ${action.slot_code || '-'} 数量增加 ${action.delta || 1}`;
        }
        if (type === 'add_slot') {
            return `新增人员槽位 ${action.slot?.slot_code || action.slot_code || '-'}，资质 ${action.slot?.qualification_code || '-'}`;
        }
        if (type === 'upgrade_min_level') {
            return `人员槽位 ${action.slot_code || '-'} 最低等级提升到 ${action.min_level_code || '-'}`;
        }
        if (type === 'extend_duration') {
            return `任务时长增加 ${action.delta_minutes || 0} 分钟`;
        }
        if (type === 'advance_publish_offset') {
            return `发布时间提前 ${action.delta_minutes || 0} 分钟`;
        }
        if (type === 'delay_publish_offset') {
            return `发布时间延后 ${action.delta_minutes || 0} 分钟`;
        }
        if (type === 'increase_equipment_count') {
            return `设备槽位 ${action.slot_code || '-'} 数量增加 ${action.delta || 1}`;
        }
        if (type === 'add_equipment_type_requirement') {
            return `新增设备槽位 ${action.equipment_slot?.slot_code || action.slot_code || '-'}，类型 ${action.equipment_slot?.equipment_type_code || action.equipment_type_code || '-'}`;
        }
        if (type === 'require_driver_for_equipment') {
            return `设备槽位 ${action.slot_code || '-'} 强制配司机，司机资质 ${action.driver_qualification_code || '-'} ${action.driver_min_level_code || ''}`.trim();
        }
        return JSON.stringify(action);
    }

    function summarizeAdjustmentRule(rule) {
        const actionText = (rule.actions || []).map(summarizeAdjustmentAction).join('；') || '无动作';
        return `当作业类型 ${rule.task_type || '-'} 命中 ${summarizeConditions(rule.conditions || {})} 时，执行：${actionText}。`;
    }

    function summarizeRequirementVersion(version) {
        const crew = (version.crew_requirements || []).map((item) => `${item.slot_code}:${item.qualification_code}${item.min_level_code ? `>=${item.min_level_code}` : ''} x${item.required_count || 1}`).join('；') || '无人员槽位';
        const equipment = (version.equipment_requirements || []).map((item) => `${item.slot_code}:${item.equipment_type_code || item.equipment_type_id || '-'} x${item.required_count || 1}${item.requires_driver ? ' +司机' : ''}`).join('；') || '无设备槽位';
        return `作业类型 ${version.task_type || '-'} 当前版本包含人员需求 ${crew}；设备需求 ${equipment}。`;
    }

    function summarizeTurnaroundRule(rule) {
        const slots = (rule.slot_pairs || []).map((item) => `${item.inbound_slot_code}->${item.outbound_slot_code}`).join('、') || '未配对';
        return `与 ${rule.counterpart_leg_scope || '-'} / ${rule.counterpart_task_type || '-'} 建立 ${rule.constraint_mode || 'disabled'} 约束，槽位对 ${slots}，紧阈值 ${rule.tight_threshold_minutes ?? '-'} 分钟，放松阈值 ${rule.relax_threshold_minutes ?? '-'} 分钟。`;
    }

    function getRuleIdentity(item) {
        return `${item.rule_name || item.task_type || item.id || '未命名'} (${item.id || '-'})`;
    }

    function buildDiffRows(left, right) {
        const fields = Array.from(new Set([...Object.keys(left || {}), ...Object.keys(right || {})]));
        return fields.map((field) => {
            const leftValue = normalizeForDiff(left ? left[field] : undefined);
            const rightValue = normalizeForDiff(right ? right[field] : undefined);
            const changed = leftValue !== rightValue;
            return `
                <tr${changed ? ' style="background: rgba(255,149,0,0.12);"' : ''}>
                    <td>${escapeHtml(field)}</td>
                    <td>${escapeHtml(leftValue)}</td>
                    <td>${escapeHtml(rightValue)}</td>
                </tr>
            `;
        }).join('');
    }

    function normalizeForDiff(value) {
        if (value == null) {
            return '';
        }
        if (typeof value === 'object') {
            return JSON.stringify(value);
        }
        return String(value);
    }

    async function ensureTeamMembers(teamId) {
        const normalized = String(teamId || '').trim();
        if (!normalized) {
            return [];
        }
        if (state.teamMembersByTeam[normalized]) {
            return state.teamMembersByTeam[normalized];
        }
        const members = await apiGet(`/api/v2/dispatch/resources/teams/${encodeURIComponent(normalized)}/members`);
        state.teamMembersByTeam[normalized] = Array.isArray(members) ? members : [];
        return state.teamMembersByTeam[normalized];
    }

    function getTeamOptions(selectedValue) {
        return '<option value="">请选择班组</option>' + state.teams.map((item) => {
            const value = String(item.id || '');
            const label = String(item.name || item.code || value);
            return `<option value="${escapeHtml(value)}"${value === String(selectedValue || '') ? ' selected' : ''}>${escapeHtml(label)}</option>`;
        }).join('');
    }

    function getEquipmentOptions(selectedValues) {
        const chosen = new Set((selectedValues || []).map((item) => String(item)));
        return state.equipment.map((item) => {
            const value = String(item.id || '');
            const label = String(item.name || item.code || value);
            return `<label><input type="checkbox" data-manual-equipment-id="${escapeHtml(value)}"${chosen.has(value) ? ' checked' : ''}> ${escapeHtml(label)}</label>`;
        }).join('') || '<span class="muted">当前终端暂无可选设备。</span>';
    }

    function buildCompareOptions(items, labelBuilder, defaultIndex) {
        if (!items.length) {
            return '<option value="">暂无数据</option>';
        }
        return items.map((item, index) => {
            return `<option value="${index}"${index === Number(defaultIndex || 0) ? ' selected' : ''}>${escapeHtml(labelBuilder(item))}</option>`;
        }).join('');
    }

    function groupRequirementVersionsByTaskType() {
        const grouped = new Map();
        for (const item of state.requirementVersions) {
            const key = String(item.task_type || '');
            if (!grouped.has(key)) {
                grouped.set(key, []);
            }
            grouped.get(key).push(item);
        }
        return grouped;
    }

    function findLatestPublishedRequirement(taskType) {
        const versions = state.requirementVersions
            .filter((item) => String(item.task_type || '') === String(taskType || ''))
            .sort((left, right) => Number(right.version_no || 0) - Number(left.version_no || 0));
        return versions.find((item) => String(item.status || '') === 'published') || versions[0] || null;
    }

    function escapeHtml(value) {
        return String(value == null ? '' : value)
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;')
            .replace(/'/g, '&#39;');
    }

    function hasSelectedDepartment() {
        return Boolean(String(state.selectedDepartmentId || '').trim());
    }

    function isAllDepartmentsView() {
        return String(state.selectedDepartmentId || '').trim() === '__all__';
    }

    function requireConcreteDepartmentSelection(statusId, message) {
        if (!hasSelectedDepartment()) {
            if (statusId) {
                setLocalStatus(statusId, '请先选择科室。', 'warn');
            }
            return false;
        }
        if (isAllDepartmentsView()) {
            if (statusId) {
                setLocalStatus(
                    statusId,
                    message || '“全部科室”模式仅支持汇总查看，请切换到具体科室后再操作。',
                    'warn'
                );
            }
            return false;
        }
        return true;
    }

    function formatApiError(payload, status) {
        if (payload == null) {
            return `请求失败: ${status}`;
        }
        if (typeof payload === 'string') {
            return payload;
        }
        if (typeof payload.detail === 'string' && payload.detail.trim()) {
            return payload.detail.trim();
        }
        if (typeof payload.error === 'string' && payload.error.trim()) {
            return payload.error.trim();
        }
        if (payload.error && typeof payload.error === 'object') {
            if (typeof payload.error.message === 'string' && payload.error.message.trim()) {
                return payload.error.message.trim();
            }
            try {
                return JSON.stringify(payload.error);
            } catch (_error) {
                return `请求失败: ${status}`;
            }
        }
        if (Array.isArray(payload.detail)) {
            return payload.detail.map((item) => {
                if (typeof item === 'string') {
                    return item;
                }
                if (item && typeof item.msg === 'string') {
                    return item.msg;
                }
                try {
                    return JSON.stringify(item);
                } catch (_error) {
                    return '参数校验失败';
                }
            }).join('; ');
        }
        return `请求失败: ${status}`;
    }

    async function apiGet(url) {
        const response = await Auth.fetch(url, { credentials: 'include' });
        const payload = await response.json();
        if (!response.ok || payload.success === false) {
            throw new Error(formatApiError(payload, response.status));
        }
        return payload.data;
    }

    async function apiPost(url, body) {
        const response = await Auth.fetch(url, {
            method: 'POST',
            credentials: 'include',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify(body || {}),
        });
        const payload = await response.json();
        if (!response.ok || payload.success === false) {
            throw new Error(formatApiError(payload, response.status));
        }
        return payload.data;
    }

    async function apiDelete(url) {
        const response = await Auth.fetch(url, {
            method: 'DELETE',
            credentials: 'include',
        });
        const payload = await response.json();
        if (!response.ok || payload.success === false) {
            throw new Error(formatApiError(payload, response.status));
        }
        return payload.data;
    }

    function renderGenerationPanel() {
        const panel = document.getElementById('panel-generation');
        if (!panel) {
            return;
        }
        if (shouldShowDepartmentTaskTypeEmptyState()) {
            renderDepartmentTaskTypeEmptyPanel(
                panel,
                '基础规则',
                '定义"什么条件的航班 -> 自动生成什么作业任务"。每条规则绑定一个作业类型和航班方向。',
                '当前科室暂无任务类型。请先在“任务视图”中新增并归属到当前科室，再配置基础生成规则。'
            );
            return;
        }
        const rulesHtml = state.generationRules.length
            ? state.generationRules.map((item) => {
                return `
                    <div class="item-card">
                        <strong>${escapeHtml(item.rule_name || item.task_type || '未命名规则')}</strong>
                        <div>作业类型: ${escapeHtml(item.task_type || '')}</div>
                        <div>leg_scope: ${escapeHtml(item.leg_scope || '')} / 状态: ${escapeHtml(item.status || '')}</div>
                        <div class="muted">${escapeHtml(summarizeGenerationRule(item))}</div>
                    </div>
                `;
            }).join('')
            : '<div class="empty-hint">当前科室还没有基础生成规则。</div>';

        panel.innerHTML = `
            <div class="panel-grid">
                <div>
                    <h2 class="section-title">基础规则</h2>
                    <p class="muted panel-intro">定义"什么条件的航班 -> 自动生成什么作业任务"。每条规则绑定一个作业类型和航班方向。</p>
                    <div class="info-banner">当前科室：${escapeHtml(selectedDepartmentName())} — 发布前将自动校验同方向+同作业类型的规则是否冲突。</div>
                    <div class="list-card">
                        <h3>新建基础规则</h3>
                        <div class="form-grid">
                            <label>规则名称<input id="generationRuleName" placeholder="例：国内进港客舱清洁"></label>
                            <label>作业类型<select id="generationTaskType">${getTaskTypeOptions('')}</select></label>
                            <label>航班方向
                                <select id="generationLegScope">
                                    <option value="inbound">进港</option>
                                    <option value="outbound">出港</option>
                                    <option value="none">不限方向</option>
                                </select>
                            </label>
                            <label>规则状态
                                <select id="generationStatus">
                                    <option value="draft">草稿</option>
                                    <option value="published">发布</option>
                                </select>
                            </label>
                        </div>

                        <h4 class="form-group-title">航班适用条件</h4>
                        <p class="muted">当航班满足以下条件时，自动生成该作业任务。可用"全部满足 / 任一满足"组合多个条件，也可嵌套条件组。</p>
                        ${renderConditionBuilder('generation', state.generationDraft.conditions)}

                        <h4 class="form-group-title">任务时间与发布策略</h4>
                        <div class="form-grid columns-3">
                            <label>任务开始偏移(分钟)<input id="generationStartOffset" type="number" value="0"></label>
                            <label>默认任务时长(分钟)<input id="generationDuration" type="number" value="30"></label>
                            <label>发布提前(分钟)<input id="generationPublishOffset" type="number" value="-30"></label>
                        </div>
                        <details class="advanced-options">
                            <summary>高级发布选项</summary>
                            <div class="advanced-content">
                                <div class="form-grid columns-3">
                                    <label>时间锚点<input id="generationAnchorType" value="scheduled_time"></label>
                                    <label>发布初始状态
                                        <select id="generationPublicationState">
                                            <option value="prepublished">预发布</option>
                                            <option value="published">直接发布</option>
                                        </select>
                                    </label>
                                    <label>发布触发方式
                                        <select id="generationPublishMode">
                                            <option value="time">按时间</option>
                                            <option value="event">按事件</option>
                                            <option value="either">任一触发</option>
                                            <option value="both_required">同时满足</option>
                                        </select>
                                    </label>
                                    <label>事件码<input id="generationEventCode" placeholder="如 boarding_open"></label>
                                    <label style="grid-column: span 2;">备注<textarea id="generationNotes" placeholder="补充说明业务意图"></textarea></label>
                                </div>
                            </div>
                        </details>

                        <div class="inline-actions" style="margin-top: 16px;">
                            <button class="section-btn" id="validateGenerationRuleBtn">校验冲突</button>
                            <button class="section-btn primary" id="saveGenerationRuleBtn">保存规则</button>
                        </div>
                        <div class="status-line" id="generationRuleStatus"></div>
                        <pre class="preview-box" id="generationSummaryBox" style="margin-top:12px;"></pre>
                    </div>
                </div>
                <div>
                    <div class="list-card">
                        <h3>已有规则</h3>
                        <div class="collection">${rulesHtml}</div>
                    </div>
                    <div class="list-card">
                        <h3>规则差异对比</h3>
                        <div class="form-grid">
                            <label>左侧<select id="generationCompareLeft">${buildCompareOptions(state.generationRules, getRuleIdentity)}</select></label>
                            <label>右侧<select id="generationCompareRight">${buildCompareOptions(state.generationRules, getRuleIdentity, 1)}</select></label>
                        </div>
                        <table class="builder-table">
                            <thead><tr><th>字段</th><th>左侧</th><th>右侧</th></tr></thead>
                            <tbody id="generationDiffRows"></tbody>
                        </table>
                    </div>
                </div>
            </div>
        `;

        bindGenerationPanelEvents();
        cbBindEvents('generation');
        updateGenerationSummary();
        renderGenerationDiff();
    }

    function bindGenerationPanelEvents() {
        const fieldIds = [
            'generationRuleName', 'generationTaskType', 'generationLegScope', 'generationStatus',
            'generationVip', 'generationTurnaround', 'generationBoardingRestriction', 'generationQuickTurnaround',
            'generationCommercialSigned', 'generationFlightNature', 'generationFlightStatus', 'generationTerminal',
            'generationAircraftType', 'generationStandType', 'generationAnchorType', 'generationStartOffset',
            'generationDuration', 'generationPublicationState', 'generationPublishMode',
            'generationPublishOffset', 'generationEventCode', 'generationNotes',
        ];
        for (const id of fieldIds) {
            const node = document.getElementById(id);
            if (!node) {
                continue;
            }
            node.addEventListener('input', updateGenerationSummary);
            node.addEventListener('change', updateGenerationSummary);
        }

        const validateBtn = document.getElementById('validateGenerationRuleBtn');
        if (validateBtn) {
            validateBtn.addEventListener('click', async () => {
                await validateGenerationRule();
            });
        }

        const saveBtn = document.getElementById('saveGenerationRuleBtn');
        if (saveBtn) {
            saveBtn.addEventListener('click', async () => {
                await saveGenerationRule();
            });
        }
        for (const id of ['generationCompareLeft', 'generationCompareRight']) {
            const node = document.getElementById(id);
            if (node) {
                node.addEventListener('change', renderGenerationDiff);
            }
        }
    }

    function collectGenerationPayload() {
        return {
            rule_name: readValue('generationRuleName'),
            task_type: readValue('generationTaskType'),
            leg_scope: readValue('generationLegScope') || 'inbound',
            status: readValue('generationStatus') || 'draft',
            generation_anchor_type: readValue('generationAnchorType') || 'scheduled_time',
            start_offset_minutes: Number(readValue('generationStartOffset') || 0),
            duration_minutes: nullableNumber(readValue('generationDuration')),
            publication_state: readValue('generationPublicationState') || 'prepublished',
            publish_trigger_mode: readValue('generationPublishMode') || 'time',
            publish_offset_minutes: nullableNumber(readValue('generationPublishOffset')),
            publish_event_code: readValue('generationEventCode') || null,
            notes: readValue('generationNotes') || null,
            conditions: collectConditionFields('generation'),
        };
    }

    function hasGenerationTaskTypeSelected(payload) {
        const candidate = payload || collectGenerationPayload();
        return Boolean(String(candidate.task_type || '').trim());
    }

    function updateGenerationSummary() {
        const payload = collectGenerationPayload();
        const summary = [
            `科室: ${selectedDepartmentName()}`,
            `作业类型: ${payload.task_type || '未选择'}`,
            `leg_scope: ${payload.leg_scope}`,
            `状态: ${payload.status}`,
            `计划锚点: ${payload.generation_anchor_type}, 偏移 ${payload.start_offset_minutes} 分钟, 时长 ${payload.duration_minutes ?? '未填'} 分钟`,
            `发布时间: ${payload.publish_trigger_mode}, publication_state=${payload.publication_state}, publish_offset=${payload.publish_offset_minutes ?? '未填'}`,
            `条件: ${JSON.stringify(payload.conditions, null, 2)}`,
        ].join('\n');
        const box = document.getElementById('generationSummaryBox');
        if (box) {
            box.textContent = summary;
        }
        if (!hasGenerationTaskTypeSelected(payload)) {
            state.generationValidationValid = true;
            const saveBtn = document.getElementById('saveGenerationRuleBtn');
            if (saveBtn) {
                saveBtn.disabled = true;
            }
            setLocalStatus('generationRuleStatus', '当前科室尚无可用作业类型，暂无法配置基础生成规则。', 'warn');
            return;
        }
        scheduleGenerationValidation();
    }

    function renderGenerationDiff() {
        const rows = document.getElementById('generationDiffRows');
        if (!rows) {
            return;
        }
        const left = state.generationRules[Number(readValue('generationCompareLeft') || 0)] || null;
        const right = state.generationRules[Number(readValue('generationCompareRight') || 1)] || null;
        if (!left || !right) {
            rows.innerHTML = '<tr><td colspan="3" class="muted">至少需要两条基础规则才能对比。</td></tr>';
            return;
        }
        rows.innerHTML = buildDiffRows(left, right);
    }

    function scheduleGenerationValidation() {
        if (state.generationValidationTimer) {
            window.clearTimeout(state.generationValidationTimer);
        }
        state.generationValidationTimer = window.setTimeout(() => {
            validateGenerationRule(true);
        }, 250);
    }

    async function validateGenerationRule(silent) {
        if (!hasSelectedDepartment()) {
            if (!silent) {
                setLocalStatus('generationRuleStatus', '请先选择科室。', 'warn');
            }
            return;
        }
        if (isAllDepartmentsView()) {
            state.generationValidationValid = true;
            const saveBtn = document.getElementById('saveGenerationRuleBtn');
            if (saveBtn) {
                saveBtn.disabled = true;
            }
            if (!silent) {
                setLocalStatus('generationRuleStatus', '“全部科室”模式不支持单科室规则校验。', 'warn');
            }
            return;
        }
        const payload = collectGenerationPayload();
        if (!hasGenerationTaskTypeSelected(payload)) {
            state.generationValidationValid = true;
            const saveBtn = document.getElementById('saveGenerationRuleBtn');
            if (saveBtn) {
                saveBtn.disabled = true;
            }
            if (!silent) {
                setLocalStatus('generationRuleStatus', '请先配置至少一个作业类型后，再进行规则校验。', 'warn');
            }
            return;
        }
        try {
            const result = await apiPost(
                `/api/v2/dispatch/rules/validate`,
                { department_id: state.selectedDepartmentId, generation_rule: payload }
            );
            state.generationValidationValid = Boolean(result.valid);
            const messages = []
                .concat(result.valid ? ['校验通过。'] : [])
                .concat((result.messages || []).map((item) => `消息: ${item}`))
                .concat((result.conflicts || []).map((item) => `冲突: ${item.task_type}/${item.leg_scope}/${item.rule_id}`));
            setLocalStatus('generationRuleStatus', messages.join(' ') || '校验完成。', result.valid ? 'success' : 'error');
            const saveBtn = document.getElementById('saveGenerationRuleBtn');
            if (saveBtn) {
                saveBtn.disabled = !result.valid && collectGenerationPayload().status === 'published';
            }
        } catch (error) {
            state.generationValidationValid = false;
            if (!silent) {
                setLocalStatus('generationRuleStatus', error.message, 'error');
            }
        }
    }

    async function saveGenerationRule() {
        if (!requireConcreteDepartmentSelection('generationRuleStatus', '“全部科室”模式不支持保存基础生成规则。')) {
            return;
        }
        const payload = collectGenerationPayload();
        if (!hasGenerationTaskTypeSelected(payload)) {
            setLocalStatus('generationRuleStatus', '请先配置至少一个作业类型后，再保存基础生成规则。', 'warn');
            return;
        }
        try {
            await apiPost(
                `/api/v2/dispatch/rules/departments/${encodeURIComponent(state.selectedDepartmentId)}/flight-generation-rules`,
                payload
            );
            await loadDepartmentData(state.selectedDepartmentId);
            renderGenerationPanel();
            setLocalStatus('generationRuleStatus', '基础生成规则已保存。', 'success');
        } catch (error) {
            setLocalStatus('generationRuleStatus', error.message, 'error');
        }
    }

    function renderAdjustmentPanel() {
        const panel = document.getElementById('panel-adjustment');
        if (!panel) {
            return;
        }
        if (shouldShowDepartmentTaskTypeEmptyState()) {
            renderDepartmentTaskTypeEmptyPanel(
                panel,
                '增量调整',
                '如果航班还满足额外条件，就在基础任务上做调整（加人、加设备、延时等）。',
                '当前科室暂无任务类型。请先在“任务视图”中新增并归属到当前科室，再配置增量调整规则。'
            );
            return;
        }
        const rulesHtml = state.adjustmentRules.length
            ? state.adjustmentRules.map((item) => `
                <div class="item-card">
                    <strong>${escapeHtml(item.rule_name || item.task_type || '未命名调整规则')}</strong>
                    <div>作业类型: ${escapeHtml(item.task_type || '')} / 状态: ${escapeHtml(item.status || '')}</div>
                    <div>动作数: ${(item.actions || []).length}</div>
                    <div class="muted">${escapeHtml(summarizeAdjustmentRule(item))}</div>
                </div>
            `).join('')
            : '<div class="empty-hint">当前科室还没有增量调整规则。</div>';

        panel.innerHTML = `
            <div class="panel-grid">
                <div>
                    <h2 class="section-title">增量调整</h2>
                    <p class="muted panel-intro">如果航班还满足额外条件，就在基础任务上做调整（加人、加设备、延时等）。</p>
                    <div class="list-card">
                        <h3>新建增量调整规则</h3>
                        <div class="form-grid">
                            <label>规则名称<input id="adjustmentRuleName" placeholder="例：VIP 加派规则"></label>
                            <label>作业类型<select id="adjustmentTaskType">${getTaskTypeOptions('')}</select></label>
                            <label>规则状态
                                <select id="adjustmentStatus">
                                    <option value="draft">草稿</option>
                                    <option value="published">发布</option>
                                </select>
                            </label>
                        </div>

                        <h4 class="form-group-title">附加触发条件</h4>
                        <p class="muted">当航班额外满足以下条件时，对已生成的基础任务执行调整动作。</p>
                        ${renderConditionBuilder('adjustment', state.adjustmentDraft.conditions)}
                        
                        <h4 class="form-group-title">调整动作配置</h4>
                        <div class="form-grid columns-3">
                            <label>触发动作类型
                                <select id="adjustmentActionType">
                                    ${Object.entries(ACTION_TEMPLATES).map(([value, label]) => `<option value="${escapeHtml(value)}">${escapeHtml(label)}</option>`).join('')}
                                </select>
                            </label>
                            <label>资源槽位编码<input id="adjustmentActionSlotCode" placeholder="如: cleaner_2"></label>
                            <label>增减数量/调整时长(分)<input id="adjustmentActionDelta" type="number" value="1"></label>
                            <label>最低等级要求<select id="adjustmentActionLevel">${getLevelOptions('')}</select></label>
                            <label>指定资质类型<select id="adjustmentActionQualification">${getQualificationOptions('')}</select></label>
                            <label>指定新增设备类型<select id="adjustmentActionEquipmentType">${getEquipmentTypeOptions('')}</select></label>
                        </div>
                        <div class="inline-actions" style="margin-top:12px;">
                            <button class="section-btn" id="addAdjustmentActionBtn">+ 将动作加入当前调整列表</button>
                        </div>
                        
                        <h4 class="form-group-title">当前已添加的动作列表</h4>
                        <div class="collection" id="adjustmentActionsList" style="margin-bottom: 20px;"></div>
                        
                        <div style="margin-top:8px;"></div>
                        <div class="inline-actions">
                            <button class="section-btn primary" id="saveAdjustmentRuleBtn">保存调整规则</button>
                        </div>
                        <div class="status-line" id="adjustmentRuleStatus"></div>
                    </div>
                </div>
                <div>
                    <div class="list-card">
                        <h3>已存在增量调整规则</h3>
                        <div class="collection">${rulesHtml}</div>
                    </div>
                    <div class="list-card">
                        <h3>对比差异</h3>
                        <div class="form-grid">
                            <label>左侧规则
                                <select id="adjustmentCompareLeft">${buildCompareOptions(state.adjustmentRules, getRuleIdentity)}</select>
                            </label>
                            <label>右侧规则
                                <select id="adjustmentCompareRight">${buildCompareOptions(state.adjustmentRules, getRuleIdentity, 1)}</select>
                            </label>
                        </div>
                        <table class="builder-table">
                            <thead><tr><th>字段</th><th>左侧</th><th>右侧</th></tr></thead>
                            <tbody id="adjustmentDiffRows"></tbody>
                        </table>
                    </div>
                </div>
            </div>
        `;

        bindAdjustmentPanelEvents();
        cbBindEvents('adjustment');
        renderAdjustmentActions();
        renderAdjustmentDiff();
    }

    function bindAdjustmentPanelEvents() {
        const addBtn = document.getElementById('addAdjustmentActionBtn');
        if (addBtn) {
            addBtn.addEventListener('click', () => {
                const type = readValue('adjustmentActionType');
                const slotCode = readValue('adjustmentActionSlotCode');
                const delta = nullableNumber(readValue('adjustmentActionDelta'));
                const levelCode = readValue('adjustmentActionLevel');
                const qualificationCode = readValue('adjustmentActionQualification');
                const equipmentTypeCode = readValue('adjustmentActionEquipmentType');
                const action = buildAdjustmentAction(type, slotCode, delta, levelCode, qualificationCode, equipmentTypeCode);
                state.adjustmentDraft.actions.push(action);
                renderAdjustmentActions();
            });
        }
        const saveBtn = document.getElementById('saveAdjustmentRuleBtn');
        if (saveBtn) {
            saveBtn.addEventListener('click', async () => {
                await saveAdjustmentRule();
            });
        }
        for (const id of ['adjustmentCompareLeft', 'adjustmentCompareRight']) {
            const node = document.getElementById(id);
            if (node) {
                node.addEventListener('change', renderAdjustmentDiff);
            }
        }
    }

    function buildAdjustmentAction(type, slotCode, delta, levelCode, qualificationCode, equipmentTypeCode) {
        const action = { action_type: type };
        if (slotCode) {
            action.slot_code = slotCode;
        }
        if (type === 'add_slot') {
            action.slot = {
                slot_code: slotCode || `slot_${state.adjustmentDraft.actions.length + 1}`,
                qualification_code: qualificationCode || null,
                min_level_code: levelCode || null,
                required_count: delta || 1,
                must_be_distinct: true,
            };
        } else if (type === 'add_equipment_type_requirement') {
            action.equipment_slot = {
                slot_code: slotCode || `equipment_${state.adjustmentDraft.actions.length + 1}`,
                equipment_type_code: equipmentTypeCode || null,
                required_count: delta || 1,
                must_be_distinct: true,
            };
        } else if (type === 'upgrade_min_level') {
            action.min_level_code = levelCode || null;
        } else if (type === 'require_driver_for_equipment') {
            action.driver_qualification_code = qualificationCode || null;
            action.driver_min_level_code = levelCode || null;
        } else if (type === 'extend_duration' || type === 'advance_publish_offset' || type === 'delay_publish_offset') {
            action.delta_minutes = delta || 0;
        } else {
            action.delta = delta || 1;
        }
        if (equipmentTypeCode && !action.equipment_slot) {
            action.equipment_type_code = equipmentTypeCode;
        }
        return action;
    }

    function renderAdjustmentActions() {
        const node = document.getElementById('adjustmentActionsList');
        if (!node) {
            return;
        }
        if (!state.adjustmentDraft.actions.length) {
            node.innerHTML = '<div class="empty-hint">还没有配置动作。可连续添加多条，同类动作默认累加。</div>';
            return;
        }
        node.innerHTML = state.adjustmentDraft.actions.map((item, index) => `
            <div class="item-card">
                <strong>${escapeHtml(ACTION_TEMPLATES[item.action_type] || item.action_type || '动作')}</strong>
                <div class="muted">${escapeHtml(JSON.stringify(item))}</div>
                <div class="inline-actions">
                    <button class="tiny-btn" data-remove-adjustment-index="${index}">删除</button>
                </div>
            </div>
        `).join('');

        for (const button of node.querySelectorAll('[data-remove-adjustment-index]')) {
            button.addEventListener('click', () => {
                const index = Number(button.dataset.removeAdjustmentIndex || -1);
                if (index >= 0) {
                    state.adjustmentDraft.actions.splice(index, 1);
                    renderAdjustmentActions();
                }
            });
        }
    }

    async function saveAdjustmentRule() {
        if (!requireConcreteDepartmentSelection('adjustmentRuleStatus', '“全部科室”模式不支持保存增量调整规则。')) {
            return;
        }
        try {
            const payload = {
                rule_id: readValue('adjustmentRuleId') || null,
                rule_name: readValue('adjustmentRuleName') || null,
                task_type: readValue('adjustmentTaskType'),
                status: readValue('adjustmentStatus') || 'draft',
                conditions: collectConditionFields('adjustment'),
                actions: state.adjustmentDraft.actions,
            };
            await apiPost(
                `/api/v2/dispatch/rules/departments/${encodeURIComponent(state.selectedDepartmentId)}/generation-adjustment-rules`,
                payload
            );
            state.adjustmentDraft.actions = [];
            state.adjustmentDraft.conditions = {};
            state._tdEditingAdj = null;
            await loadDepartmentData(state.selectedDepartmentId);
            renderTaskDrivenPanel();
            setLocalStatus('adjustmentRuleStatus', '增量调整规则已保存。', 'success');
        } catch (error) {
            setLocalStatus('adjustmentRuleStatus', error.message, 'error');
        }
    }

    function renderAdjustmentDiff() {
        const rows = document.getElementById('adjustmentDiffRows');
        if (!rows) {
            return;
        }
        const left = state.adjustmentRules[Number(readValue('adjustmentCompareLeft') || 0)] || null;
        const right = state.adjustmentRules[Number(readValue('adjustmentCompareRight') || 1)] || null;
        if (!left || !right) {
            rows.innerHTML = '<tr><td colspan="3" class="muted">至少需要两条调整规则才能对比。</td></tr>';
            return;
        }
        rows.innerHTML = buildDiffRows(left, right);
    }

    function renderRequirementsPanel() {
        const panel = document.getElementById('panel-requirements');
        if (!panel) {
            return;
        }
        if (shouldShowDepartmentTaskTypeEmptyState()) {
            renderDepartmentTaskTypeEmptyPanel(
                panel,
                '资质与设备要求',
                '定义每一类作业需要哪些资质的人员和哪类设备。编辑完成后保存草稿或直接发布为新版本。',
                '当前科室暂无任务类型。请先在“任务视图”中新增并归属到当前科室，再维护资质与设备要求。'
            );
            return;
        }
        const grouped = groupRequirementVersionsByTaskType();
        const listHtml = grouped.size
            ? Array.from(grouped.entries()).map(([taskType, items]) => {
                const current = items.sort((left, right) => Number(right.version_no || 0) - Number(left.version_no || 0))[0];
                return `
                    <div class="item-card">
                        <strong>${escapeHtml(taskType)}</strong>
                        <div>最新版本: v${escapeHtml(current.version_no || 1)} / ${escapeHtml(current.status || '')}</div>
                        <div>人员槽位: ${(current.crew_requirements || []).length} / 设备槽位: ${(current.equipment_requirements || []).length}</div>
                        <div class="muted">${escapeHtml(summarizeRequirementVersion(current))}</div>
                    </div>
                `;
            }).join('')
            : '<div class="empty-hint">当前没有作业类型资质与设备要求版本。</div>';

        panel.innerHTML = `
            <div class="panel-grid">
                <div>
                    <h2 class="section-title">资质与设备要求</h2>
                    <p class="muted panel-intro">定义每一类作业需要哪些资质的人员和哪类设备。编辑完成后保存草稿或直接发布为新版本。</p>
                    <div class="list-card">
                        <h3>编辑作业类型规则</h3>
                        <div class="form-grid">
                            <label>作业类型<select id="requirementTaskType">${getTaskTypeOptions('')}</select></label>
                            <label>版本备注<textarea id="requirementNotes" placeholder="说明本次调整的原因"></textarea></label>
                        </div>

                        <h4 class="form-group-title">人员资质槽位</h4>
                        <p class="muted">每个槽位代表一个需要分配的人员角色，指定所需资质和最低等级。</p>
                        <table class="builder-table">
                            <thead>
                                <tr><th>槽位名称</th><th>资质要求</th><th>最低等级</th><th>数量</th><th>要求不同人</th><th>操作</th></tr>
                            </thead>
                            <tbody id="crewRequirementRows"></tbody>
                        </table>
                        <button class="section-btn" id="addCrewRequirementBtn" style="margin-top: 8px;">+ 添加人员槽位</button>

                        <h4 class="form-group-title">设备类型槽位</h4>
                        <p class="muted">每个槽位代表一类需要调度的设备，可指定是否需要配司机。</p>
                        <table class="builder-table">
                            <thead>
                                <tr><th>槽位名称</th><th>设备类型</th><th>数量</th><th>不同设备</th><th>司机资质</th><th>司机等级</th><th>操作</th></tr>
                            </thead>
                            <tbody id="equipmentRequirementRows"></tbody>
                        </table>
                        <button class="section-btn" id="addEquipmentRequirementBtn" style="margin-top: 8px;">+ 添加设备槽位</button>

                        <div class="inline-actions" style="margin-top: 16px;">
                            <button class="section-btn" id="loadLatestRequirementBtn">载入线上最新版本</button>
                            <button class="section-btn" id="saveRequirementDraftBtn">保存草稿</button>
                            <button class="section-btn primary" id="publishRequirementBtn">发布为新版本</button>
                        </div>
                        <div class="status-line" id="requirementStatus"></div>
                    </div>
                </div>
                <div>
                    <div class="list-card">
                        <h3>版本历史</h3>
                        <div class="collection">${listHtml}</div>
                    </div>
                    <div class="list-card">
                        <h3>版本差异对比</h3>
                        <div class="form-grid">
                            <label>作业类型<select id="requirementCompareTaskType">${buildTaskTypeCompareOptions()}</select></label>
                            <label>左侧版本<select id="requirementCompareLeft"></select></label>
                            <label>右侧版本<select id="requirementCompareRight"></select></label>
                        </div>
                        <table class="builder-table">
                            <thead><tr><th>字段</th><th>左侧</th><th>右侧</th></tr></thead>
                            <tbody id="requirementDiffRows"></tbody>
                        </table>
                    </div>
                </div>
            </div>
        `;

        bindRequirementsPanelEvents();
        ensureRequirementRows();
        renderRequirementRows();
        renderRequirementDiffSelectors();
        renderRequirementDiff();
    }

    function bindRequirementsPanelEvents() {
        const taskTypeSelect = document.getElementById('requirementTaskType');
        if (taskTypeSelect) {
            taskTypeSelect.addEventListener('change', () => {
                state._tdDraftTaskType = String(taskTypeSelect.value || '').trim();
                loadRequirementVersion(taskTypeSelect.value);
            });
        }
        const notesNode = document.getElementById('requirementNotes');
        if (notesNode) {
            const syncRequirementNotes = () => {
                state._tdRequirementNotes = readValue('requirementNotes') || null;
            };
            notesNode.addEventListener('input', syncRequirementNotes);
            notesNode.addEventListener('change', syncRequirementNotes);
        }
        const addCrewBtn = document.getElementById('addCrewRequirementBtn');
        if (addCrewBtn) {
            addCrewBtn.addEventListener('click', () => {
                state.taskTypeDraft.crew_requirements.push(defaultCrewRequirement());
                renderRequirementRows();
            });
        }
        const addEquipmentBtn = document.getElementById('addEquipmentRequirementBtn');
        if (addEquipmentBtn) {
            addEquipmentBtn.addEventListener('click', () => {
                state.taskTypeDraft.equipment_requirements.push(defaultEquipmentRequirement());
                renderRequirementRows();
            });
        }
        const loadBtn = document.getElementById('loadLatestRequirementBtn');
        if (loadBtn) {
            loadBtn.addEventListener('click', () => {
                const taskType = readValue('requirementTaskType');
                loadRequirementVersion(taskType);
            });
        }
        const saveBtn = document.getElementById('saveRequirementDraftBtn');
        if (saveBtn) {
            saveBtn.addEventListener('click', async () => {
                await saveRequirementDraft();
            });
        }
        const publishBtn = document.getElementById('publishRequirementBtn');
        if (publishBtn) {
            publishBtn.addEventListener('click', async () => {
                await publishRequirementDraft();
            });
        }
        const compareTaskType = document.getElementById('requirementCompareTaskType');
        if (compareTaskType) {
            compareTaskType.addEventListener('change', () => {
                renderRequirementDiffSelectors();
                renderRequirementDiff();
            });
        }
        for (const id of ['requirementCompareLeft', 'requirementCompareRight']) {
            const node = document.getElementById(id);
            if (node) {
                node.addEventListener('change', renderRequirementDiff);
            }
        }
    }

    function defaultCrewRequirement() {
        return {
            slot_code: '',
            qualification_code: '',
            min_level_code: '',
            required_count: 1,
            must_be_distinct: true,
        };
    }

    function defaultEquipmentRequirement() {
        return {
            slot_code: '',
            equipment_type_code: '',
            required_count: 1,
            must_be_distinct: true,
            requires_driver: false,
            driver_qualification_code: '',
            driver_min_level_code: '',
        };
    }

    function ensureRequirementRows() {
        if (!state.taskTypeDraft.crew_requirements.length) {
            state.taskTypeDraft.crew_requirements.push(defaultCrewRequirement());
        }
        if (!state.taskTypeDraft.equipment_requirements.length) {
            state.taskTypeDraft.equipment_requirements.push(defaultEquipmentRequirement());
        }
    }

    function renderRequirementRows() {
        const crewBody = document.getElementById('crewRequirementRows');
        const equipmentBody = document.getElementById('equipmentRequirementRows');
        if (crewBody) {
            crewBody.innerHTML = state.taskTypeDraft.crew_requirements.map((item, index) => `
                <tr>
                    <td><input data-crew-field="slot_code" data-crew-index="${index}" value="${escapeHtml(item.slot_code || '')}"></td>
                    <td><select data-crew-field="qualification_code" data-crew-index="${index}">${getQualificationOptions(item.qualification_code)}</select></td>
                    <td><select data-crew-field="min_level_code" data-crew-index="${index}">${getLevelOptions(item.min_level_code)}</select></td>
                    <td><input data-crew-field="required_count" data-crew-index="${index}" type="number" min="1" value="${escapeHtml(item.required_count || 1)}"></td>
                    <td><input data-crew-field="must_be_distinct" data-crew-index="${index}" type="checkbox"${item.must_be_distinct ? ' checked' : ''}></td>
                    <td><button class="tiny-btn" data-remove-crew-index="${index}">删除</button></td>
                </tr>
            `).join('');
        }
        if (equipmentBody) {
            equipmentBody.innerHTML = state.taskTypeDraft.equipment_requirements.map((item, index) => `
                <tr>
                    <td><input data-equipment-field="slot_code" data-equipment-index="${index}" value="${escapeHtml(item.slot_code || '')}"></td>
                    <td><select data-equipment-field="equipment_type_code" data-equipment-index="${index}">${getEquipmentTypeOptions(item.equipment_type_code)}</select></td>
                    <td><input data-equipment-field="required_count" data-equipment-index="${index}" type="number" min="1" value="${escapeHtml(item.required_count || 1)}"></td>
                    <td><input data-equipment-field="must_be_distinct" data-equipment-index="${index}" type="checkbox"${item.must_be_distinct ? ' checked' : ''}></td>
                    <td><select data-equipment-field="driver_qualification_code" data-equipment-index="${index}">${getQualificationOptions(item.driver_qualification_code)}</select></td>
                    <td><select data-equipment-field="driver_min_level_code" data-equipment-index="${index}">${getLevelOptions(item.driver_min_level_code)}</select></td>
                    <td><button class="tiny-btn" data-remove-equipment-index="${index}">删除</button></td>
                </tr>
            `).join('');
        }

        bindRequirementRowEvents();
    }

    function bindRequirementRowEvents() {
        for (const node of document.querySelectorAll('[data-crew-field]')) {
            node.addEventListener('input', syncCrewRequirementRow);
            node.addEventListener('change', syncCrewRequirementRow);
        }
        for (const node of document.querySelectorAll('[data-equipment-field]')) {
            node.addEventListener('input', syncEquipmentRequirementRow);
            node.addEventListener('change', syncEquipmentRequirementRow);
        }
        for (const button of document.querySelectorAll('[data-remove-crew-index]')) {
            button.addEventListener('click', () => {
                const index = Number(button.dataset.removeCrewIndex || -1);
                if (index >= 0) {
                    state.taskTypeDraft.crew_requirements.splice(index, 1);
                    ensureRequirementRows();
                    renderRequirementRows();
                }
            });
        }
        for (const button of document.querySelectorAll('[data-remove-equipment-index]')) {
            button.addEventListener('click', () => {
                const index = Number(button.dataset.removeEquipmentIndex || -1);
                if (index >= 0) {
                    state.taskTypeDraft.equipment_requirements.splice(index, 1);
                    ensureRequirementRows();
                    renderRequirementRows();
                }
            });
        }
    }

    function syncCrewRequirementRow(event) {
        const node = event.target;
        const index = Number(node.dataset.crewIndex || -1);
        const field = node.dataset.crewField;
        if (index < 0 || !field) {
            return;
        }
        state.taskTypeDraft.crew_requirements[index][field] = node.type === 'checkbox' ? node.checked : node.value;
    }

    function syncEquipmentRequirementRow(event) {
        const node = event.target;
        const index = Number(node.dataset.equipmentIndex || -1);
        const field = node.dataset.equipmentField;
        if (index < 0 || !field) {
            return;
        }
        state.taskTypeDraft.equipment_requirements[index][field] = node.type === 'checkbox' ? node.checked : node.value;
        if (field === 'driver_qualification_code' || field === 'driver_min_level_code') {
            state.taskTypeDraft.equipment_requirements[index].requires_driver = Boolean(
                state.taskTypeDraft.equipment_requirements[index].driver_qualification_code
                || state.taskTypeDraft.equipment_requirements[index].driver_min_level_code
            );
        }
    }

    function loadRequirementVersion(taskType) {
        if (!taskType) {
            setLocalStatus('requirementStatus', '请先选择作业类型。', 'warn');
            return;
        }
        const normalizedTaskType = String(taskType || '').trim();
        const notesNode = document.getElementById('requirementNotes');
        const version = findLatestPublishedRequirement(taskType);
        if (!version) {
            state.taskTypeDraft.crew_requirements = [defaultCrewRequirement()];
            state.taskTypeDraft.equipment_requirements = [defaultEquipmentRequirement()];
            state.taskTypeDraft.turnaround_continuity_rules = [];
            state._tdRequirementNotes = null;
            state._tdDraftTaskType = normalizedTaskType;
            if (notesNode) {
                notesNode.value = '';
            }
            renderRequirementRows();
            setLocalStatus('requirementStatus', '未找到已发布版本，已重置为空白草稿。', 'warn');
            return;
        }
        state.taskTypeDraft.crew_requirements = (version.crew_requirements || []).map((item) => ({ ...item }));
        state.taskTypeDraft.equipment_requirements = (version.equipment_requirements || []).map((item) => ({ ...item }));
        state.taskTypeDraft.turnaround_continuity_rules = (version.turnaround_continuity_rules || []).map((item) => ({ ...item }));
        state._tdRequirementNotes = version.notes || null;
        state._tdDraftTaskType = normalizedTaskType;
        if (notesNode) {
            notesNode.value = version.notes || '';
        }
        ensureRequirementRows();
        renderRequirementRows();
        setLocalStatus('requirementStatus', `已载入 ${taskType} 的最近已发布版本。`, 'success');
    }

    function collectRequirementPayload() {
        const taskType = readValue('requirementTaskType') || readValue('turnaroundTaskType');
        const notesNode = document.getElementById('requirementNotes');
        const notesValue = notesNode ? (readValue('requirementNotes') || null) : (state._tdRequirementNotes || null);
        return {
            task_type: taskType,
            notes: notesValue,
            crew_requirements: state.taskTypeDraft.crew_requirements.map(normalizeCrewRequirement),
            requirements: state.taskTypeDraft.crew_requirements.map(normalizeCrewRequirement),
            equipment_requirements: state.taskTypeDraft.equipment_requirements.map(normalizeEquipmentRequirement),
            turnaround_continuity_rules: state.taskTypeDraft.turnaround_continuity_rules.map(normalizeTurnaroundRule),
        };
    }

    async function saveRequirementDraft() {
        if (!requireConcreteDepartmentSelection('requirementStatus', '“全部科室”模式不支持保存作业类型规则草稿。')) {
            return;
        }
        try {
            await apiPost(
                `/api/v2/dispatch/rules/departments/${encodeURIComponent(state.selectedDepartmentId)}/task-type-requirements/drafts`,
                collectRequirementPayload()
            );
            state._tdDraftTaskType = collectRequirementPayload().task_type || state._tdDraftTaskType;
            await loadDepartmentData(state.selectedDepartmentId);
            renderTaskDrivenPanel();
            setLocalStatus('requirementStatus', '作业类型规则草稿已保存。', 'success');
        } catch (error) {
            setLocalStatus('requirementStatus', error.message, 'error');
        }
    }

    async function publishRequirementDraft() {
        if (!requireConcreteDepartmentSelection('requirementStatus', '“全部科室”模式不支持发布作业类型规则。')) {
            return;
        }
        try {
            const taskType = readValue('requirementTaskType');
            await apiPost(
                `/api/v2/dispatch/rules/departments/${encodeURIComponent(state.selectedDepartmentId)}/task-type-requirements/publish`,
                { task_type: taskType }
            );
            await loadDepartmentData(state.selectedDepartmentId);
            renderTaskDrivenPanel();
            setLocalStatus('requirementStatus', '作业类型规则已发布。', 'success');
        } catch (error) {
            setLocalStatus('requirementStatus', error.message, 'error');
        }
    }

    function buildTaskTypeCompareOptions() {
        const task_types = Array.from(new Set(state.requirementVersions.map((item) => String(item.task_type || '')).filter(Boolean)));
        if (!task_types.length) {
            return '<option value="">暂无作业类型版本</option>';
        }
        return task_types.map((taskType, index) => `<option value="${escapeHtml(taskType)}"${index === 0 ? ' selected' : ''}>${escapeHtml(taskType)}</option>`).join('');
    }

    function renderRequirementDiffSelectors() {
        const taskType = readValue('requirementCompareTaskType') || readValue('requirementTaskType');
        const versions = state.requirementVersions
            .filter((item) => String(item.task_type || '') === String(taskType || ''))
            .sort((left, right) => Number(right.version_no || 0) - Number(left.version_no || 0));
        const left = document.getElementById('requirementCompareLeft');
        const right = document.getElementById('requirementCompareRight');
        const options = versions.map((item, index) => {
            return `<option value="${index}">v${escapeHtml(item.version_no || 1)} / ${escapeHtml(item.status || '')}</option>`;
        }).join('') || '<option value="">暂无版本</option>';
        if (left) {
            left.innerHTML = options;
            left.value = versions.length ? '0' : '';
        }
        if (right) {
            right.innerHTML = options;
            right.value = versions.length > 1 ? '1' : (versions.length ? '0' : '');
        }
    }

    function renderRequirementDiff() {
        const rows = document.getElementById('requirementDiffRows');
        if (!rows) {
            return;
        }
        const taskType = readValue('requirementCompareTaskType') || readValue('requirementTaskType');
        const versions = state.requirementVersions
            .filter((item) => String(item.task_type || '') === String(taskType || ''))
            .sort((left, right) => Number(right.version_no || 0) - Number(left.version_no || 0));
        const left = versions[Number(readValue('requirementCompareLeft') || 0)] || null;
        const right = versions[Number(readValue('requirementCompareRight') || 1)] || null;
        if (!left || !right) {
            rows.innerHTML = '<tr><td colspan="3" class="muted">至少需要两个版本才能对比。</td></tr>';
            return;
        }
        rows.innerHTML = buildDiffRows(left, right);
    }

    function renderTurnaroundPanel() {
        const panel = document.getElementById('panel-turnaround');
        if (!panel) {
            return;
        }
        if (shouldShowDepartmentTaskTypeEmptyState()) {
            renderDepartmentTaskTypeEmptyPanel(
                panel,
                '过站约束',
                '当进出港航班紧密衔接时，定义两端作业之间的人员复用或交接关系。',
                '当前科室暂无任务类型。请先在“任务视图”中新增并归属到当前科室，再配置过站约束。'
            );
            return;
        }
        panel.innerHTML = `
            <div class="panel-grid">
                <div>
                    <h2 class="section-title">过站约束</h2>
                    <p class="muted panel-intro">当进出港航班紧密衔接时，定义两端作业之间的人员复用或交接关系。</p>
                    <div class="list-card">
                        <h3>新建过站约束</h3>
                        <div class="form-grid">
                            <label>本作业类型<select id="turnaroundTaskType">${getTaskTypeOptions('')}</select></label>
                            <label>关联作业类型<select id="turnaroundCounterpartTaskType">${getTaskTypeOptions('')}</select></label>
                            <label>关联方向
                                <select id="turnaroundCounterpartLegScope">
                                    <option value="inbound">进港</option>
                                    <option value="outbound" selected>出港</option>
                                </select>
                            </label>
                        </div>

                        <h4 class="form-group-title">人员关联模式</h4>
                        <div class="info-banner warning">模式说明："强制同一人"要求进出港同槽位必须同一人；"偏好同一人"为软约束，排班器会优先但不强制。</div>
                        <div class="form-grid">
                            <label>关联模式
                                <select id="turnaroundConstraintMode">
                                    <option value="same_person">强制同一人</option>
                                    <option value="soft_prefer_same_person">偏好同一人</option>
                                    <option value="handover_required">必须交接</option>
                                    <option value="disabled">无绑定</option>
                                </select>
                            </label>
                            <label>本端槽位<input id="turnaroundInboundSlot" placeholder="如 cleaner_1"></label>
                            <label>关联端槽位<input id="turnaroundOutboundSlot" placeholder="如 cleaner_1"></label>
                        </div>

                        <details class="advanced-options">
                            <summary>时间门槛与机型过滤</summary>
                            <div class="advanced-content">
                                <div class="form-grid">
                                    <label>紧凑生效最小间隔(分钟)<input id="turnaroundTight" type="number" value="20"></label>
                                    <label>放松判定门槛(分钟)<input id="turnaroundRelax" type="number" value="45"></label>
                                    <label>机型白名单<input id="turnaroundAircraftFilters" placeholder="A330,B787（留空=全部适用）"></label>
                                </div>
                            </div>
                        </details>

                        <h4 class="form-group-title">附加航班条件</h4>
                        <p class="muted">仅当航班满足以下条件时，此过站约束才生效。</p>
                        ${renderConditionBuilder('turnaround', {})}

                        <div class="inline-actions" style="margin-top: 12px;">
                            <button class="section-btn" id="addTurnaroundRuleBtn">+ 添加到约束列表</button>
                            <button class="section-btn primary" id="saveTurnaroundRequirementBtn">保存草稿</button>
                        </div>
                        <div class="status-line" id="turnaroundStatus"></div>
                    </div>
                </div>
                <div>
                    <div class="list-card">
                        <h3>已添加的过站约束</h3>
                        <div class="collection" id="turnaroundRuleList"></div>
                    </div>
                </div>
            </div>
        `;
        bindTurnaroundPanelEvents();
        cbBindEvents('turnaround');
        renderTurnaroundRuleList();
    }

    function bindTurnaroundPanelEvents() {
        const addBtn = document.getElementById('addTurnaroundRuleBtn');
        if (addBtn) {
            addBtn.addEventListener('click', () => {
                const rule = {
                    enabled: true,
                    counterpart_leg_scope: readValue('turnaroundCounterpartLegScope') || 'outbound',
                    counterpart_task_type: readValue('turnaroundCounterpartTaskType') || '',
                    slot_pairs: [{
                        inbound_slot_code: readValue('turnaroundInboundSlot') || '',
                        outbound_slot_code: readValue('turnaroundOutboundSlot') || '',
                    }],
                    constraint_mode: readValue('turnaroundConstraintMode') || 'same_person',
                    tight_threshold_minutes: nullableNumber(readValue('turnaroundTight')),
                    relax_threshold_minutes: nullableNumber(readValue('turnaroundRelax')),
                    flight_filters: collectConditionTree('turnaround'),
                    aircraft_type_filters: splitCsv(readValue('turnaroundAircraftFilters')),
                    notes: null,
                };
                state.taskTypeDraft.turnaround_continuity_rules.push(rule);
                const requirementTaskType = document.getElementById('requirementTaskType');
                if (requirementTaskType) {
                    requirementTaskType.value = readValue('turnaroundTaskType');
                }
                renderTurnaroundRuleList();
                setLocalStatus('turnaroundStatus', '过站约束已追加到当前作业类型草稿。', 'success');
            });
        }
        const saveBtn = document.getElementById('saveTurnaroundRequirementBtn');
        if (saveBtn) {
            saveBtn.addEventListener('click', async () => {
                const requirementTaskType = document.getElementById('requirementTaskType');
                if (requirementTaskType) {
                    requirementTaskType.value = readValue('turnaroundTaskType');
                }
                await saveRequirementDraft();
                setLocalStatus('turnaroundStatus', '已通过作业类型规则草稿接口保存过站约束。', 'success');
            });
        }
    }

    function renderTurnaroundRuleList() {
        const node = document.getElementById('turnaroundRuleList');
        if (!node) {
            return;
        }
        if (!state.taskTypeDraft.turnaround_continuity_rules.length) {
            node.innerHTML = '<div class="empty-hint">当前草稿还没有过站约束。</div>';
            return;
        }
        node.innerHTML = state.taskTypeDraft.turnaround_continuity_rules.map((item, index) => `
            <div class="item-card">
                <strong>${escapeHtml(item.constraint_mode || 'disabled')}</strong>
                <div>${escapeHtml(item.counterpart_task_type || '')} / ${escapeHtml(item.counterpart_leg_scope || '')}</div>
                <div class="muted">${escapeHtml(summarizeTurnaroundRule(item))}</div>
                <div class="inline-actions">
                    <button class="tiny-btn" data-remove-turnaround-index="${index}">删除</button>
                </div>
            </div>
        `).join('');
        for (const button of node.querySelectorAll('[data-remove-turnaround-index]')) {
            button.addEventListener('click', () => {
                const index = Number(button.dataset.removeTurnaroundIndex || -1);
                if (index >= 0) {
                    state.taskTypeDraft.turnaround_continuity_rules.splice(index, 1);
                    renderTurnaroundRuleList();
                }
            });
        }
    }

    function renderPreviewPanel() {
        const panel = document.getElementById('panel-preview');
        if (!panel) {
            return;
        }
        panel.innerHTML = `
            <div class="panel-grid">
                <div>
                    <h2 class="section-title">仿真验证</h2>
                    <div class="info-banner">模拟模式 — 不会产生实际派工单。输入一航班参数，查看引擎会生成什么任务、配置什么资源。</div>
                    <div class="list-card">
                        <h3>模拟航班参数</h3>
                        <div class="form-grid columns-3">
                            <label>航班号<input id="previewFlightId" placeholder="选填"></label>
                            <label>航班方向
                                <select id="previewLegScope">
                                    <option value="inbound">进港</option>
                                    <option value="outbound">出港</option>
                                </select>
                            </label>
                            <label>航班性质<select id="previewFlightNature">${getFlightNatureOptions('domestic', '未指定')}</select></label>
                            <label>航班状态<select id="previewFlightStatus">${getFlightStatusOptions('', '未指定')}</select></label>
                            <label>航站楼<input id="previewTerminal" placeholder="T1"></label>
                            <label>机型<input id="previewAircraftType" placeholder="A320"></label>
                            <label>机位类型<input id="previewStandType" placeholder="remote"></label>
                        </div>
                        <div class="checkbox-row" style="margin-top: 8px;">
                            <label><input type="checkbox" id="previewVip"> VIP</label>
                            <label><input type="checkbox" id="previewTurnaround" checked> 过站</label>
                            <label><input type="checkbox" id="previewBoardingRestriction"> 限制登机</label>
                            <label><input type="checkbox" id="previewQuickTurnaround"> 快速过站</label>
                            <label><input type="checkbox" id="previewCommercialSigned"> 商务签署</label>
                        </div>
                        <details class="advanced-options">
                            <summary>时间参数</summary>
                            <div class="advanced-content">
                                <div class="form-grid">
                                    <label>提早时间(分)<input id="previewDeltaT" type="number" value="60"></label>
                                    <label>最小过站时长(分)<input id="previewMinTurnaround" type="number" value="40"></label>
                                </div>
                            </div>
                        </details>
                        <div class="inline-actions" style="margin-top: 12px;">
                            <button class="section-btn primary" id="runPreviewBtn">执行仿真</button>
                        </div>
                        <div class="status-line" id="previewStatus"></div>
                    </div>
                </div>
                <div>
                    <div class="list-card">
                        <h3>仿真结果</h3>
                        <pre class="preview-box" id="previewResultBox">点击“执行仿真”查看引擎输出…</pre>
                    </div>
                </div>
            </div>
        `;
        const runBtn = document.getElementById('runPreviewBtn');
        if (runBtn) {
            runBtn.addEventListener('click', async () => {
                await runPreview();
            });
        }
    }

    async function runPreview() {
        if (!requireConcreteDepartmentSelection('previewStatus', '“全部科室”模式不支持单科室规则预演。')) {
            return;
        }
        try {
            const selectedTaskType = String(readValue('previewTaskType') || state._tdPreviewTaskType || '').trim();
            state._tdPreviewTaskType = selectedTaskType;
            const payload = {
                flight_id: readValue('previewFlightId') || null,
                sample_flight: {
                    flight_id: readValue('previewFlightId') || null,
                    leg_scope: readValue('previewLegScope') || 'inbound',
                    flight_nature: readValue('previewFlightNature') || null,
                    flight_status: readValue('previewFlightStatus') || null,
                    is_vip: readChecked('previewVip'),
                    is_turnaround: readChecked('previewTurnaround'),
                    has_boarding_restriction: readChecked('previewBoardingRestriction'),
                    is_quick_turnaround: readChecked('previewQuickTurnaround'),
                    is_commercial_signed: readChecked('previewCommercialSigned'),
                    terminal: readValue('previewTerminal') || null,
                    aircraft_type: readValue('previewAircraftType') || null,
                    stand_type: readValue('previewStandType') || null,
                    delta_t_minutes: nullableNumber(readValue('previewDeltaT')),
                    minimum_turnaround_minutes: nullableNumber(readValue('previewMinTurnaround')),
                    turnaround_pair_key: readValue('previewFlightId') || 'sample_pair',
                },
            };
            const result = await apiPost(
                `/api/v2/dispatch/rules/preview`,
                { department_id: state.selectedDepartmentId, ...payload }
            );
            state.previewResult = result;
            const box = document.getElementById('previewResultBox');
            if (box) {
                box.textContent = formatPreviewResultForDisplay(result, selectedTaskType);
            }
            setLocalStatus(
                'previewStatus',
                selectedTaskType ? `仿真完成，结果已按任务类型 ${selectedTaskType} 过滤展示。` : '仿真完成。',
                'success'
            );
        } catch (error) {
            setLocalStatus('previewStatus', error.message, 'error');
        }
    }

    function filterPreviewResultByTaskType(result, taskType) {
        if (!result || typeof result !== 'object') {
            return null;
        }
        const normalizedTaskType = String(taskType || '').trim();
        if (!normalizedTaskType) {
            return { ...result };
        }
        const matchesTaskType = (item) => String(
            item?.task_type
            || item?.task_type_code
            || item?.primary_task_type
            || item?.counterpart_task_type
            || ''
        ).trim() === normalizedTaskType;
        const matchesError = (message) => String(message || '').includes(normalizedTaskType);
        return {
            ...result,
            generated_orders: Array.isArray(result.generated_orders)
                ? result.generated_orders.filter(matchesTaskType)
                : [],
            applied_adjustments: Array.isArray(result.applied_adjustments)
                ? result.applied_adjustments.filter(matchesTaskType)
                : [],
            turnaround_constraints: Array.isArray(result.turnaround_constraints)
                ? result.turnaround_constraints.filter(matchesTaskType)
                : [],
            conflicts: Array.isArray(result.conflicts)
                ? result.conflicts.filter(matchesTaskType)
                : [],
            blocking_errors: Array.isArray(result.blocking_errors)
                ? result.blocking_errors.filter(matchesError)
                : [],
        };
    }

    function formatPreviewResultForDisplay(result, taskType) {
        if (!result) {
            return '点击“执行仿真”查看引擎输出…';
        }
        const normalizedTaskType = String(taskType || '').trim();
        if (!normalizedTaskType) {
            return JSON.stringify(result, null, 2);
        }
        const filtered = filterPreviewResultByTaskType(result, normalizedTaskType) || {};
        const displayPayload = {
            task_type_filter: normalizedTaskType,
            summary: {
                generated_order_count: Array.isArray(filtered.generated_orders) ? filtered.generated_orders.length : 0,
                adjustment_count: Array.isArray(filtered.applied_adjustments) ? filtered.applied_adjustments.length : 0,
                turnaround_constraint_count: Array.isArray(filtered.turnaround_constraints) ? filtered.turnaround_constraints.length : 0,
                conflict_count: Array.isArray(filtered.conflicts) ? filtered.conflicts.length : 0,
                blocking_error_count: Array.isArray(filtered.blocking_errors) ? filtered.blocking_errors.length : 0,
            },
            generated_orders: filtered.generated_orders || [],
            applied_adjustments: filtered.applied_adjustments || [],
            turnaround_constraints: filtered.turnaround_constraints || [],
            conflicts: filtered.conflicts || [],
            blocking_errors: filtered.blocking_errors || [],
        };
        return JSON.stringify(displayPayload, null, 2);
    }

    function renderManualPanel() {
        const panel = document.getElementById('panel-manual');
        if (!panel) {
            return;
        }
        if (shouldShowDepartmentTaskTypeEmptyState()) {
            renderDepartmentTaskTypeEmptyPanel(
                panel,
                '临时加单',
                '规则之外的突发情况，人工创建一张带预设人员/设备的派工单。',
                '当前科室暂无任务类型。请先在“任务视图”中新增并归属到当前科室，再创建临时加单。'
            );
            return;
        }
        panel.innerHTML = `
            <div class="panel-grid">
                <div>
                    <h2 class="section-title">临时加单</h2>
                    <p class="muted panel-intro">规则之外的突发情况，人工创建一张带预设人员/设备的派工单。</p>
                    <div class="list-card">
                        <h3>创建临时工单</h3>
                        <div class="form-grid">
                            <label>来源方式
                                <select id="manualMode">
                                    <option value="task_type">基于作业类型</option>
                                    <option value="template">基于任务模板</option>
                                </select>
                            </label>
                            <label>航班号(可选)<input id="manualFlightId" placeholder="留空即不绑定"></label>
                            <label>作业类型<select id="manualTaskType">${getTaskTypeOptions('')}</select></label>
                            <label>任务模板<select id="manualTemplateCode">${getTemplateOptions('')}</select></label>
                        </div>

                        <h4 class="form-group-title">时间与位置</h4>
                        <div class="form-grid">
                            <label>开始时间<input id="manualStartTime" type="datetime-local"></label>
                            <label>结束时间<input id="manualEndTime" type="datetime-local"></label>
                            <label>机位<input id="manualStandId" placeholder="101"></label>
                            <label>位置描述<input id="manualLocation" placeholder="辅助定位"></label>
                        </div>

                        <details class="advanced-options">
                            <summary>优先级与发布策略</summary>
                            <div class="advanced-content">
                                <div class="form-grid">
                                    <label>优先级<input id="manualPriority" type="number" value="50"></label>
                                    <label>发布状态
                                        <select id="manualPublicationState">
                                            <option value="published">立即生效</option>
                                            <option value="prepublished">预备状态</option>
                                        </select>
                                    </label>
                                </div>
                            </div>
                        </details>

                        <h4 class="form-group-title">人员与设备指定</h4>
                        <div class="form-grid">
                            <label>资源锁定
                                <select id="manualLockMode">
                                    <option value="none">系统自动安排</option>
                                    <option value="team">指定班组</option>
                                    <option value="members">指定人员</option>
                                </select>
                            </label>
                            <label>班组<select id="manualTeamId">${getTeamOptions('')}</select></label>
                        </div>
                        <div class="checkbox-row" style="margin-top: 6px;">
                            <label><input type="checkbox" id="manualLock"> 锁定此单，禁止排班器调配</label>
                        </div>
                        <div class="summary-card" style="margin-top:8px;">
                            <h3>选择人员</h3>
                            <div class="collection" id="manualMemberChoices"><div class="empty-hint">先选择班组后加载成员。</div></div>
                        </div>
                        <div class="summary-card">
                            <h3>固定设备</h3>
                            <div class="collection" id="manualEquipmentChoices">${getEquipmentOptions([])}</div>
                        </div>

                        <div style="margin: 12px 0;">
                            <label>加单事由<textarea id="manualRemarks" placeholder="说明理由，如：值机要求补件…"></textarea></label>
                        </div>
                        <div class="inline-actions">
                            <button class="section-btn" id="loadManualSnapshotBtn">预览数据包</button>
                            <button class="section-btn primary" id="createManualOrderBtn">推送生成工单</button>
                        </div>
                        <div class="status-line" id="manualStatus"></div>
                    </div>
                </div>
                <div>
                    <div class="list-card">
                        <h3>数据包快照</h3>
                        <pre class="preview-box" id="manualSnapshotBox">点击“预览数据包”查看实际发送内容…</pre>
                    </div>
                </div>
            </div>
        `;
        bindManualPanelEvents();
    }

    function bindManualPanelEvents() {
        const teamSelect = document.getElementById('manualTeamId');
        if (teamSelect) {
            teamSelect.addEventListener('change', async () => {
                await renderManualMemberChoices(teamSelect.value);
            });
        }
        const loadBtn = document.getElementById('loadManualSnapshotBtn');
        if (loadBtn) {
            loadBtn.addEventListener('click', () => {
                loadManualSnapshotPreview();
            });
        }
        const createBtn = document.getElementById('createManualOrderBtn');
        if (createBtn) {
            createBtn.addEventListener('click', async () => {
                await createManualOrder();
            });
        }
        renderManualMemberChoices(readValue('manualTeamId'));
    }

    function getTemplateOptions(selectedValue) {
        return '<option value="">请选择模板</option>' + state.temporaryTaskTemplates.map((item) => {
            const value = String(item.template_code || '');
            const label = String(item.template_name || value);
            return `<option value="${escapeHtml(value)}"${value === String(selectedValue || '') ? ' selected' : ''}>${escapeHtml(label)}</option>`;
        }).join('');
    }

    function loadManualSnapshotPreview() {
        const mode = readValue('manualMode') || 'task_type';
        let snapshot = null;
        if (mode === 'template') {
            const templateCode = readValue('manualTemplateCode');
            snapshot = state.temporaryTaskTemplates.find((item) => String(item.template_code) === String(templateCode));
        } else {
            snapshot = findLatestPublishedRequirement(readValue('manualTaskType'));
        }
        const box = document.getElementById('manualSnapshotBox');
        if (!box) {
            return;
        }
        if (!snapshot) {
            box.textContent = '未找到对应的作业类型规则或模板。';
            setLocalStatus('manualStatus', '未找到对应规则。', 'warn');
            return;
        }
        const payload = {
            crew_requirement_snapshot: snapshot.crew_requirements || snapshot.requirements || [],
            equipment_requirement_snapshot: snapshot.equipment_requirements || [],
        };
        box.textContent = JSON.stringify(payload, null, 2);
        setLocalStatus('manualStatus', '需求快照已加载。', 'success');
    }

    async function createManualOrder() {
        if (!requireConcreteDepartmentSelection('manualStatus', '“全部科室”模式不支持直接创建临时工单。')) {
            return;
        }
        try {
            const mode = readValue('manualMode') || 'task_type';
            let snapshot = null;
            if (mode === 'template') {
                snapshot = state.temporaryTaskTemplates.find((item) => String(item.template_code) === String(readValue('manualTemplateCode')));
            } else {
                snapshot = findLatestPublishedRequirement(readValue('manualTaskType'));
            }
            const selectedMembers = collectSelectedManualMembers();
            const selectedEquipment = collectSelectedManualEquipment();
            const crewSnapshot = snapshot ? (snapshot.crew_requirements || snapshot.requirements || []) : [];
            const equipmentSnapshot = snapshot ? (snapshot.equipment_requirements || []) : [];
            const taskCrew = buildManualTaskCrew(selectedMembers, crewSnapshot);
            const equipmentAssignment = buildManualEquipmentAssignments(selectedEquipment, equipmentSnapshot);
            const lockMode = readValue('manualLockMode') || 'none';
            const payload = {
                flight_id: readValue('manualFlightId') || null,
                department_id: state.selectedDepartmentId || null,
                task_type: mode === 'task_type' ? readValue('manualTaskType') || null : null,
                temporary_task_template_code: mode === 'template' ? readValue('manualTemplateCode') || null : null,
                stand_id: readValue('manualStandId') || null,
                location: readValue('manualLocation') || null,
                planned_start_time: toIsoOrNull(readValue('manualStartTime')),
                planned_end_time: toIsoOrNull(readValue('manualEndTime')),
                priority: nullableNumber(readValue('manualPriority')),
                publication_state: readValue('manualPublicationState') || 'published',
                manual_lock: readChecked('manualLock') || lockMode !== 'none' || selectedEquipment.length > 0,
                remarks: readValue('manualRemarks') || null,
                crew_requirement_snapshot: crewSnapshot,
                equipment_requirement_snapshot: equipmentSnapshot,
                task_crew: taskCrew,
                equipment_assignment: equipmentAssignment,
                assignee_type: lockMode === 'team' ? 'team' : (selectedMembers.length === 1 ? 'individual' : null),
                team_id: lockMode === 'team' ? (readValue('manualTeamId') || null) : null,
                individual_user_id: selectedMembers.length === 1 ? selectedMembers[0].user_id : null,
            };
            const result = await apiPost('/api/v2/dispatch-orders', payload);
            const box = document.getElementById('manualSnapshotBox');
            if (box) {
                box.textContent = JSON.stringify(result, null, 2);
            }
            setLocalStatus('manualStatus', '人工工单已创建并进入统一工单链路。', 'success');
        } catch (error) {
            setLocalStatus('manualStatus', error.message, 'error');
        }
    }

    async function renderManualMemberChoices(teamId) {
        const node = document.getElementById('manualMemberChoices');
        if (!node) {
            return;
        }
        const normalized = String(teamId || '').trim();
        if (!normalized) {
            node.innerHTML = '<div class="empty-hint">先选择班组后加载成员。</div>';
            return;
        }
        const members = await ensureTeamMembers(normalized);
        if (!members.length) {
            node.innerHTML = '<div class="empty-hint">该班组暂无成员。</div>';
            return;
        }
        node.innerHTML = members.map((item) => {
            const label = item.user_display_name || item.username || item.user_id;
            return `<label><input type="checkbox" data-manual-member-id="${escapeHtml(item.user_id)}" data-manual-member-name="${escapeHtml(label)}" data-manual-member-team="${escapeHtml(normalized)}"> ${escapeHtml(label)}</label>`;
        }).join('');
    }

    function collectSelectedManualMembers() {
        return Array.from(document.querySelectorAll('[data-manual-member-id]:checked')).map((node) => {
            return {
                user_id: node.dataset.manualMemberId,
                username: node.dataset.manualMemberName,
                source_team_id: node.dataset.manualMemberTeam,
            };
        });
    }

    function collectSelectedManualEquipment() {
        return Array.from(document.querySelectorAll('[data-manual-equipment-id]:checked')).map((node) => {
            const equipmentId = node.dataset.manualEquipmentId;
            const equipment = state.equipment.find((item) => String(item.id) === String(equipmentId));
            return equipment ? equipment : { id: equipmentId };
        });
    }

    function buildManualTaskCrew(selectedMembers, crewSnapshot) {
        if (!selectedMembers.length) {
            return {};
        }
        const slotCodes = [];
        for (const item of crewSnapshot || []) {
            const count = Number(item.required_count || 1);
            for (let index = 0; index < count; index += 1) {
                slotCodes.push(item.slot_code || null);
            }
        }
        return {
            members: selectedMembers.map((member, index) => ({
                user_id: member.user_id,
                username: member.username,
                source_team_id: member.source_team_id,
                slot_code: slotCodes[index] || null,
            })),
            source_team_ids: Array.from(new Set(selectedMembers.map((item) => item.source_team_id).filter(Boolean))),
            generated_from: 'manual_lock',
        };
    }

    function buildManualEquipmentAssignments(selectedEquipment, equipmentSnapshot) {
        if (!selectedEquipment.length) {
            return [];
        }
        const slotCodes = [];
        for (const item of equipmentSnapshot || []) {
            const count = Number(item.required_count || 1);
            for (let index = 0; index < count; index += 1) {
                slotCodes.push(item.slot_code || null);
            }
        }
        return selectedEquipment.map((item, index) => ({
            slot_code: slotCodes[index] || null,
            equipment_id: item.id || null,
            equipment_code: item.code || item.name || item.id || null,
        }));
    }

    function normalizeCrewRequirement(item) {
        return {
            slot_code: String(item.slot_code || '').trim(),
            qualification_code: String(item.qualification_code || '').trim(),
            min_level_code: String(item.min_level_code || '').trim() || null,
            required_count: Number(item.required_count || 1),
            must_be_distinct: Boolean(item.must_be_distinct),
        };
    }

    function normalizeEquipmentRequirement(item) {
        return {
            slot_code: String(item.slot_code || '').trim(),
            equipment_type_code: String(item.equipment_type_code || '').trim() || null,
            required_count: Number(item.required_count || 1),
            must_be_distinct: Boolean(item.must_be_distinct),
            requires_driver: Boolean(item.requires_driver || item.driver_qualification_code || item.driver_min_level_code),
            driver_qualification_code: String(item.driver_qualification_code || '').trim() || null,
            driver_min_level_code: String(item.driver_min_level_code || '').trim() || null,
        };
    }

    function normalizeTurnaroundRule(item) {
        return {
            enabled: Boolean(item.enabled),
            counterpart_leg_scope: item.counterpart_leg_scope || 'outbound',
            counterpart_task_type: item.counterpart_task_type || '',
            slot_pairs: (item.slot_pairs || []).map((pair) => ({
                inbound_slot_code: pair.inbound_slot_code || '',
                outbound_slot_code: pair.outbound_slot_code || '',
            })),
            constraint_mode: item.constraint_mode || 'disabled',
            tight_threshold_minutes: nullableNumber(item.tight_threshold_minutes),
            relax_threshold_minutes: nullableNumber(item.relax_threshold_minutes),
            flight_filters: item.flight_filters || {},
            aircraft_type_filters: item.aircraft_type_filters || [],
            notes: item.notes || null,
        };
    }

    function readValue(id) {
        const node = document.getElementById(id);
        return node ? String(node.value || '').trim() : '';
    }

    function readChecked(id) {
        const node = document.getElementById(id);
        return Boolean(node && node.checked);
    }

    function nullableNumber(value) {
        if (value == null || value === '') {
            return null;
        }
        const result = Number(value);
        return Number.isFinite(result) ? result : null;
    }

    function safeParseJson(text, fallback) {
        const normalized = String(text || '').trim();
        if (!normalized) {
            return fallback;
        }
        try {
            return JSON.parse(normalized);
        } catch (_error) {
            throw new Error('JSON 格式不合法');
        }
    }

    function splitCsv(text) {
        return String(text || '')
            .split(',')
            .map((item) => item.trim())
            .filter(Boolean);
    }

    function toIsoOrNull(value) {
        if (!value) {
            return null;
        }
        return new Date(value).toISOString();
    }

    function setLocalStatus(id, message, kind) {
        const node = document.getElementById(id);
        if (!node) {
            return;
        }
        node.textContent = message || '';
        node.className = `status-line${kind ? ` ${kind}` : ''}`;
    }
})();
