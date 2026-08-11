import '@/styles/main.css';
import '@/styles/ontology-center.css';
import { createApp } from 'vue';
import OntologyCenter from '@/pages/ontology_center/OntologyCenter.vue';
import { bootstrapProtectedPage, markWorkspaceEmbed } from '@/shared/bootstrapProtectedPage';

markWorkspaceEmbed();

await bootstrapProtectedPage(() => createApp(OntologyCenter).mount('#app'));
