import '@/styles/main.css';
import { createApp } from 'vue';
import ResourceUtilization from '@/pages/resource_utilization/ResourceUtilization.vue';
import { bootstrapProtectedPage } from '@/shared/bootstrapProtectedPage';

await bootstrapProtectedPage(() => createApp(ResourceUtilization).mount('#app'));
