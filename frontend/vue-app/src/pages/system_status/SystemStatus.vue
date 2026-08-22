<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
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
import UiButton from '@/components/ui/UiButton.vue';
import UiPill from '@/components/ui/UiPill.vue';
import UiReadoutStrip from '@/components/ui/UiReadoutStrip.vue';
import UiSkeleton from '@/components/ui/UiSkeleton.vue';

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

const metricItems = computed(() => [
  { id: 'countFlights', label: '航班存储总量', value: countFlights.value },
  { id: 'countSSE', label: '活跃并发连接(SSE)', value: countSSE.value },
  { id: 'countErrors', label: '当日捕获异常', value: countErrors.value, tone: 'warn' as const },
  { id: 'responseTime', label: '接口平均响应', value: responseTime.value, unit: 'ms' },
]);

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
// 首轮快照未回来之前画骨架（§3.9）；已有内容后的轮询刷新不退回骨架。
const initialLoading = ref(true);

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

// 服务健康是事态：语义 tone 只在页里映射一次（§5.3 状态章回 tone，不回类名串）
type PillTone = 'ok' | 'warn' | 'danger' | 'mute';

function servicePillTone(tone: ServiceTone): PillTone {
  switch (tone) {
    case 'up':
      return 'ok';
    case 'down':
      return 'danger';
    case 'degraded':
      return 'warn';
    default:
      return 'mute';
  }
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
    initialLoading.value = false;
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
// Overall status pill tone (color lives in UiPill via tone)
// ============================================================
const statusPillTone = computed<PillTone>(() => {
  switch (overallStatus.value) {
    case 'healthy':
      return 'ok';
    case 'degraded':
      return 'warn';
    case 'down':
      return 'danger';
    default:
      return 'mute';
  }
});

// ============================================================
// Lifecycle
// ============================================================
onMounted(async () => {
  await fetchStatusSnapshot();

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
    <div class="nav-brand">
      <a
        :href="pageUrl('dashboard')"
        title="返回工作台"
        class="nav-home"
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
        <UiPill id="statusText" :tone="statusPillTone">
          {{ statusText }}
        </UiPill>
      </div>

      <UiReadoutStrip class="metrics-strip" :items="metricItems" />
    </div>

    <!-- Main Grid -->
    <div class="main-grid">
      <!-- Column 1: Services -->
      <div class="col-panel">
        <div class="panel">
          <div class="panel-header">
            SSE 实时推流网关
          </div>
          <div v-if="initialLoading" class="sk-list" aria-busy="true" aria-label="正在读取 SSE 网关指标">
            <UiSkeleton v-for="i in 4" :key="i" height="14px" />
          </div>
          <div v-else class="data-list">
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
          <div v-if="initialLoading" class="sk-list" aria-busy="true" aria-label="正在读取基础设施状态">
            <UiSkeleton v-for="i in 4" :key="i" height="14px" />
          </div>
          <div v-else class="data-list">
            <div class="data-row">
              <span class="label">API Server</span>
              <UiPill :tone="servicePillTone(infraApiTone)">
                {{ infraApi }}
              </UiPill>
            </div>
            <div class="data-row">
              <span class="label">Postgres 数据库</span>
              <UiPill :tone="servicePillTone(infraPostgresTone)">
                {{ infraPostgres }}
              </UiPill>
            </div>
            <div class="data-row">
              <span class="label">Redis 缓存层</span>
              <UiPill :tone="servicePillTone(infraRedisTone)">
                {{ infraRedis }}
              </UiPill>
            </div>
            <div class="data-row">
              <span class="label">Auth 鉴权服务</span>
              <UiPill :tone="servicePillTone(infraAuthTone)">
                {{ infraAuth }}
              </UiPill>
            </div>
          </div>
        </div>
      </div>

      <!-- Column 2: Performance (Full Height) -->
      <div class="col-panel">
        <div class="panel panel-flex">
          <div class="panel-header">
            性能指标监控
            <span id="perfTimestamp" class="panel-header-meta">{{ perfTimestamp }}</span>
          </div>
          <div v-if="initialLoading" class="sk-list" aria-busy="true" aria-label="正在读取性能指标">
            <UiSkeleton v-for="i in 4" :key="i" height="14px" />
          </div>
          <div v-else class="data-list">
            <div class="data-row">
              <span class="label">DB 连接池使用率</span>
              <div class="metric-inline">
                <div class="bar-bg">
                  <div
                    id="dbPoolBar"
                    class="bar-fill"
                    :data-tone="dbPoolPct > 80 ? 'danger' : dbPoolPct > 60 ? 'warn' : 'act'"
                    :style="{ '--bar-pct': dbPoolPct + '%' }"
                  />
                </div>
                <span id="dbPoolPct" class="value">{{ dbPoolPct }}%</span>
              </div>
            </div>
            <div class="data-row">
              <span class="label">Redis 延迟</span>
              <span
                id="redisLatency"
                class="value"
                :data-tone="servicePillTone(redisLatencyTone)"
              >{{ redisLatency === '已断开' ? redisLatency : `${redisLatency} ms` }}</span>
            </div>
            <div class="data-row">
              <span class="label">请求 P99 延迟</span><span id="requestP99" class="value">{{ requestP99 }} ms</span>
            </div>
            <div class="data-row">
              <span class="label">SSE 连接数</span>
              <div class="metric-inline">
                <div class="bar-bg">
                  <div
                    id="sseBar"
                    class="bar-fill"
                    data-tone="ok"
                    :style="{ '--bar-pct': sseConnPct + '%' }"
                  />
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
            <UiButton
              variant="quiet"
              size="sm"
              :disabled="isClearingErrors"
              @click="clearErrors"
            >
              {{ isClearingErrors ? '清空中' : '清空' }}
            </UiButton>
          </div>
          <div v-if="logActionError" class="log-error">
            {{ logActionError }}
          </div>
          <div v-if="initialLoading" class="log-container sk-list" aria-busy="true" aria-label="正在读取运行时错误">
            <UiSkeleton v-for="i in 8" :key="i" height="12px" />
          </div>
          <div v-else id="logConsole" class="log-container">
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
/* 信号面：本页是独立 iframe 页面，直接用标本 token，两面自动变位。
   旧别名块已移除：variables.css 已定义同名旧色，本块无人消费。 */

:global(body) {
  background-color: var(--face-page);
  color: var(--ink);
  font-family: var(--sans);
}

:global(#app) {
  background-color: var(--face-page);
  color: var(--ink);
  font-family: var(--sans);
  height: 100vh;
}

.navbar {
  height: var(--h-lg);
  background: var(--face-raised);
  border-bottom: 1px solid var(--line);
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 var(--s4);
  flex-shrink: 0;
}

.nav-brand {
  display: flex;
  align-items: center;
  gap: var(--s3);
}

.nav-home {
  display: flex;
  align-items: center;
  text-decoration: none;
  color: inherit;
}

.logo {
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.dashboard-container {
  flex: 1;
  display: grid;
  grid-template-rows: auto 1fr;
  padding: var(--s2);
  gap: var(--s2);
  height: calc(100vh - var(--h-lg));
  box-sizing: border-box;
}

.top-bar {
  display: flex;
  gap: var(--s2);
  height: 80px;
}

.status-card {
  background: var(--face-work);
  border: 1px solid var(--line);
  border-radius: var(--r-cell);
  padding: 0 var(--s4);
  display: flex;
  align-items: center;
  justify-content: space-between;
  min-width: 300px;
  flex: 1;
}

.inline-error {
  color: var(--danger);
}

.metrics-strip {
  display: flex;
  flex: 3;
  align-items: center;
  background: var(--face-work);
  border: 1px solid var(--line);
  border-radius: var(--r-cell);
}

.main-grid {
  display: grid;
  grid-template-columns: 350px 350px 1fr;
  gap: var(--s2);
  overflow: hidden;
}

.col-panel {
  display: flex;
  flex-direction: column;
  gap: var(--s2);
  height: 100%;
  overflow: hidden;
}

.panel {
  background: var(--face-work);
  border: 1px solid var(--line);
  border-radius: var(--r-cell);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.panel-flex {
  flex: 1;
}

.panel-header {
  padding: var(--s2) var(--s3);
  background: var(--face-raised);
  border-bottom: 1px solid var(--line);
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  color: var(--ink-muted);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s2);
}

.panel-header-meta {
  font-weight: var(--fw-regular);
  color: var(--ink-muted);
}

.data-list {
  padding: var(--s2) var(--s3);
  overflow-y: auto;
}

.data-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: var(--s2) 0;
  border-bottom: 1px solid var(--line);
  font-size: var(--fs-body);
  gap: var(--s3);
}

.data-row:last-child {
  border-bottom: none;
}

.label {
  color: var(--ink-muted);
  flex-shrink: 0;
}

.value {
  color: var(--ink);
  font-family: var(--mono);
  font-variant-numeric: tabular-nums;
  text-align: right;
  word-break: break-all;
}

/* 读数带声时声画在数上（§3.2），tone 名与 UiPill 同一套 */
.value[data-tone='ok'] {
  color: var(--ok);
}

.value[data-tone='danger'] {
  color: var(--danger);
}

.value[data-tone='warn'] {
  color: var(--warn);
}

.metric-inline {
  display: flex;
  align-items: center;
  gap: var(--s2);
}

.bar-bg {
  width: 80px;
  height: 6px;
  background: color-mix(in srgb, var(--ink) 8%, transparent);
  border-radius: var(--r-pill);
  overflow: hidden;
}

.bar-fill {
  /* 宽度是数据，经 --bar-pct 桥进来；其余形全在这一份规则里 */
  width: var(--bar-pct, 0%);
  height: 100%;
  background: var(--act);
  border-radius: var(--r-pill);
  transition: width var(--t-slow) var(--ease);
}

.bar-fill[data-tone='act'] {
  background: var(--act);
}

.bar-fill[data-tone='ok'] {
  background: var(--ok);
}

.bar-fill[data-tone='warn'] {
  background: var(--warn);
}

.bar-fill[data-tone='danger'] {
  background: var(--danger);
}

/* 日志台：墨分三阶读，不再用终端仿色的固定蓝橙 */
.log-container {
  flex: 1;
  background: var(--face-page);
  padding: var(--s2);
  font-family: var(--mono);
  font-size: var(--fs-label);
  line-height: 1.4;
  overflow-y: auto;
  color: var(--ink-subtle);
}

.log-error {
  background: var(--danger-soft);
  border-bottom: 1px solid color-mix(in srgb, var(--danger) 35%, transparent);
  color: var(--danger);
  font-size: var(--fs-label);
  padding: var(--s2) var(--s3);
}

.log-line {
  display: flex;
  gap: var(--s2);
  padding: 2px 0;
  border-bottom: 1px solid var(--line);
}

.l-time {
  color: var(--ink-muted);
  min-width: 70px;
}

.l-tag {
  font-weight: var(--fw-semibold);
  min-width: 50px;
}

.l-msg {
  color: var(--ink-subtle);
  word-break: break-all;
}

.tag-low {
  color: var(--ok);
}

.tag-medium {
  color: var(--warn);
}

.tag-high {
  color: var(--danger);
}

/* 首轮等待的骨架群：与 data-row 同构，洗光配方只在 UiSkeleton */
.sk-list {
  padding: var(--s2) var(--s3);
  display: flex;
  flex-direction: column;
  gap: var(--s3);
  overflow-y: auto;
}

h1 {
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
  margin: 0;
  color: var(--ink);
}

p {
  margin: var(--s1) 0 0 0;
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

@media (max-width: 1200px) {
  .main-grid {
    grid-template-columns: 1fr 1fr;
    grid-template-rows: 1fr 1fr;
  }
}
</style>
