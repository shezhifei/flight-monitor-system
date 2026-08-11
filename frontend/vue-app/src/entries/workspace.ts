import '@/styles/main.css';
import { createApp } from 'vue';
import WorkspacePage from '@/pages/workspace/WorkspacePage.vue';
import { bootstrapProtectedPage } from '@/shared/bootstrapProtectedPage';

await bootstrapProtectedPage(() => createApp(WorkspacePage).mount('#app'));
