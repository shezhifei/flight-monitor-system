<script setup lang="ts">
import CapabilityStatusPanel from '../CapabilityStatusPanel.vue';
import type {
  EnrichedCapabilitySnapshot,
  ValidationResult,
  CacheMetricsSummary,
} from '../aiConfigTypesV2';
import type { ModelsTabForm, NormalizedModelOption } from '../composables/useAiConfigCenter';
import SvgIcon from '../../../components/ui/SvgIcon.vue';

defineProps<{
  modelsForm: ModelsTabForm;
  modelsLoading: boolean;
  modelsTesting: boolean;
  modelOptions: NormalizedModelOption[];
  modalityOptions: { value: string; label: string }[];
  selectedEntityId: string;
  capabilitySnapshot: EnrichedCapabilitySnapshot | null;
  capabilityValidation: ValidationResult | null;
  cacheMetrics: CacheMetricsSummary | null;
  capabilityLoading: boolean;
}>();
const emit = defineEmits<{
  testConnection: [];
  validateCapability: [];
  toggleInputModality: [value: string, enabled: boolean];
  toggleOutputModality: [value: string, enabled: boolean];
}>();
</script>

<template>
  <fieldset class="models-section" :disabled="modelsLoading">
    <legend>高级参数与能力</legend>
    <div class="form-row">
      <div class="form-group">
        <label for="model-timeout">超时 (秒)</label>
        <input
          id="model-timeout"
          v-model.number="modelsForm.timeout"
          type="number"
          min="1"
          max="300"
          step="1"
          class="form-input"
        >
      </div>
      <div class="form-group">
        <label for="model-retries">最大重试次数</label>
        <input
          id="model-retries"
          v-model.number="modelsForm.max_retries"
          type="number"
          min="0"
          max="10"
          step="1"
          class="form-input"
        >
      </div>
      <div class="form-group">
        <label for="model-retry-delay">重试延迟 (秒)</label>
        <input
          id="model-retry-delay"
          v-model.number="modelsForm.retry_delay"
          type="number"
          min="0"
          max="20"
          step="0.1"
          class="form-input"
        >
      </div>
    </div>

    <div class="form-row">
      <div class="form-group">
        <label for="model-context-window">Context Window</label>
        <input
          id="model-context-window"
          v-model.number="modelsForm.context_window"
          type="number"
          min="1"
          step="1"
          class="form-input"
        >
      </div>
      <div class="form-group">
        <label for="model-max-output">Max Output Tokens</label>
        <input
          id="model-max-output"
          v-model.number="modelsForm.max_output_tokens"
          type="number"
          min="1"
          step="1"
          class="form-input"
        >
      </div>
    </div>

    <div class="capability-grid">
      <div>
        <div class="tool-category-title">
          输入模态
        </div>
        <div class="checkbox-grid">
          <label
            v-for="option in modalityOptions"
            :key="`input-${option.value}`"
            class="checkbox-label"
          >
            <input
              type="checkbox"
              :checked="modelsForm.model_input_modalities.includes(option.value)"
              @change="emit('toggleInputModality', option.value, ($event.target as HTMLInputElement).checked)"
            >
            <span>{{ option.label }}</span>
          </label>
        </div>
      </div>
      <div>
        <div class="tool-category-title">
          输出模态
        </div>
        <div class="checkbox-grid">
          <label
            v-for="option in modalityOptions"
            :key="`output-${option.value}`"
            class="checkbox-label"
          >
            <input
              type="checkbox"
              :checked="modelsForm.model_output_modalities.includes(option.value)"
              @change="emit('toggleOutputModality', option.value, ($event.target as HTMLInputElement).checked)"
            >
            <span>{{ option.label }}</span>
          </label>
        </div>
      </div>
      <div>
        <div class="tool-category-title">
          模型能力
        </div>
        <div class="checkbox-grid">
          <label class="checkbox-label">
            <input v-model="modelsForm.model_tool_calling" type="checkbox">
            <span>Tool calling</span>
          </label>
          <label class="checkbox-label">
            <input v-model="modelsForm.model_parallel_tool_calls" type="checkbox">
            <span>Parallel tools</span>
          </label>
          <label class="checkbox-label">
            <input v-model="modelsForm.model_streaming" type="checkbox">
            <span>Streaming</span>
          </label>
          <label class="checkbox-label">
            <input v-model="modelsForm.model_structured_output" type="checkbox">
            <span>Structured output</span>
          </label>
          <label class="checkbox-label">
            <input v-model="modelsForm.model_prompt_cache" type="checkbox">
            <span>Prompt cache</span>
          </label>
        </div>
      </div>
    </div>

    <div class="form-group" style="display:flex;gap:8px;flex-wrap:wrap;">
      <button
        type="button"
        class="btn btn-secondary"
        :disabled="modelsTesting || !selectedEntityId"
        @click="emit('testConnection')"
      >
        <SvgIcon src="/frontend/icons/connection.svg" :size="14" style="vertical-align: -2px;" />
        {{ modelsTesting ? '正在测试...' : '测试连接' }}
      </button>
      <button
        type="button"
        class="btn btn-secondary"
        :disabled="capabilityLoading || !selectedEntityId"
        @click="emit('validateCapability')"
      >
        {{ capabilityLoading ? '验证中...' : '验证能力配置' }}
      </button>
    </div>

    <CapabilityStatusPanel
      :snapshot="capabilitySnapshot"
      :validation="capabilityValidation"
      :cache-metrics="cacheMetrics"
      :loading="capabilityLoading"
      style="margin-top: 8px;"
    />
  </fieldset>
</template>
