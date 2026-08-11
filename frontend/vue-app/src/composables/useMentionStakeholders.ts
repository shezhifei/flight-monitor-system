import { ref, watch, type Ref } from 'vue';
import { useApi } from './useApi';

export interface Stakeholder {
  user_id: string;
  username: string;
  is_assignee: boolean;
  is_dispatcher: boolean;
}

/**
 * Fetches and caches the list of chat group members for a flight.
 * Used to populate @-mention autocomplete in business case appends.
 */
export function useMentionStakeholders(flightId: Ref<string | null | undefined>) {
  const stakeholders = ref<Stakeholder[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  const { get } = useApi();

  async function fetchStakeholders(id: string) {
    if (!id) {
      stakeholders.value = [];
      return;
    }

    loading.value = true;
    error.value = null;

    try {
      const result = await get<{ items: Stakeholder[]; flight_id: string }>(
        `/api/v2/dispatch/collaboration/flights/${encodeURIComponent(id)}/stakeholders`,
      );

      if (result.ok && result.data?.items) {
        stakeholders.value = result.data.items;
      } else {
        error.value = '加载群聊成员失败';
        stakeholders.value = [];
      }
    } catch (err) {
      error.value = err instanceof Error ? err.message : '加载群聊成员失败';
      stakeholders.value = [];
    } finally {
      loading.value = false;
    }
  }

  watch(
    flightId,
    (newId) => {
      if (newId) {
        fetchStakeholders(String(newId));
      } else {
        stakeholders.value = [];
      }
    },
    { immediate: true },
  );

  function filterByKeyword(keyword: string): Stakeholder[] {
    const normalized = String(keyword || '').trim().toLowerCase();
    if (!normalized) return stakeholders.value;
    return stakeholders.value.filter(
      (s) =>
        s.username.toLowerCase().includes(normalized) ||
        s.user_id.toLowerCase().includes(normalized),
    );
  }

  return {
    stakeholders,
    loading,
    error,
    filterByKeyword,
    refetch: () => fetchStakeholders(String(flightId.value || '')),
  };
}
