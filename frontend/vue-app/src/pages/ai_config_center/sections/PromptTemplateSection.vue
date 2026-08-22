<script setup lang="ts">
import { computed } from 'vue';
import type { ModelsTabForm } from '../composables/useAiConfigCenter';

const props = defineProps<{
  modelsForm: ModelsTabForm;
  modelsLoading: boolean;
}>();

const form = computed(() => props.modelsForm);
</script>

<template>
  <fieldset class="models-section" :disabled="modelsLoading">
    <legend>Prompt 配置</legend>
    <div class="form-group">
      <label for="model-system-prompt">System Prompt</label>
      <textarea
        id="model-system-prompt"
        v-model="form.system_prompt"
        class="form-textarea form-textarea-mono"
        rows="8"
      />
    </div>
    <div class="form-group">
      <label for="model-task-template">Task Template</label>
      <textarea
        id="model-task-template"
        v-model="form.task_template"
        class="form-textarea form-textarea-mono"
        rows="6"
      />
    </div>
  </fieldset>
</template>
