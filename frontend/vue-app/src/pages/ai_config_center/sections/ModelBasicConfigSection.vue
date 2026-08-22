<script setup lang="ts">
import { computed } from 'vue';
import UiButton from '../../../components/ui/UiButton.vue';
import type { ModelsTabForm } from '../composables/useAiConfigCenter';

const props = defineProps<{
  modelsForm: ModelsTabForm;
  modelsLoading: boolean;
}>();

const form = computed(() => props.modelsForm);
const emit = defineEmits<{
  addProvider: [];
  removeProvider: [index: number];
}>();
</script>

<template>
  <fieldset class="models-section" :disabled="modelsLoading">
    <legend>模型配置</legend>
    <div class="form-row">
      <div class="form-group">
        <label for="model-config-version">配置版本</label>
        <input
          id="model-config-version"
          v-model.number="form.config_version"
          type="number"
          min="1"
          max="99"
          step="1"
          class="form-input"
        >
      </div>
      <div class="form-group">
        <label for="model-api-format">API 格式</label>
        <select
          id="model-api-format"
          v-model="form.api_format"
          class="form-select"
        >
          <option value="chat_completions">
            chat_completions
          </option>
          <option value="responses">
            responses
          </option>
        </select>
      </div>
    </div>
    <p class="models-section-hint">
      默认 Provider (<code>default</code>) 由下方 Base URL / API Key / API 格式表达；
      附加 Provider (如 <code>asr</code>/<code>tts</code>) 在「Provider 字典」中维护，模型通过 <code>provider_ref</code> 引用。
    </p>
    <div class="form-group">
      <label for="model-base-url">API Base URL <span class="provider-tag">default</span></label>
      <input
        id="model-base-url"
        v-model="form.base_url"
        type="text"
        class="form-input"
        placeholder="https://api.openai.com/v1"
      >
    </div>
    <div class="form-group">
      <label for="model-api-key">API Key <span class="provider-tag">default</span></label>
      <input
        id="model-api-key"
        v-model="form.api_key"
        type="password"
        class="form-input"
        autocomplete="off"
      >
    </div>

    <div class="provider-dict">
      <div class="provider-dict-header">
        <div class="tool-category-title">
          Provider 字典（附加）
        </div>
        <UiButton variant="ghost" @click="emit('addProvider')">
          新增 Provider
        </UiButton>
      </div>
      <div v-if="form.providers.length === 0" class="empty-state-inline">
        暂无附加 Provider（仅使用 default）
      </div>
      <div
        v-for="(prov, idx) in form.providers"
        :key="`provider-${idx}`"
        class="provider-entry"
      >
        <div class="form-row">
          <div class="form-group">
            <label :for="`provider-key-${idx}`">键名 (provider_ref)</label>
            <input
              :id="`provider-key-${idx}`"
              v-model="prov.key"
              type="text"
              class="form-input"
              placeholder="asr / tts / embedding"
            >
          </div>
          <div class="form-group">
            <label :for="`provider-format-${idx}`">API 格式</label>
            <select
              :id="`provider-format-${idx}`"
              v-model="prov.api_format"
              class="form-select"
            >
              <option value="chat_completions">
                chat_completions
              </option>
              <option value="responses">
                responses
              </option>
            </select>
          </div>
        </div>
        <div class="form-group">
          <label :for="`provider-base-${idx}`">Base URL</label>
          <input
            :id="`provider-base-${idx}`"
            v-model="prov.base_url"
            type="text"
            class="form-input"
            placeholder="https://api.openai.com/v1"
          >
        </div>
        <div class="form-group">
          <label :for="`provider-key-secret-${idx}`">API Key</label>
          <input
            :id="`provider-key-secret-${idx}`"
            v-model="prov.api_key"
            type="password"
            class="form-input"
            autocomplete="off"
          >
        </div>
        <div class="provider-entry-actions">
          <UiButton
            variant="danger"
            @click="emit('removeProvider', idx)"
          >
            删除
          </UiButton>
        </div>
      </div>
    </div>
  </fieldset>
</template>
