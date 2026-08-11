import { createApp } from 'vue';
import FlightMonitorPage from './FlightMonitorPage.vue';

export function mountFlightMonitorPage(): void {
  const mountTarget = document.querySelector<HTMLElement>('#app');

  if (!mountTarget) {
    throw new Error('Missing #app mount target for flight_monitor');
  }

  document.title = '航班监控系统';
  document.documentElement.lang = 'zh-CN';

  createApp(FlightMonitorPage).mount(mountTarget);
}
