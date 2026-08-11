import type { Ref } from 'vue';
import { ref } from 'vue';
import { useApi } from './useApi';
import type { CaseWorkflowFormsResponse } from '../types/backend';

interface ApiEnvelope<T> {
  success?: boolean;
  data?: T;
  message?: string;
}

export interface FlightCaseWorkflowFormsState {
  activeCaseWorkflowForms: Ref<CaseWorkflowFormsResponse | null>;
  workflowFormsLoading: Ref<boolean>;
  workflowFormsError: Ref<string>;
  getCachedCaseWorkflowForms: (caseId: string | null | undefined) => CaseWorkflowFormsResponse | null | undefined;
  hasLoadedCaseWorkflowForms: (caseId: string | null | undefined) => boolean;
  loadCaseWorkflowForms: (caseId: string, options?: { force?: boolean }) => Promise<CaseWorkflowFormsResponse | null>;
}

export function useFlightCaseWorkflowForms(activeCaseId: Ref<string | null>): FlightCaseWorkflowFormsState {
  const api = useApi();

  const activeCaseWorkflowForms = ref<CaseWorkflowFormsResponse | null>(null);
  const workflowFormsByCaseId = ref<Record<string, CaseWorkflowFormsResponse | null>>({});
  const workflowFormsLoadedByCaseId = ref<Record<string, boolean>>({});
  const workflowFormsLoading = ref(false);
  const workflowFormsError = ref('');

  function getCachedCaseWorkflowForms(caseId: string | null | undefined): CaseWorkflowFormsResponse | null | undefined {
    const normalizedId = String(caseId || '').trim();
    if (!normalizedId) {
      return undefined;
    }
    return workflowFormsByCaseId.value[normalizedId];
  }

  function hasLoadedCaseWorkflowForms(caseId: string | null | undefined): boolean {
    const normalizedId = String(caseId || '').trim();
    return Boolean(normalizedId && workflowFormsLoadedByCaseId.value[normalizedId]);
  }

  async function loadCaseWorkflowForms(
    caseId: string,
    options: { force?: boolean } = {},
  ): Promise<CaseWorkflowFormsResponse | null> {
    if (!options.force && hasLoadedCaseWorkflowForms(caseId)) {
      const cached = getCachedCaseWorkflowForms(caseId) || null;
      if (activeCaseId.value === caseId) {
        activeCaseWorkflowForms.value = cached;
        workflowFormsError.value = '';
      }
      return cached;
    }

    if (activeCaseId.value === caseId) {
      workflowFormsLoading.value = true;
      workflowFormsError.value = '';
    }

    try {
      const { get } = api;
      const result = await get<ApiEnvelope<CaseWorkflowFormsResponse>>(
        `/api/v2/business_cases/${encodeURIComponent(caseId)}/workflow/forms`,
      );

      if (result.ok && result.data?.data) {
        workflowFormsByCaseId.value = {
          ...workflowFormsByCaseId.value,
          [caseId]: result.data.data,
        };
        workflowFormsLoadedByCaseId.value = {
          ...workflowFormsLoadedByCaseId.value,
          [caseId]: true,
        };
        if (activeCaseId.value === caseId) {
          activeCaseWorkflowForms.value = result.data.data;
        }
        return result.data.data;
      }

      const errorMessage = String(result.data?.message || '加载流程表单失败');
      if (activeCaseId.value === caseId) {
        workflowFormsError.value = errorMessage;
        activeCaseWorkflowForms.value = null;
      }
      return null;
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : '加载流程表单失败';
      if (activeCaseId.value === caseId) {
        workflowFormsError.value = errorMessage;
        activeCaseWorkflowForms.value = null;
      }
      console.error('Failed to load workflow forms:', err);
      return null;
    } finally {
      if (activeCaseId.value === caseId) {
        workflowFormsLoading.value = false;
      }
    }
  }

  return {
    activeCaseWorkflowForms,
    workflowFormsLoading,
    workflowFormsError,
    getCachedCaseWorkflowForms,
    hasLoadedCaseWorkflowForms,
    loadCaseWorkflowForms,
  };
}
