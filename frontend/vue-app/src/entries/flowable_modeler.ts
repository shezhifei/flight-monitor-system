import '@/styles/main.css';
import '@/styles/flowable-modeler.css';
// bpmn-js 运行必需样式：缺了会导致画布/工具箱空白、无法拖拽建模
import 'bpmn-js/dist/assets/diagram-js.css';
import 'bpmn-js/dist/assets/bpmn-js.css';
import 'bpmn-js/dist/assets/bpmn-font/css/bpmn-embedded.css';
import { createApp } from 'vue';
import FlowableModeler from '@/pages/flowable_modeler/FlowableModeler.vue';
import { bootstrapProtectedPage } from '@/shared/bootstrapProtectedPage';

await bootstrapProtectedPage(() => createApp(FlowableModeler).mount('#app'));
