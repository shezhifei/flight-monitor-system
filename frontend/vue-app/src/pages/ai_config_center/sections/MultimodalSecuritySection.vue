<script setup lang="ts">
import type { ModelsTabForm } from '../composables/useAiConfigCenter';

defineProps<{
  modelsForm: ModelsTabForm;
  modelsLoading: boolean;
}>();
</script>

<template>
  <fieldset class="models-section" :disabled="modelsLoading">
    <legend>多模态输入安全</legend>
    <div class="form-row">
      <div class="form-group">
        <label for="max-input-bytes">最大输入字节数</label>
        <input
          id="max-input-bytes"
          v-model.number="modelsForm.max_input_bytes"
          type="number"
          min="1024"
          step="1024"
          class="form-input"
        >
      </div>
      <div class="form-group">
        <label for="allowed-input-mime-types">允许 MIME 类型</label>
        <textarea
          id="allowed-input-mime-types"
          v-model="modelsForm.allowed_input_mime_types"
          class="form-textarea form-textarea-mono"
          rows="3"
        />
      </div>
    </div>
  </fieldset>
</template>
