import '@/styles/main.css';
import { createApp } from 'vue';
import KpiDashboard from '@/pages/kpi_dashboard/KpiDashboard.vue';
import { bootstrapProtectedPage } from '@/shared/bootstrapProtectedPage';

await bootstrapProtectedPage(() => createApp(KpiDashboard).mount('#app'));
