import { initChart } from '@/lib/echarts';
import type { ECharts, EChartsCoreOption } from '@/lib/echarts';

let chart: ECharts | null = null;
let host: HTMLElement | null = null;

export function renderTrendChartInto(el: HTMLElement, option: EChartsCoreOption): void {
  if (chart && host !== el) {
    chart.dispose();
    chart = null;
  }
  if (!chart) {
    chart = initChart(el, null, { renderer: 'canvas' });
    host = el;
  }
  chart.setOption(option);
}

export function disposeTrendChart(): void {
  chart?.dispose();
  chart = null;
  host = null;
}
