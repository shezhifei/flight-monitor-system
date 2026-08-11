import '@/styles/main.css';
import { createApp } from 'vue';
import AnomalyMonitor from '@/pages/anomaly_monitor/AnomalyMonitor.vue';
import { bootstrapProtectedPage } from '@/shared/bootstrapProtectedPage';

await bootstrapProtectedPage(() => createApp(AnomalyMonitor).mount('#app'));
