import { createApp } from 'vue';
import AiMonitor from '@/pages/ai_monitor/AiMonitor.vue';
import { bootstrapProtectedPage } from '@/shared/bootstrapProtectedPage';

await bootstrapProtectedPage(() => createApp(AiMonitor).mount('#app'));
