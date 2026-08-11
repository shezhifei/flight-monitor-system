import { computed } from 'vue';
import { useTheme } from './useTheme';

export function useChartTheme() {
  const { theme } = useTheme();

  const chartColors = computed(() => {
    const isDark = theme.value === 'dark';

    if (isDark) {
      return {
        axisText: '#8298b5',
        axisLine: 'rgba(100,140,190,0.11)',
        gridLine: 'rgba(100,140,190,0.08)',
        itemText: '#e8f0fa',
        itemBg: 'rgba(14, 25, 42, 0.86)',
        nowLine: '#ef5350',
        focusStroke: '#4db8ff',
        focusFill: 'rgba(77, 184, 255, 0.12)',
        statusPending: '#f0a030',
        statusAssigned: '#3daeff',
        statusProgress: '#2ec47e',
        statusCompleted: '#2ec47e',
        statusCancelled: '#627a97',
        statusAlert: '#ef5350',
        statusLock: '#627a97',
        splitLine: 'rgba(100,140,190,0.06)',
        lockMarker: '#627a97',
        zoomBg: 'rgba(255,255,255,0.02)',
        emptyText: '#627a97',
        laneLabel: '#8298b5',
        laneFocusFill: 'rgba(77, 184, 255, 0.10)',
        laneFocusStroke: 'rgba(77, 184, 255, 0.22)',
        laneSecondaryFocusFill: 'rgba(77, 184, 255, 0.05)',
        laneSecondaryFocusStroke: 'rgba(77, 184, 255, 0.12)',
        laneFocusLabelText: '#4db8ff',
        laneFocusLabelBg: 'rgba(77, 184, 255, 0.14)',
        laneSecondaryFocusLabelText: '#6bb8ff',
        laneSecondaryFocusLabelBg: 'rgba(77, 184, 255, 0.08)',
        tooltipBorder: 'rgba(100,140,190,0.15)',
        zoomBorder: 'rgba(100,140,190,0.12)',
        zoomFiller: 'rgba(77, 184, 255, 0.2)',
        nowLabelText: '#ef5350',
        nowLabelBg: 'rgba(239, 83, 80, 0.12)',
        nowLabelBorder: 'rgba(239, 83, 80, 0.35)',
        itemHighlightStroke: '#8298b5',
        itemConflictStroke: '#ef5350',
        itemSummaryStroke: '#3daeff',
        itemStroke: 'rgba(100,140,190,0.18)',
        detailSubText: '#627a97',
        summaryText: '#e8f0fa',
        metaText: '#8298b5',
      };
    }

    return {
      axisText: '#5f7082',
      axisLine: '#8a97a8',
      gridLine: 'rgba(15, 23, 42, 0.08)',
      itemText: '#ffffff',
      itemBg: '#ffffff',
      nowLine: '#FF3B30',
      focusStroke: '#007AFF',
      focusFill: 'rgba(0, 122, 255, 0.08)',
      statusPending: '#D97706',
      statusAssigned: '#2563EB',
      statusProgress: '#0F9D8A',
      statusCompleted: '#2F9E44',
      statusCancelled: '#94A3B8',
      statusAlert: '#D64545',
      statusLock: '#475569',
      splitLine: 'rgba(15, 23, 42, 0.08)',
      lockMarker: '#475569',
      zoomBg: 'rgba(255,255,255,0.74)',
      emptyText: '#6a7788',
      laneLabel: '#33485f',
      laneFocusFill: 'rgba(0, 122, 255, 0.08)',
      laneFocusStroke: 'rgba(0, 122, 255, 0.22)',
      laneSecondaryFocusFill: 'rgba(0, 122, 255, 0.04)',
      laneSecondaryFocusStroke: 'rgba(0, 122, 255, 0.12)',
      laneFocusLabelText: '#0f3e73',
      laneFocusLabelBg: 'rgba(0, 122, 255, 0.14)',
      laneSecondaryFocusLabelText: '#29547f',
      laneSecondaryFocusLabelBg: 'rgba(0, 122, 255, 0.08)',
      tooltipBorder: 'rgba(15, 23, 42, 0.1)',
      zoomBorder: 'rgba(15, 23, 42, 0.12)',
      zoomFiller: 'rgba(11,119,227,0.2)',
      nowLabelText: '#FF3B30',
      nowLabelBg: 'rgba(255, 59, 48, 0.12)',
      nowLabelBorder: 'rgba(255, 59, 48, 0.35)',
      itemHighlightStroke: '#12293f',
      itemConflictStroke: '#D64545',
      itemSummaryStroke: '#1D4ED8',
      itemStroke: 'rgba(15, 23, 42, 0.22)',
      detailSubText: '#5f7082',
      summaryText: '#203246',
      metaText: '#5f7082',
    };
  });

  return { chartColors };
}
