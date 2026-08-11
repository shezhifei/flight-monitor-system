import { ref } from 'vue';
import { useApi } from './useApi';
import { useToast } from './useToast';

export interface PendingAction {
  actionId: string;
  toolName: string;
  status: 'pending' | 'approved' | 'rejected' | 'expired' | 'executed';
  message?: string;
  createdAt?: string;
  expiresAt?: string;
  toolArguments?: unknown;
}

interface ApiErrorPayload {
  message?: string;
  detail?: string;
  error?: string | { message?: string };
}

function extractApiError(data: unknown, fallback: string): string {
  if (data && typeof data === 'object') {
    const payload = data as ApiErrorPayload;
    if (typeof payload.message === 'string' && payload.message.trim()) return payload.message;
    if (typeof payload.detail === 'string' && payload.detail.trim()) return payload.detail;
    if (typeof payload.error === 'string' && payload.error.trim()) return payload.error;
    if (payload.error && typeof payload.error === 'object' && typeof payload.error.message === 'string') {
      return payload.error.message;
    }
  }
  return fallback;
}

export function usePendingActions() {
  const api = useApi();
  const toast = useToast();
  const actions = ref<PendingAction[]>([]);
  const loading = ref(false);

  async function fetchPending() {
    loading.value = true;
    try {
      const res = await api.get<{ items?: Array<Record<string, unknown>> }>(
        '/api/v2/ai/pending-actions?status=pending&limit=50&offset=0',
      );
      if (!res.ok) {
        toast.showToast('error', '待办动作加载失败');
        return;
      }
      const items = res.data?.items ?? [];
      actions.value = items.map((item) => ({
        actionId: String(item.action_id ?? ''),
        toolName: String(item.tool_name ?? ''),
        status: (item.status as PendingAction['status']) ?? 'pending',
        message: typeof item.message === 'string' ? item.message : undefined,
        createdAt: typeof item.created_at === 'string' ? item.created_at : undefined,
        expiresAt: typeof item.expires_at === 'string' ? item.expires_at : undefined,
        toolArguments: item.tool_arguments,
      }));
    } finally { loading.value = false; }
  }

  async function approve(actionId: string) {
    const res = await api.post(`/api/v2/ai/pending-actions/${encodeURIComponent(actionId)}/approve`);
    if (res.ok) {
      actions.value = actions.value.filter(a => a.actionId !== actionId);
      toast.showToast('success', '已批准');
    } else {
      toast.showToast('error', extractApiError(res.data, `批准失败 (${res.status})`));
    }
    return res;
  }

  async function reject(actionId: string, reason?: string) {
    const res = await api.post(`/api/v2/ai/pending-actions/${encodeURIComponent(actionId)}/reject`, { reason });
    if (res.ok) {
      actions.value = actions.value.filter(a => a.actionId !== actionId);
      toast.showToast('info', '已拒绝');
    } else {
      toast.showToast('error', extractApiError(res.data, `拒绝失败 (${res.status})`));
    }
    return res;
  }

  function addAction(action: PendingAction) {
    if (!actions.value.find(a => a.actionId === action.actionId)) {
      actions.value.push(action);
    }
  }

  function removeAction(actionId: string) {
    actions.value = actions.value.filter(a => a.actionId !== actionId);
  }

  return { actions, loading, fetchPending, approve, reject, addAction, removeAction };
}
