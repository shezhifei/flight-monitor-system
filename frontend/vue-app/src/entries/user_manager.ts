import '@/styles/main.css';
import { createApp } from 'vue';
import UserManager from '@/pages/user_manager/UserManager.vue';
import { bootstrapProtectedPage } from '@/shared/bootstrapProtectedPage';

await bootstrapProtectedPage(() => createApp(UserManager).mount('#app'));
