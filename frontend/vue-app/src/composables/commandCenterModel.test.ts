import { describe, expect, it } from 'vitest';
import {
  buildCommandCenterSnapshot,
  calculateVerdict,
  unwrapList,
} from './commandCenterModel';

describe('command center model', () => {
  it('unwraps direct arrays, ApiResponse envelopes, and paged payloads', () => {
    expect(unwrapList([{ id: 'a' }])).toEqual([{ id: 'a' }]);
    expect(unwrapList({ success: true, data: [{ id: 'b' }] })).toEqual([{ id: 'b' }]);
    expect(unwrapList({ data: { items: [{ id: 'c' }], total: 1 } })).toEqual([{ id: 'c' }]);
    expect(unwrapList({ orders: [{ id: 'd' }] })).toEqual([{ id: 'd' }]);
  });

  it('builds populated command-center panels from real backend-shaped payloads', () => {
    const now = new Date('2026-06-08T08:00:00Z');
    const snapshot = buildCommandCenterSnapshot(
      {
        success: true,
        data: [
          {
            flight_id: 'flt-1',
            flight_number: 'MU100',
            scheduled_departure: '2026-06-08T08:30:00Z',
            estimated_departure: '2026-06-08T09:20:00Z',
            terminal: 'T1',
            stand: 'A12',
          },
          {
            flight_id: 'flt-2',
            flight_number: 'CZ200',
            scheduled_departure: '2026-06-08T10:30:00Z',
            estimated_departure: '2026-06-08T10:35:00Z',
            terminal: 'T2',
            stand: 'B08',
          },
        ],
      },
      {
        data: {
          items: [
            {
              anomaly_id: 'ano-1',
              flight_number: 'MU100',
              status: 'open',
              severity: 'high',
              stand: 'A12',
              terminal: 'T1',
              message: '登机口异常',
            },
          ],
        },
      },
      [
        {
          order_id: 'ord-1',
          flight_number: 'MU100',
          status: 'pending',
          blocked: true,
          task_type: 'cleaning',
          team_name: '保洁一组',
        },
      ],
      6,
      now,
    );

    expect(snapshot.kpis).toEqual({
      decisionCount: 1,
      riskFlights: 1,
      openAnomalies: 1,
      dispatchBlockers: 1,
      delayPressure: 28,
    });
    expect(snapshot.verdict.title).toBe('需关注');
    expect(snapshot.priorityQueue.map((item) => item.id)).toContain('ord-1');
    expect(snapshot.priorityQueue.map((item) => item.id)).toContain('ano-1');
    expect(snapshot.windowPressure.find((item) => item.id === '60-120')?.value).toBe(1);
    expect(snapshot.heatmapData).toEqual([
      { id: 'stand-A12', label: 'A12', value: 1, detail: '1 个未闭环异常', severity: 'ok' },
    ]);
    expect(snapshot.dispatchLoad).toEqual([
      { id: 'dispatch-保洁一组', label: '保洁一组', value: 1, detail: '1 个待执行任务', severity: 'ok' },
    ]);
    expect(snapshot.terminalLoad).toEqual([
      { id: 'terminal-T1', label: 'T1', value: 1, detail: '1 个窗口内航班', severity: 'ok' },
      { id: 'terminal-T2', label: 'T2', value: 1, detail: '1 个窗口内航班', severity: 'ok' },
    ]);
  });

  it('escalates the verdict for severe operational pressure', () => {
    expect(calculateVerdict({
      decisionCount: 8,
      riskFlights: 11,
      openAnomalies: 1,
      dispatchBlockers: 0,
      delayPressure: 45,
    }, 12)).toMatchObject({
      title: '高压力态势',
      severity: 'critical',
      window: '当前 12h 窗口',
    });
  });
});
