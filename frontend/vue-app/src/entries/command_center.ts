import '@/styles/main.css';
import { createApp } from 'vue';
import CommandCenter from '@/pages/command_center/CommandCenter.vue';
import { bootstrapProtectedPage } from '@/shared/bootstrapProtectedPage';

await bootstrapProtectedPage(() => createApp(CommandCenter).mount('#app'));
