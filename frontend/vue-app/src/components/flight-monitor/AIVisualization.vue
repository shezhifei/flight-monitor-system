<script setup lang="ts">
import { computed, ref, onMounted, onBeforeUnmount, watch, nextTick } from 'vue';
import * as echarts from 'echarts';
import { useToast } from '@/composables/useToast';
import { downloadTextFile } from '@/lib/download';
import { useTheme } from '@/composables/useTheme';
import { useChartTheme } from '@/composables/useChartTheme';

const props = defineProps<{
  type?: string;
  data?: unknown | unknown[] | Record<string, unknown>;
}>();

const toast = useToast();

const isTable = computed(() => props.type === 'table');
const isBarChart = computed(() => props.type === 'bar_chart');
const isTimeline = computed(() => props.type === 'timeline');

const dataRecord = computed(() => (props.data && typeof props.data === 'object' && !Array.isArray(props.data) ? props.data : {}) as Record<string, unknown>);

const isInsightCard = computed(() => {
  if (props.data && typeof props.data === 'object' && !Array.isArray(props.data)) {
    return dataRecord.value.kind === 'history_report' || dataRecord.value.kind === 'event_timeline' || dataRecord.value.title;
  }
  return false;
});

const isAnyChart = computed(() => isBarChart.value || isTimeline.value);

const insightPayload = computed(() => {
  if (isInsightCard.value) return dataRecord.value;
  return null;
});

const tableRows = computed(() => {
  if (!isTable.value || !props.data) return [];
  const items = dataRecord.value.items;
  const rows = Array.isArray(props.data) ? props.data : (Array.isArray(items) ? items : []);
  return rows.slice(0, 20) as Record<string, unknown>[];
});

const tableHeaders = computed(() => {
  if (tableRows.value.length > 0) {
    return Object.keys(tableRows.value[0]).slice(0, 6);
  }
  return [];
});

// -- Chart logic --
const chartContainer = ref<HTMLElement | null>(null);
let chartInstance: echarts.ECharts | null = null;

const renderChart = () => {
  if (!chartContainer.value) return;
  if (!chartInstance) {
    chartInstance = echarts.init(chartContainer.value, isDark() ? 'dark' : null);
  }
  
  let options: echarts.EChartsCoreOption = {};
  
  if (isBarChart.value) {
    const groups = (dataRecord.value.group_by_status as Record<string, unknown> | undefined) || dataRecord.value || {};
    const labels: string[] = [];
    const values: number[] = [];
    if (typeof groups === 'object' && !Array.isArray(groups)) {
      for (const [k, v] of Object.entries(groups)) {
        labels.push(String(k));
        values.push(Number(v) || 0);
      }
    }
    options = {
      tooltip: { trigger: 'axis' },
      grid: { left: '3%', right: '4%', bottom: '3%', top: '10%', containLabel: true },
      xAxis: { type: 'value' },
      yAxis: { type: 'category', data: labels, axisLabel: { interval: 0, width: 80, overflow: 'truncate' } },
      series: [{ type: 'bar', data: values, itemStyle: { color: chartColors.value.focusStroke, borderRadius: [0, 4, 4, 0] } }]
    };
  } else if (isTimeline.value) {
    const items = Array.isArray(dataRecord.value.items) ? dataRecord.value.items : [];
    const dataPoints = items.slice(0, 30).map((item: Record<string, unknown>, i: number) => {
        const timeStr = (item.scheduled_departure || item.time || item.timestamp) as string | undefined;
        const timeVal = timeStr ? new Date(timeStr).getTime() : Date.now() + i * 1000;
        return [timeVal, i, (item.flight_number || item.flight_id || `事件 ${i+1}`) as string];
     });
     options = {
       tooltip: { 
         trigger: 'item',
         formatter: (params: Record<string, unknown>) => {
            const value = params.value as unknown[];
            return `${value[2]}<br/>${new Date(value[0] as number).toLocaleString()}`;
         }
      },
      grid: { left: '3%', right: '4%', bottom: '3%', top: '10%', containLabel: true },
      xAxis: { type: 'time', splitLine: { show: false } },
      yAxis: { type: 'value', show: false },
      series: [{ 
        type: 'scatter', 
        data: dataPoints,
        symbolSize: 10,
        itemStyle: { color: chartColors.value.nowLine }
      }]
    };
  }
  
  chartInstance.setOption(options, true);
};

const { isDark, theme } = useTheme();
const { chartColors } = useChartTheme();
watch(theme, () => {
  chartInstance?.dispose();
  chartInstance = null;
  nextTick(() => renderChart());
});

onMounted(() => {
  if (isAnyChart.value) {
    nextTick(() => renderChart());
  }
});

watch(() => props.data, () => {
  if (isAnyChart.value) {
    nextTick(() => renderChart());
  }
}, { deep: true });

onBeforeUnmount(() => {
  if (chartInstance) {
    chartInstance.dispose();
    chartInstance = null;
  }
});

// -- Export logic --
const exportFile = (content: string, filename: string, type: string) => {
  try {
    downloadTextFile({ content, filename, mimeType: type });
    toast.showToast('success', '洞察内容已导出', { duration: 3200 });
  } catch (error) {
    toast.showToast('error', `导出失败: ${error instanceof Error ? error.message : String(error)}`, { duration: 5000 });
  }
};

const handleExportMd = () => {
  if (!insightPayload.value) return;
  const md = String(insightPayload.value.markdown || '(未返回 Markdown 内容)');
  exportFile(md, `Insight_Export_${Date.now()}.md`, 'text/markdown;charset=utf-8');
};

const handleExportJson = () => {
  if (!insightPayload.value) return;
  const jsonPayload = (insightPayload.value.jsonPayload as Record<string, unknown> | undefined) || insightPayload.value;
  exportFile(JSON.stringify(jsonPayload, null, 2), `Insight_Export_${Date.now()}.json`, 'application/json;charset=utf-8');
};

</script>

<template>
  <div v-if="isTable || isInsightCard || isAnyChart" class="ai-chat-viz">
    <div v-if="isTable && tableRows.length > 0" class="viz-table-wrapper">
      <table class="viz-table">
        <thead>
          <tr>
            <th v-for="h in tableHeaders" :key="h">
              {{ h }}
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(row, idx) in tableRows" :key="idx">
            <td v-for="h in tableHeaders" :key="h">
              {{ row[h] }}
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <div v-if="isAnyChart" class="viz-chart-wrapper">
      <div ref="chartContainer" class="echarts-container" />
    </div>

    <div v-if="isInsightCard && insightPayload" class="ai-chat-insight-card">
      <div class="ai-chat-insight-header">
        <strong>{{ insightPayload.title || '航班分析报告' }}</strong>
        <span>{{ insightPayload.flightNo || '--' }}</span>
      </div>
      <div class="ai-chat-insight-summary">
        {{ insightPayload.kind === 'history_report' ? '已生成航班动态报表，可展开全文并导出。' : '已生成航班事件经过，可展开全文并导出。' }}
      </div>
      <div class="ai-chat-insight-meta">
        生成时间: {{ insightPayload.generatedAt || '--' }} | 模型: {{ insightPayload.model || 'unknown-model' }}
      </div>

      <details class="ai-chat-markdown-details">
        <summary>展开 Markdown 全文</summary>
        <pre>{{ insightPayload.markdown || '(未返回 Markdown 内容)' }}</pre>
      </details>

      <div class="ai-chat-insight-actions">
        <button class="viz-action-btn" @click="handleExportMd">
          导出 md
        </button>
        <button class="viz-action-btn" @click="handleExportJson">
          导出 json
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.ai-chat-viz {
  margin-top: 12px;
  background: var(--glass-bg);
  border-radius: 8px;
  padding: 12px;
  font-size: 13px;
  border: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
}

.viz-table-wrapper {
  overflow-x: auto;
}

.viz-table {
  width: 100%;
  border-collapse: collapse;
}

.viz-table th, .viz-table td {
  padding: 6px 8px;
  border: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
  text-align: left;
}

.viz-table th {
  background: rgba(0, 0, 0, 0.02);
  font-weight: 600;
  color: var(--text-secondary, #3A3A3C);
}

.viz-chart-wrapper {
  width: 100%;
  height: 240px;
  margin-bottom: 8px;
}

.echarts-container {
  width: 100%;
  height: 100%;
}

.ai-chat-insight-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.ai-chat-insight-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 14px;
}

.ai-chat-insight-meta {
  color: var(--text-tertiary, #8E8E93);
  font-size: 12px;
}

.ai-chat-markdown-details {
  background: rgba(0, 0, 0, 0.02);
  border-radius: 6px;
  padding: 8px;
  cursor: pointer;
}

.ai-chat-markdown-details pre {
  margin: 8px 0 0 0;
  white-space: pre-wrap;
  font-size: 12px;
  max-height: 200px;
  overflow-y: auto;
}

.ai-chat-insight-actions {
  display: flex;
  gap: 8px;
  margin-top: 4px;
}

.viz-action-btn {
  padding: 4px 12px;
  background: white;
  border: 1px solid var(--system-blue, #007AFF);
  color: var(--system-blue, #007AFF);
  border-radius: 4px;
  cursor: pointer;
  font-size: 12px;
  transition: all 0.2s;
}

.viz-action-btn:hover {
  background: var(--system-blue, #007AFF);
  color: white;
}
</style>
