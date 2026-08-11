/**
 * 按 entity 拉取「模型能力快照」中的输入模态与允许的 MIME 类型，
 * 直接喂给 <AiChatModal> 的 inputModalities / allowedInputMimeTypes props，
 * 从而让聊天输入的文件 accept 白名单与该实体所用 LLM 的模态范围对齐。
 *
 * 数据源：GET /api/v2/ai/entities/{id}/capabilities（已含 input_modalities + security）。
 * 这是 P0b/P1a 的连接件：后端已提供数据，此处把它接到任意聊天界面。
 */
import { ref, computed } from 'vue';
import { useAiConfigApiV2 } from '../pages/ai_config_center/aiConfigApiV2';
import { deriveInputAccept } from '../utils/aiInputAccept';
import type { ModalityType } from '../pages/ai_config_center/aiConfigTypesV2';

export function useEntityModalities() {
  const api = useAiConfigApiV2();

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
