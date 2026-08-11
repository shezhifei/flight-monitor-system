import '@/styles/main.css';
import '@/styles/system-flags.css';
import { createApp } from 'vue';
import SystemFlags from '@/pages/system_flags/SystemFlags.vue';
import { bootstrapProtectedPage } from '@/shared/bootstrapProtectedPage';

await bootstrapProtectedPage(() => createApp(SystemFlags).mount('#app'));
