import '@/styles/main.css';
import { mountFlightMonitorPage } from '@/pages/flight_monitor/mount';
import { bootstrapProtectedPage } from '@/shared/bootstrapProtectedPage';

await bootstrapProtectedPage(() => mountFlightMonitorPage());
