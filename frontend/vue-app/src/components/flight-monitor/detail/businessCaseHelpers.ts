import type {
  BusinessCaseAppendDetail,
  BusinessCaseDetail,
  BusinessCaseSummary,
  BusinessCaseWorkflowFormProjection,
  BusinessCaseWorkflowReceiptItem,
  BusinessCaseWorkflowReceiptProjection,
  WorkflowFormSubmissionResponse,
  WorkflowTaskFormView,
} from '../../../types/backend';
import { getCaseReceiptProjection } from '../helpers';
import { getBusinessCaseVisibilityInfo } from '../../../composables/useFlightData';

export { getCaseReceiptProjection };

export interface BusinessCaseStatusMetadata {
  value: string;
  label: string;
  color?: string;
  category?: string;
  is_terminal?: boolean;
  manual_transition_enabled?: boolean;
  workflow_target_enabled?: boolean;
  default_for_actions?: string[];
}

export const DEFAULT_CASE_STATUS_OPTIONS: BusinessCaseStatusMetadata[] = [
  { value: 'INITIAL', label: '初始', category: 'active', is_terminal: false, manual_transition_enabled: true, workflow_target_enabled: true },
  { value: 'PENDING', label: '待处理', category: 'active', is_terminal: false, manual_transition_enabled: true, workflow_target_enabled: true },
  { value: 'PROCESSING', label: '处理中', category: 'active', is_terminal: false, manual_transition_enabled: true, workflow_target_enabled: true },
  { value: 'SUCCESS', label: '成功', category: 'terminal', is_terminal: true, manual_transition_enabled: true, workflow_target_enabled: true },
  { value: 'COMPLETED', label: '已完成', category: 'terminal', is_terminal: true, manual_transition_enabled: true, workflow_target_enabled: true },
  { value: 'FAILED', label: '失败', category: 'terminal', is_terminal: true, manual_transition_enabled: true, workflow_target_enabled: true },
];

export function normalizeCaseStatusValue(status: string | null | undefined): string {
  return String(status || '').trim().toUpperCase();
}

export function getCaseStatusDraftValue(status: string | null | undefined): string {
  return normalizeCaseStatusValue(status) || 'PENDING';
}

export function normalizeCaseStatusMetadataOption(
  item: Partial<BusinessCaseStatusMetadata> | null | undefined,
): BusinessCaseStatusMetadata | null {
  const value = normalizeCaseStatusValue(item?.value);
  if (!value) {
    return null;
  }
  const fallback = DEFAULT_CASE_STATUS_OPTIONS.find((option) => option.value === value);
  return {
    ...fallback,
    ...item,
    value,
    label: String(item?.label || fallback?.label || value).trim(),
    category: String(item?.category || fallback?.category || '').trim(),
    is_terminal: Boolean(item?.is_terminal ?? fallback?.is_terminal),
    manual_transition_enabled: item?.manual_transition_enabled ?? fallback?.manual_transition_enabled ?? true,
    workflow_target_enabled: item?.workflow_target_enabled ?? fallback?.workflow_target_enabled ?? true,
    default_for_actions: Array.isArray(item?.default_for_actions)
      ? item.default_for_actions.map((action) => String(action || '').trim()).filter(Boolean)
      : (fallback?.default_for_actions || []),
  };
}

export function getCaseStatusOption(
  status: string | null | undefined,
  options: BusinessCaseStatusMetadata[],
): BusinessCaseStatusMetadata | null {
  const normalized = normalizeCaseStatusValue(status);
  return options.find((option) => option.value === normalized) || null;
}

export function formatCaseTime(isoString: string | null | undefined): string {
  if (!isoString) return '—';
  return new Date(isoString).toLocaleString('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

/** 起止时间展示：相同分钟只显示一次，同日结束只写时刻，避免「04/27 21:02 → 04/27 21:02」 */
export function formatCaseTimeRange(
  createdAt: string | null | undefined,
  finishedAt?: string | null,
): string {
  const start = formatCaseTime(createdAt);
  if (!finishedAt) {
    return start;
  }
  const end = formatCaseTime(finishedAt);
  if (!createdAt || start === '—' || end === '—' || start === end) {
    return start;
  }
  try {
    const startDate = new Date(createdAt);
    const endDate = new Date(finishedAt);
    const sameDay = startDate.getFullYear() === endDate.getFullYear()
      && startDate.getMonth() === endDate.getMonth()
      && startDate.getDate() === endDate.getDate();
    if (sameDay) {
      const endClock = endDate.toLocaleTimeString('zh-CN', {
        hour: '2-digit',
        minute: '2-digit',
      });
      return `${start} – ${endClock}`;
    }
  } catch {
    // fall through
  }
  return `${start} – ${end}`;
}

export function getCaseStatusLabel(status: string | null | undefined, options: BusinessCaseStatusMetadata[]): string {
  const normalized = normalizeCaseStatusValue(status);
  return getCaseStatusOption(normalized, options)?.label ?? status ?? '—';
}

/** 胶囊用的声（信号面 §2.4）：四声 + 无事态时的 mute，别在页面里各自映射一遍。 */
export type CaseTone = 'mute' | 'act' | 'ok' | 'warn' | 'danger';

const CASE_STATUS_TONES: Record<string, CaseTone> = {
  INITIAL: 'mute',
  PENDING: 'warn',
  PROCESSING: 'act',
  SUCCESS: 'ok',
  COMPLETED: 'ok',
  FAILED: 'danger',
};

export function getCaseStatusTone(
  status: string | null | undefined,
  options: BusinessCaseStatusMetadata[],
): CaseTone {
  const normalized = normalizeCaseStatusValue(status);
  const known = CASE_STATUS_TONES[normalized];
  if (known) {
    return known;
  }
  const option = getCaseStatusOption(normalized, options);
  if (option?.category === 'terminal' || option?.is_terminal) {
    return 'ok';
  }
  return 'mute';
}

export function getCaseDisplayName(
  caseData: { case_type?: string | null; case_type_name?: string | null } | null | undefined,
): string {
  return String(caseData?.case_type_name || caseData?.case_type || '-').trim() || '-';
}

export function getBoundCaseFlightLabel(context: Record<string, unknown> | null | undefined): string | null {
  const boundFlightNo = String(context?.bound_flight_no || '').trim();
  if (!boundFlightNo) {
    return null;
  }
  const legType = String(context?.bound_leg_type || '').trim();
  const legLabel = legType === 'inbound' ? '进港' : legType === 'outbound' ? '出港' : '航班';
  return `${legLabel} ${boundFlightNo}`;
}

export function getCaseVisibilityLabel(
  caseData: BusinessCaseSummary | BusinessCaseDetail | null | undefined,
): string {
  return getBusinessCaseVisibilityInfo(caseData).scopeLabel;
}

export function getReceiptItemStatusLabel(item: BusinessCaseWorkflowReceiptItem): string {
  const status = String(item.ack_status || '').trim().toLowerCase();
  if (status === 'acknowledged') {
    return '已回执';
  }
  if (status === 'rejected') {
    return '已拒绝';
  }
  return '待回执';
}

export function getReceiptItemStatusTone(item: BusinessCaseWorkflowReceiptItem): CaseTone {
  const status = String(item.ack_status || '').trim().toLowerCase();
  if (status === 'acknowledged') {
    return 'ok';
  }
  if (status === 'rejected') {
    return 'danger';
  }
  return 'warn';
}

export function getReceiptItemAccountName(item: BusinessCaseWorkflowReceiptItem): string {
  const candidates = [
    item.recipient_username,
    item.username,
    item.account_name,
    item.recipient_display_name,
    item.display_name,
  ];
  for (const value of candidates) {
    const normalized = String(value || '').trim();
    if (normalized) {
      return normalized;
    }
  }
  return String(item.recipient_user_id || item.user_id || '-').trim() || '-';
}

export function getReceiptSeverityLabel(receipt: BusinessCaseWorkflowReceiptProjection | null | undefined): string {
  const severity = String(receipt?.severity || '').trim().toLowerCase();
  if (severity === 'critical') {
    return 'CRITICAL';
  }
  if (severity === 'warning') {
    return 'WARNING';
  }
  if (severity === 'info') {
    return 'INFO';
  }
  return severity ? severity.toUpperCase() : '—';
}

export function getReceiptSeverityTone(
  receipt: BusinessCaseWorkflowReceiptProjection | null | undefined,
): CaseTone {
  const severity = String(receipt?.severity || '').trim().toLowerCase();
  if (severity === 'critical') {
    return 'danger';
  }
  if (severity === 'warning') {
    return 'warn';
  }
  return 'mute';
}

export function hasRecordContent(value: Record<string, unknown> | null | undefined): boolean {
  return Boolean(value && Object.keys(value).length > 0);
}

export function getCaseWorkflowFormProjection(
  caseData: BusinessCaseSummary | BusinessCaseDetail | null | undefined,
  formCode: string | null | undefined,
): BusinessCaseWorkflowFormProjection | null {
  const normalizedCode = String(formCode || '').trim();
  if (!normalizedCode) {
    return null;
  }
  const projections = caseData?.context?.forms;
  if (!projections || typeof projections !== 'object') {
    return null;
  }
  return projections[normalizedCode] || null;
}

export function getCaseWorkflowFormProjections(
  caseData: BusinessCaseSummary | BusinessCaseDetail | null | undefined,
): BusinessCaseWorkflowFormProjection[] {
  const projections = caseData?.context?.forms;
  if (!projections || typeof projections !== 'object') {
    return [];
  }
  return (Object.values(projections) as BusinessCaseWorkflowFormProjection[]).sort(
    (a, b) => new Date(b.submitted_at).getTime() - new Date(a.submitted_at).getTime(),
  );
}

export function getWorkflowSubmissionTimestamp(
  source:
    | Pick<WorkflowFormSubmissionResponse, 'submitted_at'>
    | Pick<BusinessCaseWorkflowFormProjection, 'submitted_at'>
    | null
    | undefined,
): string | null {
  return source?.submitted_at || null;
}

export function getWorkflowSubmissionDisplayData(
  form: WorkflowTaskFormView,
  caseData: BusinessCaseSummary | BusinessCaseDetail | null | undefined,
): Record<string, unknown> {
  const latestSubmission = form.latest_submission;
  const projection = getCaseWorkflowFormProjection(caseData, form.form_code);
  if (hasRecordContent(latestSubmission?.data)) {
    return latestSubmission?.data || {};
  }
  if (hasRecordContent(projection?.data)) {
    return projection?.data || {};
  }
  if (hasRecordContent(latestSubmission?.summary)) {
    return latestSubmission?.summary || {};
  }
  if (hasRecordContent(projection?.summary)) {
    return projection?.summary || {};
  }
  return {};
}

export function getWorkflowProjectionDisplayData(projection: BusinessCaseWorkflowFormProjection): Record<string, unknown> {
  if (hasRecordContent(projection.data)) {
    return projection.data;
  }
  if (hasRecordContent(projection.summary)) {
    return projection.summary;
  }
  return {};
}

export function getWorkflowFormStatusLabel(
  form: WorkflowTaskFormView,
  caseData: BusinessCaseSummary | BusinessCaseDetail | null | undefined,
): string {
  if (form.can_submit) {
    return '待填写';
  }
  if (form.latest_submission || getCaseWorkflowFormProjection(caseData, form.form_code)) {
    return '已提交';
  }
  return '只读';
}

export function getWorkflowFormStatusTone(
  form: WorkflowTaskFormView,
  caseData: BusinessCaseSummary | BusinessCaseDetail | null | undefined,
): CaseTone {
  if (form.can_submit) {
    return 'warn';
  }
  if (form.latest_submission || getCaseWorkflowFormProjection(caseData, form.form_code)) {
    return 'ok';
  }
  return 'mute';
}

export function getWorkflowFormMetaText(
  form: WorkflowTaskFormView,
  caseData: BusinessCaseSummary | BusinessCaseDetail | null | undefined,
): string | null {
  const latestSubmission = form.latest_submission;
  const projection = getCaseWorkflowFormProjection(caseData, form.form_code);
  const latestTimestamp = getWorkflowSubmissionTimestamp(latestSubmission) || getWorkflowSubmissionTimestamp(projection);
  const operatorName = latestSubmission?.submitted_operator_name || projection?.submitted_operator_name;
  const departmentName = latestSubmission?.submitted_department_name || projection?.submitted_department_name;

  if (!latestTimestamp && !form.readonly_reason) {
    return null;
  }

  const parts: string[] = [];
  if (latestTimestamp) {
    parts.push(`最近提交 ${formatCaseTime(latestTimestamp)}`);
  }
  if (operatorName) {
    parts.push(operatorName);
  }
  if (departmentName) {
    parts.push(departmentName);
  }
  if (parts.length > 0) {
    return parts.join(' · ');
  }
  return form.readonly_reason || null;
}

export function getWorkflowProjectionMetaText(projection: BusinessCaseWorkflowFormProjection): string {
  const parts = [`提交于 ${formatCaseTime(projection.submitted_at)}`];
  if (projection.submitted_operator_name) {
    parts.push(projection.submitted_operator_name);
  }
  if (projection.submitted_department_name) {
    parts.push(projection.submitted_department_name);
  }
  return parts.join(' · ');
}

export function getCaseAuthorBadge(author: string | null | undefined): string {
  return String(author || '系统').trim().slice(0, 1) || '系';
}

export function getAppendDisplayName(entry: BusinessCaseAppendDetail): string {
  return String(entry.submitted_operator_name || entry.submitted_by || '未命名值班人').trim() || '未命名值班人';
}

export function getAppendAuthorBadge(entry: BusinessCaseAppendDetail): string {
  return getAppendDisplayName(entry).slice(0, 1) || '回';
}

export function getAppendAcknowledgedCount(entry: BusinessCaseAppendDetail): number {
  return Object.keys(entry.metadata?.acknowledgments || {}).length;
}

export function getAppendMentionCount(entry: BusinessCaseAppendDetail): number {
  return Array.isArray(entry.metadata?.mention_user_ids) ? entry.metadata.mention_user_ids.length : 0;
}

export function hasAppendAcknowledged(entry: BusinessCaseAppendDetail, userId: string): boolean {
  if (!userId) {
    return false;
  }
  return Boolean(entry.metadata?.acknowledgments?.[userId]);
}

export function getAppendAcknowledgedTime(entry: BusinessCaseAppendDetail, userId: string): string {
  const acknowledgedAt = entry.metadata?.acknowledgments?.[userId]?.acknowledged_at;
  if (!acknowledgedAt) {
    return '';
  }
  return new Date(acknowledgedAt).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });
}
