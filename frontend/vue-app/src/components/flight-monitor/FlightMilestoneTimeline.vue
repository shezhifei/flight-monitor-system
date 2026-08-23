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
import { alpha, useSignalTokens } from '@/composables/useSignalTokens';
import { useChartTheme } from '@/composables/useChartTheme';
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

/**
 * 时间刻（信号面 §2.3）：以前每个里程碑挂一个 Apple 系统色，17 个节点 17 种颜色 ——
 * 颜色不说话，只是装饰，而且和事态抢声。这里只留数据里真有的那一条区分：
 * `scheduled_*` 是计划（静，参考线），其余是观测到的实际（行动色）。
 * 节点靠标签认，不靠颜色认。
 */
type NodeKind = 'plan' | 'actual';

const TIME_NODE_CONFIG: { field: string; label: string; kind: NodeKind }[] = [
  { field: 'scheduled_arrival', label: '计划到达', kind: 'plan' },
  { field: 'actual_arrival', label: '实际到达', kind: 'actual' },
  { field: 'on_blocks_time', label: '上轮挡', kind: 'actual' },
  { field: 'cabin_door_open_time', label: '开舱门', kind: 'actual' },
  { field: 'deboarding_complete_time', label: '下客完成', kind: 'actual' },
  { field: 'cleaning_start_time', label: '清洁开始', kind: 'actual' },
  { field: 'cleaning_end_time', label: '清洁结束', kind: 'actual' },
  { field: 'cabin_door_close_time', label: '关客舱门', kind: 'actual' },
  { field: 'cargo_door_close_time', label: '关货舱门', kind: 'actual' },
  { field: 'loading_complete_time', label: '装载完成', kind: 'actual' },
  { field: 'boarding_allowed_time', label: '允许登机', kind: 'actual' },
  { field: 'start_boarding_time', label: '开始登机', kind: 'actual' },
  { field: 'passenger_ready_time', label: '人齐', kind: 'actual' },
  { field: 'end_boarding_time', label: '结束登机', kind: 'actual' },
  { field: 'off_blocks_time', label: '撤轮挡', kind: 'actual' },
  { field: 'scheduled_departure', label: '计划起飞', kind: 'plan' },
  { field: 'actual_departure', label: '实际起飞', kind: 'actual' },
];

/** 业务事项状态 → 四声。声只表事态，不表种类。 */
const CASE_TONE: Record<string, 'mute' | 'act' | 'ok' | 'warn' | 'danger'> = {
  INITIAL: 'mute',
  PENDING: 'warn',
  PROCESSING: 'act',
  SUCCESS: 'ok',
  COMPLETED: 'ok',
  FAILED: 'danger',
};

const FLIGHT_STATUS_LANE = '航班状态转换';

interface TimeNode {
  field: string;
  label: string;
  kind: NodeKind;
  timestamp: number;
}

type GanttRaw =
  | { type: 'flight_status'; from: string; to: string; fromTime: number; toTime: number }
  | { type: 'time_node'; label: string; time: number }
  | { type: 'business_case'; caseId: string; name: string; status: string; description?: string | null; createdAt: number; finishedAt: number | null; appendCount: number };

interface GanttItem {
  name: string;
  value: [number, number, number];
  /** 面：段=常态脊线洗底，刻=静/行动，事项=四声 */
  fill: string;
  /** 字：透明洗底上用墨，实色声上用反墨 */
  textFill: string;
  /** 段与计划刻是参考，压一档不透明度 */
  quiet: boolean;
  raw: GanttRaw;
}

const { tokens } = useSignalTokens();
const { chartBase } = useChartTheme();

/** 画布取声：token 真值现读，主题一换整块重算（见 useSignalTokens） */
const paint = computed(() => {
  const t = tokens.value;
  return {
    /** 状态段：常态脊线，只是「这段时间过去了」，不该有声 */
    spine: alpha(t.ink, 0.1),
    spineText: t.ink,
    /** 计划刻：静，参考用 */
    plan: alpha(t['ink-subtle'], 0.28),
    planText: t.ink,
    /** 实际刻：行动色，实底 */
    actual: t.act,
    actualText: t['ink-inverse'],
    /** 事项四声 */
    tone: {
      mute: t['ink-muted'],
      act: t.act,
      ok: t.ok,
      warn: t.warn,
      danger: t.danger,
    },
    toneText: t['ink-inverse'],
    /** 条边与此刻线（骨架其余部分见 chartBase） */
    stroke: alpha(t.ink, 0.22),
    now: t.danger,
    nowBg: alpha(t.danger, 0.12),
    nowLine: alpha(t.danger, 0.35),
  };
});

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
  const c = paint.value;

  for (let i = 0; i < nodes.length - 1; i++) {
    items.push({
      name: `${nodes[i].label} → ${nodes[i + 1].label}`,
      value: [nodes[i].timestamp, nodes[i + 1].timestamp, 0],
      fill: c.spine,
      textFill: c.spineText,
      quiet: true,
      raw: { type: 'flight_status', from: nodes[i].label, to: nodes[i + 1].label, fromTime: nodes[i].timestamp, toTime: nodes[i + 1].timestamp },
    });
  }

  nodes.forEach((node) => {
    const isPlan = node.kind === 'plan';
    items.push({
      name: node.label,
      value: [node.timestamp - 60000, node.timestamp + 60000, 0],
      fill: isPlan ? c.plan : c.actual,
      textFill: isPlan ? c.planText : c.actualText,
      quiet: isPlan,
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
      fill: paint.value.tone[CASE_TONE[normalizeCaseStatusValue(c.status)] ?? 'mute'],
      textFill: paint.value.toneText,
      quiet: false,
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

const { theme } = useTheme();
watch(theme, () => {
  chartInstance?.dispose();
  chartInstance = null;
  scheduleRender();
});

function buildTooltip(raw: GanttRaw): string {
  if (raw.type === 'flight_status') {
    return `
      <div class="gantt-tip__title">${FLIGHT_STATUS_LANE}</div>
      <div>${raw.from} → ${raw.to}</div>
      <div>时间: ${formatTooltipTime(raw.fromTime)} - ${formatTooltipTime(raw.toTime)}</div>
    `;
  }
  if (raw.type === 'time_node') {
    return `
      <div class="gantt-tip__title">${raw.label}</div>
      <div>时间: ${formatTooltipTime(raw.time)}</div>
    `;
  }
  return `
    <div class="gantt-tip__title">${raw.name}</div>
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
    // 不用 echarts 内建 dark 主题：明暗全由 token 现算，见 paint
    chartInstance = echarts.init(element, null, { renderer: 'canvas' });
  }

  const items = seriesData.value;
  if (items.length === 0) {
    chartInstance.clear();
    return;
  }

  const c = paint.value;
  const base = chartBase.value;
  const font = base.fontFamily;

  // Fit Y-axis gutter to longest label (was fixed 140px → large blank when only short lanes).
  const longestLabelChars = yAxisData.value.reduce((max, label) => {
    const len = Array.from(String(label || '')).length;
    return Math.max(max, len);
  }, 4);
  const yLabelWidth = Math.min(108, Math.max(64, Math.ceil(longestLabelChars * 12.5) + 4));
  const gridLeft = yLabelWidth + 14;

  const option = {
    animation: false,
    ...base.root,
    tooltip: {
      ...base.tooltip,
      trigger: 'item',
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
      ...base.axis,
      axisLabel: {
        ...base.axis.axisLabel,
        formatter: (value: number) => formatAxisTime(value),
      },
    },
    yAxis: {
      type: 'category',
      inverse: true,
      data: yAxisData.value,
      ...base.laneAxis,
      axisLabel: {
        ...base.laneAxis.axisLabel,
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
        ...base.zoom,
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

        return {
          type: 'group',
          children: [
            {
              type: 'rect',
              shape: { ...clippedRect, r: 4 },
              style: {
                fill: dataItem.fill,
                stroke: c.stroke,
                lineWidth: 1,
                opacity: dataItem.quiet ? 0.85 : 1
              }
            },
            {
              type: 'text',
              style: {
                x: clippedRect.x + 6,
                y: clippedRect.y + clippedRect.height / 2,
                text: clippedRect.width > 60 ? dataItem.name : '',
                verticalAlign: 'middle',
                fill: dataItem.textFill,
                font: `500 11px ${font}`,
                fontSize: 11,
                fontWeight: 500,
                fontFamily: font,
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
    },
    // echarts 6 回归：markLine 直接挂在 custom series 上会在 SeriesData 初始化时
    // 对 y 类目轴取 ordinalMeta 崩溃（getOrdinalMeta of undefined）。
    // 挪到一条空的隐形 line series 上承载「现在」竖线，行为不变。
    {
      type: 'line',
      data: [],
      silent: true,
      markLine: {
        symbol: ['none', 'none'],
        silent: true,
        lineStyle: { color: c.now, width: 1, type: 'dashed' },
        label: {
          show: true,
          formatter: '现在',
          color: c.now,
          fontWeight: 600,
          fontFamily: font,
          backgroundColor: c.nowBg,
          borderColor: c.nowLine,
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

.gantt-chart-container :deep(.gantt-tip__title) {
  font-weight: var(--fw-semibold);
  margin-bottom: var(--s1);
}

.timeline-empty {
  color: var(--ink-muted);
  font-size: var(--fs-body);
  text-align: center;
  padding: 40px 0;
}
</style>
