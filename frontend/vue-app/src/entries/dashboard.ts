import '@/styles/main.css';
import { createApp } from 'vue';
import Dashboard from '@/pages/dashboard/Dashboard.vue';
import { bootstrapProtectedPage } from '@/shared/bootstrapProtectedPage';

await bootstrapProtectedPage(() => createApp(Dashboard).mount('#app'));
