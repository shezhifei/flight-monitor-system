/**
 * 业务事项时间轴和甘特图组件
 * 用于航班详情面板中的业务事项可视化展示
 * 使用 ECharts 渲染甘特图
 */

let currentCaseFilter = 'all';
let ganttChart = null;
let currentFlight = null;
let ganttResizeHandlerBound = false;
let ganttHostElement = null;
let businessCaseModalState = {
    open: false,
    loading: false,
    submitting: false,
    statusUpdating: false,
    workflowFormsLoading: false,
    workflowFormsError: '',
    workflowFormsSubmittingKey: '',
    workflowFormsPayload: null,
    workflowFormDrafts: {},
    caseId: null,
    caseData: null,
};

const fetchBusinessCaseDetailOptions = Object.freeze({ suppressGlobalLoader: true });
const fetchBusinessCaseWorkflowFormsOptions = Object.freeze({ suppressGlobalLoader: true });
const fetchBusinessCaseModalMutationOptions = Object.freeze({ suppressGlobalLoader: true });
const fetchBusinessCaseStakeholdersOptions = Object.freeze({ suppressGlobalLoader: true });

let businessCaseMentionState = {
    stakeholders: [],
    selectedIds: new Set(),
    showDropdown: false,
    flightId: null,
};

function resolveCaseTypeName(caseOrCode) {
    if (!caseOrCode) return '-';
    if (typeof caseOrCode === 'object') {
        return caseOrCode.case_type_name || caseOrCode.case_type || '-';
    }
    return caseOrCode;
}

const STATUS_COLORS = {
    INITIAL: '#8E8E93',
    PENDING: '#FF9500',
    PROCESSING: '#5856D6',
    SUCCESS: '#34C759',
    COMPLETED: '#34C759',
    FAILED: '#FF3B30',
};
const BUSINESS_CASE_STATUSES = ['INITIAL', 'PENDING', 'PROCESSING', 'SUCCESS', 'COMPLETED', 'FAILED'];
const BUSINESS_CASE_STATUS_LABELS = {
    INITIAL: '初始',
    PENDING: '待处理',
    PROCESSING: '处理中',
    SUCCESS: '成功',
    COMPLETED: '已完成',
    FAILED: '失败',
};

const FLIGHT_STATUS_COLORS = {
    '计划中': '#007AFF',
    '登机中': '#FF9500',
    '已起飞': '#34C759',
    '到达': '#5856D6',
    '延误': '#FF3B30',
    '取消': '#8E8E93'
};

const TIME_NODE_CONFIG = [
    { field: 'scheduled_arrival', label: '计划到达', color: '#007AFF' },
    { field: 'actual_arrival', label: '实际到达', color: '#34C759' },
    { field: 'on_blocks_time', label: '上轮挡', color: '#FF9500' },
    { field: 'cabin_door_open_time', label: '开舱门', color: '#5856D6' },
    { field: 'deboarding_complete_time', label: '下客完成', color: '#FF2D55' },
    { field: 'cleaning_start_time', label: '清洁开始', color: '#AF52DE' },
    { field: 'cleaning_end_time', label: '清洁结束', color: '#FF9500' },
    { field: 'cabin_door_close_time', label: '关客舱门', color: '#5856D6' },
    { field: 'cargo_door_close_time', label: '关货舱门', color: '#007AFF' },
    { field: 'loading_complete_time', label: '装载完成', color: '#34C759' },
    { field: 'boarding_allowed_time', label: '允许登机', color: '#FF9500' },
    { field: 'start_boarding_time', label: '开始登机', color: '#FFCC00' },
    { field: 'passenger_ready_time', label: '人齐', color: '#34C759' },
    { field: 'end_boarding_time', label: '结束登机', color: '#5856D6' },
    { field: 'off_blocks_time', label: '撤轮挡', color: '#FF9500' },
    { field: 'scheduled_departure', label: '计划起飞', color: '#007AFF' },
    { field: 'actual_departure', label: '实际起飞', color: '#34C759' }
];

const DEPARTMENT_DEFAULT_LABEL = '未标注科室';
const DEPARTMENT_PENDING_MARKER_MS = 8 * 60 * 1000;
const DISPATCH_ORDER_PAGE_SIZE = 100;
const DEPARTMENT_PRESENCE_COLORS = {
    pending: '#9CA3AF',
    in_progress: '#0EA5A4',
    completed: '#2563EB'
};

const dispatchOrderCache = new Map();
const dispatchOrderPromiseCache = new Map();

function escapeHtml(value) {
    return String(value ?? '')
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
}

function jsStringForInlineHandler(value) {
    return escapeHtml(JSON.stringify(String(value ?? '')));
}

function formatDateTime(value) {
    if (!value) {
        return '--';
    }
    const date = value instanceof Date ? value : new Date(value);
    if (Number.isNaN(date.getTime())) {
        return '--';
    }
    return date.toLocaleString('zh-CN');
}

function getBusinessCaseStatusText(status) {
    const normalized = String(status || '').trim().toUpperCase();
    return BUSINESS_CASE_STATUS_LABELS[normalized] || normalized || '-';
}

function getBusinessCaseStatusValue(status) {
    return String(status || '').trim().toUpperCase() || 'PENDING';
}

function getBusinessCaseStatusOptions(caseData) {
    const currentStatus = getBusinessCaseStatusValue(caseData?.status);
    const statuses = BUSINESS_CASE_STATUSES.includes(currentStatus)
        ? BUSINESS_CASE_STATUSES
        : [currentStatus, ...BUSINESS_CASE_STATUSES];
    return statuses.map((status) => ({
        value: status,
        label: getBusinessCaseStatusText(status),
    }));
}

function getBusinessCaseCurrentUser() {
    if (typeof Auth === 'undefined' || typeof Auth.getUser !== 'function') {
        return null;
    }
    return Auth.getUser() || null;
}

function getBusinessCaseCurrentUsername() {
    const user = getBusinessCaseCurrentUser();
    return String(user?.username || user?.user_id || user?.sub || '').trim();
}

function hasFlightManagePermission() {
    if (typeof Auth !== 'undefined' && typeof Auth.hasAnyPermission === 'function') {
        return Auth.hasAnyPermission([
            'business_case.update',
            'business_case.status_transition',
            'business_case.delete',
            'business_case.*',
        ]);
    }

    const user = getBusinessCaseCurrentUser();
    if (!user) {
        return false;
    }
    if (user.is_admin === true || user.role === 'admin') {
        return true;
    }
    const permissions = Array.isArray(user.permissions) ? user.permissions : [];
    return [
        'business_case.update',
        'business_case.status_transition',
        'business_case.delete',
        'business_case.*',
    ].some((permission) => permissions.includes(permission));
}

function canEditBusinessCaseStatus(caseData) {
    if (!caseData) {
        return false;
    }
    if (hasFlightManagePermission()) {
        return true;
    }
    const currentUsername = getBusinessCaseCurrentUsername();
    const creator = String(caseData.created_by || '').trim();
    return Boolean(currentUsername && creator && currentUsername === creator);
}

function normalizeBusinessCaseVisibilityScope(value) {
    const normalized = String(value || '').trim().toUpperCase();
    if (normalized === 'COMMON' || normalized === 'DEPARTMENT') {
        return normalized;
    }
    return '';
}

function readBusinessCaseText(source, keys) {
    if (!source || typeof source !== 'object') {
        return null;
    }
    for (const key of keys) {
        const value = String(source[key] || '').trim();
        if (value) {
            return value;
        }
    }
    return null;
}

function getBusinessCaseVisibilityInfo(caseData) {
    const context = caseData && typeof caseData.context === 'object' && caseData.context !== null
        ? caseData.context
        : {};
    const departmentId = readBusinessCaseText(caseData, ['department_id', 'departmentId'])
        || readBusinessCaseText(context, ['department_id', 'departmentId']);
    const departmentName = readBusinessCaseText(caseData, [
        'department_name_snapshot',
        'department_name',
        'departmentName',
    ]) || readBusinessCaseText(context, [
        'department_name_snapshot',
        'department_name',
        'departmentName',
    ]);
    const scope = normalizeBusinessCaseVisibilityScope(
        (caseData && (caseData.visibility_scope || caseData.visibilityScope))
        || context.visibility_scope
        || context.visibilityScope,
    ) || (departmentId || departmentName ? 'DEPARTMENT' : 'COMMON');

    return {
        scope,
        scopeLabel: scope === 'COMMON'
            ? '通用'
            : (departmentName ? `所属部门 · ${departmentName}` : '所属部门'),
        departmentId,
        departmentName,
        isCommon: scope === 'COMMON',
    };
}

function getBusinessCaseBinding(caseData) {
    const context = caseData && typeof caseData.context === 'object' && caseData.context !== null
        ? caseData.context
        : {};
    const legType = String(context.bound_leg_type || '').trim().toLowerCase();
    const flightNo = String(context.bound_flight_no || '').trim().toUpperCase();
    if (!flightNo) {
        return '';
    }
    const legLabel = legType === 'inbound'
        ? '进港'
        : (legType === 'outbound' ? '出港' : '绑定');
    return `${legLabel} ${flightNo}`;
}

function getLatestAppendSummary(caseItem) {
    const appendCount = Number(caseItem?.append_count || 0);
    const latest = caseItem?.latest_append || null;
    if (!appendCount || !latest) {
        return '';
    }

    const operatorName = escapeHtml(latest.submitted_operator_name || '未命名值班人');
    const accountName = escapeHtml(latest.submitted_by || '-');
    const content = escapeHtml(latest.content || '');

    return `
        <div style="margin-top:6px; color:#526477; font-size:12px; line-height:1.5;">
            <div><strong>已追加 ${appendCount} 次</strong></div>
            <div>最近追加: ${escapeHtml(formatDateTime(latest.appended_at))} ${operatorName} (${accountName})</div>
            <div style="color:#6f8093;">${content || '-'}</div>
        </div>
    `;
}

function getBusinessCaseContext(caseData) {
    if (!caseData || typeof caseData !== 'object' || typeof caseData.context !== 'object' || caseData.context === null) {
        return {};
    }
    return caseData.context;
}

function getBusinessCaseWorkflowReceipt(caseData) {
    // Backend refactor: workflow_receipt is now a first-class field on the
    // case object, populated via SQL JOINs from notifications. Fall back to
    // the legacy context path for backward compatibility with cached data.
    if (caseData && caseData.workflow_receipt && typeof caseData.workflow_receipt === 'object') {
        return caseData.workflow_receipt;
    }
    const context = getBusinessCaseContext(caseData);
    if (context.workflow_receipt && typeof context.workflow_receipt === 'object') {
        return context.workflow_receipt;
    }
    return null;
}

function getBusinessCaseReceiptStatusLabel(caseData) {
    const overallStatus = String(getBusinessCaseWorkflowReceipt(caseData)?.summary?.overall_status || '').trim().toLowerCase();
    if (!overallStatus) {
        return '';
    }
    if (overallStatus === 'acknowledged' || overallStatus === 'confirmed' || overallStatus === 'completed') {
        return '已全部回执';
    }
    if (overallStatus === 'rejected') {
        return '存在拒绝';
    }
    if (overallStatus === 'pending') {
        return '待回执';
    }
    return overallStatus;
}

function getBusinessCaseReceiptSummaryText(caseData) {
    const summary = getBusinessCaseWorkflowReceipt(caseData)?.summary || null;
    if (!summary) {
        return '';
    }
    const acknowledgedCount = Number(summary.acknowledged_count || 0);
    const pendingCount = Number(summary.pending_count || 0);
    const rejectedCount = Number(summary.rejected_count || 0);
    return `已回执 ${acknowledgedCount} / 待回执 ${pendingCount} / 拒绝 ${rejectedCount}`;
}

function renderBusinessCaseReceiptSummary(caseData) {
    const statusLabel = getBusinessCaseReceiptStatusLabel(caseData);
    const summaryText = getBusinessCaseReceiptSummaryText(caseData);
    if (!statusLabel && !summaryText) {
        return '';
    }
    const isRejected = statusLabel === '存在拒绝';
    const isDone = statusLabel === '已全部回执';
    const pillStyle = isRejected
        ? 'background:rgba(255,59,48,0.12); color:#b42318;'
        : (isDone
            ? 'background:rgba(52,199,89,0.12); color:#1f8f49;'
            : 'background:rgba(255,149,0,0.14); color:#b26a00;');
    return `
        <div style="margin-top:8px; display:flex; align-items:center; gap:8px; flex-wrap:wrap;">
            <span style="display:inline-flex; align-items:center; padding:2px 8px; border-radius:999px; font-size:11px; font-weight:600; ${pillStyle}">通知回执 · ${escapeHtml(statusLabel || '已同步')}</span>
            ${summaryText ? `<span style="font-size:12px; color:#526477;">${escapeHtml(summaryText)}</span>` : ''}
        </div>
    `;
}

function getWorkflowFormsProjection(caseData) {
    const context = getBusinessCaseContext(caseData);
    if (context.forms && typeof context.forms === 'object' && context.forms !== null) {
        return context.forms;
    }
    return {};
}

function getWorkflowFormProjectionEntries(caseData) {
    const formsProjection = getWorkflowFormsProjection(caseData);
    return Object.values(formsProjection)
        .filter((item) => item && typeof item === 'object')
        .sort((left, right) => {
            const leftAt = new Date(left.submitted_at || 0).getTime();
            const rightAt = new Date(right.submitted_at || 0).getTime();
            return rightAt - leftAt;
        });
}

function summarizeWorkflowFormProjectionEntry(item) {
    if (!item || typeof item !== 'object') {
        return '';
    }
    const summary = item.summary && typeof item.summary === 'object' ? item.summary : {};
    const summaryPairs = Object.entries(summary)
        .filter(([, value]) => value !== null && value !== undefined && String(value).trim() !== '')
        .slice(0, 3)
        .map(([key, value]) => `${key}: ${Array.isArray(value) ? value.join('、') : String(value)}`);
    if (summaryPairs.length > 0) {
        return summaryPairs.join('；');
    }
    const data = item.data && typeof item.data === 'object' ? item.data : {};
    const dataPairs = Object.entries(data)
        .filter(([, value]) => value !== null && value !== undefined && String(value).trim() !== '')
        .slice(0, 2)
        .map(([key, value]) => `${key}: ${Array.isArray(value) ? value.join('、') : String(value)}`);
    return dataPairs.join('；');
}

function getBusinessCaseFormsSummaryText(caseData) {
    const entries = getWorkflowFormProjectionEntries(caseData);
    if (entries.length <= 0) {
        return '';
    }
    const latestEntry = entries[0];
    const summaryText = summarizeWorkflowFormProjectionEntry(latestEntry);
    const title = String(latestEntry.name || latestEntry.form_name || latestEntry.form_code || '表单').trim();
    return `${title}${summaryText ? `：${summaryText}` : ''}`;
}

function renderBusinessCaseFormsSummary(caseData) {
    const entries = getWorkflowFormProjectionEntries(caseData);
    if (entries.length <= 0) {
        return '';
    }
    const summaryText = getBusinessCaseFormsSummaryText(caseData);
    return `
        <div style="margin-top:8px; color:#526477; font-size:12px; line-height:1.5;">
            <div><strong>流程表单</strong> · 已提交 ${entries.length} 份</div>
            ${summaryText ? `<div style="color:#6f8093;">最近表单: ${escapeHtml(summaryText)}</div>` : ''}
        </div>
    `;
}

function normalizeWorkflowFormsPayload(payload) {
    const container = payload && typeof payload === 'object' ? payload : {};
    const forms = Array.isArray(container.forms) ? container.forms : [];
    return {
        case_id: String(container.case_id || '').trim(),
        run_id: String(container.run_id || '').trim(),
        process_instance_id: String(container.process_instance_id || '').trim(),
        forms: forms.map((item, index) => normalizeWorkflowFormItem(item, index)),
    };
}

function normalizeWorkflowFormItem(item, index) {
    const latestSubmission = item?.latest_submission && typeof item.latest_submission === 'object'
        ? item.latest_submission
        : null;
    const rawSchema = item?.schema && typeof item.schema === 'object' ? item.schema : {};
    const rawUiSchema = item?.ui_schema && typeof item.ui_schema === 'object' ? item.ui_schema : {};
    return {
        key: String(item?.task_id || item?.form_code || `workflow-form-${index}`),
        task_id: String(item?.task_id || '').trim(),
        task_definition_key: String(item?.task_definition_key || '').trim(),
        task_name: String(item?.task_name || item?.name || item?.form_code || '流程表单').trim(),
        form_code: String(item?.form_code || '').trim(),
        form_version: Number(item?.form_version || 0) || null,
        name: String(item?.name || item?.form_name || item?.form_code || '未命名表单').trim(),
        description: String(item?.description || rawSchema.description || '').trim(),
        schema: rawSchema,
        ui_schema: rawUiSchema,
        can_submit: item?.can_submit === true,
        readonly_reason: String(item?.readonly_reason || '').trim(),
        allow_resubmit: item?.allow_resubmit === true
            || rawSchema.allow_resubmit === true
            || Boolean(latestSubmission && item?.can_submit === true),
        complete_task_on_submit: item?.complete_task_on_submit !== false,
        latest_submission: latestSubmission,
    };
}

function getWorkflowFormsState() {
    return normalizeWorkflowFormsPayload(businessCaseModalState.workflowFormsPayload);
}

function getWorkflowFormDraftKey(formItem) {
    if (!formItem) {
        return '';
    }
    return String(formItem.task_id || formItem.form_code || formItem.key || '').trim();
}

function extractWorkflowFormFields(schema, uiSchema) {
    const normalizedSchema = schema && typeof schema === 'object' ? schema : {};
    const normalizedUiSchema = uiSchema && typeof uiSchema === 'object' ? uiSchema : {};
    if (Array.isArray(normalizedSchema.fields)) {
        return normalizedSchema.fields
            .map((field, index) => normalizeWorkflowFormField(field, index))
            .sort((left, right) => left.order - right.order);
    }

    const properties = normalizedSchema.properties && typeof normalizedSchema.properties === 'object'
        ? normalizedSchema.properties
        : {};
    const requiredKeys = Array.isArray(normalizedSchema.required) ? normalizedSchema.required : [];
    const schemaKeys = Object.keys(properties);
    const orderedKeys = Array.isArray(normalizedUiSchema.order)
        ? normalizedUiSchema.order.filter((item) => item && item !== '*')
        : schemaKeys;
    const mergedOrder = Array.from(new Set([...orderedKeys, ...schemaKeys]));

    return mergedOrder
        .filter((key) => properties[key] && typeof properties[key] === 'object')
        .map((key, index) => normalizeWorkflowFormField({
            key,
            label: properties[key].title || properties[key].label || key,
            type: inferWorkflowFormFieldType(properties[key], normalizedUiSchema?.[key]),
            required: requiredKeys.includes(key),
            placeholder: properties[key].placeholder || normalizedUiSchema?.[key]?.placeholder || '',
            default: properties[key].default,
            options: extractWorkflowFormFieldOptions(properties[key]),
        }, index))
        .sort((left, right) => left.order - right.order);
}

function inferWorkflowFormFieldType(propertyDefinition, uiDefinition) {
    const propertySchema = propertyDefinition && typeof propertyDefinition === 'object' ? propertyDefinition : {};
    const uiSchema = uiDefinition && typeof uiDefinition === 'object' ? uiDefinition : {};
    const widget = String(
        propertySchema['x-fm-type']
        || uiSchema.widget
        || uiSchema['ui:widget']
        || ''
    ).trim().toLowerCase();
    if (widget) {
        return widget;
    }
    const propertyType = String(propertySchema.type || '').trim().toLowerCase();
    const format = String(propertySchema.format || '').trim().toLowerCase();
    if (format === 'date-time') {
        return 'date-time';
    }
    if (propertyType === 'array') {
        return 'multi-select';
    }
    if (propertyType === 'string' && Array.isArray(propertySchema.enum)) {
        return 'single-select';
    }
    return propertyType || 'string';
}

function extractWorkflowFormFieldOptions(propertyDefinition) {
    const propertySchema = propertyDefinition && typeof propertyDefinition === 'object' ? propertyDefinition : {};
    if (Array.isArray(propertySchema.options)) {
        return propertySchema.options;
    }
    if (Array.isArray(propertySchema.enum)) {
        return propertySchema.enum.map((value) => ({ value, label: String(value) }));
    }
    const items = propertySchema.items && typeof propertySchema.items === 'object'
        ? propertySchema.items
        : {};
    if (Array.isArray(items.enum)) {
        return items.enum.map((value) => ({ value, label: String(value) }));
    }
    return [];
}

function normalizeWorkflowFormField(field, index) {
    const normalized = field && typeof field === 'object' ? field : {};
    const rawType = String(
        normalized.type
        || normalized.widget
        || normalized.format
        || normalized.field_type
        || 'string'
    ).trim().toLowerCase();
    const typeMap = {
        text: 'string',
        string: 'string',
        textarea: 'textarea',
        integer: 'integer',
        number: 'number',
        boolean: 'boolean',
        checkbox: 'boolean',
        'date-time': 'date-time',
        datetime: 'date-time',
        select: 'single-select',
        radio: 'single-select',
        'single-select': 'single-select',
        array: 'multi-select',
        multiselect: 'multi-select',
        checkboxgroup: 'multi-select',
        'multi-select': 'multi-select',
    };
    const normalizedType = typeMap[rawType] || 'string';
    const options = Array.isArray(normalized.options)
        ? normalized.options
        : (Array.isArray(normalized.enum)
            ? normalized.enum.map((value) => ({ value, label: String(value) }))
            : []);
    return {
        order: Number(normalized.order ?? index) || index,
        key: String(normalized.key || normalized.name || `field_${index + 1}`).trim(),
        label: String(normalized.label || normalized.title || normalized.key || `字段 ${index + 1}`).trim(),
        type: normalizedType,
        required: normalized.required === true,
        placeholder: String(normalized.placeholder || '').trim(),
        default: normalized.default,
        options: options.map((item) => normalizeWorkflowFormOption(item)).filter((item) => item.value !== ''),
    };
}

function normalizeWorkflowFormOption(option) {
    if (option && typeof option === 'object') {
        return {
            value: String(option.value ?? '').trim(),
            label: String(option.label ?? option.value ?? '').trim(),
        };
    }
    return {
        value: String(option ?? '').trim(),
        label: String(option ?? '').trim(),
    };
}

function getWorkflowFormDefaultValue(field) {
    if (!field) {
        return '';
    }
    if (field.type === 'boolean') {
        return field.default === true;
    }
    if (field.type === 'multi-select') {
        return Array.isArray(field.default) ? field.default.map((item) => String(item)) : [];
    }
    if (field.default === null || field.default === undefined) {
        return '';
    }
    return field.default;
}

function getWorkflowFormDraftValue(formItem, field) {
    const formDraftKey = getWorkflowFormDraftKey(formItem);
    const formDraft = businessCaseModalState.workflowFormDrafts[formDraftKey];
    if (formDraft && Object.prototype.hasOwnProperty.call(formDraft, field.key)) {
        return formDraft[field.key];
    }
    const latestData = formItem?.latest_submission?.data && typeof formItem.latest_submission.data === 'object'
        ? formItem.latest_submission.data
        : {};
    if (formItem?.allow_resubmit && Object.prototype.hasOwnProperty.call(latestData, field.key)) {
        return latestData[field.key];
    }
    return getWorkflowFormDefaultValue(field);
}

function getWorkflowFormSubmissionSummary(submission) {
    if (!submission || typeof submission !== 'object') {
        return '';
    }
    const summary = submission.summary && typeof submission.summary === 'object' ? submission.summary : {};
    const summaryEntries = Object.entries(summary)
        .filter(([, value]) => value !== null && value !== undefined && String(value).trim() !== '')
        .slice(0, 4)
        .map(([key, value]) => `${key}: ${Array.isArray(value) ? value.join('、') : String(value)}`);
    if (summaryEntries.length > 0) {
        return summaryEntries.join('；');
    }
    return summarizeWorkflowFormProjectionEntry(submission);
}

function formatWorkflowFormValue(value, field) {
    if (value === null || value === undefined || value === '') {
        return '--';
    }
    if (field?.type === 'boolean') {
        return value ? '是' : '否';
    }
    if (Array.isArray(value)) {
        return value.map((item) => String(item)).join('、') || '--';
    }
    if (field?.type === 'date-time') {
        return formatDateTime(value);
    }
    return String(value);
}

function escapeJsString(value) {
    return String(value ?? '')
        .replace(/\\/g, '\\\\')
        .replace(/'/g, "\\'");
}

function formatWorkflowFormDateTimeInput(value) {
    if (!value) {
        return '';
    }
    const date = value instanceof Date ? value : new Date(value);
    if (Number.isNaN(date.getTime())) {
        return '';
    }
    const offset = date.getTimezoneOffset() * 60000;
    return new Date(date.getTime() - offset).toISOString().slice(0, 16);
}

function normalizeWorkflowFormDateTimeOutput(value) {
    const normalized = String(value || '').trim();
    if (!normalized) {
        return '';
    }
    const parsed = new Date(normalized);
    if (Number.isNaN(parsed.getTime())) {
        return '';
    }
    return parsed.toISOString();
}

function getWorkflowFormFields(formItem) {
    return extractWorkflowFormFields(formItem?.schema, formItem?.ui_schema)
        .filter((field) => field.key)
        .sort((left, right) => left.order - right.order);
}

function getWorkflowFormByKey(formKey) {
    const normalized = String(formKey || '').trim();
    if (!normalized) {
        return null;
    }
    return getWorkflowFormsState().forms.find((item) => getWorkflowFormDraftKey(item) === normalized) || null;
}

function resetBusinessCaseWorkflowFormsState(options = {}) {
    const preserveDrafts = options.preserveDrafts === true;
    businessCaseModalState.workflowFormsLoading = false;
    businessCaseModalState.workflowFormsError = '';
    businessCaseModalState.workflowFormsSubmittingKey = '';
    businessCaseModalState.workflowFormsPayload = null;
    if (!preserveDrafts) {
        businessCaseModalState.workflowFormDrafts = {};
    }
}

function clearBusinessCaseWorkflowFormDraft(formKey) {
    const normalized = String(formKey || '').trim();
    if (!normalized || !businessCaseModalState.workflowFormDrafts[normalized]) {
        return;
    }
    delete businessCaseModalState.workflowFormDrafts[normalized];
}

function updateBusinessCaseWorkflowFormDraft(formKey, fieldKey, rawValue, fieldType) {
    const normalizedFormKey = String(formKey || '').trim();
    const normalizedFieldKey = String(fieldKey || '').trim();
    if (!normalizedFormKey || !normalizedFieldKey) {
        return;
    }
    if (!businessCaseModalState.workflowFormDrafts[normalizedFormKey]) {
        businessCaseModalState.workflowFormDrafts[normalizedFormKey] = {};
    }
    let nextValue = rawValue;
    if (fieldType === 'boolean') {
        nextValue = rawValue === true || rawValue === 'true' || rawValue === '1' || rawValue === 1;
    } else if (nextValue === null || nextValue === undefined) {
        nextValue = '';
    }
    businessCaseModalState.workflowFormDrafts[normalizedFormKey][normalizedFieldKey] = nextValue;
}

function toggleBusinessCaseWorkflowFormMultiSelectOption(formKey, fieldKey, optionValue, checked) {
    const normalizedFormKey = String(formKey || '').trim();
    const normalizedFieldKey = String(fieldKey || '').trim();
    const normalizedOptionValue = String(optionValue || '').trim();
    if (!normalizedFormKey || !normalizedFieldKey || !normalizedOptionValue) {
        return;
    }
    const formItem = getWorkflowFormByKey(normalizedFormKey);
    const field = formItem ? getWorkflowFormFields(formItem).find((item) => item.key === normalizedFieldKey) : null;
    const formDraft = businessCaseModalState.workflowFormDrafts[normalizedFormKey]
        && typeof businessCaseModalState.workflowFormDrafts[normalizedFormKey] === 'object'
        ? businessCaseModalState.workflowFormDrafts[normalizedFormKey]
        : {};
    const currentValue = Array.isArray(formDraft[normalizedFieldKey])
        ? formDraft[normalizedFieldKey].map((item) => String(item))
        : (Array.isArray(getWorkflowFormDraftValue(formItem, field || { key: normalizedFieldKey, type: 'multi-select' }))
            ? getWorkflowFormDraftValue(formItem, field || { key: normalizedFieldKey, type: 'multi-select' }).map((item) => String(item))
            : []);
    const nextValues = checked
        ? Array.from(new Set([...currentValue, normalizedOptionValue]))
        : currentValue.filter((item) => item !== normalizedOptionValue);
    if (!businessCaseModalState.workflowFormDrafts[normalizedFormKey]) {
        businessCaseModalState.workflowFormDrafts[normalizedFormKey] = {};
    }
    businessCaseModalState.workflowFormDrafts[normalizedFormKey][normalizedFieldKey] = nextValues;
}

async function loadBusinessCaseWorkflowForms(caseId, options = {}) {
    const normalizedCaseId = String(caseId || '').trim();
    if (!normalizedCaseId) {
        resetBusinessCaseWorkflowFormsState({ preserveDrafts: options.preserveDrafts });
        renderBusinessCaseDetailModal();
        return;
    }

    businessCaseModalState.workflowFormsLoading = true;
    businessCaseModalState.workflowFormsError = '';
    businessCaseModalState.workflowFormsSubmittingKey = '';
    if (!options.preservePayload) {
        businessCaseModalState.workflowFormsPayload = null;
    }

    if (typeof Auth === 'undefined' || typeof Auth.fetch !== 'function') {
        if (String(businessCaseModalState.caseId || '').trim() === normalizedCaseId) {
            businessCaseModalState.workflowFormsLoading = false;
            businessCaseModalState.workflowFormsError = '认证上下文不可用';
            renderBusinessCaseDetailModal();
        }
        return;
    }

    try {
        const response = await Auth.fetch(
            `/api/v2/business_cases/${encodeURIComponent(normalizedCaseId)}/workflow/forms`,
            fetchBusinessCaseWorkflowFormsOptions
        );
        const payload = await response.json().catch(() => ({}));
        if (String(businessCaseModalState.caseId || '').trim() !== normalizedCaseId) {
            return;
        }
        if (!response.ok || payload?.success === false) {
            if (response.status === 404) {
                businessCaseModalState.workflowFormsPayload = {
                    case_id: normalizedCaseId,
                    run_id: '',
                    process_instance_id: '',
                    forms: [],
                };
            } else {
                throw new Error(payload?.detail || payload?.message || '加载流程表单失败');
            }
        } else {
            businessCaseModalState.workflowFormsPayload = payload?.data || payload || {
                case_id: normalizedCaseId,
                run_id: '',
                process_instance_id: '',
                forms: [],
            };
        }
    } catch (error) {
        if (String(businessCaseModalState.caseId || '').trim() !== normalizedCaseId) {
            return;
        }
        businessCaseModalState.workflowFormsError = error?.message || '加载流程表单失败';
    } finally {
        if (String(businessCaseModalState.caseId || '').trim() === normalizedCaseId) {
            businessCaseModalState.workflowFormsLoading = false;
            renderBusinessCaseDetailModal();
        }
    }
}

function getWorkflowReceiptAccountName(item) {
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

function buildWorkflowReceiptFallbackHtml(receipt) {
    const payload = receipt && typeof receipt === 'object' ? receipt : {};
    const summary = payload.summary && typeof payload.summary === 'object' ? payload.summary : {};
    const items = Array.isArray(payload.items) ? payload.items : [];
    const overallStatus = String(summary.overall_status || '').trim();
    const statusText = getBusinessCaseReceiptStatusLabel({ context: { workflow_receipt: payload } });
    const summaryText = getBusinessCaseReceiptSummaryText({ context: { workflow_receipt: payload } });
    const statusChip = overallStatus
        ? `<span style="display:inline-flex;align-items:center;padding:2px 8px;border-radius:999px;font-size:11px;font-weight:600;background:rgba(11,119,227,0.1);color:#0b77e3;">${escapeHtml(statusText || overallStatus)}</span>`
        : '';
    const metaChips = `
        ${payload.receipt_group_id ? `<span style="display:inline-flex;align-items:center;padding:2px 8px;border-radius:999px;font-size:11px;background:rgba(15,23,42,0.06);color:#475569;">批次 ${escapeHtml(String(payload.receipt_group_id))}</span>` : ''}
        ${payload.created_at ? `<span style="display:inline-flex;align-items:center;padding:2px 8px;border-radius:999px;font-size:11px;background:rgba(15,23,42,0.06);color:#475569;">发送 ${escapeHtml(formatDateTime(payload.created_at))}</span>` : ''}
        ${payload.severity ? `<span style="display:inline-flex;align-items:center;padding:2px 8px;border-radius:999px;font-size:11px;background:rgba(15,23,42,0.06);color:#475569;">级别 ${escapeHtml(String(payload.severity).toUpperCase())}</span>` : ''}
    `;
    const rowsHtml = items.map((item) => {
        const ackStatus = String(item?.ack_status || 'pending').trim().toLowerCase();
        const ackText = ackStatus === 'acknowledged'
            ? '已确认'
            : (ackStatus === 'rejected'
                ? `已拒绝${item?.ack_note ? `：${escapeHtml(String(item.ack_note))}` : ''}`
                : '待确认');
        return `
            <div style="display:flex;justify-content:space-between;gap:12px;padding:10px 0;border-bottom:1px solid rgba(15,23,42,0.06);">
                <div>
                    <div style="font-size:13px;font-weight:600;color:#102132;">${escapeHtml(getWorkflowReceiptAccountName(item))}</div>
                    <div style="font-size:12px;color:#64748b;margin-top:4px;">${escapeHtml(String(item?.title || payload.title || '通知回执'))}</div>
                    ${item?.ack_at ? `<div style="font-size:11px;color:#94a3b8;margin-top:4px;">${escapeHtml(formatDateTime(item.ack_at))}</div>` : ''}
                </div>
                <div style="font-size:12px;color:#334155;text-align:right;white-space:nowrap;">${ackText}</div>
            </div>
        `;
    }).join('');

    return `
        <div style="display:grid;gap:12px;">
            <div style="display:flex;align-items:center;gap:8px;flex-wrap:wrap;">
                ${statusChip}
                ${metaChips}
            </div>
            ${summaryText ? `<div style="font-size:12px;color:#526477;">${escapeHtml(summaryText)}</div>` : ''}
            <div style="display:flex;gap:8px;flex-wrap:wrap;">
                <span style="display:inline-flex;align-items:center;padding:4px 10px;border-radius:999px;font-size:12px;background:rgba(15,23,42,0.05);color:#334155;">总数 ${Number(summary.total_count || 0)}</span>
                <span style="display:inline-flex;align-items:center;padding:4px 10px;border-radius:999px;font-size:12px;background:rgba(255,149,0,0.12);color:#b26a00;">待确认 ${Number(summary.pending_count || 0)}</span>
                <span style="display:inline-flex;align-items:center;padding:4px 10px;border-radius:999px;font-size:12px;background:rgba(52,199,89,0.12);color:#1f8f49;">已确认 ${Number(summary.acknowledged_count || 0)}</span>
                <span style="display:inline-flex;align-items:center;padding:4px 10px;border-radius:999px;font-size:12px;background:rgba(255,59,48,0.12);color:#b42318;">已拒绝 ${Number(summary.rejected_count || 0)}</span>
            </div>
            <div style="border:1px solid #e6edf4;border-radius:12px;padding:0 14px;background:#fff;">
                ${rowsHtml || '<div style="padding:14px 0;font-size:12px;color:#6f8093;">暂无回执明细</div>'}
            </div>
        </div>
    `;
}

function renderBusinessCaseWorkflowReceiptSection(caseData) {
    const receipt = getBusinessCaseWorkflowReceipt(caseData);
    const openNotifyButton = typeof openDispatchNotifyModal === 'function'
        ? `
            <button
                type="button"
                onclick="openDispatchNotifyModal()"
                style="border:1px solid #d7e0e8;background:#fff;color:#33485f;padding:6px 12px;border-radius:999px;font-size:12px;cursor:pointer;"
            >打开调度通知中心</button>
        `
        : '';
    const contentHtml = receipt
        ? (
            typeof buildDispatchNotifyReceiptGroupHtml === 'function' && typeof renderOriginBadge === 'function'
                ? buildDispatchNotifyReceiptGroupHtml(receipt, {
                    emptyMessage: '暂无回执明细。',
                    heading: '回执明细',
                })
                : buildWorkflowReceiptFallbackHtml(receipt)
        )
        : '<div style="padding:12px 14px;border:1px dashed #d8e0e8;border-radius:12px;color:#6f8093;background:#f8fafc;">当前事项暂无通知回执数据</div>';

    return `
        <section style="padding:16px 18px;background:#fff;border:1px solid #e6edf4;border-radius:16px;">
            <div style="display:flex;justify-content:space-between;align-items:center;gap:12px;margin-bottom:12px;">
                <div style="font-size:13px;font-weight:700;color:#102132;">通知回执</div>
                ${openNotifyButton}
            </div>
            ${contentHtml}
        </section>
    `;
}

function renderWorkflowFormProjectionSummary(entry) {
    const title = String(entry?.name || entry?.form_name || entry?.form_code || '流程表单').trim();
    const submittedBy = String(entry?.submitted_operator_name || entry?.submitted_by || '-').trim() || '-';
    const submittedDepartment = String(entry?.submitted_department_name || '').trim();
    const summaryText = getWorkflowFormSubmissionSummary(entry);
    const detailPairs = Object.entries(entry?.data && typeof entry.data === 'object' ? entry.data : {})
        .filter(([, value]) => value !== null && value !== undefined && String(value).trim() !== '')
        .slice(0, 4)
        .map(([key, value]) => `
            <div style="display:flex;gap:8px;align-items:flex-start;">
                <span style="min-width:96px;color:#526477;">${escapeHtml(key)}</span>
                <span style="color:#102132;flex:1;">${escapeHtml(Array.isArray(value) ? value.join('、') : String(value))}</span>
            </div>
        `)
        .join('');

    return `
        <div style="padding:14px;border:1px solid #e6edf4;border-radius:14px;background:#fff;">
            <div style="display:flex;justify-content:space-between;gap:12px;align-items:flex-start;">
                <div>
                    <div style="font-size:13px;font-weight:700;color:#102132;">${escapeHtml(title)}</div>
                    <div style="margin-top:4px;font-size:12px;color:#64748b;">${escapeHtml(formatDateTime(entry?.submitted_at))}</div>
                </div>
                <div style="font-size:12px;color:#526477;text-align:right;">
                    <div>${escapeHtml(submittedBy)}</div>
                    ${submittedDepartment ? `<div style="margin-top:4px;">${escapeHtml(submittedDepartment)}</div>` : ''}
                </div>
            </div>
            ${summaryText ? `<div style="margin-top:10px;font-size:12px;color:#334155;line-height:1.6;">${escapeHtml(summaryText)}</div>` : ''}
            ${detailPairs ? `<div style="margin-top:10px;display:grid;gap:8px;font-size:12px;">${detailPairs}</div>` : ''}
        </div>
    `;
}

function renderWorkflowFormFieldReadOnlyRows(fields, data) {
    const payload = data && typeof data === 'object' ? data : {};
    const normalizedFields = Array.isArray(fields) && fields.length > 0
        ? fields
        : Object.keys(payload).map((key, index) => ({
            key,
            label: key,
            type: Array.isArray(payload[key]) ? 'multi-select' : typeof payload[key],
            order: index,
        }));
    const rowsHtml = normalizedFields
        .filter((field) => Object.prototype.hasOwnProperty.call(payload, field.key))
        .map((field) => `
            <div style="display:flex;gap:10px;align-items:flex-start;">
                <span style="min-width:110px;font-size:12px;color:#526477;">${escapeHtml(field.label || field.key)}</span>
                <span style="font-size:12px;color:#102132;line-height:1.6;">${escapeHtml(formatWorkflowFormValue(payload[field.key], field))}</span>
            </div>
        `)
        .join('');
    if (!rowsHtml) {
        return '<div style="font-size:12px;color:#6f8093;">暂无已提交字段</div>';
    }
    return `<div style="display:grid;gap:8px;">${rowsHtml}</div>`;
}

function renderWorkflowFormEditorField(formItem, field) {
    const formKey = escapeJsString(getWorkflowFormDraftKey(formItem));
    const fieldKey = escapeJsString(field.key);
    const value = getWorkflowFormDraftValue(formItem, field);
    const labelHtml = `
        <label style="display:block;margin-bottom:6px;font-size:12px;font-weight:600;color:#334155;">
            ${escapeHtml(field.label)}
            ${field.required ? '<span style="color:#ef4444;"> *</span>' : ''}
        </label>
    `;
    const helpTextHtml = field.placeholder
        ? `<div style="margin-top:6px;font-size:11px;color:#8a97a8;">${escapeHtml(field.placeholder)}</div>`
        : '';
    const inputStyle = 'width:100%;border:1px solid #d7e0e8;border-radius:10px;padding:10px 12px;font-size:13px;color:#102132;background:#fff;box-sizing:border-box;';

    if (field.type === 'textarea') {
        return `
            <div style="display:flex;flex-direction:column;">
                ${labelHtml}
                <textarea
                    rows="4"
                    placeholder="${escapeHtml(field.placeholder || '')}"
                    oninput="updateBusinessCaseWorkflowFormDraft('${formKey}', '${fieldKey}', this.value, 'textarea')"
                    style="${inputStyle} resize:vertical;line-height:1.6;"
                >${escapeHtml(String(value ?? ''))}</textarea>
                ${helpTextHtml}
            </div>
        `;
    }

    if (field.type === 'boolean') {
        return `
            <div style="display:flex;flex-direction:column;">
                ${labelHtml}
                <label style="display:inline-flex;align-items:center;gap:8px;font-size:13px;color:#102132;cursor:pointer;">
                    <input
                        type="checkbox"
                        ${value === true ? 'checked' : ''}
                        onchange="updateBusinessCaseWorkflowFormDraft('${formKey}', '${fieldKey}', this.checked ? 'true' : 'false', 'boolean')"
                        style="width:16px;height:16px;cursor:pointer;"
                    >
                    <span>勾选表示“是”</span>
                </label>
                ${helpTextHtml}
            </div>
        `;
    }

    if (field.type === 'single-select') {
        const optionsHtml = field.options.map((option) => `
            <option value="${escapeHtml(option.value)}" ${String(value ?? '') === option.value ? 'selected' : ''}>${escapeHtml(option.label || option.value)}</option>
        `).join('');
        return `
            <div style="display:flex;flex-direction:column;">
                ${labelHtml}
                <select
                    onchange="updateBusinessCaseWorkflowFormDraft('${formKey}', '${fieldKey}', this.value, 'single-select')"
                    style="${inputStyle}"
                >
                    <option value="">请选择</option>
                    ${optionsHtml}
                </select>
                ${helpTextHtml}
            </div>
        `;
    }

    if (field.type === 'multi-select') {
        const selectedValues = Array.isArray(value) ? value.map((item) => String(item)) : [];
        const optionsHtml = field.options.map((option) => `
            <label style="display:flex;align-items:flex-start;gap:8px;padding:8px 10px;border:1px solid #e6edf4;border-radius:10px;background:#fff;cursor:pointer;">
                <input
                    type="checkbox"
                    ${selectedValues.includes(option.value) ? 'checked' : ''}
                    onchange="toggleBusinessCaseWorkflowFormMultiSelectOption('${formKey}', '${fieldKey}', '${escapeJsString(option.value)}', this.checked)"
                    style="margin-top:2px;width:16px;height:16px;cursor:pointer;"
                >
                <span style="font-size:13px;color:#102132;">${escapeHtml(option.label || option.value)}</span>
            </label>
        `).join('');
        return `
            <div style="display:flex;flex-direction:column;">
                ${labelHtml}
                <div style="display:grid;gap:8px;">
                    ${optionsHtml || '<div style="font-size:12px;color:#6f8093;">当前表单未配置可选项</div>'}
                </div>
                ${helpTextHtml}
            </div>
        `;
    }

    const inputType = field.type === 'integer' || field.type === 'number'
        ? 'number'
        : (field.type === 'date-time' ? 'datetime-local' : 'text');
    const inputValue = field.type === 'date-time'
        ? formatWorkflowFormDateTimeInput(value)
        : (value === null || value === undefined ? '' : String(value));
    const stepValue = field.type === 'integer' ? '1' : (field.type === 'number' ? 'any' : '');

    return `
        <div style="display:flex;flex-direction:column;">
            ${labelHtml}
            <input
                type="${inputType}"
                value="${escapeHtml(inputValue)}"
                ${stepValue ? `step="${stepValue}"` : ''}
                placeholder="${escapeHtml(field.placeholder || '')}"
                oninput="updateBusinessCaseWorkflowFormDraft('${formKey}', '${fieldKey}', this.value, '${escapeJsString(field.type)}')"
                style="${inputStyle}"
            >
            ${helpTextHtml}
        </div>
    `;
}

function normalizeWorkflowFormSubmissionFieldValue(field, rawValue) {
    if (field.type === 'boolean') {
        return { include: true, value: rawValue === true || rawValue === 'true' || rawValue === '1' || rawValue === 1 };
    }

    if (field.type === 'multi-select') {
        const normalizedValues = Array.isArray(rawValue)
            ? rawValue.map((item) => String(item).trim()).filter(Boolean)
            : [];
        if (field.required && normalizedValues.length <= 0) {
            return { error: `请填写“${field.label}”` };
        }
        return normalizedValues.length > 0
            ? { include: true, value: normalizedValues }
            : { include: false };
    }

    const normalizedText = String(rawValue ?? '').trim();
    if (!normalizedText) {
        if (field.required) {
            return { error: `请填写“${field.label}”` };
        }
        return { include: false };
    }

    if (field.type === 'integer') {
        if (!/^-?\d+$/.test(normalizedText)) {
            return { error: `“${field.label}”必须为整数` };
        }
        return { include: true, value: Number.parseInt(normalizedText, 10) };
    }

    if (field.type === 'number') {
        const parsed = Number(normalizedText);
        if (!Number.isFinite(parsed)) {
            return { error: `“${field.label}”必须为数字` };
        }
        return { include: true, value: parsed };
    }

    if (field.type === 'date-time') {
        const isoValue = normalizeWorkflowFormDateTimeOutput(normalizedText);
        if (!isoValue) {
            return { error: `“${field.label}”时间格式不合法` };
        }
        return { include: true, value: isoValue };
    }

    return { include: true, value: normalizedText };
}

function collectWorkflowFormSubmissionPayload(formItem) {
    const fields = getWorkflowFormFields(formItem);
    const payload = {};

    for (const field of fields) {
        const normalized = normalizeWorkflowFormSubmissionFieldValue(field, getWorkflowFormDraftValue(formItem, field));
        if (normalized.error) {
            return { error: normalized.error };
        }
        if (normalized.include) {
            payload[field.key] = normalized.value;
        }
    }

    return { payload };
}

function renderBusinessCaseWorkflowFormsSection(caseData) {
    const projectionEntries = getWorkflowFormProjectionEntries(caseData);
    const workflowFormsState = getWorkflowFormsState();
    const taskForms = Array.isArray(workflowFormsState.forms) ? workflowFormsState.forms : [];
    const shouldShow = projectionEntries.length > 0 || taskForms.length > 0;
    if (!shouldShow) {
        return '';
    }
    const projectionHtml = projectionEntries.length > 0
        ? projectionEntries.map((entry) => renderWorkflowFormProjectionSummary(entry)).join('')
        : '<div style="padding:12px 14px;border:1px dashed #d8e0e8;border-radius:12px;color:#6f8093;background:#f8fafc;">当前事项暂无已回写的流程表单摘要</div>';

    let taskFormsHtml = '';
    if (businessCaseModalState.workflowFormsLoading) {
        taskFormsHtml = '<div style="padding:12px 14px;border:1px dashed #d8e0e8;border-radius:12px;color:#6f8093;background:#f8fafc;">流程表单加载中...</div>';
    } else if (businessCaseModalState.workflowFormsError) {
        taskFormsHtml = `<div style="padding:12px 14px;border:1px solid rgba(239,68,68,0.18);border-radius:12px;color:#b42318;background:rgba(254,242,242,0.92);">${escapeHtml(businessCaseModalState.workflowFormsError)}</div>`;
    } else if (taskForms.length <= 0) {
        taskFormsHtml = '<div style="padding:12px 14px;border:1px dashed #d8e0e8;border-radius:12px;color:#6f8093;background:#f8fafc;">当前流程暂无可查看或可提交的表单任务</div>';
    } else {
        taskFormsHtml = taskForms.map((formItem) => {
            const formKey = getWorkflowFormDraftKey(formItem);
            const fields = getWorkflowFormFields(formItem);
            const latestSubmission = formItem.latest_submission;
            const isSubmitting = businessCaseModalState.workflowFormsSubmittingKey === formKey;
            const submissionSummary = getWorkflowFormSubmissionSummary(latestSubmission);
            const readOnlyMessage = !formItem.can_submit && formItem.readonly_reason
                ? `<div style="margin-top:12px;padding:10px 12px;border-radius:10px;background:rgba(15,23,42,0.04);font-size:12px;color:#526477;">${escapeHtml(formItem.readonly_reason)}</div>`
                : '';
            const editorHtml = formItem.can_submit
                ? `
                    <div style="margin-top:14px;padding-top:14px;border-top:1px solid #edf2f7;">
                        <div style="font-size:12px;font-weight:700;color:#334155;margin-bottom:12px;">填写表单</div>
                        <div style="display:grid;gap:12px;">
                            ${fields.length > 0
                                ? fields.map((field) => renderWorkflowFormEditorField(formItem, field)).join('')
                                : '<div style="font-size:12px;color:#6f8093;">当前表单未配置可渲染字段</div>'}
                        </div>
                        <div style="margin-top:14px;display:flex;justify-content:flex-end;">
                            <button
                                type="button"
                                onclick="submitBusinessCaseWorkflowForm('${escapeJsString(formKey)}')"
                                ${isSubmitting ? 'disabled' : ''}
                                style="border:none;background:#0b77e3;color:#fff;padding:8px 14px;border-radius:999px;cursor:pointer;${isSubmitting ? 'opacity:0.6;' : ''}"
                            >${isSubmitting ? '提交中...' : '提交表单'}</button>
                        </div>
                    </div>
                `
                : '';

            return `
                <div style="padding:14px;border:1px solid #e6edf4;border-radius:14px;background:#fff;">
                    <div style="display:flex;justify-content:space-between;gap:12px;align-items:flex-start;">
                        <div>
                            <div style="font-size:13px;font-weight:700;color:#102132;">${escapeHtml(formItem.name)}</div>
                            <div style="margin-top:4px;font-size:12px;color:#64748b;">任务：${escapeHtml(formItem.task_name || formItem.task_definition_key || '-')}</div>
                            ${formItem.description ? `<div style="margin-top:6px;font-size:12px;color:#526477;line-height:1.6;">${escapeHtml(formItem.description)}</div>` : ''}
                        </div>
                        <div style="display:flex;gap:6px;flex-wrap:wrap;justify-content:flex-end;">
                            ${formItem.form_code ? `<span style="display:inline-flex;align-items:center;padding:2px 8px;border-radius:999px;font-size:11px;background:rgba(15,23,42,0.06);color:#475569;">${escapeHtml(formItem.form_code)}</span>` : ''}
                            <span style="display:inline-flex;align-items:center;padding:2px 8px;border-radius:999px;font-size:11px;${formItem.can_submit ? 'background:rgba(52,199,89,0.12);color:#1f8f49;' : 'background:rgba(15,23,42,0.06);color:#526477;'}">${formItem.can_submit ? '可提交' : '只读'}</span>
                            ${formItem.allow_resubmit ? '<span style="display:inline-flex;align-items:center;padding:2px 8px;border-radius:999px;font-size:11px;background:rgba(255,149,0,0.12);color:#b26a00;">允许重提</span>' : ''}
                        </div>
                    </div>
                    ${latestSubmission ? `
                        <div style="margin-top:14px;padding:12px;border-radius:12px;background:#f8fafc;border:1px solid #edf2f7;">
                            <div style="display:flex;justify-content:space-between;gap:12px;align-items:flex-start;">
                                <div style="font-size:12px;font-weight:700;color:#334155;">最近一次提交</div>
                                <div style="font-size:12px;color:#64748b;text-align:right;">
                                    <div>${escapeHtml(formatDateTime(latestSubmission.submitted_at))}</div>
                                    <div style="margin-top:4px;">${escapeHtml(latestSubmission.submitted_operator_name || latestSubmission.submitted_by || '-')}</div>
                                </div>
                            </div>
                            ${submissionSummary ? `<div style="margin-top:10px;font-size:12px;color:#334155;line-height:1.6;">${escapeHtml(submissionSummary)}</div>` : ''}
                            <div style="margin-top:10px;">${renderWorkflowFormFieldReadOnlyRows(fields, latestSubmission.data)}</div>
                        </div>
                    ` : '<div style="margin-top:12px;font-size:12px;color:#6f8093;">该表单尚未提交</div>'}
                    ${readOnlyMessage}
                    ${editorHtml}
                </div>
            `;
        }).join('');
    }

    return `
        <section style="padding:16px 18px;background:#fff;border:1px solid #e6edf4;border-radius:16px;">
            <div style="font-size:13px;font-weight:700;color:#102132;">流程表单</div>
            <div style="margin-top:12px;display:grid;gap:18px;">
                <div>
                    <div style="display:flex;justify-content:space-between;align-items:center;gap:12px;margin-bottom:10px;">
                        <div style="font-size:12px;font-weight:700;color:#334155;">回写摘要</div>
                        <div style="font-size:12px;color:#6f8093;">已回写 ${projectionEntries.length} 份</div>
                    </div>
                    <div style="display:grid;gap:10px;">${projectionHtml}</div>
                </div>
                <div>
                    <div style="display:flex;justify-content:space-between;align-items:center;gap:12px;margin-bottom:10px;">
                        <div style="font-size:12px;font-weight:700;color:#334155;">当前流程任务</div>
                        <div style="font-size:12px;color:#6f8093;">${taskForms.length} 个任务表单</div>
                    </div>
                    <div style="display:grid;gap:10px;">${taskFormsHtml}</div>
                </div>
            </div>
        </section>
    `;
}

async function submitBusinessCaseWorkflowForm(formKey) {
    const caseId = String(businessCaseModalState.caseId || '').trim();
    const formItem = getWorkflowFormByKey(formKey);
    if (!caseId || !formItem) {
        return;
    }
    if (!formItem.can_submit) {
        if (typeof showToast === 'function') {
            showToast(formItem.readonly_reason || '当前表单不可提交', 'warning');
        }
        return;
    }
    if (typeof Auth === 'undefined' || typeof Auth.fetch !== 'function') {
        if (typeof showToast === 'function') {
            showToast('认证上下文不可用', 'error');
        }
        return;
    }

    const submission = collectWorkflowFormSubmissionPayload(formItem);
    if (submission.error) {
        if (typeof showToast === 'function') {
            showToast(submission.error, 'warning');
        }
        return;
    }

    businessCaseModalState.workflowFormsSubmittingKey = getWorkflowFormDraftKey(formItem);
    renderBusinessCaseDetailModal();

    try {
        const response = await Auth.fetch(
            `/api/v2/business_cases/${encodeURIComponent(caseId)}/workflow/forms/${encodeURIComponent(formItem.form_code)}/submit`,
            {
                ...fetchBusinessCaseModalMutationOptions,
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                },
                body: JSON.stringify({
                    task_id: formItem.task_id,
                    data: submission.payload || {},
                }),
            }
        );
        const payload = await response.json().catch(() => ({}));
        if (!response.ok || payload?.success === false) {
            throw new Error(payload?.detail || payload?.message || '提交流程表单失败');
        }

        const businessCaseData = payload?.data?.business_case || payload?.business_case || null;
        if (businessCaseData) {
            applyBusinessCaseSummaryToCurrentFlight(businessCaseData);
        }
        clearBusinessCaseWorkflowFormDraft(formKey);
        businessCaseModalState.workflowFormsSubmittingKey = '';
        await openBusinessCaseDetail(caseId, {
            preserveWorkflowFormDrafts: true,
        });
        if (typeof renderFlightDetail === 'function') {
            renderFlightDetail();
        }
        if (typeof showToast === 'function') {
            showToast('流程表单提交成功', 'success');
        }
    } catch (error) {
        businessCaseModalState.workflowFormsSubmittingKey = '';
        renderBusinessCaseDetailModal();
        if (typeof showToast === 'function') {
            showToast(error?.message || '提交流程表单失败', 'error');
        }
    }
}

function filterBusinessCases(status) {
    currentCaseFilter = status;
    document.querySelectorAll('.filter-btn').forEach(btn => {
        btn.classList.toggle('active', btn.dataset.filter === status);
    });
    if (currentFlight) {
        renderGanttChart(currentFlight);
    }
}

function getStatusClass(status) {
    if (!status) return 'pending';
    const s = String(status).toUpperCase();
    if (s === 'SUCCESS' || s === 'COMPLETED') return 'success';
    if (s === 'PROCESSING') return 'processing';
    if (s === 'FAILED') return 'failed';
    if (s === 'INITIAL') return 'initial';
    return 'pending';
}

function renderTimelineHTML(cases) {
    let filtered = [...cases].sort((a, b) => new Date(b.created_at) - new Date(a.created_at));
    if (currentCaseFilter !== 'all') {
        filtered = filtered.filter(c => c.status === currentCaseFilter);
    }

    if (filtered.length === 0) {
        return '<div class="gantt-empty">无匹配的业务事项</div>';
    }

    return `<div class="timeline">${filtered.map(c => {
        const statusClass = getStatusClass(c.status);
        const createdTime = new Date(c.created_at).toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' });
        const finishedTime = c.finished_at ? new Date(c.finished_at).toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }) : null;
        const appendSummary = getLatestAppendSummary(c);
        const receiptSummary = renderBusinessCaseReceiptSummary(c);
        const formsSummary = renderBusinessCaseFormsSummary(c);
        const bindingLabel = getBusinessCaseBinding(c);
        const visibilityInfo = getBusinessCaseVisibilityInfo(c);
        const visibilityStyle = visibilityInfo.isCommon
            ? 'background:rgba(14, 165, 233, 0.14); color:#0369a1;'
            : 'background:rgba(15, 118, 110, 0.14); color:#0f766e;';
        return `
            <div class="timeline-item status-${statusClass}" data-case-id="${c.case_id}" onclick="toggleTimelineDetail(this); openBusinessCaseDetail('${c.case_id}');">
                <div class="timeline-header">
                    <span class="timeline-type">${escapeHtml(resolveCaseTypeName(c))}</span>
                    <div style="display:flex; align-items:center; gap:6px; flex-wrap:wrap; justify-content:flex-end;">
                        <span style="display:inline-flex; align-items:center; padding:2px 8px; border-radius:999px; font-size:11px; font-weight:600; ${visibilityStyle}">${escapeHtml(visibilityInfo.scopeLabel)}</span>
                        <span class="timeline-status ${statusClass}">${escapeHtml(getBusinessCaseStatusText(c.status))}</span>
                    </div>
                </div>
                <div class="timeline-time">
                    <span><img src="/frontend/icons/clock.svg" class="svg-icon-sm" style="vertical-align:-2px;"> ${escapeHtml(createdTime)}</span>
                    ${finishedTime ? `<span><img src="/frontend/icons/ok.svg" class="svg-icon-sm" style="vertical-align:-2px;"> ${finishedTime}</span>` : ''}
                </div>
                <div class="timeline-details">
                    <div><strong>描述:</strong> ${escapeHtml(c.description || '-')}</div>
                    <div><strong>创建者:</strong> ${escapeHtml(c.created_by || '-')}</div>
                    <div><strong>创建来源:</strong> ${escapeHtml(visibilityInfo.scopeLabel)}</div>
                    ${visibilityInfo.departmentName ? `<div><strong>归属部门:</strong> ${escapeHtml(visibilityInfo.departmentName)}</div>` : ''}
                    ${bindingLabel ? `<div><strong>绑定航班:</strong> ${escapeHtml(bindingLabel)}</div>` : ''}
                    ${c.stand ? `<div><strong>机位:</strong> ${escapeHtml(c.stand)}</div>` : ''}
                    ${c.gate ? `<div><strong>登机口:</strong> ${escapeHtml(c.gate)}</div>` : ''}
                    ${appendSummary}
                    ${receiptSummary}
                    ${formsSummary}
                </div>
            </div>`;
    }).join('')}</div>`;
}

function toggleTimelineDetail(el) {
    el.classList.toggle('expanded');
}

function normalizeBusinessTimelineFlightId(value) {
    if (typeof normalizeFlightId === 'function') {
        return normalizeFlightId(value);
    }
    return String(value ?? '').trim();
}

function getCachedDispatchOrdersForFlight(flightId) {
    return dispatchOrderCache.get(normalizeBusinessTimelineFlightId(flightId)) || [];
}

function syncDispatchOrdersToKnownFlights(flightId, orders, fallbackFlight) {
    const normalizedFlightId = normalizeBusinessTimelineFlightId(flightId);
    const targets = [];

    if (fallbackFlight) {
        targets.push(fallbackFlight);
    }
    if (currentFlight && normalizeBusinessTimelineFlightId(currentFlight.flight_id) === normalizedFlightId) {
        targets.push(currentFlight);
    }
    if (typeof findFlightById === 'function') {
        const liveFlight = findFlightById(normalizedFlightId);
        if (liveFlight) {
            targets.push(liveFlight);
        }
    }
    if (typeof originalFlights !== 'undefined' && Array.isArray(originalFlights)) {
        const originalFlight = originalFlights.find((item) => normalizeBusinessTimelineFlightId(item?.flight_id) === normalizedFlightId);
        if (originalFlight) {
            targets.push(originalFlight);
        }
    }

    const seen = new Set();
    targets.forEach((target) => {
        if (!target || seen.has(target)) {
            return;
        }
        seen.add(target);
        target.dispatch_orders = Array.isArray(orders) ? orders : [];
    });
}

function invalidateDispatchOrdersForFlight(flightId) {
    const normalizedFlightId = normalizeBusinessTimelineFlightId(flightId);
    if (!normalizedFlightId) {
        return;
    }

    dispatchOrderCache.delete(normalizedFlightId);
    dispatchOrderPromiseCache.delete(normalizedFlightId);
    syncDispatchOrdersToKnownFlights(normalizedFlightId, [], null);
}

function invalidateAllDispatchOrderCaches() {
    dispatchOrderCache.clear();
    dispatchOrderPromiseCache.clear();

    const clearTargets = [];
    if (currentFlight) {
        clearTargets.push(currentFlight);
    }
    if (typeof flights !== 'undefined' && Array.isArray(flights)) {
        clearTargets.push(...flights);
    }
    if (typeof originalFlights !== 'undefined' && Array.isArray(originalFlights)) {
        clearTargets.push(...originalFlights);
    }

    const seen = new Set();
    clearTargets.forEach((target) => {
        if (!target || seen.has(target)) {
            return;
        }
        seen.add(target);
        target.dispatch_orders = [];
    });
}

async function loadDispatchOrdersForFlight(flightId, options = {}) {
    const normalizedFlightId = normalizeBusinessTimelineFlightId(flightId);
    if (!normalizedFlightId) {
        return [];
    }

    const force = options.force === true;
    const fallbackFlight = options.fallbackFlight || null;

    if (!force && dispatchOrderCache.has(normalizedFlightId)) {
        return dispatchOrderCache.get(normalizedFlightId) || [];
    }
    if (!force && dispatchOrderPromiseCache.has(normalizedFlightId)) {
        return dispatchOrderPromiseCache.get(normalizedFlightId);
    }
    if (typeof Auth === 'undefined' || typeof Auth.fetch !== 'function') {
        return [];
    }

    const promise = (async () => {
        const response = await Auth.fetch(
            `/api/v2/dispatch-orders?flight_id=${encodeURIComponent(normalizedFlightId)}&page=1&page_size=${DISPATCH_ORDER_PAGE_SIZE}`
        );
        const payload = await response.json().catch(() => ({}));
        if (!response.ok || payload?.success === false) {
            throw new Error(payload?.detail || payload?.message || `加载航班派工单失败 (${response.status})`);
        }

        const orders = Array.isArray(payload?.data) ? payload.data : (Array.isArray(payload) ? payload : []);
        dispatchOrderCache.set(normalizedFlightId, orders);
        syncDispatchOrdersToKnownFlights(normalizedFlightId, orders, fallbackFlight);
        return orders;
    })();

    dispatchOrderPromiseCache.set(normalizedFlightId, promise);
    try {
        return await promise;
    } finally {
        dispatchOrderPromiseCache.delete(normalizedFlightId);
    }
}

async function refreshDispatchOrdersForFlight(flightId, options = {}) {
    const normalizedFlightId = normalizeBusinessTimelineFlightId(flightId);
    if (!normalizedFlightId) {
        return [];
    }

    invalidateDispatchOrdersForFlight(normalizedFlightId);
    return loadDispatchOrdersForFlight(normalizedFlightId, {
        ...options,
        force: true,
    });
}

function getOrderEffectiveStartTime(order) {
    return order?.effective_start_time
        || order?.actual_start_time
        || order?.planned_start_time
        || order?.assignment_deadline
        || order?.created_at
        || null;
}

function getDepartmentPendingMarkerStartTime(order) {
    return order?.effective_start_time
        || order?.planned_start_time
        || null;
}

function getOrderEffectiveEndTime(order) {
    return order?.effective_end_time
        || order?.actual_end_time
        || order?.estimated_completion_time
        || order?.planned_end_time
        || getOrderEffectiveStartTime(order)
        || null;
}

function getDepartmentPresenceSummaries(dispatchOrders) {
    const orders = Array.isArray(dispatchOrders) ? dispatchOrders : [];
    const groups = new Map();

    orders.forEach((order) => {
        const departmentLabel = String(order?.department || '').trim() || DEPARTMENT_DEFAULT_LABEL;
        if (!groups.has(departmentLabel)) {
            groups.set(departmentLabel, {
                department: departmentLabel,
                orders: [],
                stepNames: new Set(),
                teamNames: new Set(),
                memberStates: new Map(),
                earliestCheckIn: null,
                latestCheckOut: null,
                fallbackStart: null,
            });
        }

        const group = groups.get(departmentLabel);
        group.orders.push(order);

        const stepName = String(order?.task_type_name || order?.task_type || '').trim();
        if (stepName) {
            group.stepNames.add(stepName);
        }

        const teamName = String(order?.team_name || '').trim();
        if (teamName) {
            group.teamNames.add(teamName);
        }

        const fallbackStart = getDepartmentPendingMarkerStartTime(order);
        if (fallbackStart) {
            const fallbackTime = new Date(fallbackStart).getTime();
            if (!Number.isNaN(fallbackTime) && (group.fallbackStart === null || fallbackTime < group.fallbackStart)) {
                group.fallbackStart = fallbackTime;
            }
        }

        const members = Array.isArray(order?.members) ? order.members : [];
        members.forEach((member, index) => {
            if (!member || member.is_active === false) {
                return;
            }

            const memberKey = String(member.user_id || `${order.id || 'order'}:${member.id || index}`);
            if (!group.memberStates.has(memberKey)) {
                group.memberStates.set(memberKey, {
                    username: String(member.username || member.user_id || '').trim(),
                    hasCheckedIn: false,
                    hasOpenAssignment: false,
                    latestCheckOut: null,
                });
            }

            const memberState = group.memberStates.get(memberKey);
            const checkInValue = member.check_in_time ? new Date(member.check_in_time).getTime() : NaN;
            const checkOutValue = member.check_out_time ? new Date(member.check_out_time).getTime() : NaN;

            if (!Number.isNaN(checkInValue)) {
                memberState.hasCheckedIn = true;
                if (group.earliestCheckIn === null || checkInValue < group.earliestCheckIn) {
                    group.earliestCheckIn = checkInValue;
                }
            }

            if (!Number.isNaN(checkOutValue)) {
                if (memberState.latestCheckOut === null || checkOutValue > memberState.latestCheckOut) {
                    memberState.latestCheckOut = checkOutValue;
                }
                if (group.latestCheckOut === null || checkOutValue > group.latestCheckOut) {
                    group.latestCheckOut = checkOutValue;
                }
            }

            if (!Number.isNaN(checkInValue) && Number.isNaN(checkOutValue)) {
                memberState.hasOpenAssignment = true;
            }
        });
    });

    return Array.from(groups.values())
        .map((group) => {
            const members = Array.from(group.memberStates.values());
            const checkedInCount = members.filter((item) => item.hasCheckedIn).length;
            const inProgressCount = members.filter((item) => item.hasOpenAssignment).length;
            const checkedOutCount = members.filter((item) => item.hasCheckedIn && !item.hasOpenAssignment && item.latestCheckOut !== null).length;
            const status = group.earliestCheckIn === null
                ? 'pending'
                : (inProgressCount > 0 ? 'in_progress' : 'completed');
            const startTime = group.earliestCheckIn ?? group.fallbackStart ?? Date.now();
            const endTime = status === 'pending'
                ? startTime + DEPARTMENT_PENDING_MARKER_MS
                : (status === 'in_progress' ? Date.now() : (group.latestCheckOut ?? startTime));

            return {
                department: group.department,
                orders: group.orders,
                orderCount: group.orders.length,
                memberCount: members.length,
                checkedInCount,
                checkedOutCount,
                inProgressCount,
                earliestCheckIn: group.earliestCheckIn,
                latestCheckOut: group.latestCheckOut,
                fallbackStart: group.fallbackStart,
                startTime,
                endTime: Math.max(startTime + 60 * 1000, endTime),
                status,
                stepNames: Array.from(group.stepNames),
                teamNames: Array.from(group.teamNames),
                memberNames: members.map((item) => item.username).filter(Boolean),
            };
        })
        .sort((left, right) => {
            if (left.startTime !== right.startTime) {
                return left.startTime - right.startTime;
            }
            return left.department.localeCompare(right.department, 'zh-CN');
        });
}

function getFlightTimeRange(flight) {
    const times = [];

    TIME_NODE_CONFIG.forEach(node => {
        if (flight[node.field]) {
            times.push(new Date(flight[node.field]).getTime());
        }
    });

    if (flight.business_cases) {
        flight.business_cases.forEach(c => {
            times.push(new Date(c.created_at).getTime());
            if (c.finished_at) {
                times.push(new Date(c.finished_at).getTime());
            }
        });
    }

    const departmentPresence = getDepartmentPresenceSummaries(
        flight?.dispatch_orders || getCachedDispatchOrdersForFlight(flight?.flight_id)
    );
    departmentPresence.forEach((item) => {
        times.push(item.startTime);
        times.push(item.endTime);
    });

    if (times.length === 0) {
        const now = Date.now();
        return { min: now - 3600000 * 4, max: now + 3600000 * 4 };
    }

    const min = Math.min(...times);
    const max = Math.max(...times);
    const padding = (max - min) * 0.1 || 3600000;

    return { min: min - padding, max: max + padding };
}

function initGanttChart() {
    const chartDom = document.getElementById('ganttChart');
    if (!chartDom || typeof echarts === 'undefined') return null;

    let shouldRecreate = false;
    if (ganttChart) {
        try {
            const disposed = typeof ganttChart.isDisposed === 'function' ? ganttChart.isDisposed() : false;
            const currentDom = typeof ganttChart.getDom === 'function' ? ganttChart.getDom() : null;
            shouldRecreate = disposed || currentDom !== chartDom;
        } catch (_error) {
            shouldRecreate = true;
        }
    }

    if (shouldRecreate && ganttChart) {
        try {
            ganttChart.dispose();
        } catch (_error) {
            // ignore dispose error
        }
        ganttChart = null;
    }

    if (!ganttChart) {
        const existing = echarts.getInstanceByDom(chartDom);
        ganttChart = existing || echarts.init(chartDom, null, { renderer: 'canvas' });
    }

    ganttHostElement = chartDom;

    ganttChart.off('click');
    ganttChart.on('click', function (params) {
        var raw = params.data && params.data.raw;
        if (!raw && params.seriesIndex != null && params.dataIndex != null) {
            try {
                var opt = ganttChart.getOption();
                var sData = opt && opt.series && opt.series[params.seriesIndex] && opt.series[params.seriesIndex].data;
                if (sData && sData[params.dataIndex]) {
                    raw = sData[params.dataIndex].raw;
                }
            } catch (_e) { /* ignore */ }
        }
        if (raw) {
            showGanttItemDetail(raw);
        }
    });

    if (!ganttResizeHandlerBound) {
        window.addEventListener('resize', function () {
            if (!ganttChart) {
                return;
            }
            try {
                ganttChart.resize();
            } catch (_error) {
                // ignore resize error
            }
        });
        ganttResizeHandlerBound = true;
    }

    return ganttChart;
}

function renderGanttChart(flight) {
    currentFlight = flight;

    ganttChart = initGanttChart();

    if (!ganttChart) return;

    const normalizedFlightId = normalizeBusinessTimelineFlightId(flight?.flight_id);
    const cachedDispatchOrders = normalizedFlightId
        ? (getCachedDispatchOrdersForFlight(normalizedFlightId).length
            ? getCachedDispatchOrdersForFlight(normalizedFlightId)
            : (Array.isArray(flight?.dispatch_orders) ? flight.dispatch_orders : []))
        : [];
    if (normalizedFlightId && !dispatchOrderCache.has(normalizedFlightId) && !dispatchOrderPromiseCache.has(normalizedFlightId)) {
        loadDispatchOrdersForFlight(normalizedFlightId, { fallbackFlight: flight })
            .then(() => {
                if (normalizeBusinessTimelineFlightId(currentFlight?.flight_id) !== normalizedFlightId) {
                    return;
                }
                const latestFlight = typeof findFlightById === 'function' ? (findFlightById(normalizedFlightId) || flight) : flight;
                renderGanttChart(latestFlight);
            })
            .catch((error) => {
                console.warn('加载航班派工单失败:', error);
            });
    }

    const timeRange = getFlightTimeRange(flight);
    const yAxisData = ['航班状态转换'];
    const seriesData = [];
    const departmentPresence = getDepartmentPresenceSummaries(cachedDispatchOrders);

    const timeNodes = TIME_NODE_CONFIG.filter(node => flight[node.field])
        .map(node => ({
            field: node.field,
            label: node.label,
            time: new Date(flight[node.field]).getTime(),
            color: node.color
        }))
        .sort((a, b) => a.time - b.time);

    for (let i = 0; i < timeNodes.length - 1; i++) {
        seriesData.push({
            name: `${timeNodes[i].label} → ${timeNodes[i + 1].label}`,
            value: [timeNodes[i].time, timeNodes[i + 1].time, 0],
            itemStyle: { color: timeNodes[i].color },
            raw: {
                type: 'flight_status',
                from: timeNodes[i].label,
                to: timeNodes[i + 1].label,
                fromTime: timeNodes[i].time,
                toTime: timeNodes[i + 1].time,
                color: timeNodes[i].color
            }
        });
    }

    timeNodes.forEach(node => {
        seriesData.push({
            name: node.label,
            value: [node.time - 60000, node.time + 60000, 0],
            itemStyle: { color: node.color },
            raw: {
                type: 'time_node',
                label: node.label,
                time: node.time,
                color: node.color,
                field: node.field
            }
        });
    });

    departmentPresence.forEach((item) => {
        const yIndex = yAxisData.length;
        yAxisData.push(`科室:${item.department}`);

        seriesData.push({
            name: item.department,
            value: [item.startTime, item.endTime, yIndex],
            itemStyle: { color: DEPARTMENT_PRESENCE_COLORS[item.status] || DEPARTMENT_PRESENCE_COLORS.pending },
            raw: {
                type: 'department_presence',
                department: item.department,
                status: item.status,
                order_count: item.orderCount,
                member_count: item.memberCount,
                checked_in_count: item.checkedInCount,
                checked_out_count: item.checkedOutCount,
                in_progress_count: item.inProgressCount,
                earliest_check_in: item.earliestCheckIn,
                latest_check_out: item.latestCheckOut,
                fallback_start: item.fallbackStart,
                task_type_names: item.stepNames,
                team_names: item.teamNames,
                member_names: item.memberNames,
            }
        });
    });

    let cases = flight.business_cases || [];
    if (currentCaseFilter !== 'all') {
        cases = cases.filter(c => c.status === currentCaseFilter);
    }

    cases.forEach((c, index) => {
        const yIndex = departmentPresence.length + index + 1;
        yAxisData.push(resolveCaseTypeName(c));

        const startTime = new Date(c.created_at).getTime();
        const endTime = c.finished_at ? new Date(c.finished_at).getTime() : Date.now();

        seriesData.push({
            name: resolveCaseTypeName(c),
            value: [startTime, endTime, yIndex],
            itemStyle: { color: STATUS_COLORS[c.status] || '#8E8E93' },
            raw: {
                type: 'business_case',
                case_id: c.case_id,
                case_type: c.case_type,
                case_type_name: c.case_type_name,
                status: c.status,
                description: c.description,
                created_at: c.created_at,
                finished_at: c.finished_at,
                created_by: c.created_by,
                stand: c.stand,
                gate: c.gate,
                append_count: c.append_count || 0,
                latest_append: c.latest_append || null
            }
        });
    });

    const option = {
        animation: false,
        backgroundColor: 'transparent',
        tooltip: {
            trigger: 'item',
            confine: true,
            borderWidth: 1,
            borderColor: 'rgba(15, 23, 42, 0.1)',
            formatter: function (params) {
                const dataItem = seriesData[params.dataIndex];
                if (!dataItem || !dataItem.raw) return '';
                const raw = dataItem.raw;

                if (raw.type === 'flight_status') {
                    return `
                        <div style="font-weight:700;margin-bottom:4px;">航班状态转换</div>
                        <div>${raw.from} → ${raw.to}</div>
                        <div>时间: ${formatTime(raw.fromTime)} - ${formatTime(raw.toTime)}</div>
                    `;
                } else if (raw.type === 'time_node') {
                    return `
                        <div style="font-weight:700;margin-bottom:4px;">${raw.label}</div>
                        <div>时间: ${formatTime(raw.time)}</div>
                    `;
                } else if (raw.type === 'business_case') {
                    return `
                        <div style="font-weight:700;margin-bottom:4px;">${resolveCaseTypeName(raw)}</div>
                        <div>状态: ${raw.status}</div>
                        <div>创建时间: ${formatTime(new Date(raw.created_at).getTime())}</div>
                        ${raw.finished_at ? `<div>完成时间: ${formatTime(new Date(raw.finished_at).getTime())}</div>` : ''}
                        ${raw.description ? `<div>描述: ${raw.description}</div>` : ''}
                        ${raw.append_count ? `<div>已追加: ${raw.append_count} 次</div>` : ''}
                    `;
                } else if (raw.type === 'department_presence') {
                    const statusText = raw.status === 'pending'
                        ? '未到位'
                        : (raw.status === 'in_progress' ? '进行中' : '已签退');
                    return `
                        <div style="font-weight:700;margin-bottom:4px;">调度口科室到位/签退</div>
                        <div>科室: ${escapeHtml(raw.department || DEPARTMENT_DEFAULT_LABEL)}</div>
                        <div>状态: ${statusText}</div>
                        <div>工单数: ${raw.order_count || 0}</div>
                        <div>成员数: ${raw.member_count || 0}</div>
                        <div>已到位: ${raw.checked_in_count || 0}，已签退: ${raw.checked_out_count || 0}，进行中: ${raw.in_progress_count || 0}</div>
                        <div>最早到位: ${raw.earliest_check_in ? formatTime(raw.earliest_check_in) : '--'}</div>
                        <div>最晚签退: ${raw.latest_check_out ? formatTime(raw.latest_check_out) : '--'}</div>
                        <div>涉及作业类型: ${escapeHtml((raw.task_type_names || []).join('、') || '-')}</div>
                    `;
                } else if (raw.type === 'cascade_shadow') {
                    return `
                        <div style="font-weight:700;margin-bottom:4px;color:#dc2626;">⚠ 级联推演</div>
                        <div>作业类型: ${raw.task_type_name}</div>
                        <div>预计推迟: ${Math.round(raw.shift_minutes)} 分钟</div>
                        <div>预计结束: ${formatTime(new Date(raw.projected_end).getTime())}</div>
                    `;
                }
                return '';
            }
        },
        grid: {
            left: 140,
            right: 20,
            top: 20,
            bottom: 40,
            containLabel: false
        },
        xAxis: {
            type: 'time',
            min: timeRange.min,
            max: timeRange.max,
            axisLine: { lineStyle: { color: '#8a97a8' } },
            axisLabel: {
                color: '#5f7082',
                fontSize: 12,
                formatter: function (value) {
                    return formatTime(value);
                }
            },
            splitLine: {
                lineStyle: {
                    color: 'rgba(15, 23, 42, 0.08)',
                    type: 'dashed'
                }
            }
        },
        yAxis: {
            type: 'category',
            inverse: true,
            data: yAxisData,
            axisTick: { show: false },
            axisLine: { show: false },
            axisLabel: {
                color: '#33485f',
                fontSize: 12,
                width: 120,
                overflow: 'truncate'
            }
        },
        dataZoom: [
            {
                type: 'inside',
                xAxisIndex: 0,
                zoomOnMouseWheel: true,
                moveOnMouseMove: true,
                moveOnMouseWheel: true
            },
            {
                type: 'slider',
                xAxisIndex: 0,
                bottom: 8,
                height: 12,
                borderColor: 'rgba(15, 23, 42, 0.12)',
                backgroundColor: 'rgba(255,255,255,0.74)',
                fillerColor: 'rgba(11,119,227,0.2)',
                showDetail: false
            }
        ],
        series: [{
            type: 'custom',
            animation: false,
            renderItem: function (params, api) {
                const dataItem = seriesData[params.dataIndex];
                if (!dataItem) return null;

                const categoryIndex = api.value(2);
                const startCoord = api.coord([api.value(0), categoryIndex]);
                const endCoord = api.coord([api.value(1), categoryIndex]);
                const laneHeight = Math.min(60, Math.max(20, api.size([0, 1])[1]));

                const x = Math.min(startCoord[0], endCoord[0]);
                const width = Math.max(4, Math.abs(endCoord[0] - startCoord[0]));
                const y = startCoord[1] - laneHeight / 2 + 4;
                const height = laneHeight - 8;

                const clippedRect = echarts.graphic.clipRectByRect({
                    x, y, width, height
                }, {
                    x: params.coordSys.x,
                    y: params.coordSys.y,
                    width: params.coordSys.width,
                    height: params.coordSys.height
                });

                if (!clippedRect) return null;

                clippedRect.r = 4;

                const raw = dataItem.raw || {};
                const isTimeNode = raw.type === 'time_node';
                const isDepartmentPresence = raw.type === 'department_presence';
                const isDepartmentPending = isDepartmentPresence && raw.status === 'pending';

                return {
                    type: 'group',
                    children: [
                        {
                            type: 'rect',
                            shape: clippedRect,
                            style: {
                                fill: api.style().fill,
                                stroke: isDepartmentPresence ? 'rgba(15, 23, 42, 0.18)' : 'rgba(15, 23, 42, 0.22)',
                                lineWidth: 1,
                                opacity: isTimeNode ? 0.6 : (isDepartmentPending ? 0.72 : 0.9),
                                lineDash: isDepartmentPending ? [4, 3] : null
                            }
                        },
                        {
                            type: 'text',
                            style: {
                                x: clippedRect.x + 6,
                                y: clippedRect.y + clippedRect.height / 2,
                                text: clippedRect.width > 60 ? (dataItem.name || '') : '',
                                verticalAlign: 'middle',
                                fill: isDepartmentPending ? '#33485f' : '#ffffff',
                                fontSize: 11,
                                fontWeight: 500,
                                width: Math.max(20, clippedRect.width - 10),
                                overflow: 'truncate'
                            },
                            silent: true
                        }
                    ]
                };
            },
            encode: {
                x: [0, 1],
                y: 2
            },
            data: seriesData,
            markLine: {
                symbol: ['none', 'none'],
                silent: true,
                lineStyle: {
                    color: '#FF3B30',
                    width: 1,
                    type: 'dashed'
                },
                label: {
                    show: true,
                    formatter: '现在',
                    color: '#FF3B30',
                    fontWeight: 600,
                    backgroundColor: 'rgba(255, 59, 48, 0.12)',
                    borderColor: 'rgba(255, 59, 48, 0.35)',
                    borderWidth: 1,
                    borderRadius: 4,
                    padding: [2, 6]
                },
                data: [{ xAxis: Date.now() }]
            }
        }]
    };

    ganttChart.clear();
    ganttChart.setOption(option, true);

    requestAnimationFrame(() => {
        if (!ganttChart || !ganttHostElement) {
            return;
        }
        if (!document.body.contains(ganttHostElement)) {
            return;
        }
        try {
            ganttChart.resize();
        } catch (_error) {
            // ignore resize error
        }
    });
}

function formatTime(timestamp) {
    return new Date(timestamp).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });
}

function showGanttItemDetail(raw) {
    if (!raw) return;

    if (raw.type === 'flight_status') {
        const detail = `
            <div style="padding: 16px;">
                <h3 style="margin: 0 0 12px 0;">航班状态转换</h3>
                <p><strong>从:</strong> ${raw.from}</p>
                <p><strong>到:</strong> ${raw.to}</p>
                <p><strong>开始时间:</strong> ${new Date(raw.fromTime).toLocaleString('zh-CN')}</p>
                <p><strong>结束时间:</strong> ${new Date(raw.toTime).toLocaleString('zh-CN')}</p>
            </div>
        `;
        alert(detail.replace(/<[^>]*>/g, ''));
        return;
    } else if (raw.type === 'time_node') {
        const detail = `
            <div style="padding: 16px;">
                <h3 style="margin: 0 0 12px 0;">时间节点</h3>
                <p><strong>名称:</strong> ${raw.label}</p>
                <p><strong>时间:</strong> ${new Date(raw.time).toLocaleString('zh-CN')}</p>
            </div>
        `;
        alert(detail.replace(/<[^>]*>/g, ''));
        return;
    } else if (raw.type === 'department_presence') {
        const statusText = raw.status === 'pending'
            ? '未到位'
            : (raw.status === 'in_progress' ? '进行中' : '已签退');
        const detail = `
            <div style="padding: 16px;">
                <h3 style="margin: 0 0 12px 0;">调度口科室到位/签退</h3>
                <p><strong>科室:</strong> ${raw.department || DEPARTMENT_DEFAULT_LABEL}</p>
                <p><strong>状态:</strong> ${statusText}</p>
                <p><strong>工单数:</strong> ${raw.order_count || 0}</p>
                <p><strong>成员数:</strong> ${raw.member_count || 0}</p>
                <p><strong>已到位:</strong> ${raw.checked_in_count || 0}</p>
                <p><strong>已签退:</strong> ${raw.checked_out_count || 0}</p>
                <p><strong>进行中:</strong> ${raw.in_progress_count || 0}</p>
                <p><strong>最早到位:</strong> ${raw.earliest_check_in ? new Date(raw.earliest_check_in).toLocaleString('zh-CN') : '--'}</p>
                <p><strong>最晚签退:</strong> ${raw.latest_check_out ? new Date(raw.latest_check_out).toLocaleString('zh-CN') : '--'}</p>
                <p><strong>涉及作业类型:</strong> ${(raw.task_type_names || []).join('、') || '-'}</p>
                <p><strong>涉及班组:</strong> ${(raw.team_names || []).join('、') || '-'}</p>
            </div>
        `;
        alert(detail.replace(/<[^>]*>/g, ''));
        return;
    } else if (raw.type === 'business_case') {
        openBusinessCaseDetail(raw.case_id);
    }
}

async function fetchBusinessCaseDetail(caseId) {
    if (!caseId) {
        throw new Error('事项ID不能为空');
    }
    if (typeof Auth === 'undefined' || typeof Auth.fetch !== 'function') {
        throw new Error('认证上下文不可用');
    }

    const response = await Auth.fetch(
        `/api/v2/business-cases/${encodeURIComponent(caseId)}`,
        fetchBusinessCaseDetailOptions
    );
    const payload = await response.json().catch(() => ({}));
    if (!response.ok || !payload?.success) {
        throw new Error(payload?.detail || payload?.message || '加载业务事项详情失败');
    }
    return payload.data || null;
}

function closeBusinessCaseDetail() {
    businessCaseModalState.open = false;
    businessCaseModalState.loading = false;
    businessCaseModalState.submitting = false;
    businessCaseModalState.statusUpdating = false;
    businessCaseModalState.workflowFormsLoading = false;
    businessCaseModalState.workflowFormsError = '';
    businessCaseModalState.workflowFormsSubmittingKey = '';
    businessCaseModalState.workflowFormsPayload = null;
    businessCaseModalState.workflowFormDrafts = {};
    businessCaseModalState.caseId = null;
    businessCaseModalState.caseData = null;
    businessCaseMentionState.selectedIds.clear();
    businessCaseMentionState.showDropdown = false;
    renderBusinessCaseDetailModal();
}

async function acknowledgeBusinessCaseAppend(caseId, appendId) {
    if (!caseId || !appendId || typeof Auth === 'undefined' || typeof Auth.fetch !== 'function') {
        return;
    }

    try {
        const response = await Auth.fetch(
            `/api/v2/business-cases/${encodeURIComponent(caseId)}/appends/${encodeURIComponent(appendId)}/acknowledge`,
            {
                ...fetchBusinessCaseModalMutationOptions,
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
            }
        );
        const payload = await response.json().catch(() => ({}));
        if (!response.ok) {
            if (typeof showToast === 'function') {
                showToast(payload?.detail || '确认失败', 'error');
            }
            return;
        }

        if (typeof showToast === 'function') {
            showToast('已确认收到', 'success');
        }

        // Refresh modal to show updated acknowledge state
        if (businessCaseModalState.caseId) {
            openBusinessCaseDetail(businessCaseModalState.caseId);
        }
    } catch (error) {
        console.warn('确认失败:', error);
    }
}

function getCurrentUserIdForAcknowledge() {
    if (typeof Auth !== 'undefined' && Auth.currentUser) {
        return String(Auth.currentUser.user_id || Auth.currentUser.username || '').trim();
    }
    return '';
}

function renderAppendEntries(entries, caseId) {
    if (!Array.isArray(entries) || entries.length === 0) {
        return '<div style="padding:16px; border:1px dashed #d8e0e8; border-radius:14px; color:#6f8093; background:#f8fafc; font-size:13px; line-height:1.7;">暂无回复。</div>';
    }

    const currentUserId = getCurrentUserIdForAcknowledge();
    const sortedEntries = [...entries].sort((a, b) => new Date(a.appended_at) - new Date(b.appended_at));

    return sortedEntries.map((entry) => {
        const mentionIds = (entry.metadata && Array.isArray(entry.metadata.mention_user_ids))
            ? entry.metadata.mention_user_ids
            : [];
        const acknowledgments = (entry.metadata && typeof entry.metadata.acknowledgments === 'object' && entry.metadata.acknowledgments !== null)
            ? entry.metadata.acknowledgments
            : {};
        const ackCount = Object.keys(acknowledgments).length;
        const authorName = String(entry.submitted_operator_name || entry.submitted_by || '未命名值班人').trim() || '未命名值班人';
        const avatarText = escapeHtml(authorName.slice(0, 1) || '回');

        let mentionAckHtml = '';
        if (mentionIds.length > 0) {
            const progressHtml = `<span style="font-size:12px; color:#6f8093;">提醒已发送给相关人员 · 已确认 ${ackCount}/${mentionIds.length}</span>`;

            let ackButtonHtml = '';
            if (currentUserId && mentionIds.includes(currentUserId)) {
                if (acknowledgments[currentUserId]) {
                    const ackTime = new Date(acknowledgments[currentUserId].acknowledged_at)
                        .toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });
                    ackButtonHtml = `<span style="font-size:11px; color:#8e8e93; font-weight:500;">✓ 已确认 ${ackTime}</span>`;
                } else {
                    ackButtonHtml = `<button type="button" onclick="acknowledgeBusinessCaseAppend('${escapeHtml(caseId || '')}', '${escapeHtml(entry.append_id || '')}')"
                        style="border:1px solid #007AFF; background:rgba(0,122,255,0.06); color:#007AFF; padding:3px 10px; border-radius:999px; font-size:11px; font-weight:600; cursor:pointer;">
                        ✓ 确认收到
                    </button>`;
                }
            }

            mentionAckHtml = `
                <div style="display:flex; align-items:center; justify-content:space-between; gap:10px; flex-wrap:wrap; margin-top:12px; padding-top:10px; border-top:1px solid #edf2f7;">
                    ${progressHtml}
                    ${ackButtonHtml}
                </div>`;
        }

        return `
        <article style="display:grid; grid-template-columns:56px minmax(0,1fr); gap:14px;">
            <div style="display:flex; flex-direction:column; align-items:center; gap:8px;">
                <div style="width:42px; height:42px; border-radius:50%; display:flex; align-items:center; justify-content:center; background:linear-gradient(135deg,#0ea5e9 0%,#2563eb 100%); color:#fff; font-size:16px; font-weight:700; box-shadow:0 10px 18px rgba(37,99,235,0.18);">${avatarText}</div>
                <div style="font-size:11px; color:#8e8e93; font-weight:600; max-width:56px; overflow-wrap:anywhere; text-align:center; line-height:1.35;">${escapeHtml(authorName)}</div>
            </div>
            <div style="border:1px solid #e6edf4; border-radius:16px; background:#fff; padding:14px 16px;">
                <div style="display:flex; justify-content:space-between; align-items:flex-start; gap:12px;">
                    <div style="min-width:0;">
                        <div style="margin-top:4px; font-size:12px; color:#708195;">${escapeHtml(formatDateTime(entry.appended_at))} · ${escapeHtml(entry.submitted_by || '-')}</div>
                    </div>
                    <div style="display:flex; align-items:center; gap:8px; flex-wrap:wrap; justify-content:flex-end;">
                        ${mentionIds.length > 0 ? `<span style="display:inline-flex; align-items:center; padding:4px 10px; border-radius:999px; font-size:11px; font-weight:700; background:rgba(59,130,246,0.12); color:#1d4ed8;">@${mentionIds.length} 人</span>` : ''}
                        ${mentionIds.length > 0 ? `<span style="display:inline-flex; align-items:center; padding:4px 10px; border-radius:999px; font-size:11px; font-weight:700; background:rgba(16,185,129,0.12); color:#047857;">已确认 ${ackCount}/${mentionIds.length}</span>` : ''}
                    </div>
                </div>
                <div style="margin-top:12px; font-size:14px; line-height:1.8; color:#102132; white-space:pre-wrap; word-break:break-word;">${escapeHtml(entry.content || '')}</div>
                ${mentionAckHtml}
            </div>
        </article>
    `}).join('');
}

function renderBusinessCaseDetailModal() {
    let host = document.getElementById('businessCaseDetailModal');
    if (!host) {
        host = document.createElement('div');
        host.id = 'businessCaseDetailModal';
        document.body.appendChild(host);
    } else if (host.parentNode !== document.body) {
        document.body.appendChild(host);
    }

    if (!businessCaseModalState.open) {
        host.style.display = 'none';
        host.innerHTML = '';
        return;
    }

    host.style.display = 'flex';

    if (businessCaseModalState.loading) {
        host.innerHTML = `
            <div style="position:fixed; inset:0; background:rgba(15, 23, 42, 0.45); z-index: 99998;" onclick="closeBusinessCaseDetail()"></div>
            <div style="position:fixed; top:50%; left:50%; transform:translate(-50%, -50%); width:min(760px, calc(100vw - 32px)); max-height:calc(100vh - 48px); overflow:auto; background:#fff; border-radius:20px; box-shadow:0 24px 80px rgba(15, 23, 42, 0.28); padding:24px; z-index: 99999;">
                <div style="font-size:16px; font-weight:700; color:#102132;">加载业务事项详情中...</div>
            </div>
        `;
        return;
    }

    const caseData = businessCaseModalState.caseData || {};
    const appendCount = Number(caseData.append_count || 0);
    const statusText = getBusinessCaseStatusText(caseData.status);
    const bindingLabel = getBusinessCaseBinding(caseData);
    const canUpdateStatus = canEditBusinessCaseStatus(caseData);
    const visibilityInfo = getBusinessCaseVisibilityInfo(caseData);
    const appendEntries = Array.isArray(caseData.append_entries) ? caseData.append_entries : [];
    const threadTotal = 1 + appendEntries.length;
    const rootAuthorName = String(caseData.created_by || '系统').trim() || '系统';
    const rootAvatarText = rootAuthorName.slice(0, 1) || '系';
    const workflowFormsSectionHtml = renderBusinessCaseWorkflowFormsSection(caseData);
    const hasWorkflowFormsColumn = workflowFormsSectionHtml !== '';
    const currentStatusValue = getBusinessCaseStatusValue(caseData.status);
    const statusOptionsHtml = getBusinessCaseStatusOptions(caseData).map((option) => `
        <option value="${escapeHtml(option.value)}" ${currentStatusValue === option.value ? 'selected' : ''}>${escapeHtml(option.label)}</option>
    `).join('');
    const modalMaxWidth = hasWorkflowFormsColumn ? '1520px' : '1180px';
    const gridColumns = hasWorkflowFormsColumn
        ? 'minmax(0, 1.2fr) minmax(280px, 0.7fr) minmax(320px, 0.9fr)'
        : 'minmax(0, 1.5fr) minmax(320px, 0.92fr)';
    host.innerHTML = `
        <div style="position:fixed; inset:0; background:rgba(15, 23, 42, 0.45); z-index: 99998;" onclick="closeBusinessCaseDetail()"></div>
        <div style="position:fixed; top:50%; left:50%; transform:translate(-50%, -50%); width:min(${modalMaxWidth}, calc(100vw - 32px)); max-height:calc(100vh - 48px); overflow:auto; background:#fff; border-radius:20px; box-shadow:0 24px 80px rgba(15, 23, 42, 0.28); z-index: 99999;">
            <div style="padding:20px 24px; border-bottom:1px solid #e7edf4; display:flex; justify-content:space-between; align-items:flex-start; gap:16px;">
                <div>
                    <div style="font-size:18px; font-weight:700; color:#102132;">业务事项详情</div>
                    <div style="margin-top:6px; font-size:12px; color:#6f8093;">事项类型：${escapeHtml(resolveCaseTypeName(caseData))} | 状态：${escapeHtml(statusText)} | ${escapeHtml(visibilityInfo.scopeLabel)}</div>
                </div>
                <button type="button" onclick="closeBusinessCaseDetail()" style="border:none; background:transparent; font-size:24px; line-height:1; color:#6f8093; cursor:pointer;">×</button>
            </div>
            <div style="padding:20px 24px; background:#f8fafc;">
                <div style="display:grid; grid-template-columns:${gridColumns}; gap:20px; align-items:start;">
                    <div style="min-width:0; display:flex; flex-direction:column; gap:16px;">
                        <section style="padding:18px; background:linear-gradient(180deg, rgba(255,255,255,0.98) 0%, rgba(248,250,252,0.96) 100%); border:1px solid #e6edf4; border-radius:16px; box-shadow:0 10px 24px rgba(15,23,42,0.04);">
                            <div style="display:flex; justify-content:space-between; align-items:flex-start; gap:12px; margin-bottom:14px;">
                                <div>
                                    <div style="font-size:15px; font-weight:700; color:#102132;">事项记录</div>
                                </div>
                                <div style="display:inline-flex; flex-wrap:wrap; gap:8px; justify-content:flex-end; font-size:12px; color:#708195;">
                                    <span>共 ${threadTotal} 条记录</span>
                                    <span>${appendCount > 0 ? `追加 ${appendCount} 次` : '暂无回复'}</span>
                                </div>
                            </div>

                            <article style="display:grid; grid-template-columns:56px minmax(0,1fr); gap:14px;">
                                <div style="display:flex; flex-direction:column; align-items:center; gap:8px;">
                                    <div style="width:42px; height:42px; border-radius:50%; display:flex; align-items:center; justify-content:center; background:linear-gradient(135deg,#f59e0b 0%,#f97316 100%); color:#fff; font-size:16px; font-weight:700; box-shadow:0 10px 18px rgba(249,115,22,0.18);">${escapeHtml(rootAvatarText)}</div>
                                    <div style="font-size:11px; color:#8e8e93; font-weight:600; max-width:56px; overflow-wrap:anywhere; text-align:center; line-height:1.35;">${escapeHtml(rootAuthorName)}</div>
                                </div>
                                <div style="border:1px solid #e6edf4; border-radius:16px; background:linear-gradient(180deg, rgba(239,246,255,0.7) 0%, rgba(255,255,255,1) 100%); padding:14px 16px;">
                                    <div style="display:flex; justify-content:space-between; align-items:flex-start; gap:12px;">
                                        <div style="min-width:0;">
                                            <div style="margin-top:4px; font-size:12px; color:#708195;">
                                                发布于 ${escapeHtml(formatDateTime(caseData.created_at))}
                                                ${caseData.finished_at ? ` · 完成于 ${escapeHtml(formatDateTime(caseData.finished_at))}` : ''}
                                            </div>
                                        </div>
                                        <div style="display:flex; align-items:center; gap:8px; flex-wrap:wrap; justify-content:flex-end;">
                                            <span style="display:inline-flex; align-items:center; padding:4px 10px; border-radius:999px; font-size:11px; font-weight:700; background:rgba(14,165,233,0.12); color:#0369a1;">${escapeHtml(visibilityInfo.scopeLabel)}</span>
                                            <span style="display:inline-flex; align-items:center; padding:4px 10px; border-radius:999px; font-size:11px; font-weight:700; background:rgba(15,23,42,0.08); color:#475569;">${escapeHtml(statusText)}</span>
                                        </div>
                                    </div>
                                    <div style="margin-top:12px; font-size:14px; line-height:1.8; color:#102132; white-space:pre-wrap; word-break:break-word;">${escapeHtml(caseData.description || '当前事项未填写描述。')}</div>
                                    <div style="margin-top:12px; display:flex; flex-wrap:wrap; gap:10px 14px; font-size:12px; line-height:1.6; color:#708195;">
                                        ${visibilityInfo.departmentName ? `<span>归属部门：${escapeHtml(visibilityInfo.departmentName)}</span>` : ''}
                                        ${bindingLabel ? `<span>绑定航班：${escapeHtml(bindingLabel)}</span>` : ''}
                                        <span>事项类型：${escapeHtml(resolveCaseTypeName(caseData))}</span>
                                        ${caseData.stand ? `<span>机位：${escapeHtml(caseData.stand)}</span>` : ''}
                                        ${caseData.gate ? `<span>登机口：${escapeHtml(caseData.gate)}</span>` : ''}
                                    </div>
                                </div>
                            </article>

                            <div style="margin-top:16px; padding-top:14px; border-top:1px solid #edf2f7; display:grid; gap:14px;">
                                ${renderAppendEntries(appendEntries, caseData.case_id || '')}
                            </div>
                        </section>

                        <section style="padding:18px; background:linear-gradient(180deg, rgba(255,255,255,0.98) 0%, rgba(248,250,252,0.96) 100%); border:1px solid #e6edf4; border-radius:16px; box-shadow:0 10px 24px rgba(15,23,42,0.04);">
                            <div style="display:flex; justify-content:space-between; align-items:flex-start; gap:12px; margin-bottom:12px;">
                                <div>
                                    <div style="font-size:15px; font-weight:700; color:#102132;">追加回复</div>
                                </div>
                                <button type="button" onclick="toggleMentionDropdown()" style="border:none; background:transparent; color:#007AFF; font-size:12px; cursor:pointer;">@ 提醒相关人员</button>
                            </div>
                            <div id="mentionPickerContainer" style="position:relative; margin-bottom:12px; display:${businessCaseMentionState.showDropdown ? 'block' : 'none'};">
                                <div style="border:1px solid #d7e0e8; border-radius:8px; max-height:200px; overflow-y:auto; background:#fff;">
                                    ${renderMentionPickerDropdown()}
                                </div>
                            </div>
                            <textarea id="businessCaseAppendContent" rows="4" maxlength="2000" placeholder="填写回复内容。" style="width:100%; resize:vertical; border:1px solid #d7e0e8; border-radius:12px; padding:12px 14px; font-size:13px; line-height:1.6; box-sizing:border-box;"></textarea>
                            <div id="mentionSummaryText" style="margin-top:6px; font-size:11px; color:#007AFF; min-height:16px;">
                                ${businessCaseMentionState.selectedIds.size > 0 ? `将通知 ${businessCaseMentionState.selectedIds.size} 人` : ''}
                            </div>
                            <div style="margin-top:12px; display:flex; justify-content:flex-end; gap:10px;">
                                <button type="button" onclick="closeBusinessCaseDetail()" style="border:1px solid #d7e0e8; background:#fff; color:#33485f; padding:8px 14px; border-radius:999px; cursor:pointer;">关闭</button>
                                <button type="button" onclick="submitBusinessCaseAppend()" ${businessCaseModalState.submitting ? 'disabled' : ''} style="border:none; background:#0b77e3; color:#fff; padding:8px 14px; border-radius:999px; cursor:pointer; ${businessCaseModalState.submitting ? 'opacity:0.6;' : ''}">${businessCaseModalState.submitting ? '提交中...' : '发布回复'}</button>
                            </div>
                        </section>
                    </div>

                    <div style="min-width:0; display:flex; flex-direction:column; gap:16px;">
                        <section style="padding:16px; background:linear-gradient(180deg, rgba(255,255,255,0.98) 0%, rgba(248,250,252,0.96) 100%); border:1px solid #e6edf4; border-radius:16px; box-shadow:0 10px 24px rgba(15,23,42,0.04);">
                            <div style="font-size:15px; font-weight:700; color:#102132;">事项概览</div>
                            <div style="margin-top:12px; display:grid; grid-template-columns:repeat(2, minmax(0,1fr)); gap:10px;">
                                <div style="padding:12px; border-radius:12px; background:rgba(248,250,252,0.9); border:1px solid #edf2f7;">
                                    <div style="font-size:11px; color:#8e8e93; margin-bottom:6px;">状态</div>
                                    <div style="font-size:13px; font-weight:600; color:#102132; line-height:1.5;">${escapeHtml(statusText)}</div>
                                </div>
                                <div style="padding:12px; border-radius:12px; background:rgba(248,250,252,0.9); border:1px solid #edf2f7;">
                                    <div style="font-size:11px; color:#8e8e93; margin-bottom:6px;">范围</div>
                                    <div style="font-size:13px; font-weight:600; color:#102132; line-height:1.5;">${escapeHtml(visibilityInfo.scopeLabel)}</div>
                                </div>
                                <div style="padding:12px; border-radius:12px; background:rgba(248,250,252,0.9); border:1px solid #edf2f7;">
                                    <div style="font-size:11px; color:#8e8e93; margin-bottom:6px;">创建人</div>
                                    <div style="font-size:13px; font-weight:600; color:#102132; line-height:1.5;">${escapeHtml(caseData.created_by || '-')}</div>
                                </div>
                                <div style="padding:12px; border-radius:12px; background:rgba(248,250,252,0.9); border:1px solid #edf2f7;">
                                    <div style="font-size:11px; color:#8e8e93; margin-bottom:6px;">创建时间</div>
                                    <div style="font-size:13px; font-weight:600; color:#102132; line-height:1.5;">${escapeHtml(formatDateTime(caseData.created_at))}</div>
                                </div>
                                ${caseData.finished_at ? `<div style="padding:12px; border-radius:12px; background:rgba(248,250,252,0.9); border:1px solid #edf2f7;">
                                    <div style="font-size:11px; color:#8e8e93; margin-bottom:6px;">完成时间</div>
                                    <div style="font-size:13px; font-weight:600; color:#102132; line-height:1.5;">${escapeHtml(formatDateTime(caseData.finished_at))}</div>
                                </div>` : ''}
                                ${visibilityInfo.departmentName ? `<div style="padding:12px; border-radius:12px; background:rgba(248,250,252,0.9); border:1px solid #edf2f7;">
                                    <div style="font-size:11px; color:#8e8e93; margin-bottom:6px;">归属部门</div>
                                    <div style="font-size:13px; font-weight:600; color:#102132; line-height:1.5;">${escapeHtml(visibilityInfo.departmentName)}</div>
                                </div>` : ''}
                            </div>

                            ${canUpdateStatus ? `
                            <div style="margin-top:16px; padding-top:14px; border-top:1px solid #edf2f7;">
                                <div style="margin-bottom:10px; font-size:12px; font-weight:700; color:#102132;">状态流转</div>
                                <div style="display:flex; gap:10px; align-items:center; flex-wrap:wrap;">
                                    <select id="businessCaseStatusSelect" style="min-width:180px; border:1px solid #d7e0e8; border-radius:10px; padding:8px 10px; font-size:13px; color:#102132; background:#fff;">
                                        ${statusOptionsHtml}
                                    </select>
                                    <button
                                        type="button"
                                        onclick="submitBusinessCaseStatusUpdate()"
                                        ${businessCaseModalState.statusUpdating ? 'disabled' : ''}
                                        style="border:none; background:#0b77e3; color:#fff; padding:8px 14px; border-radius:999px; cursor:pointer; ${businessCaseModalState.statusUpdating ? 'opacity:0.6;' : ''}"
                                    >${businessCaseModalState.statusUpdating ? '提交中...' : '更新状态'}</button>
                                </div>
                            </div>` : ''}
                        </section>

                        ${renderBusinessCaseWorkflowReceiptSection(caseData)}
                    </div>
                    ${hasWorkflowFormsColumn ? `<div style="min-width:0; display:flex; flex-direction:column; gap:16px;">${workflowFormsSectionHtml}</div>` : ''}
                </div>
            </div>
        </div>
    `;
}

async function openBusinessCaseDetail(caseId, options = {}) {
    const normalizedCaseId = String(caseId || '').trim();
    if (!normalizedCaseId) {
        return;
    }
    const preserveWorkflowFormDrafts = options.preserveWorkflowFormDrafts === true;
    const isSameCase = String(businessCaseModalState.caseId || '').trim() === normalizedCaseId;

    businessCaseModalState.open = true;
    businessCaseModalState.caseId = normalizedCaseId;
    if (!isSameCase) {
        businessCaseMentionState.selectedIds.clear();
        businessCaseMentionState.showDropdown = false;
        resetBusinessCaseWorkflowFormsState({ preserveDrafts: preserveWorkflowFormDrafts });
    }

    if (currentFlight && currentFlight.flight_id) {
        businessCaseMentionState.flightId = currentFlight.flight_id;
        fetchStakeholdersForFlight(currentFlight.flight_id);
    }

    if (options.prefetchedData) {
        businessCaseModalState.loading = false;
        businessCaseModalState.caseData = options.prefetchedData;
        renderBusinessCaseDetailModal();
        void loadBusinessCaseWorkflowForms(normalizedCaseId, {
            preserveDrafts: preserveWorkflowFormDrafts,
            preservePayload: false,
        });
        return;
    }

    businessCaseModalState.loading = true;
    businessCaseModalState.caseData = null;
    renderBusinessCaseDetailModal();
    void loadBusinessCaseWorkflowForms(normalizedCaseId, {
        preserveDrafts: preserveWorkflowFormDrafts,
        preservePayload: false,
    });

    try {
        const detail = await fetchBusinessCaseDetail(normalizedCaseId);
        if (String(businessCaseModalState.caseId || '').trim() !== normalizedCaseId) {
            return;
        }
        businessCaseModalState.caseData = detail;
    } catch (error) {
        if (typeof showToast === 'function') {
            showToast(error.message || '加载业务事项详情失败', 'error');
        }
        businessCaseModalState.open = false;
    } finally {
        businessCaseModalState.loading = false;
        renderBusinessCaseDetailModal();
    }
}

function applyBusinessCaseSummaryToCurrentFlight(caseData) {
    if (!currentFlight || !Array.isArray(currentFlight.business_cases) || !caseData) {
        return;
    }
    const caseId = String(caseData.case_id || '').trim();
    const nextSummary = { ...caseData };
    delete nextSummary.append_entries;

    const updateSummary = (flightLike) => {
        if (!flightLike || !Array.isArray(flightLike.business_cases)) {
            return;
        }
        const index = flightLike.business_cases.findIndex((item) => String(item.case_id || '').trim() === caseId);
        if (index >= 0) {
            flightLike.business_cases[index] = {
                ...flightLike.business_cases[index],
                ...nextSummary,
            };
        }
    };

    updateSummary(currentFlight);
    if (typeof findFlightById === 'function' && currentFlight?.flight_id) {
        updateSummary(findFlightById(currentFlight.flight_id));
    }
}

async function submitBusinessCaseStatusUpdate() {
    const caseId = String(businessCaseModalState.caseId || '').trim();
    const caseData = businessCaseModalState.caseData || null;
    const selectEl = document.getElementById('businessCaseStatusSelect');
    const nextStatus = String(selectEl?.value || '').trim().toUpperCase();

    if (!caseId || !caseData) {
        return;
    }
    if (!canEditBusinessCaseStatus(caseData)) {
        if (typeof showToast === 'function') {
            showToast('当前账号无权修改该业务事项状态', 'warning');
        }
        return;
    }
    if (getBusinessCaseStatusValue(caseData.status) === nextStatus) {
        if (typeof showToast === 'function') {
            showToast('状态未发生变化', 'info');
        }
        return;
    }
    if (!BUSINESS_CASE_STATUSES.includes(nextStatus)) {
        if (typeof showToast === 'function') {
            showToast('请选择合法的事项状态', 'warning');
        }
        return;
    }
    if (typeof Auth === 'undefined' || typeof Auth.fetch !== 'function') {
        if (typeof showToast === 'function') {
            showToast('认证上下文不可用', 'error');
        }
        return;
    }

    businessCaseModalState.statusUpdating = true;
    renderBusinessCaseDetailModal();

    try {
        const response = await Auth.fetch(`/api/v2/business-cases/${encodeURIComponent(caseId)}/status`, {
            ...fetchBusinessCaseModalMutationOptions,
            method: 'PATCH',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({ status: nextStatus }),
        });
        const payload = await response.json().catch(() => ({}));
        if (!response.ok || !payload?.success) {
            throw new Error(payload?.detail || payload?.message || '更新业务事项状态失败');
        }

        businessCaseModalState.caseData = payload.data || null;
        applyBusinessCaseSummaryToCurrentFlight(payload.data || null);
        if (typeof renderFlightDetail === 'function') {
            renderFlightDetail();
        }
        businessCaseModalState.open = true;
        businessCaseModalState.caseId = caseId;
        businessCaseModalState.statusUpdating = false;
        renderBusinessCaseDetailModal();
        if (typeof showToast === 'function') {
            showToast('业务事项状态更新成功', 'success');
        }
    } catch (error) {
        businessCaseModalState.statusUpdating = false;
        renderBusinessCaseDetailModal();
        if (typeof showToast === 'function') {
            showToast(error.message || '更新业务事项状态失败', 'error');
        }
    }
}

async function submitBusinessCaseAppend() {
    const caseId = String(businessCaseModalState.caseId || '').trim();
    const contentInput = document.getElementById('businessCaseAppendContent');
    const content = String(contentInput?.value || '').trim();

    if (!caseId) {
        return;
    }
    if (!content) {
        if (typeof showToast === 'function') {
            showToast('请输入追加内容', 'error');
        }
        return;
    }
    if (typeof Auth === 'undefined' || typeof Auth.fetch !== 'function') {
        if (typeof showToast === 'function') {
            showToast('认证上下文不可用', 'error');
        }
        return;
    }

    businessCaseModalState.submitting = true;
    renderBusinessCaseDetailModal();

    try {
        const payloadBody = { content };
        if (businessCaseMentionState.selectedIds.size > 0) {
            payloadBody.mention_user_ids = Array.from(businessCaseMentionState.selectedIds);
        }

        const response = await Auth.fetch(`/api/v2/business-cases/${encodeURIComponent(caseId)}/appends`, {
            ...fetchBusinessCaseModalMutationOptions,
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify(payloadBody),
        });
        const payload = await response.json().catch(() => ({}));
        if (!response.ok || !payload?.success) {
            throw new Error(payload?.detail || payload?.message || '提交追加失败');
        }

        businessCaseModalState.caseData = payload.data || null;
        applyBusinessCaseSummaryToCurrentFlight(payload.data || null);
        if (typeof renderFlightDetail === 'function') {
            renderFlightDetail();
        }
        businessCaseModalState.open = true;
        businessCaseModalState.caseId = caseId;
        businessCaseModalState.submitting = false;
        renderBusinessCaseDetailModal();
        if (typeof showToast === 'function') {
            showToast('业务事项追加成功', 'success');
        }
    } catch (error) {
        businessCaseModalState.submitting = false;
        renderBusinessCaseDetailModal();
        if (typeof showToast === 'function') {
            showToast(error.message || '业务事项追加失败', 'error');
        }
    }
}

function renderBusinessEventsSection(flight) {
    const cases = flight.business_cases || [];
    requestAnimationFrame(() => {
        renderBusinessCaseDetailModal();
    });

    return `
        <div class="events-section business-events-section" style="display:flex; flex-direction:column; height:100%; padding:0;">
            <div class="events-header-row" style="padding: 12px 16px; border-bottom: 1px solid var(--border-light); background: #fff;">
                <div style="display:flex; justify-content:space-between; align-items:center; width:100%;">
                    <span class="events-title" style="font-weight:600; color:var(--text-primary);">
                        <img src="/frontend/icons/bar_chart.svg" class="svg-icon" style="vertical-align:-2px; margin-right:6px;"> 
                        业务全景监控
                    </span>
                    <div class="business-insight-actions">
                        <button class="btn btn-secondary business-insight-btn" id="generateHistoryReportBtn" type="button"
                            onclick="generateSelectedFlightHistoryReport()">生成动态报表</button>
                        <button class="btn btn-secondary business-insight-btn" id="generateEventJourneyBtn" type="button"
                            onclick="generateSelectedFlightEventJourney()">生成事件经过</button>
                        <button class="btn-edit" id="createEventBtn" style="padding: 4px 12px; font-size: 12px;" onclick="createNewEvent()">+ 新建事项</button>
                    </div>
                </div>
                <div class="business-ai-capability-hint" id="businessAiCapabilityHint" hidden></div>
            </div>
            
            <div class="ops-gantt-area" id="ganttContainer" style="flex: 1; min-height: 300px; background: #fff;">
                <div id="cascadeImpactBanner"></div>
                <div id="ganttChart" style="width: 100%; height: 100%;"></div>
            </div>

            <div class="ops-log-area" id="timelineContainer" style="background: #fff; flex: 1; display: flex; flex-direction: column;">
                <div class="log-toolbar" style="padding: 12px 16px; display:flex; justify-content:space-between; align-items:center; background:#fff; border-bottom:1px solid #eee;">
                    <span style="font-size:13px; font-weight:600;">事件日志</span>
                    <div class="filter-container" id="caseFilterContainer" style="margin-bottom:0;">
                        <button class="filter-btn ${currentCaseFilter === 'all' ? 'active' : ''}" data-filter="all" onclick="filterBusinessCases('all')">全部</button>
                        <button class="filter-btn ${currentCaseFilter === 'INITIAL' ? 'active' : ''}" data-filter="INITIAL" onclick="filterBusinessCases('INITIAL')">初始</button>
                        <button class="filter-btn ${currentCaseFilter === 'PENDING' ? 'active' : ''}" data-filter="PENDING" onclick="filterBusinessCases('PENDING')">待处理</button>
                        <button class="filter-btn ${currentCaseFilter === 'PROCESSING' ? 'active' : ''}" data-filter="PROCESSING" onclick="filterBusinessCases('PROCESSING')">处理中</button>
                        <button class="filter-btn ${currentCaseFilter === 'SUCCESS' ? 'active' : ''}" data-filter="SUCCESS" onclick="filterBusinessCases('SUCCESS')">成功</button>
                        <button class="filter-btn ${currentCaseFilter === 'FAILED' ? 'active' : ''}" data-filter="FAILED" onclick="filterBusinessCases('FAILED')">失败</button>
                    </div>
                </div>
                
                <div style="padding: 0 16px 16px 16px; overflow-y: auto;">
                    ${renderTimelineHTML(cases)}
                </div>
            </div>
        </div>`;
}

window.renderBusinessEventsSection = renderBusinessEventsSection;
window.renderGanttChart = renderGanttChart;
window.filterBusinessCases = filterBusinessCases;
window.invalidateDispatchOrdersForFlight = invalidateDispatchOrdersForFlight;
window.invalidateAllDispatchOrderCaches = invalidateAllDispatchOrderCaches;
window.refreshDispatchOrdersForFlight = refreshDispatchOrdersForFlight;
window.openBusinessCaseDetail = openBusinessCaseDetail;
window.closeBusinessCaseDetail = closeBusinessCaseDetail;
window.applyBusinessCaseSummaryToCurrentFlight = applyBusinessCaseSummaryToCurrentFlight;
window.submitBusinessCaseStatusUpdate = submitBusinessCaseStatusUpdate;
window.submitBusinessCaseAppend = submitBusinessCaseAppend;
window.submitBusinessCaseWorkflowForm = submitBusinessCaseWorkflowForm;
window.updateBusinessCaseWorkflowFormDraft = updateBusinessCaseWorkflowFormDraft;
window.toggleBusinessCaseWorkflowFormMultiSelectOption = toggleBusinessCaseWorkflowFormMultiSelectOption;
window.getBusinessCaseVisibilityInfo = getBusinessCaseVisibilityInfo;

/**
 * EP-03: 级联延误推演 — 前端甘特图阴影叠加
 * 调用方式: fetchCascadePreview(flightId, stepCode, delayMinutes, scheduledDeparture)
 */
async function fetchCascadePreview(flightId, stepCode, delayMinutes, scheduledDeparture) {
    if (!flightId || !stepCode || !delayMinutes) return null;

    const params = new URLSearchParams({
        flight_id: flightId,
        task_type: stepCode,
        delay_minutes: String(delayMinutes),
    });
    if (scheduledDeparture) {
        params.set('scheduled_departure', new Date(scheduledDeparture).toISOString());
    }

    try {
        if (typeof Auth === 'undefined' || typeof Auth.fetch !== 'function') {
            return null;
        }
        const resp = await Auth.fetch(`/api/v2/dispatch-orders/cascade-preview?${params}`);
        if (!resp.ok) return null;
        return await resp.json();
    } catch (e) {
        console.warn('[CascadePreview] fetch failed:', e);
        return null;
    }
}

function overlayCascadeOnGantt(cascadeData) {
    if (!ganttChart || !cascadeData || !cascadeData.cascaded_task_types) return;

    const task_types = cascadeData.cascaded_task_types.filter(s => s.shift_minutes > 0);
    if (task_types.length === 0) return;

    const maxShift = Math.max(...task_types.map(s => s.shift_minutes));

    // Build overlay series data
    const overlayData = task_types.map((taskTypeResult, idx) => {
        const projStart = new Date(taskTypeResult.projected_start).getTime();
        const origEnd = new Date(taskTypeResult.original_end).getTime();
        // The shadow region is from original_end to projected_end
        const projEnd = new Date(taskTypeResult.projected_end).getTime();
        const intensity = Math.min(step.shift_minutes / Math.max(maxShift, 1), 1);

        return {
            name: `${step.task_type_name} 推迟 ${Math.round(step.shift_minutes)}min`,
            value: [origEnd, projEnd, 0],
            itemStyle: {
                color: intensity > 0.6 ? 'rgba(220, 38, 38, 0.35)'
                    : intensity > 0.3 ? 'rgba(249, 115, 22, 0.30)'
                        : 'rgba(254, 240, 138, 0.35)',
            },
            raw: {
                type: 'cascade_shadow',
                task_type: step.task_type,
                task_type_name: step.task_type_name,
                shift_minutes: step.shift_minutes,
                projected_start: step.projected_start,
                projected_end: step.projected_end,
            },
        };
    });

    // Get current option and add overlay series
    const currentOption = ganttChart.getOption();
    const existingSeries = currentOption.series || [];

    // Remove previous cascade overlay if present
    const filteredSeries = existingSeries.filter(s => s._cascadeOverlay !== true);

    filteredSeries.push({
        _cascadeOverlay: true,
        type: 'custom',
        animation: false,
        renderItem: function (params, api) {
            const startCoord = api.coord([api.value(0), api.value(2)]);
            const endCoord = api.coord([api.value(1), api.value(2)]);
            const laneHeight = Math.max(20, api.size([0, 1])[1]);

            const x = Math.min(startCoord[0], endCoord[0]);
            const width = Math.max(2, Math.abs(endCoord[0] - startCoord[0]));
            const y = startCoord[1] - laneHeight / 2 + 2;
            const height = laneHeight - 4;

            const clipped = echarts.graphic.clipRectByRect(
                { x, y, width, height },
                {
                    x: params.coordSys.x,
                    y: params.coordSys.y,
                    width: params.coordSys.width,
                    height: params.coordSys.height,
                }
            );
            if (!clipped) return null;
            clipped.r = 3;

            return {
                type: 'rect',
                shape: clipped,
                style: {
                    fill: api.style().fill,
                    stroke: 'rgba(220, 38, 38, 0.5)',
                    lineWidth: 1,
                    lineDash: [4, 2],
                },
            };
        },
        encode: { x: [0, 1], y: 2 },
        data: overlayData,
        z: 10,
    });

    ganttChart.setOption({ series: filteredSeries }, { replaceMerge: ['series'] });

    // Show departure impact banner if applicable
    if (cascadeData.departure_impact_minutes > 0) {
        const banner = document.getElementById('cascadeImpactBanner');
        if (banner) {
            banner.textContent = `⚠️ 级联推演：预计航班将延误约 ${Math.round(cascadeData.departure_impact_minutes)} 分钟`;
            banner.style.display = 'block';
        }
    }
}

window.fetchCascadePreview = fetchCascadePreview;
window.overlayCascadeOnGantt = overlayCascadeOnGantt;

async function handleBusinessCaseRealtimePayload(data) {
    if (!data || !data.flight_id) return;
    
    const flightIdStr = String(data.flight_id).trim();
    // Use the global flight ID resolver if available to ensure we map correctly
    if (typeof findFlightById === 'function') {
        const flightLike = findFlightById(flightIdStr);
        if (flightLike && data.case_id) {
            try {
                // Fetch the detailed case data explicitly to merge
                const caseDetail = await fetchBusinessCaseDetail(data.case_id);
                if (caseDetail) {
                    if (!flightLike.business_cases) {
                        flightLike.business_cases = [];
                    }
                    const caseIndex = flightLike.business_cases.findIndex(item => String(item.case_id).trim() === String(caseDetail.case_id).trim());
                    if (caseIndex >= 0) {
                        flightLike.business_cases[caseIndex] = { ...flightLike.business_cases[caseIndex], ...caseDetail };
                        delete flightLike.business_cases[caseIndex].append_entries;
                    } else {
                        const summary = { ...caseDetail };
                        delete summary.append_entries;
                        flightLike.business_cases.push(summary);
                    }
                }
            } catch (e) {
                console.warn('[SSE] Failed to fetch updated business case content', e);
            }
        }
    }
    
    // If the active viewed flight matches this flight, repaint the UI
    const currentFlightId = (typeof currentFlight !== 'undefined' && currentFlight) ? String(currentFlight.flight_id).trim() : null;
    if (flightIdStr === currentFlightId) {
        if (typeof renderFlightDetail === 'function') {
            renderFlightDetail();
        }
    }
}

window.handleBusinessCaseRealtimePayload = handleBusinessCaseRealtimePayload;

/* --- Mention System Functions --- */

async function fetchStakeholdersForFlight(flightId) {
    if (!flightId || typeof Auth === 'undefined' || typeof Auth.fetch !== 'function') {
        businessCaseMentionState.stakeholders = [];
        return;
    }

    try {
        const response = await Auth.fetch(
            `/api/v2/dispatch/collaboration/flights/${encodeURIComponent(flightId)}/stakeholders`,
            fetchBusinessCaseStakeholdersOptions
        );
        const payload = await response.json().catch(() => ({}));
        if (response.ok && Array.isArray(payload?.items)) {
            businessCaseMentionState.stakeholders = payload.items;
        } else {
            businessCaseMentionState.stakeholders = [];
        }
    } catch (error) {
        console.warn('加载群聊成员失败:', error);
        businessCaseMentionState.stakeholders = [];
    }
}

function renderMentionPickerDropdown() {
    if (businessCaseMentionState.stakeholders.length === 0) {
        return '<div style="padding:12px; font-size:12px; color:#8e8e93; text-align:center;">暂无群聊成员可提醒</div>';
    }

    return businessCaseMentionState.stakeholders.map(s => {
        const isChecked = businessCaseMentionState.selectedIds.has(s.user_id) ? 'checked' : '';
        const roleHtml = s.is_assignee
            ? '<span style="font-size:10px; padding:1px 6px; border-radius:999px; background:rgba(245,158,11,0.12); color:#b45309; font-weight:600; margin-left:6px;">责任人</span>'
            : (s.is_dispatcher ? '<span style="font-size:10px; padding:1px 6px; border-radius:999px; background:rgba(0,122,255,0.1); color:#007AFF; font-weight:600; margin-left:6px;">调度</span>' : '');

        return `
            <div style="padding:8px 12px; border-bottom:1px solid #f0f3f6; display:flex; align-items:center;">
                <label style="display:flex; align-items:center; cursor:pointer; width:100%;">
                    <input type="checkbox" ${isChecked} onchange="toggleMentionUser(${jsStringForInlineHandler(s.user_id)}, ${jsStringForInlineHandler(s.username)}, this.checked)" style="margin:0 10px 0 0; cursor:pointer;">
                    <span style="font-size:13px; font-weight:500; color:#102132;">${escapeHtml(s.username)}</span>
                    ${roleHtml}
                </label>
            </div>
        `;
    }).join('');
}

function toggleMentionDropdown() {
    businessCaseMentionState.showDropdown = !businessCaseMentionState.showDropdown;
    const container = document.getElementById('mentionPickerContainer');
    if (container) {
        container.style.display = businessCaseMentionState.showDropdown ? 'block' : 'none';
        if (businessCaseMentionState.showDropdown) {
            container.innerHTML = `<div style="border:1px solid #d7e0e8; border-radius:8px; max-height:200px; overflow-y:auto; background:#fff;">${renderMentionPickerDropdown()}</div>`;
        }
    }
}

function toggleMentionUser(userId, username, isChecked) {
    if (isChecked) {
        businessCaseMentionState.selectedIds.add(userId);
        const textarea = document.getElementById('businessCaseAppendContent');
        if (textarea && !textarea.value.includes(`@${username} `)) {
            textarea.value = textarea.value + `@${username} `;
            textarea.focus();
        }
    } else {
        businessCaseMentionState.selectedIds.delete(userId);
    }
    
    // Update summary text
    const summaryEl = document.getElementById('mentionSummaryText');
    if (summaryEl) {
        summaryEl.innerHTML = businessCaseMentionState.selectedIds.size > 0 
            ? `将通知 ${businessCaseMentionState.selectedIds.size} 人` 
            : '';
    }
}
