import '@/styles/main.css';
import '@/styles/dispatch-rule-center.css';
import { createApp } from 'vue';
import DispatchRuleCenter from '@/pages/dispatch_rule_center/DispatchRuleCenter.vue';
import { bootstrapProtectedPage } from '@/shared/bootstrapProtectedPage';

await bootstrapProtectedPage(() => createApp(DispatchRuleCenter).mount('#app'));
