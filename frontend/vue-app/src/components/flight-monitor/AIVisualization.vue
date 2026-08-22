<script setup lang="ts">
import { computed, ref, onMounted, onBeforeUnmount, watch, nextTick, useId } from 'vue';
import * as echarts from 'echarts';
import { useToast } from '@/composables/useToast';
import { downloadTextFile } from '@/lib/download';
import { useTheme } from '@/composables/useTheme';
import { useChartTheme } from '@/composables/useChartTheme';
import UiButton from '@/components/ui/UiButton.vue';
import UiFacts, { type Fact } from '@/components/ui/UiFacts.vue';
import UiInset from '@/components/ui/UiInset.vue';
import UiTable from '@/components/ui/UiTable.vue';

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
    // 明暗不靠 echarts 内建主题，全部由 token 现算（chartBase / chartColors）
    chartInstance = echarts.init(chartContainer.value, null);
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
    const base = chartBase.value;
    options = {
      ...base.root,
      tooltip: { ...base.tooltip, trigger: 'axis' },
      grid: { left: '3%', right: '4%', bottom: '3%', top: '10%', containLabel: true },
      xAxis: { type: 'value', ...base.axis },
      yAxis: {
        type: 'category',
        data: labels,
        ...base.laneAxis,
        axisLabel: { ...base.laneAxis.axisLabel, interval: 0, width: 80, overflow: 'truncate' },
      },
      series: [{ type: 'bar', data: values, itemStyle: { color: chartColors.value.statusAssigned, borderRadius: [0, 4, 4, 0] } }]
    };
  } else if (isTimeline.value) {
    const items = Array.isArray(dataRecord.value.items) ? dataRecord.value.items : [];
    const dataPoints = items.slice(0, 30).map((item: Record<string, unknown>, i: number) => {
        const timeStr = (item.scheduled_departure || item.time || item.timestamp) as string | undefined;
        const timeVal = timeStr ? new Date(timeStr).getTime() : Date.now() + i * 1000;
        return [timeVal, i, (item.flight_number || item.flight_id || `事件 ${i+1}`) as string];
     });
     const base = chartBase.value;
     options = {
       ...base.root,
       tooltip: {
         ...base.tooltip,
         trigger: 'item',
         formatter: (params: Record<string, unknown>) => {
            const value = params.value as unknown[];
            return `${value[2]}<br/>${new Date(value[0] as number).toLocaleString()}`;
         }
      },
      grid: { left: '3%', right: '4%', bottom: '3%', top: '10%', containLabel: true },
      xAxis: { type: 'time', ...base.axis, splitLine: { show: false } },
      yAxis: { type: 'value', show: false },
      series: [{
        type: 'scatter',
        data: dataPoints,
        symbolSize: 10,
        // 事件点是「有这件事」，不是「出事了」——用行动色，不用危声
        itemStyle: { color: chartColors.value.statusAssigned }
      }]
    };
  }
  
  chartInstance.setOption(options, true);
};

const { theme } = useTheme();
const { chartColors, chartBase } = useChartTheme();
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

/** 一组名+值属性走事实格；缺值交给 UiFacts 写成「—」，这里不写 '--'（§3.2 / §5.2） */
const insightFacts = computed<Fact[]>(() => [
  { label: '生成时间', value: insightPayload.value?.generatedAt as Fact['value'], mono: true },
  { label: '模型', value: insightPayload.value?.model as Fact['value'] },
]);

/** 展开是持守：绑 aria-expanded，不绑一次性 class；id 用 useId，一页可能有多条消息（§2.5） */
const mdOpen = ref(false);
const mdId = useId();

</script>

<template>
  <div v-if="isTable || isInsightCard || isAnyChart" class="ai-chat-viz">
    <div v-if="isTable && tableRows.length > 0" class="viz-table-wrapper">
      <UiTable label="查询结果" :sticky-head="false">
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
      </UiTable>
    </div>

    <div v-if="isAnyChart" class="viz-chart-wrapper">
      <div ref="chartContainer" class="echarts-container" />
    </div>

    <div v-if="isInsightCard && insightPayload" class="ai-chat-insight-card">
      <div class="ai-chat-insight-header">
        <strong>{{ insightPayload.title || '航班分析报告' }}</strong>
        <span class="ai-chat-insight-flight-no">{{ insightPayload.flightNo || '—' }}</span>
      </div>
      <UiFacts :items="insightFacts" :columns="2" />

      <UiButton
        variant="quiet"
        :aria-expanded="mdOpen ? 'true' : 'false'"
        :aria-controls="mdId"
        @click="mdOpen = !mdOpen"
      >
        {{ mdOpen ? '收起 Markdown 全文' : '展开 Markdown 全文' }}
      </UiButton>
      <UiInset v-if="mdOpen" :id="mdId">
        <pre class="ai-chat-markdown-pre">{{ insightPayload.markdown || '(未返回 Markdown 内容)' }}</pre>
      </UiInset>

      <div class="ai-chat-insight-actions">
        <UiButton variant="quiet" @click="handleExportMd">
          导出 md
        </UiButton>
        <UiButton variant="quiet" @click="handleExportJson">
          导出 json
        </UiButton>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* 这里只留排布。本组件坐在气泡 #body 槽里，不再铺一张卡，只用一根线跟上面的 Markdown 分开（§3.3 / §4.21）。 */
.ai-chat-viz {
  margin-top: var(--s3);
  padding-top: var(--s3);
  border-top: 1px solid var(--line);
  font-size: var(--fs-body);
}

.viz-table-wrapper {
  overflow-x: auto;
}

.viz-chart-wrapper {
  width: 100%;
  height: 240px;
  margin-bottom: var(--s2);
}

.echarts-container {
  width: 100%;
  height: 100%;
}

.ai-chat-insight-card {
  display: flex;
  flex-direction: column;
  gap: var(--s2);
}

.ai-chat-insight-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--s3);
  font-size: var(--fs-section);
  color: var(--ink);
}

/* 航班号是标识，用等宽（§2.4） */
.ai-chat-insight-flight-no {
  font-family: var(--mono);
  font-variant-numeric: tabular-nums;
  color: var(--ink-subtle);
}

/* 原始报文放进嵌板（§3.7）；pre 自己不再描边、不再换底。 */
.ai-chat-markdown-pre {
  margin: 0;
  white-space: pre-wrap;
  font-family: var(--mono);
  font-size: var(--fs-label);
  color: var(--ink);
  max-height: 200px;
  overflow-y: auto;
}

.ai-chat-insight-actions {
  display: flex;
  gap: var(--s2);
  margin-top: var(--s1);
}
</style>
