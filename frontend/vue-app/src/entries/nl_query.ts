import { createApp } from 'vue';
import NlQuery from '@/pages/nl_query/NlQuery.vue';
import { bootstrapProtectedPage } from '@/shared/bootstrapProtectedPage';

await bootstrapProtectedPage(() => createApp(NlQuery).mount('#app'));
