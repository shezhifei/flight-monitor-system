import { describe, expect, it } from 'vitest';
import {
  formatKpiPercent,
  mapKpiSnapshot,
  mapKpiTrend,
  mapServiceNodes,
  unwrapApiData,
} from './kpiDashboardModel';

describe('kpi dashboard model', () => {
  it('unwraps both legacy envelopes and direct v2 payloads', () => {
    expect(unwrapApiData({ success: true, data: { value: 1 }, message: 'ok' })).toEqual({ value: 1 });
    expect(unwrapApiData({ items: [{ id: 1 }] })).toEqual({ items: [{ id: 1 }] });
  });

  it('formats backend ratios as user-facing percentages', () => {
    expect(formatKpiPercent(0.873)).toBe('87.3%');
    expect(formatKpiPercent(87.3)).toBe('87.3%');
    expect(formatKpiPercent(null)).toBe('-');
  });

  it('maps the actual /api/v2/kpi/snapshot contract into dashboard state', () => {
    const state = mapKpiSnapshot({
      calculated_at: '2026-06-08T10:00:00Z',
      turnaround_time_p90_minutes: 18,
      on_time_departure_rate: 0.873,
      service_node_compliance_rate: 0.961,
      equipment_utilization_rate: 0.76,
      abnormal_flight_ratio: 0.042,
      on_time_trend: [
        { date: '2026-06-07', value: 0.91 },
        { date: '2026-06-08', value: 0.873 },
      ],
      hourly_flight_volume: [
        { hour_label: '06:00', count: 12 },
        { hour_label: '07:00', count: 24 },
      ],
      turnaround_distribution: [
        { bucket: '0-30', count: 4 },
        { bucket: '30-60', count: 8 },
      ],
    });

    expect(state.scoreDepartureValue).toBe('87.3%');
    expect(state.scoreTurnValue).toBe('18 分');
    expect(state.scoreServiceValue).toBe('96.1%');
    expect(state.equipmentRate).toBe('76.0%');
    expect(state.abnormalRatio).toBe('4.2%');
    expect(state.decisionAttainment).toBe('未达标');
    expect(state.trendData).toEqual([
      { label: '06-07', value: 91, targetGap: 1 },
      { label: '06-08', value: 87.3, targetGap: -2.7 },
    ]);
    expect(state.hourlyData).toEqual([
      { hour: '06:00', value: 50 },
      { hour: '07:00', value: 100 },
    ]);
    expect(state.distributionData).toEqual([
      { label: '0-30', value: 4 },
      { label: '30-60', value: 8 },
    ]);
  });

  it('maps /api/v2/kpi/trend response items', () => {
    expect(mapKpiTrend({ items: [{ date: '2026-06-08', value: 0.925 }] })).toEqual([
      { label: '06-08', value: 92.5, targetGap: 2.5 },
    ]);
  });

  it('maps service node compliance into visible node detail rows', () => {
    expect(
      mapServiceNodes({
        date: '2026-06-08',
        items: [
          { node: 'cleaning', rate: 0.93 },
          { node: 'loading', rate: 0.81 },
          { node: 'boarding', rate: 1 },
        ],
      }),
    ).toEqual([
      { id: 'cleaning', label: '清洁', value: 93, displayValue: '93.0%', status: 'pass' },
      { id: 'loading', label: '装载', value: 81, displayValue: '81.0%', status: 'warning' },
      { id: 'boarding', label: '登机', value: 100, displayValue: '100.0%', status: 'pass' },
    ]);
  });
});
