import { computed } from 'vue';
import { alpha, mix, useSignalTokens } from './useSignalTokens';

/**
 * 图表用色（信号面 §3.4）。
 *
 * 这里不再存第二套色板：每个键都从 :root 上的 token 现算，主题切换后
 * 由 useSignalTokens 重读，画布与 DOM 永远同声。键名保持不变，
 * 调用方（甘特图 / 趋势图 / 里程碑）无需改动。
 *
 * 命名读法：
 * - 轴/网格/分隔 = 线（line / line-strong），不是「浅灰」
 * - 状态 = 四声（act 进行 / ok 已成 / warn 警 / danger 危 / ink-muted 常态）
 * - 实色条上的字用该声的 on 色，透明条上的字用墨
 */
export function useChartTheme() {
  const { tokens } = useSignalTokens();

  const chartColors = computed(() => {
    const t = tokens.value;

    return {
      // ---- 骨架：轴、网格、空态 ----
      axisText: t['ink-subtle'],
      axisLine: t['line-strong'],
      gridLine: t.line,
      splitLine: t.line,
      emptyText: t['ink-muted'],
      laneLabel: t.ink,
      tooltipBorder: t.line,
      zoomBorder: t.line,
      zoomBg: alpha(t['face-raised'], 0.74),
      zoomFiller: alpha(t.act, 0.2),

      // ---- 条上的字 ----
      itemText: t['act-on'],
      itemBg: t['face-raised'],
      summaryText: t.ink,
      metaText: t['ink-subtle'],
      detailSubText: t['ink-subtle'],

      // ---- 四声：状态画在对象上 ----
      statusPending: t.warn,
      statusAssigned: t.act,
      statusProgress: t.ok,
      statusCompleted: t.ok,
      statusCancelled: t['ink-muted'],
      statusAlert: t.danger,
      statusLock: t['ink-subtle'],
      lockMarker: t['ink-subtle'],

      // ---- 此刻线（危声，最高一档） ----
      nowLine: t.danger,
      nowLabelText: t.danger,
      nowLabelBg: alpha(t.danger, 0.12),
      nowLabelBorder: alpha(t.danger, 0.35),

      // ---- 交感 / 持守：焦点与选中一律行动色 ----
      focusStroke: t.act,
      focusFill: alpha(t.act, 0.08),
      itemHighlightStroke: t.ink,
      itemConflictStroke: t.danger,
      itemSummaryStroke: t.act,
      itemStroke: alpha(t.ink, 0.22),
      laneFocusFill: alpha(t.act, 0.08),
      laneFocusStroke: alpha(t.act, 0.22),
      laneSecondaryFocusFill: alpha(t.act, 0.04),
      laneSecondaryFocusStroke: alpha(t.act, 0.12),
      laneFocusLabelText: t.act,
      laneFocusLabelBg: alpha(t.act, 0.14),
      laneSecondaryFocusLabelText: mix(t.act, t['ink-subtle'], 0.6),
      laneSecondaryFocusLabelBg: alpha(t.act, 0.08),
    };
  });

  /**
   * 图表骨架（信号面 §3.4）：轴、网格、提示框、缩放条的常驻形。
   * 画布不继承 CSS，字族要显式传进 ECharts；这里一次给全，
   * 各图表 `...chartBase.value.axis` 摊开即可，不要再各自抄一份轴样式。
   */
  const chartBase = computed(() => {
    const t = tokens.value;
    const font = t.sans;

    return {
      fontFamily: font,
      /** option 顶层：底透明（吃页面工作面），字族全局生效 */
      root: {
        backgroundColor: 'transparent',
        textStyle: { fontFamily: font },
      },
      /** tooltip：抬升面 + 一根细线，不投重影 */
      tooltip: {
        confine: true,
        backgroundColor: t['face-raised'],
        borderColor: t.line,
        borderWidth: 1,
        textStyle: { color: t.ink, fontFamily: font, fontSize: 12 },
      },
      /** 轴：线用 line-strong，刻字用次墨，网格用 line 虚线，刻度尖收掉 */
      axis: {
        axisLine: { lineStyle: { color: t['line-strong'] } },
        axisTick: { show: false },
        axisLabel: { color: t['ink-subtle'], fontSize: 11, fontFamily: font, hideOverlap: true },
        splitLine: { lineStyle: { color: t.line, type: 'dashed' as const } },
      },
      /** 类目轴（行名）：字用墨，比刻字重一档 */
      laneAxis: {
        axisLine: { show: false },
        axisTick: { show: false },
        axisLabel: { color: t.ink, fontSize: 12, fontFamily: font, fontWeight: 500 },
      },
      /** 缩放条 */
      zoom: {
        borderColor: t.line,
        backgroundColor: alpha(t['face-raised'], 0.74),
        fillerColor: alpha(t.act, 0.2),
      },
    };
  });

  return { chartColors, chartBase };
}
