import { ref, onMounted, onUnmounted } from 'vue';
import { useApi } from './useApi';
import { useSSE } from './useSSE';
import {
  type CommandDistributionItem,
  type CommandQueueItem,
  buildCommandCenterSnapshot,
} from './commandCenterModel';
import { type CommandEvent } from './sseEvents';

export function useCommandCenter() {
  const api = useApi();
  const loading = ref(true);
  const error = ref('');
  const kpis = ref({
    decisionCount: 0,
    riskFlights: 0,
    openAnomalies: 0,
    dispatchBlockers: 0,
    delayPressure: 0,
  });
  const verdict = ref({ title: '运行正常', detail: '无重大风险', severity: 'ok', window: '当前 2h 窗口' });
  const events = ref<CommandEvent[]>([]);
  const heatmapData = ref<CommandDistributionItem[]>([]);
  const priorityQueue = ref<CommandQueueItem[]>([]);
  const windowPressure = ref<CommandDistributionItem[]>([]);
  const dispatchLoad = ref<CommandDistributionItem[]>([]);
  const terminalLoad = ref<CommandDistributionItem[]>([]);
  const systemHealth = ref({ score: 0, label: '等待检查' });

  let refreshTimer: ReturnType<typeof setInterval> | null = null;
  let currentInterval = 30000;
  const windowHours = ref(2);

  async function fetchSnapshot(hoursOverride?: number) {
    loading.value = true;
    error.value = '';
    try {
      const hours = hoursOverride ?? windowHours.value;
      if (hoursOverride !== undefined) windowHours.value = hoursOverride;
      const [flights, anomalies, dispatch] = await Promise.all([
        api.get('/api/v2/flights?page=1&page_size=500'),
        api.get('/api/v2/anomalies?status=open&limit=500'),
        api.get('/api/v2/dispatch-orders?status=pending&page=1&page_size=500'),
      ]);

      const results = [
        ['航班', flights] as const,
        ['异常', anomalies] as const,
        ['调度', dispatch] as const,
      ];
      const failed = results.filter(([, result]) => !result.ok);
      systemHealth.value = {
        score: Math.round(((results.length - failed.length) / results.length) * 100),
        label: failed.length === 0 ? '健康状态' : `部分异常: ${failed.map(([name]) => name).join('、')}`,
      };

      if (failed.length === results.length) {
        throw new Error('指挥中心数据接口均不可用');
      }
      if (failed.length > 0) {
        error.value = `${failed.map(([name, result]) => `${name} HTTP ${result.status}`).join('；')}，当前显示可用数据。`;
      }

      const snapshot = buildCommandCenterSnapshot(
        flights.ok ? flights.data : null,
        anomalies.ok ? anomalies.data : null,
        dispatch.ok ? dispatch.data : null,
        hours,
      );

      kpis.value = snapshot.kpis;
      verdict.value = snapshot.verdict;
      priorityQueue.value = snapshot.priorityQueue;
      windowPressure.value = snapshot.windowPressure;
      heatmapData.value = snapshot.heatmapData;
      dispatchLoad.value = snapshot.dispatchLoad;
      terminalLoad.value = snapshot.terminalLoad;
    } catch (err) {
      console.error('Failed to fetch snapshot:', err);
      error.value = err instanceof Error ? err.message : '指挥中心数据刷新失败';
    } finally {
      loading.value = false;
    }
  }

  const { connect, disconnect, on } = useSSE({ url: '/api/v2/sse/stream' });
  on('message', (event) => {
    if (!(event instanceof MessageEvent)) return;
    try {
      events.value.unshift(JSON.parse(event.data) as CommandEvent);
      if (events.value.length > 100) events.value.pop();
    } catch {
      // ignore non-JSON heartbeat frames
    }
  });

  function startAutoRefresh(intervalMs: number = currentInterval) {
    stopAutoRefresh();
    currentInterval = intervalMs;
    refreshTimer = setInterval(() => fetchSnapshot(Number(windowHours.value)), intervalMs);
  }

  function stopAutoRefresh() {
    if (refreshTimer) { 
      clearInterval(refreshTimer); 
      refreshTimer = null; 
    }
  }

  onMounted(() => { 
    fetchSnapshot(); 
    connect(); 
    startAutoRefresh(); 
  });
  
  onUnmounted(() => { 
    disconnect(); 
    stopAutoRefresh(); 
  });

  return { 
    loading, 
    error,
    kpis, 
    verdict, 
    events, 
    heatmapData, 
    priorityQueue, 
    windowPressure,
    dispatchLoad,
    terminalLoad,
    systemHealth,
    fetchSnapshot, 
    startAutoRefresh, 
    stopAutoRefresh 
  };
}
