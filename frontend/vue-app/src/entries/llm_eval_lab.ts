import { createApp } from 'vue';
import LlmEvalLab from '@/pages/llm_eval_lab/LlmEvalLab.vue';
import { bootstrapProtectedPage } from '@/shared/bootstrapProtectedPage';

await bootstrapProtectedPage(() => createApp(LlmEvalLab).mount('#app'));
