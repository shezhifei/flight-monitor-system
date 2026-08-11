import type { InjectionKey, Ref } from 'vue';
import { computed, onMounted, ref, watch } from 'vue';
import {
  appendBusinessCase,
  acknowledgeBusinessCaseAppend,
  getBusinessCaseVisibilityInfo,
  updateBusinessCaseStatusRequest,
} from './useFlightData';
import type { Flight } from './useFlightDataTypes';
import { useApi } from './useApi';
import { hasUserPermission, useAuth } from './useAuth';
import { useToast } from './useToast';
import { useMentionStakeholders } from './useMentionStakeholders';
import type {
  BusinessCaseAppendDetail,
  BusinessCaseDetail,
  BusinessCaseSummary,
  BusinessCaseWorkflowFormProjection,
  BusinessCaseWorkflowReceiptProjection,
  CaseWorkflowFormsResponse,
  SubmitWorkflowFormRequest,
  SubmitWorkflowFormResponse,
  WorkflowTaskFormView,
} from '../types/backend';
import { useFlightCaseAi } from './useFlightCaseAi';
import { useFlightCaseWorkflowForms } from './useFlightCaseWorkflowForms';
import type { Stakeholder } from './useMentionStakeholders';
import {
  DEFAULT_CASE_STATUS_OPTIONS,
  type BusinessCaseStatusMetadata,
  getCaseReceiptProjection,
  getCaseStatusDraftValue,
  getCaseStatusOption,
  getCaseWorkflowFormProjections,
  normalizeCaseStatusMetadataOption,
  normalizeCaseStatusValue,
} from '../components/flight-monitor/detail/businessCaseHelpers';

interface ApiEnvelope<T> {
  success?: boolean;
  data?: T;
  message?: string;
}

export interface FlightBusinessCaseContext {
  flight: Ref<Flight | null>;
  caseFilter: Ref<'all' | string>;
  caseStatusOptions: Ref<BusinessCaseStatusMetadata[]>;
  caseStatusMetadataError: Ref<string>;
  filteredCases: Ref<BusinessCaseSummary[]>;
  activeCaseId: Ref<string | null>;
  activeCaseData: Ref<BusinessCaseDetail | null>;
  caseDetailLoading: Ref<boolean>;
  activeCaseWorkflowForms: Ref<CaseWorkflowFormsResponse | null>;
  workflowFormsLoading: Ref<boolean>;
  workflowFormsError: Ref<string>;
  workflowFormSubmittingCode: Ref<string | null>;
  caseStatusDraft: Ref<string>;
  caseStatusSaving: Ref<boolean>;
  appendContent: Ref<string>;
  appendMentionIds: Ref<string[]>;
  appendSubmitting: Ref<boolean>;
  canManageBusinessCases: Ref<boolean>;
  canAttemptActiveCaseStatusEdit: Ref<boolean>;
  showCaseStatusPermissionHint: Ref<boolean>;
  activeCaseVisibility: Ref<{ scopeLabel: string; isCommon: boolean; departmentName?: string | null }>;
  activeCaseStatusValue: Ref<string>;
  activeCaseStatusOptions: Ref<{ value: string; label: string }[]>;
  activeCaseWorkflowProjectionEntries: Ref<BusinessCaseWorkflowFormProjection[]>;
  activeCaseReceipt: Ref<BusinessCaseWorkflowReceiptProjection | null>;
  activeCaseAppendEntries: Ref<BusinessCaseAppendDetail[]>;
  activeCaseThreadTotal: Ref<number>;
  activeCaseHasWorkflowPanel: Ref<boolean>;
  mentionCandidates: Ref<Stakeholder[]>;
  getCurrentUserId: () => string;
  diagnosisResult: Ref<{ summary?: string; recommendations?: string[]; details?: string } | null>;
  diagnosisLoading: Ref<boolean>;
  journeyResult: Ref<{ summary?: string; details?: string } | null>;
  journeyLoading: Ref<boolean>;
  reportResult: Ref<{ summary?: string; details?: string } | null>;
  reportLoading: Ref<boolean>;
  openCaseDetail: (caseId: string | number) => Promise<void>;
  closeCaseDetail: () => void;
  submitCaseStatusUpdate: () => Promise<void>;
  submitAppend: () => Promise<void>;
  submitWorkflowForm: (form: WorkflowTaskFormView, payload: Record<string, unknown>) => Promise<void>;
  acknowledgeAppend: (entry: BusinessCaseAppendDetail) => Promise<void>;
  runAiDiagnosis: () => Promise<void>;
  runAiEventJourney: () => Promise<void>;
  runHistoryReport: () => Promise<void>;
  getCachedCaseWorkflowForms: (caseId: string | null | undefined) => CaseWorkflowFormsResponse | null | undefined;
  hasLoadedCaseWorkflowForms: (caseId: string | null | undefined) => boolean;
  loadCaseWorkflowForms: (caseId: string, options?: { force?: boolean }) => Promise<CaseWorkflowFormsResponse | null>;
}

export const flightBusinessCaseKey: InjectionKey<FlightBusinessCaseContext> = Symbol('flightBusinessCases');

export function useFlightBusinessCases(flightRef: Ref<Flight | null>): FlightBusinessCaseContext {
  const api = useApi();
  const auth = useAuth();
  const toast = useToast();

  const flight = flightRef;
  const caseFilter = ref<'all' | string>('all');
  const caseStatusOptions = ref<BusinessCaseStatusMetadata[]>(DEFAULT_CASE_STATUS_OPTIONS);
  const caseStatusMetadataError = ref('');

  const activeCaseId = ref<string | null>(null);
  const workflowForms = useFlightCaseWorkflowForms(activeCaseId);
  const activeCaseData = ref<BusinessCaseDetail | null>(null);
  const caseDetailLoading = ref(false);
  const workflowFormSubmittingCode = ref<string | null>(null);
  const caseStatusDraft = ref('PENDING');
  const caseStatusSaving = ref(false);
  const appendContent = ref('');
  const appendMentionIds = ref<string[]>([]);
  const appendSubmitting = ref(false);

  const currentUser = computed(() => auth.getUser());
  const isAuthenticated = computed(() => auth.isAuthenticated());
  const canManageBusinessCases = computed(() => Boolean(
    hasUserPermission(currentUser.value, [
      'business_case.update',
      'business_case.status_transition',
      'business_case.delete',
      'business_case.*',
    ]),
  ));
  const canAttemptActiveCaseStatusEdit = computed(() => Boolean(activeCaseData.value && isAuthenticated.value));
  const showCaseStatusPermissionHint = computed(() => canAttemptActiveCaseStatusEdit.value && !canManageBusinessCases.value);

  const filteredCases = computed<BusinessCaseSummary[]>(() => {
    const cases: BusinessCaseSummary[] = (flight.value?.business_cases as BusinessCaseSummary[]) || [];
    const sorted = [...cases].sort(
      (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
    );
    if (caseFilter.value === 'all') return sorted;
    return sorted.filter((c) => normalizeCaseStatusValue(c.status) === caseFilter.value);
  });

  async function loadBusinessCaseStatusMetadata(): Promise<void> {
    try {
      const { get } = api;
      const result = await get<ApiEnvelope<BusinessCaseStatusMetadata[]>>('/api/v2/reference/business-case-statuses');
      if (!result.ok || !result.data?.success || !Array.isArray(result.data.data)) {
        throw new Error(result.data?.message || `HTTP ${result.status}`);
      }
      const options = result.data.data
        .map((item) => normalizeCaseStatusMetadataOption(item))
        .filter((item): item is BusinessCaseStatusMetadata => Boolean(item));
      if (options.length <= 0) {
        throw new Error('empty status metadata');
      }
      caseStatusOptions.value = options;
      caseStatusMetadataError.value = '';
    } catch (err) {
      console.error('Failed to load business case status metadata:', err);
      caseStatusOptions.value = DEFAULT_CASE_STATUS_OPTIONS;
      caseStatusMetadataError.value = '状态元数据加载失败，已使用本地兜底配置。';
    }
  }

  onMounted(() => {
    void loadBusinessCaseStatusMetadata();
  });

  function applyCaseSummaryUpdate(updatedCase: BusinessCaseSummary | BusinessCaseDetail): void {
    const flightCases = flight.value?.business_cases;
    if (!Array.isArray(flightCases)) {
      return;
    }
    const targetIndex = flightCases.findIndex((item) => String((item as BusinessCaseSummary)?.case_id || '') === updatedCase.case_id);
    if (targetIndex === -1) {
      return;
    }
    flightCases[targetIndex] = {
      ...(flightCases[targetIndex] as BusinessCaseSummary),
      ...updatedCase,
    } as BusinessCaseSummary;
  }

  watch(
    () => flight.value?.business_cases,
    (cases) => {
      if (!activeCaseId.value || !Array.isArray(cases) || !activeCaseData.value) {
        return;
      }
      const summary = cases.find((item) => String((item as BusinessCaseSummary)?.case_id || '') === activeCaseId.value);
      if (!summary) {
        return;
      }
      const previousStatus = getCaseStatusDraftValue(activeCaseData.value.status);
      const nextStatus = getCaseStatusDraftValue(summary.status);
      activeCaseData.value = {
        ...activeCaseData.value,
        context: summary.context,
        append_count: summary.append_count,
        latest_append: summary.latest_append,
        status: summary.status,
        updated_by: summary.updated_by,
        finished_at: summary.finished_at,
        cancelled_at: summary.cancelled_at,
      };
      if (!caseStatusSaving.value && caseStatusDraft.value === previousStatus) {
        caseStatusDraft.value = nextStatus;
      }
    },
    { deep: true },
  );

  watch(
    () => ((flight.value?.business_cases as BusinessCaseSummary[] | undefined) || []).map((item) => String(item.case_id || '')).join('|'),
    () => {
      const cases = (flight.value?.business_cases as BusinessCaseSummary[]) || [];
      cases.forEach((item) => {
        const caseId = String(item.case_id || '').trim();
        if (caseId && !workflowForms.hasLoadedCaseWorkflowForms(caseId)) {
          void workflowForms.loadCaseWorkflowForms(caseId);
        }
      });
    },
    { immediate: true },
  );

  const currentFlightId = computed(() => (flight.value?.flight_id ? String(flight.value.flight_id) : null));
  const { stakeholders: mentionCandidates } = useMentionStakeholders(currentFlightId);
  const ai = useFlightCaseAi(flight);

  const activeCaseVisibility = computed(() => getBusinessCaseVisibilityInfo(activeCaseData.value));
  const activeCaseStatusValue = computed(() => getCaseStatusDraftValue(activeCaseData.value?.status));
  const editableCaseStatusOptions = computed(() =>
    caseStatusOptions.value.filter((option: BusinessCaseStatusMetadata) => option.manual_transition_enabled !== false),
  );
  const activeCaseStatusOptions = computed(() => {
    const currentStatus = activeCaseStatusValue.value;
    const options = editableCaseStatusOptions.value.map((option: BusinessCaseStatusMetadata) => ({ value: option.value, label: option.label }));
    if (currentStatus && !options.some((option: { value: string; label: string }) => option.value === currentStatus)) {
      return [
        { value: currentStatus, label: getCaseStatusOption(currentStatus, caseStatusOptions.value)?.label ?? currentStatus },
        ...options,
      ];
    }
    return options;
  });

  const activeCaseWorkflowProjectionEntries = computed(() => {
    const projections = getCaseWorkflowFormProjections(activeCaseData.value);
    const activeCodes = new Set((workflowForms.activeCaseWorkflowForms.value?.forms || []).map((form) => form.form_code));
    return projections.filter((projection) => !activeCodes.has(projection.form_code));
  });

  const activeCaseReceipt = computed(() => getCaseReceiptProjection(activeCaseData.value));
  const activeCaseAppendEntries = computed<BusinessCaseAppendDetail[]>(() =>
    [...(activeCaseData.value?.append_entries || [])].sort(
      (a, b) => new Date(a.appended_at).getTime() - new Date(b.appended_at).getTime(),
    ),
  );
  const activeCaseThreadTotal = computed(() => 1 + activeCaseAppendEntries.value.length);
  const activeCaseHasWorkflowPanel = computed(() =>
    workflowForms.workflowFormsLoading.value
    || (workflowForms.activeCaseWorkflowForms.value?.forms || []).length > 0
    || activeCaseWorkflowProjectionEntries.value.length > 0,
  );

  async function openCaseDetail(caseId: string | number) {
    const id = String(caseId || '').trim();
    if (!id) return;

    const previousId = String(activeCaseId.value || '').trim();
    const isSameCase = previousId === id;
    activeCaseId.value = id;
    caseDetailLoading.value = true;

    // 切换事项时清空并走骨架；同事项重开保留旧内容，避免整页闪「加载中」框。
    if (!isSameCase) {
      activeCaseData.value = null;
      workflowForms.activeCaseWorkflowForms.value = null;
      workflowForms.workflowFormsError.value = '';
      workflowFormSubmittingCode.value = null;
      caseStatusDraft.value = 'PENDING';
      appendContent.value = '';
      appendMentionIds.value = [];
    }

    try {
      const { get } = api;
      const [detailResult] = await Promise.all([
        get<ApiEnvelope<BusinessCaseDetail>>(`/api/v2/business-cases/${encodeURIComponent(id)}`),
        workflowForms.loadCaseWorkflowForms(id, { force: true }),
      ]);
      // Ignore stale responses if user closed/switched cases while loading.
      if (String(activeCaseId.value || '').trim() !== id) {
        return;
      }
      if (detailResult.ok && detailResult.data?.data) {
        activeCaseData.value = detailResult.data.data;
        caseStatusDraft.value = getCaseStatusDraftValue(detailResult.data.data.status);
      } else {
        toast.showToast('error', `业务事项详情加载失败 (${detailResult.status})`, { duration: 5000 });
        // Match legacy: close modal when detail load fails.
        closeCaseDetail();
      }
    } catch (err) {
      console.error('Failed to load case detail:', err);
      toast.showToast('error', `业务事项详情加载失败: ${err instanceof Error ? err.message : String(err)}`, { duration: 5000 });
      if (String(activeCaseId.value || '').trim() === id) {
        closeCaseDetail();
      }
    } finally {
      if (String(activeCaseId.value || '').trim() === id) {
        caseDetailLoading.value = false;
      }
    }
  }

  function closeCaseDetail() {
    activeCaseId.value = null;
    activeCaseData.value = null;
    caseDetailLoading.value = false;
    workflowForms.activeCaseWorkflowForms.value = null;
    workflowForms.workflowFormsError.value = '';
    workflowFormSubmittingCode.value = null;
    caseStatusDraft.value = 'PENDING';
    appendContent.value = '';
    appendMentionIds.value = [];
  }

  async function submitCaseStatusUpdate() {
    if (!activeCaseId.value || !activeCaseData.value) {
      return;
    }

    caseStatusSaving.value = true;
    try {
      const result = await updateBusinessCaseStatusRequest(activeCaseId.value, caseStatusDraft.value, {
        apiBase: auth.apiBase.value,
        authFetch: auth.fetch,
      });
      if (result?.data) {
        activeCaseData.value = result.data as BusinessCaseDetail;
        caseStatusDraft.value = getCaseStatusDraftValue(activeCaseData.value.status);
        applyCaseSummaryUpdate(activeCaseData.value);
      }
    } catch (err) {
      console.error('Status update failed:', err);
      toast.showToast('error', `事项状态更新失败: ${err instanceof Error ? err.message : String(err)}`, { duration: 5000 });
    } finally {
      caseStatusSaving.value = false;
    }
  }

  async function submitAppend() {
    if (!activeCaseId.value || !appendContent.value.trim()) return;

    appendSubmitting.value = true;
    try {
      const body: { content: string; mention_user_ids?: string[] } = {
        content: appendContent.value.trim(),
      };
      if (appendMentionIds.value.length > 0) {
        body.mention_user_ids = appendMentionIds.value;
      }

      const result = await appendBusinessCase(activeCaseId.value, body, {
        apiBase: auth.apiBase.value,
        authFetch: auth.fetch,
      });

      if (result?.data) {
        activeCaseData.value = result.data as BusinessCaseDetail;
        applyCaseSummaryUpdate(result.data as BusinessCaseDetail);
        appendContent.value = '';
        appendMentionIds.value = [];
      }
    } catch (err) {
      console.error('Append failed:', err);
      toast.showToast('error', `事项补充提交失败: ${err instanceof Error ? err.message : String(err)}`, { duration: 5000 });
    } finally {
      appendSubmitting.value = false;
    }
  }

  async function submitWorkflowForm(form: WorkflowTaskFormView, payload: Record<string, unknown>) {
    if (!activeCaseId.value) {
      return;
    }

    workflowFormSubmittingCode.value = form.form_code;
    workflowForms.workflowFormsError.value = '';

    try {
      const { post } = api;
      const body: SubmitWorkflowFormRequest = {
        task_id: form.task_id,
        data: payload,
      };
      const result = await post<ApiEnvelope<SubmitWorkflowFormResponse>>(
        `/api/v2/business_cases/${encodeURIComponent(activeCaseId.value)}/workflow/forms/${encodeURIComponent(form.form_code)}/submit`,
        body,
      );

      if (result.ok && result.data?.data) {
        const returnedCase = result.data.data.business_case;
        if (returnedCase && typeof returnedCase === 'object' && 'case_id' in returnedCase) {
          applyCaseSummaryUpdate(returnedCase as BusinessCaseSummary | BusinessCaseDetail);
        }
        await openCaseDetail(activeCaseId.value);
        return;
      }

      workflowForms.workflowFormsError.value = String(result.data?.message || '流程表单提交失败');
    } catch (err) {
      workflowForms.workflowFormsError.value = err instanceof Error ? err.message : '流程表单提交失败';
      console.error('Workflow form submit failed:', err);
    } finally {
      workflowFormSubmittingCode.value = null;
    }
  }

  async function acknowledgeAppend(entry: BusinessCaseAppendDetail) {
    if (!activeCaseId.value || !entry?.append_id) return;

    try {
      const result = await acknowledgeBusinessCaseAppend(activeCaseId.value, entry.append_id, {
        apiBase: auth.apiBase.value,
        authFetch: auth.fetch,
      });

      if (result?.data) {
        await openCaseDetail(activeCaseId.value);
      }
    } catch (err) {
      console.error('Acknowledge failed:', err);
      toast.showToast('error', `补充信息确认失败: ${err instanceof Error ? err.message : String(err)}`, { duration: 5000 });
    }
  }

  function getCurrentUserId(): string {
    try {
      const user = auth.getUser();
      return String(user?.user_id || user?.username || user?.sub || '').trim();
    } catch {
      return '';
    }
  }

  const context: FlightBusinessCaseContext = {
    flight,
    caseFilter,
    caseStatusOptions,
    caseStatusMetadataError,
    filteredCases,
    activeCaseId,
    activeCaseData,
    caseDetailLoading,
    activeCaseWorkflowForms: workflowForms.activeCaseWorkflowForms,
    workflowFormsLoading: workflowForms.workflowFormsLoading,
    workflowFormsError: workflowForms.workflowFormsError,
    workflowFormSubmittingCode,
    caseStatusDraft,
    caseStatusSaving,
    appendContent,
    appendMentionIds,
    appendSubmitting,
    canManageBusinessCases,
    canAttemptActiveCaseStatusEdit,
    showCaseStatusPermissionHint,
    activeCaseVisibility,
    activeCaseStatusValue,
    activeCaseStatusOptions,
    activeCaseWorkflowProjectionEntries,
    activeCaseReceipt,
    activeCaseAppendEntries,
    activeCaseThreadTotal,
    activeCaseHasWorkflowPanel,
    mentionCandidates,
    getCurrentUserId,
    openCaseDetail,
    closeCaseDetail,
    submitCaseStatusUpdate,
    submitAppend,
    submitWorkflowForm,
    acknowledgeAppend,
    getCachedCaseWorkflowForms: workflowForms.getCachedCaseWorkflowForms,
    hasLoadedCaseWorkflowForms: workflowForms.hasLoadedCaseWorkflowForms,
    loadCaseWorkflowForms: workflowForms.loadCaseWorkflowForms,
    ...ai,
  };

  return context;
}
