<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue';
import { useApi } from '@/composables/useApi';
import { useAuth } from '@/composables/useAuth';
import { useToast } from '@/composables/useToast';
import { pageUrl } from '@/shared/page-routes';
import {
  type ApiEnvelope,
  type HealthPayload,
  type LogEntry,
  type PerformancePayload,
  type RuntimeErrorPayload,
  type ServiceTone,
  type SseStatsPayload,
  mapErrorToLogEntry,
  mapSystemStatusView,
  unwrapApiData,
} from './systemStatusModel';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';

const api = useApi();
const auth = useAuth();
const toast = useToast();

// Overall status
const lastUpdated = ref('--:--:--');
const statusText = ref('正在评估...');
const overallStatus = ref<'healthy' | 'degraded' | 'down' | 'unknown'>('unknown');

// Top metrics
const countFlights = ref('-');
const countSSE = ref('-');
const countErrors = ref('-');
const responseTime = ref('-');

// SSE gateway metrics
const sseTotal = ref('-');
const sseFlights = ref('-');
const sseStatus = ref('-');
const sseState = ref('-');

// Infrastructure health (detail text + tone for color)
const infraApi = ref('-');
const infraPostgres = ref('-');
const infraRedis = ref('-');
const infraAuth = ref('-');
const infraApiTone = ref<ServiceTone>('unknown');
const infraPostgresTone = ref<ServiceTone>('unknown');
const infraRedisTone = ref<ServiceTone>('unknown');
const infraAuthTone = ref<ServiceTone>('unknown');

// Performance metrics
const perfTimestamp = ref('-');
const dbPoolPct = ref(0);
const redisLatency = ref('-');
const redisLatencyTone = ref<ServiceTone>('unknown');
const requestP99 = ref('-');
const sseConnPct = ref(0);

// Log streaming
const logs = ref<LogEntry[]>([
  { id: 'sys-0', time: 'SYS', level: 'low', tag: 'INFO', message: '正在读取运行时错误...' },
]);
const logRealtimeLag = ref('连接中');
const statusError = ref('');
const logActionError = ref('');
const isClearingErrors = ref(false);

// Last-good snapshot pieces so a single failing endpoint does not wipe others.
const lastHealth = ref<HealthPayload | null>(null);
const lastPerformance = ref<PerformancePayload | null>(null);
const lastSseStats = ref<SseStatsPayload | null>(null);

// SSE connection / polling control
let eventSource: EventSource | null = null;
let pollTimer: ReturnType<typeof setInterval> | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let pollInFlight = false;
let reconnectAttempt = 0;
const RECONNECT_BASE_MS = 2000;
const RECONNECT_MAX_MS = 30000;

function toneClass(tone: ServiceTone): Record<string, boolean> {
  return {
    'status-up': tone === 'up',
    'status-down': tone === 'down',
    'status-degraded': tone === 'degraded',
  };
}

function applyStatusView(
  health: HealthPayload | null,
  performance: PerformancePayload | null,
  sseStats: SseStatsPayload | null,
): void {
  const view = mapSystemStatusView(health, performance, sseStats);
  statusText.value = view.statusText;
  overallStatus.value = view.overallStatus;
  countFlights.value = view.countFlights;
  countSSE.value = view.countSSE;
  countErrors.value = view.countErrors;
  responseTime.value = view.responseTime;
  sseTotal.value = view.sseTotal;
  sseFlights.value = view.sseFlights;
  sseStatus.value = view.sseStatus;
  sseState.value = view.sseState;
  infraApi.value = view.infraApi;
  infraPostgres.value = view.infraPostgres;
  infraRedis.value = view.infraRedis;
  infraAuth.value = view.infraAuth;
  infraApiTone.value = view.infraApiTone;
  infraPostgresTone.value = view.infraPostgresTone;
  infraRedisTone.value = view.infraRedisTone;
  infraAuthTone.value = view.infraAuthTone;
  perfTimestamp.value = view.perfTimestamp;
  dbPoolPct.value = view.dbPoolPct;
  redisLatency.value = view.redisLatency;
  redisLatencyTone.value = view.redisLatencyTone;
  requestP99.value = view.requestP99;
  sseConnPct.value = view.sseConnPct;
}

function applyErrorLogs(errors: RuntimeErrorPayload[]): void {
  logs.value = errors.map((entry, index) => mapErrorToLogEntry(entry, `runtime-error-${index}`));
  if (logs.value.length === 0) {
    logs.value = [{ id: 'sys-empty', time: 'SYS', level: 'low', tag: 'INFO', message: '暂无运行时错误' }];
  }
}

function upsertErrorLog(error: RuntimeErrorPayload): void {
  const entry = mapErrorToLogEntry(error, `runtime-error-${Date.now()}`);
  logs.value = [
    entry,
    ...logs.value.filter((item) => item.id !== entry.id && item.id !== 'sys-empty' && item.id !== 'sys-0'),
  ].slice(0, 200);
}

function handleRealtimeErrorPayload(payload: unknown): void {
  if (!payload || typeof payload !== 'object') return;
  const obj = payload as Record<string, unknown>;
  const errorEvent = (obj.error_event && typeof obj.error_event === 'object'
    ? obj.error_event
    : obj) as RuntimeErrorPayload;
  if (!errorEvent || typeof errorEvent !== 'object') return;
  upsertErrorLog(errorEvent);
  if (typeof obj.errors_count === 'number') {
    countErrors.value = String(obj.errors_count);
  }
  logRealtimeLag.value = '实时';
}

function setStatusUnavailable(message: string): void {
  statusError.value = message;
  statusText.value = '状态读取失败';
  overallStatus.value = 'unknown';
}

// ============================================================
// Fetch system status snapshot (non-overlapping)
// ============================================================
async function fetchStatusSnapshot() {
  if (pollInFlight) {
    return;
  }
  pollInFlight = true;
  try {
    const [healthRes, performanceRes, errorsRes, sseStatsRes] = await Promise.allSettled([
      api.get<HealthPayload>('/api/v2/health'),
      api.get<ApiEnvelope<PerformancePayload>>('/api/v2/health/performance'),
      api.get<ApiEnvelope<RuntimeErrorPayload[]>>('/api/v2/health/errors?limit=200'),
      api.get<ApiEnvelope<SseStatsPayload>>('/api/v2/system/runtime/streaming/sse-stats'),
    ]);

    if (healthRes.status === 'fulfilled' && healthRes.value.ok && healthRes.value.data) {
      // Health endpoint returns a flat payload (not always envelope-wrapped).
      const raw = healthRes.value.data;
      lastHealth.value = (unwrapApiData<HealthPayload>(raw) ?? raw) as HealthPayload;
    }
    if (performanceRes.status === 'fulfilled' && performanceRes.value.ok && performanceRes.value.data) {
      const perf = unwrapApiData<PerformancePayload>(performanceRes.value.data);
      if (perf) lastPerformance.value = perf;
    }
    if (sseStatsRes.status === 'fulfilled' && sseStatsRes.value.ok && sseStatsRes.value.data) {
      const stats = unwrapApiData<SseStatsPayload>(sseStatsRes.value.data);
      if (stats) lastSseStats.value = stats;
    }

    const health = lastHealth.value;
    const performance = lastPerformance.value;
    const sseStats = lastSseStats.value;
    const errors = errorsRes.status === 'fulfilled' && errorsRes.value.ok
      ? unwrapApiData<RuntimeErrorPayload[]>(errorsRes.value.data)
      : null;

    if (!health && !performance && !errors && !sseStats) {
      setStatusUnavailable('无法读取系统状态，请检查 API 服务或当前账号权限。');
      return;
    }

    statusError.value = '';
    applyStatusView(health, performance, sseStats);
    if (errors) {
      applyErrorLogs(Array.isArray(errors) ? errors : []);
    } else if (health?.recent_errors && Array.isArray(health.recent_errors)) {
      applyErrorLogs(health.recent_errors);
    }
    lastUpdated.value = new Date().toLocaleTimeString();
  } finally {
    pollInFlight = false;
  }
}

// ============================================================
// Connect to SSE log stream (named error_log events)
// ============================================================
function scheduleReconnect(): void {
  if (reconnectTimer) {
    return; // prevent reconnect storms
  }
  const delay = Math.min(RECONNECT_BASE_MS * (2 ** reconnectAttempt), RECONNECT_MAX_MS);
  reconnectAttempt = Math.min(reconnectAttempt + 1, 8);
  logRealtimeLag.value = '断开';
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connectSSELogs();
  }, delay);
}

function connectSSELogs() {
  if (eventSource) {
    eventSource.close();
    eventSource = null;
  }
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }

  try {
    // Prefer the general error_events topic (named event: error_log).
    const url = `${auth.apiBase.value}/sse/stream?topics=error_events`;
    eventSource = auth.getEventSource(url, { clientScope: 'system_status_errors' });

    const onErrorFrame = (event: MessageEvent) => {
      try {
        const parsed = JSON.parse(String(event.data)) as unknown;
        handleRealtimeErrorPayload(parsed);
      } catch {
        // Heartbeat and non-JSON frames are part of the SSE transport.
      }
    };

    // Named event from runtime_error_monitor (`broadcast_event(..., Some("error_log"), ...)`).
    eventSource.addEventListener('error_log', onErrorFrame as EventListener);

    // Fallback for untyped / default message frames that embed message_type.
    eventSource.onmessage = (event) => {
      try {
        const parsed = JSON.parse(event.data) as {
          message_type?: string;
          error_event?: RuntimeErrorPayload;
          errors_count?: number;
        };
        if (String(parsed.message_type ?? '').toLowerCase() === 'error_log' || parsed.error_event) {
          handleRealtimeErrorPayload(parsed);
        }
      } catch {
        // ignore non-JSON
      }
    };

    eventSource.onerror = () => {
      eventSource?.close();
      eventSource = null;
      auth.invalidateSSEToken?.();
      scheduleReconnect();
    };

    eventSource.onopen = () => {
      reconnectAttempt = 0;
      logRealtimeLag.value = '已连接';
    };
  } catch (e) {
    console.warn('SSE connection failed:', e);
    logRealtimeLag.value = '连接失败';
    scheduleReconnect();
  }
}

// ============================================================
// Clear error logs
// ============================================================
async function clearErrors() {
  if (isClearingErrors.value) {
    return;
  }
  isClearingErrors.value = true;
  logActionError.value = '';
  try {
    const res = await api.post<{ success?: boolean; message?: string }>('/api/v2/health/errors/clear');
    if (!res.ok || res.data?.success === false) {
      throw new Error(res.data?.message || `清空失败，HTTP ${res.status}`);
    }
    logs.value = [{ id: 'sys-empty', time: 'SYS', level: 'low', tag: 'INFO', message: '暂无运行时错误' }];
    countErrors.value = '0';
    if (lastHealth.value) {
      lastHealth.value = {
        ...lastHealth.value,
        errors_count: 0,
        recent_errors: [],
      };
    }
    toast.showToast('success', '运行时错误已清空');
  } catch (error) {
    logActionError.value = error instanceof Error ? error.message : '清空运行时错误失败';
    toast.showToast('error', logActionError.value);
  } finally {
    isClearingErrors.value = false;
  }
}

// ============================================================
// Status orb color
// ============================================================
const statusOrbColor = ref('grey');

function updateStatusOrbColor() {
  switch (overallStatus.value) {
    case 'healthy':
      statusOrbColor.value = '#4caf50';
      break;
    case 'degraded':
      statusOrbColor.value = '#ff9800';
      break;
    case 'down':
      statusOrbColor.value = '#f44336';
      break;
    default:
      statusOrbColor.value = 'grey';
  }
}

watch(overallStatus, updateStatusOrbColor);

// ============================================================
// Lifecycle
// ============================================================
onMounted(async () => {
  await fetchStatusSnapshot();
  updateStatusOrbColor();

  // Poll every 5 seconds (skip if previous poll still in flight).
  pollTimer = setInterval(() => {
    void fetchStatusSnapshot();
  }, 5000);

  connectSSELogs();
});

onUnmounted(() => {
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
  if (eventSource) {
    eventSource.close();
    eventSource = null;
  }
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
});
</script>

<template>
  <nav class="navbar">
    <div style="display:flex;align-items:center;gap:14px;">
      <a
        :href="pageUrl('dashboard')"
        title="返回工作台"
        style="display:flex;align-items:center;text-decoration:none;color:inherit;"
      >
        <div class="logo">系统管理</div>
      </a>
    </div>
  </nav>

  <div class="dashboard-container">
    <!-- Top Bar -->
    <div class="top-bar">
      <!-- Status Card -->
      <div class="status-card">
        <div>
          <h1 id="pageTitle">
            系统整体状态
          </h1>
          <p>最后更新: <span id="lastUpdated">{{ lastUpdated }}</span> · 状态轮询: 5s · 异常推送: SSE</p>
          <p v-if="statusError" class="inline-error">
            {{ statusError }}
          </p>
        </div>
        <div style="display:flex;align-items:center;">
          <div id="statusOrb" class="status-orb" :style="{ background: statusOrbColor }" />
          <span id="statusText" style="font-weight:600;font-size:14px;">{{ statusText }}</span>
        </div>
      </div>

      <!-- Metrics Strip -->
      <div class="metrics-strip">
        <div class="metric-box">
          <div class="m-label">
            航班存储总量
          </div>
          <div id="countFlights" class="m-value">
            {{ countFlights }}
          </div>
        </div>
        <div class="metric-box">
          <div class="m-label">
            活跃并发连接(SSE)
          </div>
          <div id="countSSE" class="m-value">
            {{ countSSE }}
          </div>
        </div>
        <div class="metric-box">
          <div class="m-label">
            当日捕获异常
          </div>
          <div id="countErrors" class="m-value" style="color:var(--system-orange)">
            {{ countErrors }}
          </div>
        </div>
        <div class="metric-box">
          <div class="m-label">
            接口平均响应
          </div>
          <div id="responseTime" class="m-value">
            {{ responseTime }} ms
          </div>
        </div>
      </div>
    </div>

    <!-- Main Grid -->
    <div class="main-grid">
      <!-- Column 1: Services -->
      <div class="col-panel">
        <div class="panel">
          <div class="panel-header">
            SSE 实时推流网关
          </div>
          <div class="data-list">
            <div class="data-row">
              <span class="label">客户端连接总数</span><span id="sseTotal" class="value">{{ sseTotal }}</span>
            </div>
            <div class="data-row">
              <span class="label">航班详情订阅流</span><span id="sseFlights" class="value">{{ sseFlights }}</span>
            </div>
            <div class="data-row">
              <span class="label">全局状态订阅流</span><span id="sseStatus" class="value">{{ sseStatus }}</span>
            </div>
            <div class="data-row">
              <span class="label">Gateway 运行状态</span><span id="sseState" class="value">{{ sseState }}</span>
            </div>
          </div>
        </div>
        <div class="panel panel-flex">
          <div class="panel-header">
            核心基础设施
          </div>
          <div class="data-list">
            <div class="data-row">
              <span class="label">API Server</span>
              <span id="infraApi" class="value" :class="toneClass(infraApiTone)">{{ infraApi }}</span>
            </div>
            <div class="data-row">
              <span class="label">Postgres 数据库</span>
              <span id="infraPostgres" class="value" :class="toneClass(infraPostgresTone)">{{ infraPostgres }}</span>
            </div>
            <div class="data-row">
              <span class="label">Redis 缓存层</span>
              <span id="infraRedis" class="value" :class="toneClass(infraRedisTone)">{{ infraRedis }}</span>
            </div>
            <div class="data-row">
              <span class="label">Auth 鉴权服务</span>
              <span id="infraAuth" class="value" :class="toneClass(infraAuthTone)">{{ infraAuth }}</span>
            </div>
          </div>
        </div>
      </div>

      <!-- Column 2: Performance (Full Height) -->
      <div class="col-panel">
        <div class="panel panel-flex">
          <div class="panel-header">
            性能指标监控
            <span id="perfTimestamp" style="font-weight:normal;opacity:0.6">{{ perfTimestamp }}</span>
          </div>
          <div class="data-list">
            <div class="data-row">
              <span class="label">DB 连接池使用率</span>
              <div style="display:flex;align-items:center;gap:8px;">
                <div class="bar-bg">
                  <div id="dbPoolBar" class="bar-fill" :style="{ width: dbPoolPct + '%', background: dbPoolPct > 80 ? 'var(--system-red)' : dbPoolPct > 60 ? 'var(--system-orange)' : 'var(--accent-blue)' }" />
                </div>
                <span id="dbPoolPct" class="value">{{ dbPoolPct }}%</span>
              </div>
            </div>
            <div class="data-row">
              <span class="label">Redis 延迟</span>
              <span
                id="redisLatency"
                class="value"
                :class="toneClass(redisLatencyTone)"
              >{{ redisLatency === '已断开' ? redisLatency : `${redisLatency} ms` }}</span>
            </div>
            <div class="data-row">
              <span class="label">请求 P99 延迟</span><span id="requestP99" class="value">{{ requestP99 }} ms</span>
            </div>
            <div class="data-row">
              <span class="label">SSE 连接数</span>
              <div style="display:flex;align-items:center;gap:8px;">
                <div class="bar-bg">
                  <div id="sseBar" class="bar-fill" :style="{ width: sseConnPct + '%', background: 'var(--system-green)' }" />
                </div>
                <span id="sseConnPct" class="value">{{ sseConnPct }}%</span>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Column 3: Logs (Full Height) -->
      <div class="col-panel">
        <div class="panel panel-flex">
          <div class="panel-header">
            <span>运行时错误监控 · <span id="logRealtimeLag">SSE {{ logRealtimeLag }}</span></span>
            <button
              :disabled="isClearingErrors"
              class="link-button"
              @click="clearErrors"
            >
              {{ isClearingErrors ? '清空中' : '清空' }}
            </button>
          </div>
          <div v-if="logActionError" class="log-error">
            {{ logActionError }}
          </div>
          <div id="logConsole" class="log-container">
            <div v-for="log in logs" :key="log.id" class="log-line">
              <span class="l-time">[{{ log.time }}]</span>
              <span class="l-tag" :class="`tag-${log.level}`">{{ log.tag }}</span>
              <span class="l-msg">{{ log.message }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
  <ThemeToggle />
</template>

<style scoped>
:global(:root) {
  --bg-body: #1e1e1e;
  --bg-panel: #252526;
  --bg-header: #333333;
  --text-main: #e0e0e0;
  --text-dim: #a0a0a0;
  --border-color: #3e3e42;
  --accent-blue: #007acc;
  --system-green: #4caf50;
  --system-red: #f44336;
  --system-orange: #ff9800;
}

:global(body) {
  background-color: var(--bg-body);
  color: var(--text-main);
  font-family: var(--font-sans, "MiSans", "PingFang SC", "Microsoft YaHei", system-ui, sans-serif);
}

:global(#app) {
  background-color: #1e1e1e;
  color: #e0e0e0;
  font-family: var(--font-sans, "MiSans", "PingFang SC", "Microsoft YaHei", system-ui, sans-serif);
  height: 100vh;
}

.navbar {
  height: 40px;
  background: #333333;
  border-bottom: 1px solid #3e3e42;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 16px;
  flex-shrink: 0;
}

.logo {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-inverse);
}

.dashboard-container {
  flex: 1;
  display: grid;
  grid-template-rows: auto 1fr;
  padding: 8px;
  gap: 8px;
  height: calc(100vh - 40px);
  box-sizing: border-box;
}

.top-bar {
  display: flex;
  gap: 8px;
  height: 80px;
}

.status-card {
  background: #252526;
  border: 1px solid #3e3e42;
  border-radius: 4px;
  padding: 0 16px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  min-width: 300px;
  flex: 1;
}

.inline-error {
  color: #ffb4ab;
}

.metrics-strip {
  display: flex;
  flex: 3;
  gap: 8px;
}

.metric-box {
  flex: 1;
  background: #252526;
  border: 1px solid #3e3e42;
  border-radius: 4px;
  display: flex;
  flex-direction: column;
  justify-content: center;
  padding: 0 16px;
}

.m-label {
  font-size: 11px;
  color: #a0a0a0;
  text-transform: uppercase;
  margin-bottom: 4px;
}

.m-value {
  font-size: 20px;
  font-weight: 700;
  color: var(--text-inverse);
  font-family: 'Segoe UI', monospace;
}

.main-grid {
  display: grid;
  grid-template-columns: 350px 350px 1fr;
  gap: 8px;
  overflow: hidden;
}

.col-panel {
  display: flex;
  flex-direction: column;
  gap: 8px;
  height: 100%;
  overflow: hidden;
}

.panel {
  background: #252526;
  border: 1px solid #3e3e42;
  border-radius: 4px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.panel-flex {
  flex: 1;
}

.panel-header {
  padding: 8px 12px;
  background: rgba(255, 255, 255, 0.03);
  border-bottom: 1px solid #3e3e42;
  font-size: 12px;
  font-weight: 600;
  color: #a0a0a0;
  text-transform: uppercase;
  display: flex;
  justify-content: space-between;
}

.link-button {
  background: none;
  border: none;
  color: var(--accent-blue);
  cursor: pointer;
  font-size: 10px;
  padding: 0;
}

.link-button:disabled {
  color: #6f6f6f;
  cursor: wait;
}

.data-list {
  padding: 8px 12px;
  overflow-y: auto;
}

.data-row {
  display: flex;
  justify-content: space-between;
  padding: 6px 0;
  border-bottom: 1px solid rgba(255, 255, 255, 0.05);
  font-size: 13px;
  gap: 12px;
}

.data-row:last-child {
  border-bottom: none;
}

.label {
  color: #a0a0a0;
  flex-shrink: 0;
}

.value {
  color: #e0e0e0;
  font-family: monospace;
  text-align: right;
  word-break: break-all;
}

.value.status-up {
  color: #4caf50;
}

.value.status-down {
  color: #f44336;
}

.value.status-degraded {
  color: #ff9800;
}

.bar-bg {
  width: 80px;
  height: 6px;
  background: #333;
  border-radius: 3px;
  overflow: hidden;
}

.bar-fill {
  height: 100%;
  background: #007acc;
  transition: width 0.3s;
}

.log-container {
  flex: 1;
  background: #101010;
  padding: 8px;
  font-family: "Consolas", "Courier New", monospace;
  font-size: 12px;
  line-height: 1.4;
  overflow-y: auto;
  color: #d4d4d4;
}

.log-error {
  background: rgba(244, 67, 54, 0.12);
  border-bottom: 1px solid rgba(244, 67, 54, 0.35);
  color: #ffb4ab;
  font-size: 12px;
  padding: 8px 12px;
}

.log-line {
  display: flex;
  gap: 8px;
  padding: 2px 0;
  border-bottom: 1px solid rgba(255, 255, 255, 0.05);
}

.l-time {
  color: #569cd6;
  min-width: 70px;
}

.l-tag {
  font-weight: bold;
  min-width: 50px;
}

.l-msg {
  color: #ce9178;
  word-break: break-all;
}

.tag-low {
  color: #4caf50;
}

.tag-medium {
  color: #ff9800;
}

.tag-high {
  color: #f44336;
}

.status-orb {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  display: inline-block;
  margin-right: 6px;
}

h1 {
  font-size: 18px;
  margin: 0;
  color: var(--text-inverse);
}

p {
  margin: 4px 0 0 0;
  font-size: 11px;
  color: #a0a0a0;
}

@media (max-width: 1200px) {
  .main-grid {
    grid-template-columns: 1fr 1fr;
    grid-template-rows: 1fr 1fr;
  }

  .col-logs {
    grid-column: 1 / -1;
  }
}
</style>
