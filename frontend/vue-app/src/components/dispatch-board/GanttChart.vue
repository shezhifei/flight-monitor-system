<script setup lang="ts">
/**
 * GanttChart.vue — Core ECharts gantt chart for the Dispatch Board.
 *
 * Ports the CORE rendering logic from legacy dispatch_board_gantt.js:
 *   - initChart (line ~2749): chart initialization
 *   - renderChart (line ~2914): main chart rendering with series config
 *   - renderDispatchItem (line ~3176): custom renderItem for timeline bars
 *   - renderFocusedLaneBand (line ~3144): focused lane background
 *   - Chart theme constants (CHART_THEME, STATUS_COLORS, etc.)
 *   - Time axis, category axis, dataZoom, markLine (now line)
 *   - Tooltip formatter
 *
 * Intentionally deferred to T14:
 *   - Worker/replan logic
 *   - Chat/AI drawer logic
 *   - Complex panel interactions (analytics, conflict, scenario, replan)
 *   - Draft multi-select, order detail drawers
 */

import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from 'vue';
import type { DispatchOrder, ResourceFocus, SafetyProgressMap, TimelineData, TimelineLane } from '@/composables/useDispatchBoardData';
import {
  SAFETY_GATE_COLORS,
  STATUS_LABELS,
  STATUS_ORDER,
  STATUS_SYMBOLS,
} from '@/composables/useDispatchBoardData';
import { initChart, echarts } from '@/lib/echarts';
import type { ECharts, ECElementEvent } from 'echarts';
import { useChartTheme } from '@/composables/useChartTheme';

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

interface Props {
  timelineData: TimelineData | null;
  windowStartMs: number;
  windowEndMs: number;
  safetyProgress?: SafetyProgressMap;
  resourceFocus?: ResourceFocus | null;
  safetyGateFilter?: 'all' | 'blocked' | 'pending' | 'ready';
  highlightedItemId?: string | null;
  selectedOrderIds?: string[];
  impactedOrderIds?: string[];
}

const props = withDefaults(defineProps<Props>(), {
  timelineData: null,
  safetyProgress: () => ({}),
  resourceFocus: null,
  safetyGateFilter: 'all',
  highlightedItemId: null,
  selectedOrderIds: () => [],
  impactedOrderIds: () => [],
});

const emit = defineEmits<{
  (e: 'item-dblclick', params: ECElementEvent): void;
  (e: 'item-click', params: ECElementEvent): void;
}>();

// ---------------------------------------------------------------------------
// Theme — reactive via useChartTheme composable
// ---------------------------------------------------------------------------

const { chartColors } = useChartTheme();

const SEMANTIC_COLORS = computed(() => ({
  alert: chartColors.value.statusAlert,
  lock: chartColors.value.statusLock,
  summaryText: chartColors.value.summaryText,
  metaText: chartColors.value.metaText,
}));

const STATUS_COLORS = computed(() => ({
  pending: chartColors.value.statusPending,
  assigned: chartColors.value.statusAssigned,
  in_progress: chartColors.value.statusProgress,
  completed: chartColors.value.statusCompleted,
  cancelled: chartColors.value.statusCancelled,
}));

const CHART_THEME = computed(() => ({
  emptyText: chartColors.value.emptyText,
  axisLine: chartColors.value.axisLine,
  axisLabel: chartColors.value.axisText,
  laneLabel: chartColors.value.laneLabel,
  splitLine: chartColors.value.splitLine,
  zoomBorder: chartColors.value.zoomBorder,
  zoomBg: chartColors.value.zoomBg,
  zoomFiller: chartColors.value.zoomFiller,
  tooltipBorder: chartColors.value.tooltipBorder,
  nowLine: chartColors.value.nowLine,
  nowLabelText: chartColors.value.nowLabelText,
  nowLabelBg: chartColors.value.nowLabelBg,
  nowLabelBorder: chartColors.value.nowLabelBorder,
  itemHighlightStroke: chartColors.value.itemHighlightStroke,
  itemConflictStroke: chartColors.value.statusAlert,
  itemSummaryStroke: chartColors.value.itemSummaryStroke,
  itemStroke: chartColors.value.itemStroke,
  laneFocusFill: chartColors.value.laneFocusFill,
  laneFocusStroke: chartColors.value.laneFocusStroke,
  laneSecondaryFocusFill: chartColors.value.laneSecondaryFocusFill,
  laneSecondaryFocusStroke: chartColors.value.laneSecondaryFocusStroke,
  laneFocusLabelText: chartColors.value.laneFocusLabelText,
  laneFocusLabelBg: chartColors.value.laneFocusLabelBg,
  laneSecondaryFocusLabelText: chartColors.value.laneSecondaryFocusLabelText,
  laneSecondaryFocusLabelBg: chartColors.value.laneSecondaryFocusLabelBg,
  itemText: chartColors.value.itemText,
  detailSubText: chartColors.value.detailSubText,
}));

// ---------------------------------------------------------------------------
// Chart instance
// ---------------------------------------------------------------------------

const chartContainer = ref<HTMLElement | null>(null);
let chartInstance: ECharts | null = null;
let nowTickTimer: number | null = null;

// ---------------------------------------------------------------------------
// Pure helper functions (ported from legacy)
// ---------------------------------------------------------------------------

function toMs(value: unknown): number {
  if (!value) return 0;
  if (typeof value === 'number') return value;
  const parsed = Date.parse(String(value));
  return Number.isNaN(parsed) ? 0 : parsed;
}

function formatAxisTime(value: number): string {
  const date = new Date(value);
  const h = String(date.getHours()).padStart(2, '0');
  const m = String(date.getMinutes()).padStart(2, '0');
  return `${h}:${m}`;
}

function formatDateTime(value: unknown): string {
  const ms = toMs(value);
  if (!ms) return '-';
  const date = new Date(ms);
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  const hour = String(date.getHours()).padStart(2, '0');
  const minute = String(date.getMinutes()).padStart(2, '0');
  return `${month}-${day} ${hour}:${minute}`;
}

function escapeHtml(str: string): string {
  return String(str ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

function statusToCode(status: string | null | undefined): number {
  switch (status) {
    case 'pending': return 0;
    case 'assigned': return 1;
    case 'in_progress': return 2;
    case 'completed': return 3;
    case 'cancelled': return 4;
    default: return 0;
  }
}

function statusCodeToKey(statusCode: number): string {
  switch (statusCode) {
    case 0: return 'pending';
    case 1: return 'assigned';
    case 2: return 'in_progress';
    case 3: return 'completed';
    case 4: return 'cancelled';
    default: return 'pending';
  }
}

function statusCodeToColor(statusCode: number): string {
  const sc = STATUS_COLORS.value;
  switch (statusCode) {
    case 0: return sc.pending;
    case 1: return sc.assigned;
    case 2: return sc.in_progress;
    case 3: return sc.completed;
    case 4: return sc.cancelled;
    default: return sc.pending;
  }
}

function adjustColorAlpha(hexColor: string, alpha: number): string {
  if (!hexColor || typeof hexColor !== 'string') return hexColor;
  const hex = hexColor.replace('#', '');
  if (hex.length < 6) return hexColor;
  const r = parseInt(hex.substring(0, 2), 16);
  const g = parseInt(hex.substring(2, 4), 16);
  const b = parseInt(hex.substring(4, 6), 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

function normalizePublicationState(value: unknown): string {
  return String(value || '').trim().toLowerCase();
}

function isDraftOrder(raw: DispatchOrder): boolean {
  if (!raw) return false;
  const pubState = normalizePublicationState(raw.publication_state);
  const status = String(raw.status || '').trim().toLowerCase();
  const taskCrew = raw.task_crew;
  const hasNoCrew = !taskCrew || (typeof taskCrew === 'object' && (!taskCrew.members || taskCrew.members.length === 0));
  return pubState === 'prepublished' && status === 'pending' && hasNoCrew;
}

function isLockedOrder(raw: DispatchOrder): boolean {
  const level = String(raw?.lock_level || '').trim().toLowerCase();
  return Boolean(level) && !['optimizable', 'none'].includes(level);
}

function hasQualificationGap(raw: DispatchOrder): boolean {
  return Array.isArray(raw?.qualification_gap) && raw.qualification_gap.length > 0;
}

function hasSemanticAlert(raw: DispatchOrder, options: { isImpacted?: boolean; safetyGateState?: string }): boolean {
  return Boolean(options.isImpacted)
    || Boolean(String(raw?.conflict_reason || '').trim())
    || Boolean(String(raw?.availability_reason || '').trim())
    || hasQualificationGap(raw)
    || options.safetyGateState === 'blocked';
}

function getTimelineSafetyGateState(progress: { enforced?: boolean; ready?: boolean; failed_required_count?: number } | null): string {
  if (!progress || !progress.enforced) return 'none';
  if (progress.ready) return 'ready';
  if (Number(progress.failed_required_count || 0) > 0) return 'blocked';
  return 'pending';
}

function isTimelineItemImpacted(item: DispatchOrder): boolean {
  const impactedOrderSet = new Set(
    (Array.isArray(props.impactedOrderIds) ? props.impactedOrderIds : [])
      .map((orderId) => String(orderId || '').trim())
      .filter(Boolean),
  );

  if (impactedOrderSet.size > 0) {
    const currentOrderId = String(item.order_id || item.id || '').trim();
    if (currentOrderId && impactedOrderSet.has(currentOrderId)) {
      return true;
    }

    const relatedOrderIds = Array.isArray(item.related_order_ids)
      ? item.related_order_ids.map((orderId) => String(orderId || '').trim()).filter(Boolean)
      : [];
    if (relatedOrderIds.some((orderId) => impactedOrderSet.has(orderId))) {
      return true;
    }
  }

  return Boolean(item.conflict_reason) || Boolean(item.availability_reason);
}

const SAFETY_GATE_FILTER_LABELS: Record<string, string> = {
  all: '全部任务',
  blocked: '仅清单阻断',
  pending: '仅清单待补齐',
  ready: '仅清单就绪',
};

function buildTimelineEmptyMessage(timeline: TimelineData | null): string {
  if (!timeline) return '暂无时间线数据\n请稍后刷新重试';
  const laneCount = Array.isArray(timeline.lanes) ? timeline.lanes.length : 0;
  if (laneCount === 0) return '当前视角暂无可展示资源行\n建议切换视角或检查基础资源配置';
  if (props.safetyGateFilter !== 'all') return `当前筛选下无任务（${SAFETY_GATE_FILTER_LABELS[props.safetyGateFilter] ?? props.safetyGateFilter}）\n建议切换门禁筛选或点击"当前时间"重试`;
  return '当前时间窗无任务\n建议点击"当前时间"或切换航站楼后重试';
}

function summarizeRelatedStatuses(relatedOrders: Array<{ status?: string } | null> | undefined): Record<string, number> {
  const counts: Record<string, number> = {};
  const entries = Array.isArray(relatedOrders) ? relatedOrders : [];
  for (const order of entries) {
    const statusKey = String(order?.status || 'pending').trim() || 'pending';
    counts[statusKey] = Number(counts[statusKey] || 0) + 1;
  }
  return counts;
}

function buildSummaryStatusSegments(rawItem: DispatchOrder, rect: { x: number; y: number; width: number; height: number }): Array<{ type: string; shape: { x: number; y: number; width: number; height: number }; style: { fill: string; opacity: number }; silent: true }> {
  const counts = summarizeRelatedStatuses(rawItem?.related_orders as Array<{ status?: string }> | undefined);
  const statuses = STATUS_ORDER.filter((statusKey) => Number(counts[statusKey] || 0) > 0);
  if (!rect || rect.width < 36 || statuses.length <= 1) return [];

  const total = statuses.reduce((sum, statusKey) => sum + Number(counts[statusKey] || 0), 0);
  if (total <= 0) return [];

  const bandHeight = Math.min(4, Math.max(2, rect.height * 0.22));
  let cursorX = rect.x;
  return statuses.map((statusKey, index) => {
    const rawWidth = index === statuses.length - 1
      ? (rect.x + rect.width - cursorX)
      : Math.max(3, Math.round(rect.width * (Number(counts[statusKey] || 0) / total)));
    const shape = {
      type: 'rect' as const,
      shape: {
        x: cursorX,
        y: rect.y + rect.height - bandHeight,
        width: rawWidth,
        height: bandHeight,
      },
      style: {
        fill: statusCodeToColor(statusToCode(statusKey)),
        opacity: 0.92,
      },
      silent: true as const,
    };
    cursorX += rawWidth;
    return shape;
  });
}

function buildStatusTextureShapes(statusKey: string, rect: { x: number; y: number; width: number; height: number }): Array<Record<string, unknown>> {
  const shapes: Array<Record<string, unknown>> = [];
  const minWidth = 18;
  const minHeight = 7;
  if (!rect || rect.width < minWidth || rect.height < minHeight) return shapes;

  if (statusKey === 'pending') {
    const step = 8;
    const stroke = 'rgba(255, 255, 255, 0.24)';
    for (let x = rect.x - rect.height; x < rect.x + rect.width; x += step) {
      shapes.push({
        type: 'line',
        shape: {
          x1: x,
          y1: rect.y + rect.height,
          x2: x + rect.height,
          y2: rect.y,
        },
        style: { stroke, lineWidth: 1 },
        silent: true,
      });
    }
    return shapes;
  }

  if (statusKey === 'completed') {
    const step = 8;
    const stroke = 'rgba(255, 255, 255, 0.18)';
    for (let x = rect.x - rect.height; x < rect.x + rect.width; x += step) {
      shapes.push({
        type: 'line',
        shape: {
          x1: x,
          y1: rect.y,
          x2: x + rect.height,
          y2: rect.y + rect.height,
        },
        style: { stroke, lineWidth: 1 },
        silent: true,
      });
    }
    return shapes;
  }

  return shapes;
}

// ---------------------------------------------------------------------------
// Tooltip rendering
// ---------------------------------------------------------------------------

function renderTooltip(item: DispatchOrder | null | undefined): string {
  if (!item) return '无数据';

  const parts: string[] = [];
  const title = item.is_flight_summary
    ? `${escapeHtml(String(item.flight_no || '-'))}`
    : `${escapeHtml(String(item.flight_no || '-'))} | ${escapeHtml(String(item.task_type_name || item.task_type || '-'))}`;
  parts.push(`<div style="font-weight:700;margin-bottom:4px;">${title}</div>`);

  // Order semantic meta (simplified)
  if (item.status) {
    parts.push(`<div>状态：${STATUS_LABELS[item.status as keyof typeof STATUS_LABELS] ?? item.status}</div>`);
  }

  parts.push(`<div>时间：${escapeHtml(formatDateTime(item.start_time))} - ${escapeHtml(formatDateTime(item.end_time))}</div>`);

  if (isTimelineItemImpacted(item)) {
    parts.push('<div>冲突治理：当前关注任务</div>');
  }

  if (item.team_name || item.individual_username) {
    parts.push(`<div>执行：${escapeHtml(item.individual_username || item.team_name || '-')}</div>`);
  }

  if (Array.isArray(item.equipment_codes) && item.equipment_codes.length > 0) {
    parts.push(`<div>设备：${escapeHtml(item.equipment_codes.join(' / '))}</div>`);
  }

  if (!item.is_flight_summary && item.order_id) {
    const progress = props.safetyProgress?.[item.order_id] ?? null;
    if (progress && progress.enforced) {
      const gateState = getTimelineSafetyGateState(progress);
      const gateLabel = gateState === 'ready' ? '清单就绪' : (gateState === 'blocked' ? '清单阻断' : '清单待补齐');
      const completedRequired = Number(progress.completed_required || 0);
      const requiredTotal = Number(progress.required_total || 0);
      parts.push(`<div>安全门禁：${escapeHtml(gateLabel)}（${completedRequired}/${requiredTotal}）</div>`);
    }
  }

  if (item.is_flight_summary) {
    const relatedCount = Array.isArray(item.related_order_ids) ? item.related_order_ids.length : 0;
    parts.push(`<div>覆盖派工：${relatedCount} 项</div>`);
  } else {
    if (item.publication_state) {
      const pubState = String(item.publication_state).trim();
      const pubLabels: Record<string, string> = {
        prepublished: '预发布',
        published: '已发布',
        archived: '已归档',
      };
      parts.push(`<div>发布状态：${escapeHtml(pubLabels[pubState] ?? pubState)}</div>`);
    }
    if (isLockedOrder(item)) {
      const lockLevel = String(item.lock_level || '').trim();
      parts.push(`<div>优化约束：${escapeHtml(lockLevel)}</div>`);
    }
    if (String(item.conflict_reason || '').trim()) {
      parts.push(`<div>冲突原因：${escapeHtml(String(item.conflict_reason).trim())}</div>`);
    } else if (String(item.availability_reason || '').trim()) {
      parts.push(`<div>资源约束：${escapeHtml(String(item.availability_reason).trim())}</div>`);
    }
  }

  return parts.join('');
}

// ---------------------------------------------------------------------------
// Focused lanes computation
// ---------------------------------------------------------------------------

const focusedLaneId = computed<string>(() => {
  const focus = props.resourceFocus;
  return String(focus?.primary_lane_id ?? focus?.lane_id ?? '').trim();
});

const focusedLaneIds = computed<string[]>(() => {
  const focus = props.resourceFocus;
  const primary = focusedLaneId.value;
  const ids = Array.isArray(focus?.lane_ids) && focus.lane_ids.length > 0
    ? focus.lane_ids.map((item) => String(item || '').trim()).filter(Boolean)
    : (primary ? [primary] : []);
  return ids;
});

// ---------------------------------------------------------------------------
// Filtered timeline items (safety gate filter)
// ---------------------------------------------------------------------------

const filteredItems = computed<DispatchOrder[]>(() => {
  const items = Array.isArray(props.timelineData?.items) ? props.timelineData.items : [];
  if (props.safetyGateFilter === 'all') return [...items] as DispatchOrder[];

  return items.filter((item) => {
    if (!item) return false;
    if (item.is_flight_summary) {
      const relatedOrderIds = Array.isArray(item.related_order_ids) ? item.related_order_ids : [];
      return relatedOrderIds.some((orderId: unknown) => {
        const progress = props.safetyProgress?.[String(orderId)] ?? null;
        const gateState = getTimelineSafetyGateState(progress);
        return gateState === props.safetyGateFilter;
      });
    }
    const progress = props.safetyProgress?.[String(item.order_id)] ?? null;
    const gateState = getTimelineSafetyGateState(progress);
    return gateState === props.safetyGateFilter;
  }) as DispatchOrder[];
});

// ---------------------------------------------------------------------------
// ECharts custom renderItem: renderFocusedLaneBand
// ---------------------------------------------------------------------------

function renderFocusedLaneBand(params: Record<string, unknown>, api: { value: (index: number) => number; coord: (value: [number, number]) => [number, number]; size: (value: [number, number]) => [number, number] }) {
  const laneIndex = Number(api.value(0) || 0);
  const isPrimaryLane = Number(api.value(1) || 0) === 1;
  const coord = api.coord([props.windowStartMs, laneIndex]);
  const laneHeight = Math.max(10, api.size([0, 1])[1]);

  const coordSys = params.coordSys as { x: number; y: number; width: number; height: number } | undefined;
  if (!coordSys) return null;

  const inputRect = {
    x: coordSys.x + 1,
    y: coord[1] - laneHeight / 2 + 1,
    width: Math.max(4, coordSys.width - 2),
    height: Math.max(8, laneHeight - 2),
  };
  const rect = echarts.graphic.clipRectByRect(inputRect, {
    x: coordSys.x,
    y: coordSys.y,
    width: coordSys.width,
    height: coordSys.height,
  });

  if (!rect) return null;

  return {
    type: 'rect',
    shape: { ...rect, r: 8 },
    style: {
      fill: isPrimaryLane ? CHART_THEME.value.laneFocusFill : CHART_THEME.value.laneSecondaryFocusFill,
      stroke: isPrimaryLane ? CHART_THEME.value.laneFocusStroke : CHART_THEME.value.laneSecondaryFocusStroke,
      lineWidth: 1,
    },
    silent: true,
  };
}

// ---------------------------------------------------------------------------
// ECharts custom renderItem: renderDispatchItem
// ---------------------------------------------------------------------------

function renderDispatchItem(params: { data: { raw: DispatchOrder }; coordSys: { x: number; y: number; width: number; height: number } }, api: { value: (index: number) => number; coord: (value: [number, number]) => [number, number]; size: (value: [number, number]) => [number, number] }) {
  const categoryIndex = api.value(2);
  const startCoord = api.coord([api.value(0), categoryIndex]);
  const endCoord = api.coord([api.value(1), categoryIndex]);
  const laneHeight = Math.max(8, api.size([0, 1])[1]);
  const subtrackIndex = Number(api.value(3) || 0);
  const subtrackCount = Math.max(1, Number(api.value(4) || 1));
  const statusCode = Number(api.value(5) || 0);
  const isHighlight = Number(api.value(6) || 0) === 1;
  const isSummary = Number(api.value(7) || 0) === 1;
  const isImpacted = Number(api.value(8) || 0) === 1;
  const isFocusedLane = Number(api.value(9) || 0) === 1;
  const isSecondaryFocusedLane = Number(api.value(10) || 0) === 1;
  const isSelected = Number(api.value(11) || 0) === 1;
  const rawItem = params.data.raw || {} as DispatchOrder;

  const safetyProgress = rawItem.order_id ? (props.safetyProgress?.[rawItem.order_id] ?? null) : null;
  const safetyGateState = getTimelineSafetyGateState(safetyProgress);
  const isDraft = isDraftOrder(rawItem);
  const isLocked = isLockedOrder(rawItem);
  const hasAlert = hasSemanticAlert(rawItem, { isImpacted, safetyGateState });

  const x = Math.min(startCoord[0], endCoord[0]);
  const width = Math.max(4, Math.abs(endCoord[0] - startCoord[0]));

  const lanePadding = isSummary ? 2 : 4;
  const subtrackGap = 1;
  const usableLaneHeight = Math.max(6, laneHeight - lanePadding * 2);
  const subtrackHeight = Math.max(6, (usableLaneHeight - (subtrackCount - 1) * subtrackGap) / subtrackCount);
  const y = startCoord[1] - usableLaneHeight / 2 + subtrackIndex * (subtrackHeight + subtrackGap);

  const coordSys = params.coordSys;
  const clippedRect = echarts.graphic.clipRectByRect({
    x,
    y,
    width,
    height: subtrackHeight,
  }, {
    x: coordSys.x,
    y: coordSys.y,
    width: coordSys.width,
    height: coordSys.height,
  });

  if (!clippedRect) return null;

  const shapeWithRadius = { ...clippedRect, r: isSummary ? 5 : 3 };

  const fillColor = statusCodeToColor(statusCode);
  const statusKey = statusCodeToKey(statusCode);
  const statusSymbol = STATUS_SYMBOLS[statusKey as keyof typeof STATUS_SYMBOLS] || '•';
  const labelText = rawItem.label || rawItem.task_type_name || rawItem.flight_no || '';

  const borderColor = hasAlert
    ? CHART_THEME.value.itemConflictStroke
    : (isHighlight || isSelected
      ? CHART_THEME.value.itemHighlightStroke
      : (isImpacted
        ? CHART_THEME.value.itemConflictStroke
        : (isFocusedLane
          ? CHART_THEME.value.itemSummaryStroke
          : (isSecondaryFocusedLane
            ? CHART_THEME.value.laneSecondaryFocusLabelText
            : (isSummary ? fillColor : CHART_THEME.value.itemStroke)))));

  const draftPrefix = isDraft ? '草稿 · ' : '';
  const summaryCount = Array.isArray(rawItem.related_order_ids) ? rawItem.related_order_ids.length : 0;
  const displayLabel = clippedRect.width > 64
    ? `${statusSymbol} ${draftPrefix}${labelText}${isSummary && summaryCount > 0 ? ` · ${summaryCount}项` : ''}`
    : '';

  const baseFill = isSummary
    ? adjustColorAlpha(fillColor, hasAlert ? 0.24 : 0.2)
    : (isDraft ? adjustColorAlpha(fillColor, 0.22) : fillColor);

  const rectStyle: Record<string, unknown> = {
    fill: baseFill,
    stroke: borderColor,
    lineWidth: isSummary ? 1.8 : (isHighlight || isSelected ? 2.5 : (isImpacted ? 1.8 : (isFocusedLane ? 1.6 : (isSecondaryFocusedLane ? 1.3 : 1)))),
    opacity: isSummary ? 0.98 : (isSelected ? 1 : (isDraft ? 0.96 : (isFocusedLane ? 0.96 : (isSecondaryFocusedLane ? 0.93 : 0.9)))),
    shadowBlur: isSelected ? 6 : (isFocusedLane ? 8 : (isSecondaryFocusedLane ? 4 : 0)),
    shadowColor: isSelected ? 'rgba(0,0,0,0.4)' : (isFocusedLane
      ? 'rgba(0, 122, 255, 0.18)'
      : (isSecondaryFocusedLane ? 'rgba(0, 122, 255, 0.12)' : 'transparent')),
  };
  if (isDraft) {
    rectStyle.lineDash = [6, 3];
  } else if (statusKey === 'cancelled') {
    rectStyle.lineDash = [2, 2];
  }

  const textureChildren = buildStatusTextureShapes(statusKey, clippedRect);
  const summarySegments = isSummary ? buildSummaryStatusSegments(rawItem, clippedRect) : [];
  const safetyGateMarker = (!isSummary && safetyGateState !== 'none' && clippedRect.width >= 22)
    ? {
        type: 'circle',
        shape: {
          cx: clippedRect.x + clippedRect.width - 6,
          cy: clippedRect.y + 6,
          r: 3.2,
        },
        style: {
          fill: SAFETY_GATE_COLORS[safetyGateState as keyof typeof SAFETY_GATE_COLORS] || SAFETY_GATE_COLORS.pending,
          stroke: 'rgba(255, 255, 255, 0.95)',
          lineWidth: 1,
        },
        silent: true,
      }
    : null;
  const conflictMarkerShape = hasAlert
    ? {
        type: 'rect',
        shape: {
          x: clippedRect.x,
          y: clippedRect.y,
          width: Math.min(4, Math.max(2, clippedRect.width * 0.08)),
          height: clippedRect.height,
        },
        style: {
          fill: CHART_THEME.value.itemConflictStroke,
          opacity: 0.92,
        },
        silent: true,
      }
    : null;
  const lockMarkerShape = isLocked && clippedRect.width >= 18
    ? {
        type: 'rect',
        shape: {
          x: clippedRect.x + 5,
          y: clippedRect.y + 4,
          width: 5,
          height: 5,
          r: 1.5,
        },
        style: {
          fill: '#ffffff',
          stroke: SEMANTIC_COLORS.value.lock,
          lineWidth: 1.1,
        },
        silent: true,
      }
    : null;

  const textShape = {
    type: 'text',
    style: {
      x: clippedRect.x + (hasAlert ? 8 : 6),
      y: clippedRect.y + clippedRect.height / 2,
      text: displayLabel,
      verticalAlign: 'middle',
      fill: isSummary ? SEMANTIC_COLORS.value.summaryText : CHART_THEME.value.itemText,
      fontSize: 12,
      fontWeight: isSummary ? 700 : 560,
      width: Math.max(20, clippedRect.width - (hasAlert ? 14 : 10)),
      overflow: 'truncate',
    },
    silent: true,
  };

  return {
    type: 'group',
    children: [
      {
        type: 'rect',
        shape: shapeWithRadius,
        style: rectStyle,
      },
      ...summarySegments,
      ...(safetyGateMarker ? [safetyGateMarker] : []),
      ...(conflictMarkerShape ? [conflictMarkerShape] : []),
      ...(lockMarkerShape ? [lockMarkerShape] : []),
      ...textureChildren,
      textShape,
    ],
  };
}

// ---------------------------------------------------------------------------
// Build chart data array
// ---------------------------------------------------------------------------

function buildChartData(): Array<{ value: [number, number, number, number, number, number, number, number, number, number, number, number]; raw: DispatchOrder }> {
  const lanes = Array.isArray(props.timelineData?.lanes) ? props.timelineData.lanes : [];
  const items = filteredItems.value;

  return items.map((item) => {
    const statusCode = statusToCode(item.status ?? null);
    const isHighlight = item.id === props.highlightedItemId ? 1 : 0;
    const isSummary = item.is_flight_summary ? 1 : 0;
    const isImpacted = isTimelineItemImpacted(item) ? 1 : 0;
    const laneId = String(item.lane_id || '').trim();
    const isFocusedLane = focusedLaneId.value && laneId === focusedLaneId.value ? 1 : 0;
    const isSecondaryFocusedLane = !isFocusedLane && focusedLaneIds.value.includes(laneId) ? 1 : 0;
    const isSelected = props.selectedOrderIds && props.selectedOrderIds.includes(String(item.order_id)) ? 1 : 0;
    const lane = lanes.find((l) => l.id === laneId);
    const laneIndex = lane ? lanes.indexOf(lane) : 0;

    return {
      value: [
        toMs(item.start_time),
        toMs(item.end_time),
        laneIndex,
        Number(item.lane_subtrack || 0),
        Number(item.lane_subtrack_count || 1),
        statusCode,
        isHighlight,
        isSummary,
        isImpacted,
        isFocusedLane,
        isSecondaryFocusedLane,
        isSelected,
      ],
      raw: item,
    };
  });
}

function buildFocusedLaneSeriesData() {
  const lanes = Array.isArray(props.timelineData?.lanes) ? props.timelineData.lanes : [];
  return lanes
    .filter((lane) => focusedLaneIds.value.includes(String(lane?.id || '').trim()))
    .map((lane) => ({
      value: [
        Number(lanes.indexOf(lane)),
        String(lane?.id || '').trim() === focusedLaneId.value ? 1 : 0,
      ],
    }));
}

// ---------------------------------------------------------------------------
// Y-axis labels with focus styling
// ---------------------------------------------------------------------------

function buildYAxisLabels(): string[] {
  const lanes = Array.isArray(props.timelineData?.lanes) ? props.timelineData.lanes : [];
  if (lanes.length === 0) return ['暂无资源行'];
  return lanes.map((lane) => lane.label ?? lane.id ?? '-');
}

function buildYAxisFormatter(lanes: TimelineLane[]) {
  return (_value: string, index: number) => {
    const lane = lanes[index];
    if (!lane) return _value;
    const laneId = String(lane.id || '').trim();
    if (focusedLaneId.value && laneId === focusedLaneId.value) {
      return `{focusedLane|${_value}}`;
    }
    if (focusedLaneIds.value.includes(laneId)) {
      return `{secondaryFocusedLane|${_value}}`;
    }
    return _value;
  };
}

// ---------------------------------------------------------------------------
// Chart rendering
// ---------------------------------------------------------------------------

function renderChart() {
  if (!chartInstance) return;

  const timeline = props.timelineData;
  const lanes = Array.isArray(timeline?.lanes) ? timeline.lanes : [];

  if (!timeline || !Array.isArray(timeline.items) || timeline.items.length === 0) {
    chartInstance.clear();
    chartInstance.setOption({
      graphic: {
        type: 'text',
        left: 'center',
        top: 'middle',
        style: {
          text: buildTimelineEmptyMessage(null),
          fontSize: 13,
          lineHeight: 20,
          textAlign: 'center',
          fill: CHART_THEME.value.emptyText,
        },
      },
    });
    return;
  }

  const chartData = buildChartData();
  const focusedLaneSeriesData = buildFocusedLaneSeriesData();
  const emptyMessage = buildTimelineEmptyMessage(timeline);

  const focusedLaneSeries = focusedLaneSeriesData.length > 0
    ? {
        id: 'dispatch-focused-lane',
        type: 'custom',
        silent: true,
        renderItem: renderFocusedLaneBand,
        encode: { y: 0 },
        data: focusedLaneSeriesData,
        z: 0,
      }
    : null;

  chartInstance.setOption({
    animation: false,
    backgroundColor: 'transparent',
    graphic: chartData.length > 0 ? [] : {
      type: 'text',
      left: 'center',
      top: 'middle',
      z: 10,
      style: {
        text: emptyMessage,
        lineHeight: 20,
        textAlign: 'center',
        fontSize: 13,
        fill: CHART_THEME.value.emptyText,
      },
    },
    tooltip: {
      trigger: 'item',
      confine: true,
      borderWidth: 1,
      borderColor: CHART_THEME.value.tooltipBorder,
      formatter: (params: ECElementEvent) => renderTooltip((params.data as { raw?: DispatchOrder } | undefined)?.raw),
    },
    grid: {
      left: 186,
      right: 20,
      top: 16,
      bottom: 38,
      containLabel: false,
    },
    xAxis: {
      type: 'time',
      min: props.windowStartMs,
      max: props.windowEndMs,
      axisLine: { lineStyle: { color: CHART_THEME.value.axisLine } },
      axisLabel: {
        color: CHART_THEME.value.axisLabel,
        fontSize: 13,
        formatter: formatAxisTime,
      },
      splitLine: {
        lineStyle: {
          color: CHART_THEME.value.splitLine,
          type: 'dashed',
        },
      },
    },
    yAxis: {
      type: 'category',
      inverse: true,
      data: buildYAxisLabels(),
      axisTick: { show: false },
      axisLine: { show: false },
      axisLabel: {
        color: CHART_THEME.value.laneLabel,
        fontSize: 13,
        width: 156,
        overflow: 'truncate',
        rich: {
          focusedLane: {
            color: CHART_THEME.value.laneFocusLabelText,
            backgroundColor: CHART_THEME.value.laneFocusLabelBg,
            borderRadius: 7,
            padding: [3, 8, 3, 8] as const,
            fontWeight: 700,
          },
          secondaryFocusedLane: {
            color: CHART_THEME.value.laneSecondaryFocusLabelText,
            backgroundColor: CHART_THEME.value.laneSecondaryFocusLabelBg,
            borderRadius: 7,
            padding: [3, 8, 3, 8] as const,
            fontWeight: 600,
          },
        },
        formatter: buildYAxisFormatter(lanes),
      },
    },
    dataZoom: [
      {
        type: 'inside',
        xAxisIndex: 0,
        zoomOnMouseWheel: true,
        moveOnMouseMove: true,
        moveOnMouseWheel: true,
      },
      {
        type: 'slider',
        xAxisIndex: 0,
        bottom: 8,
        height: 12,
        borderColor: CHART_THEME.value.zoomBorder,
        backgroundColor: CHART_THEME.value.zoomBg,
        fillerColor: CHART_THEME.value.zoomFiller,
        showDetail: false,
      },
    ],
    series: [
      ...(focusedLaneSeries ? [focusedLaneSeries] : []),
      {
        id: 'dispatch-series',
        type: 'custom',
        renderItem: renderDispatchItem,
        encode: {
          x: [0, 1],
          y: 2,
        },
        data: chartData,
      },
    ],
  }, true);
  updateNowLinePosition();
}

// ---------------------------------------------------------------------------
// Now-line overlay (CSS-driven, zero chart-engine wakeups)
// ---------------------------------------------------------------------------

const nowLineRef = ref<HTMLElement | null>(null);
const nowLineVisible = ref(false);

const nowLineStyle = computed(() => ({ borderLeftColor: chartColors.value.nowLine }));
const nowLabelStyle = computed(() => ({
  color: chartColors.value.nowLabelText,
  background: chartColors.value.nowLabelBg,
  borderColor: chartColors.value.nowLabelBorder,
}));

function updateNowLinePosition(): void {
  if (!chartInstance || !nowLineRef.value) return;
  const now = Date.now();
  if (now < props.windowStartMs || now > props.windowEndMs) {
    nowLineVisible.value = false;
    return;
  }
  const px = chartInstance.convertToPixel({ xAxisIndex: 0 }, now);
  if (typeof px !== 'number' || !Number.isFinite(px)) {
    nowLineVisible.value = false;
    return;
  }
  nowLineVisible.value = true;
  nowLineRef.value.style.transform = `translateX(${px}px)`;
}

function startNowTickTimer() {
  stopNowTickTimer();
  updateNowLinePosition();
  nowTickTimer = window.setInterval(updateNowLinePosition, 1000);
}

function stopNowTickTimer() {
  if (nowTickTimer) {
    clearInterval(nowTickTimer);
    nowTickTimer = null;
  }
}

// ---------------------------------------------------------------------------
// Resize handler
// ---------------------------------------------------------------------------

let resizeTimer: number | null = null;
function handleResize() {
  if (resizeTimer !== null) {
    window.clearTimeout(resizeTimer);
  }
  resizeTimer = window.setTimeout(() => {
    resizeTimer = null;
    chartInstance?.resize();
    updateNowLinePosition();
  }, 150);
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

onMounted(async () => {
  await nextTick();
  if (!chartContainer.value) return;

  chartInstance = initChart(chartContainer.value, null, { renderer: 'canvas' });
  chartInstance.on('dblclick', (params: ECElementEvent) => {
    emit('item-dblclick', params);
  });
  chartInstance.on('click', (params: ECElementEvent) => {
    emit('item-click', params);
  });
  chartInstance.on('dataZoom', () => updateNowLinePosition());
  window.addEventListener('resize', handleResize);
  renderChart();
  startNowTickTimer();
});

onBeforeUnmount(() => {
  window.removeEventListener('resize', handleResize);
  if (resizeTimer !== null) {
    window.clearTimeout(resizeTimer);
    resizeTimer = null;
  }
  stopNowTickTimer();
  if (chartInstance) {
    chartInstance.dispose();
    chartInstance = null;
  }
});

// Re-render when reactive props or theme change
watch(
  () => [props.timelineData, props.windowStartMs, props.windowEndMs, props.safetyProgress, props.resourceFocus, props.safetyGateFilter, props.highlightedItemId, props.selectedOrderIds, chartColors.value],
  () => {
    renderChart();
  },
);
</script>

<template>
  <div
    id="ganttChart"
    ref="chartContainer"
    class="gantt-chart-container"
  >
    <div
      v-show="nowLineVisible"
      ref="nowLineRef"
      class="gantt-now-line"
      :style="nowLineStyle"
    >
      <span class="gantt-now-label" :style="nowLabelStyle">现在</span>
    </div>
  </div>
</template>

<style scoped>
.gantt-chart-container {
  width: 100%;
  height: 100%;
  position: relative;
}
.gantt-now-line {
  position: absolute;
  top: 0;
  bottom: 0;
  left: 0;
  width: 0;
  border-left: 1px dashed var(--system-red);
  pointer-events: none;
  z-index: 3;
  will-change: transform;
}
.gantt-now-label {
  position: absolute;
  top: 4px;
  left: 4px;
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 12px;
  font-weight: 600;
  border: 1px solid transparent;
  white-space: nowrap;
}
</style>
