import * as echarts from 'echarts';

export { echarts };
export type { ECharts, EChartsCoreOption, EChartsOption, SetOptionOpts } from 'echarts';

export type ChartTheme = Parameters<typeof echarts.init>[1];
export type ChartInitOptions = Parameters<typeof echarts.init>[2];

export function initChart(
  element: HTMLElement,
  theme?: ChartTheme,
  opts?: ChartInitOptions,
): echarts.ECharts {
  return echarts.init(element, theme, opts);
}
