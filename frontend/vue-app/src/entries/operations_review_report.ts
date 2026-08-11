import { createApp } from 'vue';
import OperationsReviewReport from '@/pages/operations_review_report/OperationsReviewReport.vue';
import { bootstrapProtectedPage } from '@/shared/bootstrapProtectedPage';

await bootstrapProtectedPage(() => createApp(OperationsReviewReport).mount('#app'));
