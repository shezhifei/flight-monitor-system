<template>
  <div class="flight-milestone-timeline">
    <div
      v-if="hasChartData"
      ref="chartRef"
      class="gantt-chart-container"
      :style="{ height: `${chartHeight}px` }"
    />
    <div v-else class="timeline-empty">
      暂无时间流与业务事项数据
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, inject, nextTick, ref, onMounted, onUnmounted, watch } from 'vue';
import * as echarts from 'echarts';
import { useTheme } from '@/composables/useTheme';
import { toSortableTimestamp } from '@/composables/useFlightFilter';
import { flightBusinessCaseKey } from '@/composables/useFlightBusinessCases';
import type { BusinessCaseSummary } from '@/types/backend';
import { getCaseDisplayName, normalizeCaseStatusValue } from './detail/businessCaseHelpers';

interface Flight {
  [key: string]: unknown;
}

const props = defineProps<{
  flight: Flight | null;
}>();

const caseCtx = inject(flightBusinessCaseKey, null);

const chartRef = ref<HTMLElement | null>(null);
let chartInstance: echarts.ECharts | null = null;
let resizeObserver: ResizeObserver | null = null;
let observedElement: HTMLElement | null = null;
let renderFrame = 0;

const TIME_NODE_CONFIG = [
  { field: 'scheduled_arrival', label: '计划到达', color: '#007AFF' },
  { field: 'actual_arrival', label: '实际到达', color: '#34C759' },
  { field: 'on_blocks_time', label: '上轮挡', color: '#FF9500' },
  { field: 'cabin_door_open_time', label: '开舱门', color: '#5856D6' },
  { field: 'deboarding_complete_time', label: '下客完成', color: '#FF2D55' },
  { field: 'cleaning_start_time', label: '清洁开始', color: '#AF52DE' },
  { field: 'cleaning_end_time', label: '清洁结束', color: '#FF9500' },
  { field: 'cabin_door_close_time', label: '关客舱门', color: '#5856D6' },
  { field: 'cargo_door_close_time', label: '关货舱门', color: '#007AFF' },
  { field: 'loading_complete_time', label: '装载完成', color: '#34C759' },
  { field: 'boarding_allowed_time', label: '允许登机', color: '#FF9500' },
  { field: 'start_boarding_time', label: '开始登机', color: '#FFCC00' },
  { field: 'passenger_ready_time', label: '人齐', color: '#34C759' },
  { field: 'end_boarding_time', label: '结束登机', color: '#5856D6' },
  { field: 'off_blocks_time', label: '撤轮挡', color: '#FF9500' },
  { field: 'scheduled_departure', label: '计划起飞', color: '#007AFF' },
  { field: 'actual_departure', label: '实际起飞', color: '#34C759' }
];

const STATUS_COLORS: Record<string, string> = {
  INITIAL: '#8E8E93',
  PENDING: '#FF9500',
  PROCESSING: '#5856D6',
  SUCCESS: '#34C759',
  COMPLETED: '#34C759',
  FAILED: '#FF3B30',
};

const FLIGHT_STATUS_LANE = '航班状态转换';
/** Canvas text does not inherit CSS — pass MiSans explicitly into ECharts. */
const CHART_FONT_FAMILY = 'MiSans, "PingFang SC", "Microsoft YaHei", sans-serif';

interface TimeNode {
  field: string;
  label: string;
  color: string;
  timestamp: number;
}

type GanttRaw =
  | { type: 'flight_status'; from: string; to: string; fromTime: number; toTime: number }
  | { type: 'time_node'; label: string; time: number }
  | { type: 'business_case'; caseId: string; name: string; status: string; description?: string | null; createdAt: number; finishedAt: number | null; appendCount: number };

interface GanttItem {
  name: string;
  value: [number, number, number];
  itemStyle: { color: string };
  raw: GanttRaw;
}

const timeNodes = computed<TimeNode[]>(() => {
  if (!props.flight) return [];
  const nodes: TimeNode[] = [];
  for (const config of TIME_NODE_CONFIG) {
    const timestamp = toSortableTimestamp(props.flight[config.field])
      || toSortableTimestamp(props.flight[`${config.field}_ai_prediction`]);
    if (timestamp > 0) {
      nodes.push({ ...config, timestamp });
    }
  }
  return nodes.sort((a, b) => a.timestamp - b.timestamp);
});

const ganttCases = computed<BusinessCaseSummary[]>(() => {
  if (!props.flight) return [];
  const cases = (props.flight.business_cases as BusinessCaseSummary[] | undefined) || [];
  const filter = caseCtx?.caseFilter.value ?? 'all';
  if (filter === 'all') return cases;
  return cases.filter((c) => normalizeCaseStatusValue(c.status) === filter);
});

const yAxisData = computed<string[]>(() => [
  FLIGHT_STATUS_LANE,
  ...ganttCases.value.map((c) => getCaseDisplayName(c)),
]);

const hasChartData = computed(() => timeNodes.value.length > 0 || ganttCases.value.length > 0);

const chartHeight = computed(() => Math.min(520, Math.max(200, 80 + yAxisData.value.length * 64)));

const timeRange = computed(() => {
  const times: number[] = timeNodes.value.map((n) => n.timestamp);
  for (const c of ganttCases.value) {
    const start = toSortableTimestamp(c.created_at);
    if (start > 0) times.push(start);
    const end = toSortableTimestamp(c.finished_at);
    if (end > 0) times.push(end);
  }
  if (times.length === 0) {
    const now = Date.now();
    return { min: now - 3600000 * 4, max: now + 3600000 * 4 };
  }
  const min = Math.min(...times);
  const max = Math.max(...times);
  const padding = (max - min) * 0.1 || 3600000;
  return { min: min - padding, max: max + padding };
});

const seriesData = computed<GanttItem[]>(() => {
  const items: GanttItem[] = [];
  const nodes = timeNodes.value;

  for (let i = 0; i < nodes.length - 1; i++) {
    items.push({
      name: `${nodes[i].label} → ${nodes[i + 1].label}`,
      value: [nodes[i].timestamp, nodes[i + 1].timestamp, 0],
      itemStyle: { color: nodes[i].color },
      raw: { type: 'flight_status', from: nodes[i].label, to: nodes[i + 1].label, fromTime: nodes[i].timestamp, toTime: nodes[i + 1].timestamp },
    });
  }

  nodes.forEach((node) => {
    items.push({
      name: node.label,
      value: [node.timestamp - 60000, node.timestamp + 60000, 0],
      itemStyle: { color: node.color },
      raw: { type: 'time_node', label: node.label, time: node.timestamp },
    });
  });

  ganttCases.value.forEach((c, index) => {
    const start = toSortableTimestamp(c.created_at);
    if (start <= 0) return;
    const finishedAt = toSortableTimestamp(c.finished_at);
    const end = finishedAt > 0 ? finishedAt : Date.now();
    const name = getCaseDisplayName(c);
    items.push({
      name,
      value: [start, end, index + 1],
      itemStyle: { color: STATUS_COLORS[normalizeCaseStatusValue(c.status)] || '#8E8E93' },
      raw: {
        type: 'business_case',
        caseId: String(c.case_id || ''),
        name,
        status: normalizeCaseStatusValue(c.status),
        description: c.description,
        createdAt: start,
        finishedAt: finishedAt > 0 ? finishedAt : null,
        appendCount: Number(c.append_count || 0),
      },
    });
  });

  return items;
});

function formatAxisTime(timestamp: number): string {
  return new Date(timestamp).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', hour12: false });
}

function formatTooltipTime(timestamp: number): string {
  return new Date(timestamp).toLocaleString('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  });
}

function hasChartSize(element: HTMLElement): boolean {
  return element.clientWidth > 0 && element.clientHeight > 0;
}

function observeChartContainer(element: HTMLElement): void {
  if (observedElement === element) return;
  resizeObserver?.disconnect();
  observedElement = element;
  resizeObserver = new ResizeObserver((entries) => {
    if (entries.some((entry) => entry.contentRect.width > 0 && entry.contentRect.height > 0)) {
      scheduleRender();
    }
  });
  resizeObserver.observe(element);
}

function scheduleRender(): void {
  if (renderFrame) {
    window.cancelAnimationFrame(renderFrame);
  }
  renderFrame = window.requestAnimationFrame(() => {
    renderFrame = 0;
    void renderChart();
  });
}

const { isDark, theme } = useTheme();
watch(theme, () => {
  chartInstance?.dispose();
  chartInstance = null;
  scheduleRender();
});

function buildTooltip(raw: GanttRaw): string {
  if (raw.type === 'flight_status') {
    return `
      <div style="font-weight:700;margin-bottom:4px;">${FLIGHT_STATUS_LANE}</div>
      <div>${raw.from} → ${raw.to}</div>
      <div>时间: ${formatTooltipTime(raw.fromTime)} - ${formatTooltipTime(raw.toTime)}</div>
    `;
  }
  if (raw.type === 'time_node') {
    return `
      <div style="font-weight:700;margin-bottom:4px;">${raw.label}</div>
      <div>时间: ${formatTooltipTime(raw.time)}</div>
    `;
  }
  return `
    <div style="font-weight:700;margin-bottom:4px;">${raw.name}</div>
    <div>状态: ${raw.status}</div>
    <div>创建时间: ${formatTooltipTime(raw.createdAt)}</div>
    ${raw.finishedAt ? `<div>完成时间: ${formatTooltipTime(raw.finishedAt)}</div>` : ''}
    ${raw.description ? `<div>描述: ${raw.description}</div>` : ''}
    ${raw.appendCount ? `<div>已追加: ${raw.appendCount} 次</div>` : ''}
  `;
}

async function renderChart(): Promise<void> {
  await nextTick();
  const element = chartRef.value;
  if (!element) return;
  observeChartContainer(element);
  if (!hasChartSize(element)) return;
  if (!chartInstance) {
    chartInstance = echarts.init(element, isDark() ? 'dark' : null, { renderer: 'canvas' });
  }

  const items = seriesData.value;
  if (items.length === 0) {
    chartInstance.clear();
    return;
  }

  const dark = isDark();
  const axisLabelColor = dark ? 'rgba(159, 179, 200, 0.85)' : '#5f7082';
  const axisLineColor = dark ? 'rgba(159, 179, 200, 0.45)' : '#8a97a8';
  const yAxisLabelColor = dark ? '#c7d5e5' : '#33485f';
  const splitLineColor = dark ? 'rgba(148, 163, 184, 0.14)' : 'rgba(15, 23, 42, 0.08)';
  const barStrokeColor = dark ? 'rgba(226, 232, 240, 0.25)' : 'rgba(15, 23, 42, 0.22)';

  // Fit Y-axis gutter to longest label (was fixed 140px → large blank when only short lanes).
  const longestLabelChars = yAxisData.value.reduce((max, label) => {
    const len = Array.from(String(label || '')).length;
    return Math.max(max, len);
  }, 4);
  const yLabelWidth = Math.min(108, Math.max(64, Math.ceil(longestLabelChars * 12.5) + 4));
  const gridLeft = yLabelWidth + 14;

  const option = {
    animation: false,
    backgroundColor: 'transparent',
    textStyle: {
      fontFamily: CHART_FONT_FAMILY,
    },
    tooltip: {
      trigger: 'item',
      confine: true,
      borderWidth: 1,
      borderColor: 'rgba(15, 23, 42, 0.1)',
      textStyle: {
        fontFamily: CHART_FONT_FAMILY,
        fontSize: 12,
      },
      formatter: function (params: Record<string, unknown>) {
        const dataItem = items[params.dataIndex as number];
        if (!dataItem) return '';
        return buildTooltip(dataItem.raw);
      }
    },
    grid: { left: gridLeft, right: 16, top: 16, bottom: 36, containLabel: false },
    xAxis: {
      type: 'time',
      min: timeRange.value.min,
      max: timeRange.value.max,
      axisLine: { lineStyle: { color: axisLineColor } },
      axisLabel: {
        color: axisLabelColor,
        fontSize: 11,
        fontFamily: CHART_FONT_FAMILY,
        hideOverlap: true,
        formatter: (value: number) => formatAxisTime(value),
      },
      splitLine: { lineStyle: { color: splitLineColor, type: 'dashed' } }
    },
    yAxis: {
      type: 'category',
      inverse: true,
      data: yAxisData.value,
      axisTick: { show: false },
      axisLine: { show: false },
      axisLabel: {
        color: yAxisLabelColor,
        fontSize: 12,
        fontFamily: CHART_FONT_FAMILY,
        fontWeight: 500,
        width: yLabelWidth,
        overflow: 'truncate',
        margin: 8,
      },
    },
    dataZoom: [
      {
        type: 'inside',
        xAxisIndex: 0,
        zoomOnMouseWheel: true,
        moveOnMouseMove: true,
        moveOnMouseWheel: true
      },
      {
        type: 'slider',
        xAxisIndex: 0,
        bottom: 8,
        height: 12,
        borderColor: 'rgba(15, 23, 42, 0.12)',
        backgroundColor: dark ? 'rgba(148, 163, 184, 0.12)' : 'rgba(255,255,255,0.74)',
        fillerColor: 'rgba(11,119,227,0.2)',
        showDetail: false
      }
    ],
    series: [{
      type: 'custom',
      animation: false,
      renderItem: function (params: Record<string, unknown>, api: { value: (idx: number) => number; coord: (point: [number, number]) => [number, number]; size: (dims: [number, number]) => [number, number]; style: () => { fill?: string } }) {
        const dataItem = items[params.dataIndex as number];
        if (!dataItem) return null;

        const categoryIndex = api.value(2);
        const startCoord = api.coord([api.value(0), categoryIndex]);
        const endCoord = api.coord([api.value(1), categoryIndex]);
        const laneHeight = Math.min(60, Math.max(20, api.size([0, 1])[1]));

        const x = Math.min(startCoord[0], endCoord[0]);
        const width = Math.max(4, Math.abs(endCoord[0] - startCoord[0]));
        const y = startCoord[1] - laneHeight / 2 + 4;
        const height = laneHeight - 8;

        const coordSys = params.coordSys as { x: number; y: number; width: number; height: number };
        const clippedRect = echarts.graphic.clipRectByRect(
          { x, y, width, height },
          { x: coordSys.x, y: coordSys.y, width: coordSys.width, height: coordSys.height }
        );

        if (!clippedRect) return null;

        const isTimeNode = dataItem.raw.type === 'time_node';

        return {
          type: 'group',
          children: [
            {
              type: 'rect',
              shape: { ...clippedRect, r: 4 },
              style: {
                fill: dataItem.itemStyle.color,
                stroke: barStrokeColor,
                lineWidth: 1,
                opacity: isTimeNode ? 0.6 : 0.9
              }
            },
            {
              type: 'text',
              style: {
                x: clippedRect.x + 6,
                y: clippedRect.y + clippedRect.height / 2,
                text: clippedRect.width > 60 ? dataItem.name : '',
                verticalAlign: 'middle',
                fill: '#ffffff',
                font: `500 11px ${CHART_FONT_FAMILY}`,
                fontSize: 11,
                fontWeight: 500,
                fontFamily: CHART_FONT_FAMILY,
                width: Math.max(20, clippedRect.width - 10),
                overflow: 'truncate'
              },
              silent: true
            }
          ]
        };
      },
      encode: { x: [0, 1], y: 2 },
      data: items,
      markLine: {
        symbol: ['none', 'none'],
        silent: true,
        lineStyle: { color: '#FF3B30', width: 1, type: 'dashed' },
        label: {
          show: true,
          formatter: '现在',
          color: '#FF3B30',
          fontWeight: 600,
          fontFamily: CHART_FONT_FAMILY,
          backgroundColor: 'rgba(255, 59, 48, 0.12)',
          borderColor: 'rgba(255, 59, 48, 0.35)',
          borderWidth: 1,
          borderRadius: 4,
          padding: [2, 6]
        },
        data: [{ xAxis: Date.now() }]
      }
    }]
  };

  chartInstance.off('click');
  chartInstance.on('click', (params) => {
    const dataItem = items[(params as { dataIndex?: number }).dataIndex ?? -1];
    if (dataItem?.raw.type === 'business_case' && dataItem.raw.caseId) {
      void caseCtx?.openCaseDetail(dataItem.raw.caseId);
    }
  });

  chartInstance.clear();
  chartInstance.setOption(option, true);
}

function handleResize() {
  if (chartInstance) {
    chartInstance.resize();
  } else {
    scheduleRender();
  }
}

onMounted(() => {
  scheduleRender();
  window.addEventListener('resize', handleResize);
});

onUnmounted(() => {
  if (renderFrame) {
    window.cancelAnimationFrame(renderFrame);
    renderFrame = 0;
  }
  resizeObserver?.disconnect();
  resizeObserver = null;
  observedElement = null;
  window.removeEventListener('resize', handleResize);
  if (chartInstance) {
    chartInstance.dispose();
    chartInstance = null;
  }
});

watch(() => props.flight, () => {
  scheduleRender();
}, { deep: true });

watch(() => caseCtx?.caseFilter.value, () => {
  scheduleRender();
});
</script>

<style scoped>
.flight-milestone-timeline {
  display: flex;
  flex-direction: column;
}

.gantt-chart-container {
  flex-grow: 1;
  width: 100%;
  position: relative;
  z-index: 0;
  overflow: hidden;
  /* 防止 echarts absolute 层溢出抢点击 */
  min-height: 0;
}

.timeline-empty {
  color: var(--text-secondary, #546E7A);
  font-size: 13px;
  text-align: center;
  padding: 40px 0;
}
</style>
