import '@/styles/main.css';
import { createApp } from 'vue';
import AiConfigCenter from '@/pages/ai_config_center/AiConfigCenter.vue';
import { bootstrapProtectedPage } from '@/shared/bootstrapProtectedPage';

await bootstrapProtectedPage(() => createApp(AiConfigCenter).mount('#app'));
