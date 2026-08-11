export interface ApiEnvelope<T> {
  success?: boolean;
  data?: T | null;
  message?: string | null;
}

export interface RuntimeErrorPayload {
  error_id?: string;
  timestamp?: string;
  error_type?: string;
  message?: string;
  severity?: string;
  category?: string;
  operation?: string | null;
  emitted_at_ms?: number;
}

export interface ServiceHealth {
  status?: string;
  detail?: string;
  uptime_seconds?: number;
  [key: string]: unknown;
}

export interface HealthPayload {
  success?: boolean;
  status?: string;
  database?: {
    flights?: number;
  };
  errors_count?: number;
  recent_errors?: RuntimeErrorPayload[];
  buffer_status?: {
    total_connections?: number;
    max_connections?: number;
    status?: string;
    topics?: Record<string, number>;
  };
  services?: {
    api?: string | ServiceHealth;
    api_server?: string | ServiceHealth;
    database?: string | ServiceHealth;
    postgres?: string | ServiceHealth;
    redis?: string | ServiceHealth;
    auth?: string | ServiceHealth;
    [key: string]: unknown;
  };
  runtime?: {
    started_at?: string;
    uptime_seconds?: number;
    uptime_human?: string;
    timestamp?: string;
  };
}

export interface PerformancePayload {
  db_pool?: {
    usage_pct?: number;
  };
  redis?: {
    latency_ms?: number;
    connected?: boolean;
  };
  sse?: {
    connections?: number;
    max?: number;
    usage_pct?: number;
  };
  requests?: {
    p99?: number;
    avg?: number;
    count?: number;
  };
  timestamp?: number | string;
}

export interface SseStatsPayload {
  active_connections?: number;
  total_connections?: number;
  max_connections?: number;
  topics?: Record<string, number>;
  connection_breakdown?: {
    connected?: number;
    inactive?: number;
  };
}

export interface LogEntry {
  id: string;
  time: string;
  level: 'low' | 'medium' | 'high';
  tag: string;
  message: string;
}

export type ServiceTone = 'up' | 'down' | 'degraded' | 'unknown';

export interface SystemStatusViewModel {
  statusText: string;
  overallStatus: 'healthy' | 'degraded' | 'down' | 'unknown';
  countFlights: string;
  countSSE: string;
  countErrors: string;
  responseTime: string;
  sseTotal: string;
  sseFlights: string;
  sseStatus: string;
  sseState: string;
  /** Display text (detail + optional uptime). */
  infraApi: string;
  infraPostgres: string;
  infraRedis: string;
  infraAuth: string;
  infraApiTone: ServiceTone;
  infraPostgresTone: ServiceTone;
  infraRedisTone: ServiceTone;
  infraAuthTone: ServiceTone;
  perfTimestamp: string;
  dbPoolPct: number;
  redisLatency: string;
  redisLatencyTone: ServiceTone;
  requestP99: string;
  sseConnPct: number;
}

export function unwrapApiData<T>(payload: T | ApiEnvelope<T> | null | undefined): T | null {
  if (!payload || typeof payload !== 'object') {
    return null;
  }
  if ('data' in payload && ('success' in payload || 'message' in payload)) {
    return (payload as ApiEnvelope<T>).data ?? null;
  }
  return payload as T;
}

function numberText(value: unknown): string {
  return typeof value === 'number' && Number.isFinite(value) ? String(value) : '-';
}

function fixedNumberText(value: unknown, digits = 1): string {
  return typeof value === 'number' && Number.isFinite(value) ? value.toFixed(digits) : '-';
}

function percentValue(value: unknown): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return 0;
  }
  return Math.max(0, Math.min(100, Number(value.toFixed(1))));
}

/** Extract a nested or scalar service health value. */
export function readServiceHealth(value: unknown): ServiceHealth {
  if (!value) return {};
  if (typeof value === 'string' || typeof value === 'boolean') {
    return { status: String(value) };
  }
  if (typeof value === 'object') {
    const obj = value as Record<string, unknown>;
    return {
      status: typeof obj.status === 'string' ? obj.status : undefined,
      detail: typeof obj.detail === 'string' ? obj.detail : undefined,
      uptime_seconds: typeof obj.uptime_seconds === 'number' ? obj.uptime_seconds : undefined,
      ...obj,
    };
  }
  return { status: String(value) };
}

export function serviceTone(value: unknown): ServiceTone {
  const health = readServiceHealth(value);
  if (typeof value === 'boolean') {
    return value ? 'up' : 'down';
  }
  const normalized = String(health.status ?? value ?? '').trim().toLowerCase();
  if (['ok', 'up', 'healthy', 'connected', 'active', 'running', 'online'].includes(normalized)) {
    return 'up';
  }
  if (['degraded', 'fallback', 'warning', 'warn', 'unknown'].includes(normalized)) {
    return 'degraded';
  }
  if (['down', 'error', 'failed', 'unhealthy', 'disconnected'].includes(normalized)) {
    return 'down';
  }
  if (!normalized) return 'unknown';
  return 'unknown';
}

/** @deprecated prefer serviceTone — kept for callers expecting "up"/"down"/"-" */
function serviceStatus(value: unknown): string {
  const tone = serviceTone(value);
  if (tone === 'up') return 'up';
  if (tone === 'down') return 'down';
  if (tone === 'degraded') return 'degraded';
  return '-';
}

export function formatUptime(seconds: unknown): string {
  if (!Number.isFinite(Number(seconds)) || Number(seconds) <= 0) {
    return '-';
  }
  let remaining = Math.floor(Number(seconds));
  const hours = Math.floor(remaining / 3600);
  remaining -= hours * 3600;
  const minutes = Math.floor(remaining / 60);
  const secs = remaining - minutes * 60;
  if (hours > 0) {
    return `${hours}h ${minutes}m ${secs}s`;
  }
  if (minutes > 0) {
    return `${minutes}m ${secs}s`;
  }
  return `${secs}s`;
}

function serviceDisplayText(
  value: unknown,
  options: { fallbackDetail?: string; uptimeSeconds?: number } = {},
): string {
  const health = readServiceHealth(value);
  const detail = health.detail || options.fallbackDetail || health.status || '-';
  const uptime = options.uptimeSeconds ?? health.uptime_seconds;
  if (uptime && Number(uptime) > 0) {
    const formatted = formatUptime(uptime);
    if (formatted !== '-') {
      return `${detail} · 运行 ${formatted}`;
    }
  }
  return detail;
}

function overallStatus(value: unknown): SystemStatusViewModel['overallStatus'] {
  const normalized = String(value ?? '').trim().toLowerCase();
  if (normalized === 'healthy' || normalized === 'ok') {
    return 'healthy';
  }
  if (normalized === 'degraded' || normalized === 'warning') {
    return 'degraded';
  }
  if (normalized === 'down' || normalized === 'unhealthy' || normalized === 'error') {
    return 'down';
  }
  return 'unknown';
}

function statusLabel(value: SystemStatusViewModel['overallStatus']): string {
  switch (value) {
    case 'healthy':
      return '运行正常';
    case 'degraded':
      return '性能降级';
    case 'down':
      return '服务不可用';
    default:
      return '状态未知';
  }
}

function formatTime(value: unknown, fallback = '-'): string {
  if (typeof value === 'number' && Number.isFinite(value)) {
    // Accept both unix seconds and ms.
    const ms = value > 1e12 ? value : value * 1000;
    return new Date(ms).toLocaleTimeString();
  }
  if (typeof value === 'string' && value.trim()) {
    const timestamp = Date.parse(value);
    return Number.isNaN(timestamp) ? value : new Date(timestamp).toLocaleTimeString();
  }
  return fallback;
}

export function severityToLogLevel(severity: unknown): LogEntry['level'] {
  const normalized = String(severity ?? '').trim().toLowerCase();
  if (['critical', 'fatal', 'error', 'high'].includes(normalized)) {
    return 'high';
  }
  if (['warning', 'warn', 'medium'].includes(normalized)) {
    return 'medium';
  }
  return 'low';
}

export function mapErrorToLogEntry(error: RuntimeErrorPayload, fallbackId: string): LogEntry {
  return {
    id: error.error_id || fallbackId,
    time: formatTime(error.timestamp ?? error.emitted_at_ms, 'SYS'),
    level: severityToLogLevel(error.severity),
    tag: String(error.error_type || 'ERR').slice(0, 4),
    message: error.message || '未提供错误详情',
  };
}

export function mapSystemStatusView(
  health: HealthPayload | null,
  performance: PerformancePayload | null,
  sseStats: SseStatsPayload | null,
): SystemStatusViewModel {
  const status = overallStatus(health?.status);
  const topics = sseStats?.topics ?? health?.buffer_status?.topics ?? {};
  const sseConnections = sseStats?.total_connections
    ?? performance?.sse?.connections
    ?? health?.buffer_status?.total_connections;
  const p99 = performance?.requests?.p99 ?? performance?.requests?.avg;
  const avg = performance?.requests?.avg;
  const services = health?.services ?? {};
  const apiService = services.api_server ?? services.api;
  const postgresService = services.postgres ?? services.database;
  const redisService = services.redis;
  const authService = services.auth;
  const runtimeUptime = health?.runtime?.uptime_seconds
    ?? readServiceHealth(apiService).uptime_seconds;

  const redisConnected = performance?.redis?.connected;
  let redisLatency = '-';
  let redisLatencyTone: ServiceTone = 'unknown';
  if (redisConnected === false) {
    redisLatency = '已断开';
    redisLatencyTone = 'down';
  } else if (typeof performance?.redis?.latency_ms === 'number') {
    redisLatency = fixedNumberText(performance.redis.latency_ms);
    redisLatencyTone = 'up';
  } else if (redisConnected === true) {
    redisLatency = '0.0';
    redisLatencyTone = 'up';
  }

  // Prefer request avg for "接口平均响应" when available (legacy parity).
  const responseTimeValue = typeof avg === 'number' ? avg : p99;

  return {
    statusText: statusLabel(status),
    overallStatus: status,
    countFlights: numberText(health?.database?.flights),
    countSSE: numberText(sseConnections),
    countErrors: numberText(health?.errors_count),
    responseTime: fixedNumberText(responseTimeValue),
    sseTotal: numberText(sseStats?.total_connections ?? health?.buffer_status?.total_connections),
    sseFlights: numberText(
      topics.flights
      ?? topics.flight_updates
      ?? topics.flight_status_changes,
    ),
    sseStatus: numberText(topics.global_status ?? topics.status_changes),
    sseState: gatewayStatus(health?.buffer_status?.status ?? (sseStats ? 'active' : undefined)),
    infraApi: serviceDisplayText(apiService, {
      fallbackDetail: '在线',
      uptimeSeconds: runtimeUptime,
    }),
    infraPostgres: serviceDisplayText(postgresService),
    infraRedis: serviceDisplayText(redisService),
    infraAuth: serviceDisplayText(authService),
    infraApiTone: serviceTone(apiService ?? health?.success),
    infraPostgresTone: serviceTone(postgresService),
    infraRedisTone: serviceTone(redisService ?? performance?.redis?.connected),
    infraAuthTone: serviceTone(authService),
    perfTimestamp: formatTime(performance?.timestamp),
    dbPoolPct: percentValue(performance?.db_pool?.usage_pct),
    redisLatency,
    redisLatencyTone,
    requestP99: fixedNumberText(p99),
    sseConnPct: percentValue(performance?.sse?.usage_pct),
  };
}

function gatewayStatus(value: unknown): string {
  if (typeof value === 'boolean') {
    return value ? 'active' : 'down';
  }
  const normalized = String(value ?? '').trim().toLowerCase();
  return normalized || '-';
}

// Re-export for tests that may still assert the simple scalar mapping path.
export { serviceStatus };
