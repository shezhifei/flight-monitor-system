import { ref, computed } from 'vue';
import type { Ref } from 'vue';
import { useApi } from '@/composables/useApi';
import type { LabelDefinition, CreateLabelRequest, UpdateLabelRequest } from '@/types/backend';

interface UseLabelManagerOptions {
  autoLoad?: boolean;
}

export interface UseLabelManagerReturn {
  labels: Readonly<Ref<LabelDefinition[]>>;
  filteredLabels: Readonly<Ref<LabelDefinition[]>>;
  loading: Readonly<Ref<boolean>>;
  error: Readonly<Ref<string | null>>;
  searchQuery: Ref<string>;
  scopeFilter: Ref<string>;
  categoryFilter: Ref<string>;
  createLabel: (data: CreateLabelRequest) => Promise<void>;
  updateLabel: (id: string, data: UpdateLabelRequest) => Promise<void>;
  deleteLabel: (id: string) => Promise<void>;
  refreshLabels: () => Promise<void>;
}

export function useLabelManager(options: UseLabelManagerOptions = {}): UseLabelManagerReturn {
  const { autoLoad = true } = options;
  const api = useApi();

  const labels = ref<LabelDefinition[]>([]) as Ref<LabelDefinition[]>;
  const loading = ref(false);
  const error = ref<string | null>(null);
  const searchQuery = ref('');
  const scopeFilter = ref('');
  const categoryFilter = ref('');

  function unwrapData<T>(payload: unknown): T | null {
    if (payload && typeof payload === 'object' && 'data' in payload) {
      return (payload as { data?: T }).data ?? null;
    }
    return payload as T;
  }

  function readErrorMessage(payload: unknown, fallback: string): string {
    if (payload && typeof payload === 'object') {
      const obj = payload as { detail?: unknown; message?: unknown; error?: unknown };
      if (typeof obj.detail === 'string' && obj.detail.trim()) return obj.detail;
      if (typeof obj.message === 'string' && obj.message.trim()) return obj.message;
      if (typeof obj.error === 'string' && obj.error.trim()) return obj.error;
    }
    return fallback;
  }

  const filteredLabels = computed(() => {
    let result = labels.value;

    if (searchQuery.value) {
      const query = searchQuery.value.toLowerCase();
      result = result.filter(
        (l) =>
          l.code.toLowerCase().includes(query) ||
          l.name.toLowerCase().includes(query)
      );
    }

    if (scopeFilter.value) {
      result = result.filter(
        (l) => l.scope === scopeFilter.value || l.scope === 'both'
      );
    }

    if (categoryFilter.value) {
      result = result.filter((l) => l.category === categoryFilter.value);
    }

    return result;
  });

  async function fetchLabels(): Promise<void> {
    loading.value = true;
    error.value = null;

    try {
      const response = await api.get<unknown>('/api/v2/labels?active_only=false');
      if (!response.ok) {
        throw new Error(readErrorMessage(response.data, `加载标签失败 (${response.status})`));
      }

      const data = unwrapData<LabelDefinition[]>(response.data);
      labels.value = Array.isArray(data) ? data : [];
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to load labels';
      console.error('Failed to load labels:', e);
    } finally {
      loading.value = false;
    }
  }

  async function createLabel(data: CreateLabelRequest): Promise<void> {
    loading.value = true;
    error.value = null;

    try {
      const response = await api.post<unknown>('/api/v2/labels', data);
      if (!response.ok) {
        throw new Error(readErrorMessage(response.data, `创建标签失败 (${response.status})`));
      }

      await fetchLabels();
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to create label';
      throw e;
    } finally {
      loading.value = false;
    }
  }

  async function updateLabel(id: string, data: UpdateLabelRequest): Promise<void> {
    loading.value = true;
    error.value = null;

    try {
      const response = await api.put<unknown>(`/api/v2/labels/${encodeURIComponent(id)}`, data);
      if (!response.ok) {
        throw new Error(readErrorMessage(response.data, `更新标签失败 (${response.status})`));
      }

      await fetchLabels();
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to update label';
      throw e;
    } finally {
      loading.value = false;
    }
  }

  async function deleteLabel(id: string): Promise<void> {
    loading.value = true;
    error.value = null;

    try {
      const response = await api.delete<unknown>(`/api/v2/labels/${encodeURIComponent(id)}`);
      if (!response.ok) {
        throw new Error(readErrorMessage(response.data, `删除标签失败 (${response.status})`));
      }

      await fetchLabels();
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to delete label';
      throw e;
    } finally {
      loading.value = false;
    }
  }

  async function refreshLabels(): Promise<void> {
    await fetchLabels();
  }

  if (autoLoad) {
    fetchLabels();
  }

  return {
    labels,
    filteredLabels,
    loading,
    error,
    searchQuery,
    scopeFilter,
    categoryFilter,
    createLabel,
    updateLabel,
    deleteLabel,
    refreshLabels,
  };
}
