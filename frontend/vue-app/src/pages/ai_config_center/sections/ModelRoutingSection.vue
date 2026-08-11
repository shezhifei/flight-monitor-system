<script setup lang="ts">
import type { ModelsTabForm, NormalizedModelOption } from '../composables/useAiConfigCenter';

defineProps<{
  modelsForm: ModelsTabForm;
  modelsLoading: boolean;
  modelOptions: NormalizedModelOption[];
  providerRefOptions: string[];
}>();
</script>

<template>
  <fieldset class="models-section" :disabled="modelsLoading">
    <legend>模型路由</legend>
    <div class="form-group">
      <label for="model-provider-ref">默认模型 Provider 引用 (provider_ref)</label>
      <select
        id="model-provider-ref"
        v-model="modelsForm.model_provider_ref"
        class="form-select"
      >
        <option
          v-for="ref in providerRefOptions"
          :key="`provider-ref-${ref}`"
          :value="ref"
        >
          {{ ref }}
        </option>
      </select>
    </div>

    <div class="form-group">
      <label for="model-default">默认模型</label>
      <input
        id="model-default"
        v-model="modelsForm.default_model"
        type="text"
        class="form-input"
        placeholder="输入自定义模型或从已发现模型中选择"
        list="model-options-default"
      >
      <datalist id="model-options-default">
        <option
          v-for="opt in modelOptions"
          :key="`default-${opt.value}`"
          :value="opt.value"
        >
          {{ opt.label }}
        </option>
      </datalist>
    </div>

    <div class="form-row">
      <div class="form-group">
        <label for="model-chat">Chat 模型</label>
        <input
          id="model-chat"
          v-model="modelsForm.chat_model"
          type="text"
          class="form-input"
          placeholder="未填则使用默认模型"
          list="model-options-chat"
        >
        <datalist id="model-options-chat">
          <option
            v-for="opt in modelOptions"
            :key="`chat-${opt.value}`"
            :value="opt.value"
          >
            {{ opt.label }}
          </option>
        </datalist>
      </div>
      <div class="form-group">
        <label for="model-summary-route">Summary 模型</label>
        <input
          id="model-summary-route"
          v-model="modelsForm.summary_model"
          type="text"
          class="form-input"
          placeholder="用于摘要/压缩"
          list="model-options-summary"
        >
        <datalist id="model-options-summary">
          <option
            v-for="opt in modelOptions"
            :key="`summary-${opt.value}`"
            :value="opt.value"
          >
            {{ opt.label }}
          </option>
        </datalist>
      </div>
    </div>

    <div class="form-row">
      <div class="form-group">
        <label for="model-vision">Vision 模型</label>
        <input
          id="model-vision"
          v-model="modelsForm.vision_model"
          type="text"
          class="form-input"
          placeholder="支持 image 输入时配置"
          list="model-options-vision"
        >
        <datalist id="model-options-vision">
          <option
            v-for="opt in modelOptions"
            :key="`vision-${opt.value}`"
            :value="opt.value"
          >
            {{ opt.label }}
          </option>
        </datalist>
      </div>
      <div class="form-group">
        <label for="model-embedding">Embedding 模型</label>
        <input
          id="model-embedding"
          v-model="modelsForm.embedding_model"
          type="text"
          class="form-input"
          placeholder="可选"
          list="model-options-embedding"
        >
        <datalist id="model-options-embedding">
          <option
            v-for="opt in modelOptions"
            :key="`embedding-${opt.value}`"
            :value="opt.value"
          >
            {{ opt.label }}
          </option>
        </datalist>
      </div>
    </div>

    <div class="form-row">
      <div class="form-group">
        <label for="model-asr">ASR 模型</label>
        <input
          id="model-asr"
          v-model="modelsForm.asr_model"
          type="text"
          class="form-input"
          placeholder="如 whisper-1"
          list="model-options-asr"
        >
        <datalist id="model-options-asr">
          <option
            v-for="opt in modelOptions"
            :key="`asr-${opt.value}`"
            :value="opt.value"
          >
            {{ opt.label }}
          </option>
        </datalist>
      </div>
      <div class="form-group">
        <label for="model-tts">TTS 模型</label>
        <input
          id="model-tts"
          v-model="modelsForm.tts_model"
          type="text"
          class="form-input"
          placeholder="如 tts-1"
          list="model-options-tts"
        >
        <datalist id="model-options-tts">
          <option
            v-for="opt in modelOptions"
            :key="`tts-${opt.value}`"
            :value="opt.value"
          >
            {{ opt.label }}
          </option>
        </datalist>
      </div>
      <div class="form-group">
        <label for="model-tts-voice">TTS 声音</label>
        <input
          id="model-tts-voice"
          v-model="modelsForm.tts_voice"
          type="text"
          class="form-input"
          placeholder="如 alloy / nova / verse"
        >
      </div>
    </div>
  </fieldset>
</template>
