import { describe, expect, it } from 'vitest';
import {
  formatUptime,
  mapErrorToLogEntry,
  mapSystemStatusView,
  readServiceHealth,
  serviceTone,
  severityToLogLevel,
  unwrapApiData,
} from './systemStatusModel';

describe('system status model', () => {
  it('unwraps direct health payloads and ApiResponse envelopes', () => {
    expect(unwrapApiData({ status: 'healthy' })).toEqual({ status: 'healthy' });
    expect(unwrapApiData({ success: true, data: { db_pool: { usage_pct: 12 } }, message: 'ok' })).toEqual({
      db_pool: { usage_pct: 12 },
    });
    expect(unwrapApiData(null)).toBeNull();
  });

  it('reads nested service health objects and scalars', () => {
    expect(readServiceHealth({ status: 'healthy', detail: '连接池正常' })).toMatchObject({
      status: 'healthy',
      detail: '连接池正常',
    });
    expect(readServiceHealth('ok').status).toBe('ok');
    expect(serviceTone({ status: 'degraded', detail: '使用内存回退' })).toBe('degraded');
    expect(serviceTone({ status: 'healthy' })).toBe('up');
    expect(serviceTone({ status: 'down' })).toBe('down');
  });

  it('formats uptime for API server detail lines', () => {
    expect(formatUptime(86400)).toBe('24h 0m 0s');
    expect(formatUptime(125)).toBe('2m 5s');
    expect(formatUptime(0)).toBe('-');
  });

  it('maps nested services.api_server detail + uptime into infra fields', () => {
    const view = mapSystemStatusView(
      {
        success: true,
        status: 'healthy',
        database: { flights: 128 },
        errors_count: 1,
        buffer_status: {
          total_connections: 7,
          status: 'active',
          topics: {
            flight_updates: 3,
            global_status: 2,
          },
        },
        services: {
          api_server: {
            status: 'healthy',
            detail: 'API 服务在线',
            uptime_seconds: 86400,
          },
          postgres: {
            status: 'healthy',
            detail: '连接池正常',
          },
          redis: {
            status: 'degraded',
            detail: '使用内存回退',
          },
          auth: {
            status: 'healthy',
            detail: '认证服务正常',
          },
        },
        runtime: {
          uptime_seconds: 86400,
          uptime_human: '1d',
        },
      },
      {
        db_pool: { usage_pct: 20 },
        redis: { latency_ms: 2.4, connected: true },
        sse: { connections: 7, max: 1000, usage_pct: 0.7 },
        requests: { p99: 120, avg: 29, count: 2400 },
        timestamp: 1783996200,
      },
      null,
    );

    expect(view.overallStatus).toBe('healthy');
    expect(view.countFlights).toBe('128');
    expect(view.infraApi).toContain('API 服务在线');
    expect(view.infraApi).toContain('运行');
    expect(view.infraApiTone).toBe('up');
    expect(view.infraPostgres).toBe('连接池正常');
    expect(view.infraPostgresTone).toBe('up');
    expect(view.infraRedis).toBe('使用内存回退');
    expect(view.infraRedisTone).toBe('degraded');
    expect(view.infraAuth).toBe('认证服务正常');
    expect(view.infraAuthTone).toBe('up');
    expect(view.sseFlights).toBe('3');
    expect(view.sseStatus).toBe('2');
    expect(view.redisLatency).toBe('2.4');
    expect(view.redisLatencyTone).toBe('up');
    expect(view.responseTime).toBe('29.0');
    expect(view.requestP99).toBe('120.0');
  });

  it('maps real health, performance, and SSE stats payloads into page metrics', () => {
    const view = mapSystemStatusView(
      {
        success: true,
        status: 'degraded',
        database: { flights: 345 },
        errors_count: 4,
        buffer_status: {
          total_connections: 9,
          status: 'active',
          topics: {
            flights: 3,
            global_status: 2,
          },
        },
        services: {
          database: 'healthy',
          redis: 'connected',
          auth: 'ok',
        },
      },
      {
        db_pool: { usage_pct: 66.64 },
        redis: { latency_ms: 2.345, connected: true },
        sse: { connections: 10, max: 200, usage_pct: 5 },
        requests: { p99: 123.456, avg: 45.2 },
        timestamp: 1780893600,
      },
      {
        active_connections: 8,
        total_connections: 10,
        max_connections: 200,
        topics: {
          flights: 4,
          global_status: 1,
        },
      },
    );

    expect(view.statusText).toBe('性能降级');
    expect(view.overallStatus).toBe('degraded');
    expect(view.countFlights).toBe('345');
    expect(view.countSSE).toBe('10');
    expect(view.countErrors).toBe('4');
    expect(view.responseTime).toBe('45.2');
    expect(view.sseFlights).toBe('4');
    expect(view.sseStatus).toBe('1');
    expect(view.sseState).toBe('active');
    // Scalar services still map correctly via serviceTone.
    expect(view.infraPostgresTone).toBe('up');
    expect(view.infraRedisTone).toBe('up');
    expect(view.infraAuthTone).toBe('up');
    expect(view.dbPoolPct).toBe(66.6);
    expect(view.redisLatency).toBe('2.3');
    expect(view.requestP99).toBe('123.5');
    expect(view.sseConnPct).toBe(5);
  });

  it('shows disconnected redis latency when performance reports connected=false', () => {
    const view = mapSystemStatusView(
      { status: 'degraded', services: { redis: { status: 'down', detail: '不可用' } } },
      { redis: { connected: false, latency_ms: 0 }, requests: { p99: 10, avg: 5 } },
      null,
    );
    expect(view.redisLatency).toBe('已断开');
    expect(view.redisLatencyTone).toBe('down');
  });

  it('maps backend severities into visible log levels', () => {
    expect(severityToLogLevel('critical')).toBe('high');
    expect(severityToLogLevel('error')).toBe('high');
    expect(severityToLogLevel('warning')).toBe('medium');
    expect(severityToLogLevel('info')).toBe('low');
  });

  it('maps runtime error events into readable log rows', () => {
    const log = mapErrorToLogEntry(
      {
        error_id: 'err-1',
        timestamp: '2026-06-08T10:00:00Z',
        error_type: 'ApiInternalError',
        message: 'database timeout',
        severity: 'high',
        operation: 'flight_sync',
      },
      'fallback-id',
    );

    expect(log.id).toBe('err-1');
    expect(log.level).toBe('high');
    expect(log.tag).toBe('ApiI');
    expect(log.message).toBe('database timeout');
    expect(log.time).not.toBe('SYS');
  });

  it('falls back to success boolean for api tone when services.api_server is absent', () => {
    const view = mapSystemStatusView(
      { success: true, status: 'healthy' },
      null,
      null,
    );
    expect(view.infraApiTone).toBe('up');
  });
});
