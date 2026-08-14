import { ref, computed } from 'vue';
import { useApi } from './useApi';

interface AiCapabilitiesData {
  ai_ready: boolean;
  ai_execute_permission: boolean;
  ai_chat_permission: boolean;
  missing_reasons: string[];
}

const capabilities = ref<AiCapabilitiesData>({
  ai_ready: false,
  ai_execute_permission: false,
  ai_chat_permission: false,
  missing_reasons: [],
});
const loaded = ref(false);
const loading = ref(false);
const error = ref('');

export function useAiCapabilities() {
  const api = useApi();

  const canGenerateDraft = computed(() =>
    loaded.value && capabilities.value.ai_ready && capabilities.value.ai_execute_permission
  );

  const canUseChat = computed(() =>
    loaded.value && capabilities.value.ai_ready && capabilities.value.ai_chat_permission
  );

  const missingReasonLabels: Record<string, string> = {
    NO_AI_CONFIG: '未配置可用 AI 模型',
    NO_AI_EXECUTE_PERMISSION: '当前账号缺少 ai:execute 权限',
    NO_AI_CHAT_PERMISSION: '当前账号缺少 ai:chat 权限',
  };

  const missingLabel = computed(() => {
    if (!capabilities.value.missing_reasons.length) return '';
    return capabilities.value.missing_reasons
      .map(r => missingReasonLabels[r] || r)
      .join('；');
  });

  async function fetchCapabilities() {
    loading.value = true;
    error.value = '';
    try {
      const res = await api.get<{ success?: boolean; data?: AiCapabilitiesData }>('/api/v2/ai/capabilities');
      const payload = res.data?.data;
      if (res.ok && payload && typeof payload === 'object') {
        capabilities.value = {
          ai_ready: Boolean(payload.ai_ready),
          ai_execute_permission: Boolean(payload.ai_execute_permission),
          ai_chat_permission: Boolean(payload.ai_chat_permission),
          missing_reasons: Array.isArray(payload.missing_reasons) ? payload.missing_reasons : [],
        };
      } else {
        error.value = `AI 能力暂不可用 (${res.status || 'error'})`;
      }
    } catch (err) {
      error.value = err instanceof Error ? err.message : 'AI 能力加载失败';
      console.warn('Failed to fetch AI capabilities:', err);
    } finally {
      loaded.value = true;
      loading.value = false;
    }
  }

  return {
    capabilities,
    loaded,
    loading,
    error,
    canGenerateDraft,
    canUseChat,
    missingLabel,
    fetchCapabilities,
  };
}
