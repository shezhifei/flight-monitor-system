import '@/styles/main.css';
import '@/styles/flight-imports.css';
import { createApp } from 'vue';
import FlightImports from '@/pages/flight_imports/FlightImports.vue';
import { bootstrapProtectedPage } from '@/shared/bootstrapProtectedPage';

await bootstrapProtectedPage(() => createApp(FlightImports).mount('#app'));
