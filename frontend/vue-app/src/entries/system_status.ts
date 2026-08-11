import '@/styles/main.css';
import { createApp } from 'vue';
import SystemStatus from '@/pages/system_status/SystemStatus.vue';
import { bootstrapProtectedPage } from '@/shared/bootstrapProtectedPage';

await bootstrapProtectedPage(() => createApp(SystemStatus).mount('#app'));
