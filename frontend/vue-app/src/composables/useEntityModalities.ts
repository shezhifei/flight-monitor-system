/**
 * 按实体拉取能力快照中的输入模态与 MIME 白名单，
 * 供聊天输入的 accept 与该实体所用模型对齐。
 *
 * 数据源：GET /api/v2/ai/entities/{id}/capabilities。
 */
import { ref, computed } from 'vue';
import { useAiConfigApi } from '../pages/ai_config_center/aiConfigApi';
import { deriveInputAccept } from '../utils/aiInputAccept';
import type { ModalityType } from '../pages/ai_config_center/aiConfigTypes';

export function useEntityModalities() {
  const api = useAiConfigApi();

  const inputModalities = ref<ModalityType[]>(['text']);
  const allowedInputMimeTypes = ref<string[]>([]);
  const loading = ref(false);
  const error = ref('');
  const loaded = ref(false);

  // 直接派生的 accept（供需要 <input accept> 的调用方使用）。
  const acceptDerivation = computed(() =>
    deriveInputAccept(inputModalities.value, allowedInputMimeTypes.value),
  );

  async function fetchModalities(entityId: string): Promise<void> {
    if (!entityId) return;
    loading.value = true;
    error.value = '';
    try {
      const snapshot = await api.getEntityCapabilities(entityId);
      inputModalities.value = snapshot.input_modalities?.length
        ? snapshot.input_modalities
        : (['text'] as ModalityType[]);
      allowedInputMimeTypes.value = snapshot.security?.allowed_input_mime_types ?? [];
      loaded.value = true;
    } catch (err) {
      // 失败时退回纯文本（最安全的降级：不暴露上传入口）。
      inputModalities.value = ['text'] as ModalityType[];
      allowedInputMimeTypes.value = [];
      error.value = err instanceof Error ? err.message : '能力快照加载失败';
      console.warn('Failed to fetch entity modalities:', err);
    } finally {
      loading.value = false;
    }
  }

  return {
    inputModalities,
    allowedInputMimeTypes,
    acceptDerivation,
    loading,
    error,
    loaded,
    fetchModalities,
  };
}
