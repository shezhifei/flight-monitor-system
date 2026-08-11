import '@/styles/main.css';
import '@/styles/resource-manager.css';
import { createApp } from 'vue';
import ResourceManager from '@/pages/resource_manager/ResourceManager.vue';
import { bootstrapProtectedPage } from '@/shared/bootstrapProtectedPage';

await bootstrapProtectedPage(() => createApp(ResourceManager).mount('#app'));
