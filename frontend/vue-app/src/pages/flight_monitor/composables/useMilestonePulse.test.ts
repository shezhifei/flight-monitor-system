// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest';
import { nextTick, ref } from 'vue';
import {
  MILESTONE_PULSE_DURATION_MS,
  createMilestoneDetector,
  useMilestonePulse,
} from './useMilestonePulse';
import type { Flight } from '../../../composables/useFlightData';

function makeFlight(overrides: Record<string, unknown>): Flight {
  return {
    flight_id: 'F1',
    flight_number: 'CZ3101',
    ...overrides,
  } as unknown as Flight;
}

describe('createMilestoneDetector', () => {
  it('初始基线（prime）不触发，即使里程碑字段已有值', () => {
    const detector = createMilestoneDetector();
    detector.prime([makeFlight({ cleaning_end_time: '2026-07-22T08:00:00Z' })]);
    const events = detector.detect([makeFlight({ cleaning_end_time: '2026-07-22T08:00:00Z' })]);
    expect(events).toEqual([]);
  });

  it('cleaning_end_time 从空变为有值时触发，带航班号与节点名', () => {
    const detector = createMilestoneDetector();
    detector.prime([makeFlight({ cleaning_end_time: null })]);
    const events = detector.detect([
      makeFlight({ cleaning_end_time: '2026-07-22T08:30:00Z' }),
    ]);
    expect(events).toHaveLength(1);
    expect(events[0]).toMatchObject({
      flightId: 'F1',
      flightNo: 'CZ3101',
      field: 'cleaning_end_time',
      label: '清洁结束',
    });
  });

  it('boarding_allowed_time 从空字符串变为有值时触发', () => {
    const detector = createMilestoneDetector();
    detector.prime([makeFlight({ boarding_allowed_time: '' })]);
    const events = detector.detect([
      makeFlight({ boarding_allowed_time: '2026-07-22T09:00:00Z' }),
    ]);
    expect(events).toHaveLength(1);
    expect(events[0]).toMatchObject({ field: 'boarding_allowed_time', label: '允许登机' });
  });

  it('值→值的变化不触发', () => {
    const detector = createMilestoneDetector();
    detector.prime([makeFlight({ cleaning_end_time: '2026-07-22T08:00:00Z' })]);
    const events = detector.detect([
      makeFlight({ cleaning_end_time: '2026-07-22T08:05:00Z' }),
    ]);
    expect(events).toEqual([]);
  });

  it('有值→清空（撤销）不触发', () => {
    const detector = createMilestoneDetector();
    detector.prime([makeFlight({ cleaning_end_time: '2026-07-22T08:00:00Z' })]);
    const events = detector.detect([makeFlight({ cleaning_end_time: null })]);
    expect(events).toEqual([]);
  });

  it('同一航班同一节点不重复触发（含撤销后重新设置）', () => {
    const detector = createMilestoneDetector();
    detector.prime([makeFlight({ cleaning_end_time: null })]);
    expect(
      detector.detect([makeFlight({ cleaning_end_time: '2026-07-22T08:30:00Z' })]),
    ).toHaveLength(1);
    // 同样的值再次推送
    expect(
      detector.detect([makeFlight({ cleaning_end_time: '2026-07-22T08:30:00Z' })]),
    ).toEqual([]);
    // 撤销后重新设置也不再触发
    detector.detect([makeFlight({ cleaning_end_time: null })]);
    expect(
      detector.detect([makeFlight({ cleaning_end_time: '2026-07-22T08:45:00Z' })]),
    ).toEqual([]);
  });

  it('不同航班或不同节点互不影响', () => {
    const detector = createMilestoneDetector();
    detector.prime([
      makeFlight({ flight_id: 'F1', cleaning_end_time: null, boarding_allowed_time: null }),
      makeFlight({ flight_id: 'F2', flight_number: 'CA1302', cleaning_end_time: null }),
    ]);
    const first = detector.detect([
      makeFlight({ flight_id: 'F1', cleaning_end_time: '2026-07-22T08:30:00Z', boarding_allowed_time: null }),
      makeFlight({ flight_id: 'F2', flight_number: 'CA1302', cleaning_end_time: null }),
    ]);
    expect(first).toHaveLength(1);
    const second = detector.detect([
      makeFlight({ flight_id: 'F1', cleaning_end_time: '2026-07-22T08:30:00Z', boarding_allowed_time: '2026-07-22T09:10:00Z' }),
      makeFlight({ flight_id: 'F2', flight_number: 'CA1302', cleaning_end_time: '2026-07-22T08:40:00Z' }),
    ]);
    expect(second).toHaveLength(2);
    expect(second.map((e) => `${e.flightId}:${e.field}`)).toEqual([
      'F1:boarding_allowed_time',
      'F2:cleaning_end_time',
    ]);
  });

  it('增量更新中新出现的航班只建基线，不视为跳变', () => {
    const detector = createMilestoneDetector();
    detector.prime([]);
    const events = detector.detect([
      makeFlight({ flight_id: 'F9', cleaning_end_time: '2026-07-22T08:00:00Z' }),
    ]);
    expect(events).toEqual([]);
  });

  it('无 flight_id 的航班被忽略', () => {
    const detector = createMilestoneDetector();
    detector.prime([]);
    expect(detector.detect([makeFlight({ flight_id: '' })])).toEqual([]);
  });
});

describe('useMilestonePulse', () => {
  it('初始全量加载不触发，之后的增量跳变触发并自动消失', async () => {
    vi.useFakeTimers();
    try {
      const flights = ref<Flight[]>([]);
      const pulse = useMilestonePulse(flights);

      // 初始全量快照：只建基线
      flights.value = [makeFlight({ cleaning_end_time: null })];
      await nextTick();
      expect(pulse.activePulse.value).toBeNull();

      // SSE 增量更新：cleaning_end_time 空 → 有值
      flights.value = [makeFlight({ cleaning_end_time: '2026-07-22T08:30:00Z' })];
      await nextTick();
      expect(pulse.activePulse.value).toMatchObject({
        flightNo: 'CZ3101',
        label: '清洁结束',
      });

      // 数秒后自动消失
      vi.advanceTimersByTime(MILESTONE_PULSE_DURATION_MS);
      expect(pulse.activePulse.value).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it('同一航班同一节点的重复更新不重复触发', async () => {
    vi.useFakeTimers();
    try {
      const flights = ref<Flight[]>([]);
      const pulse = useMilestonePulse(flights);

      flights.value = [makeFlight({ cleaning_end_time: null })];
      await nextTick();
      flights.value = [makeFlight({ cleaning_end_time: '2026-07-22T08:30:00Z' })];
      await nextTick();
      expect(pulse.activePulse.value).not.toBeNull();
      vi.advanceTimersByTime(MILESTONE_PULSE_DURATION_MS);
      expect(pulse.activePulse.value).toBeNull();

      // 相同值再次推送：不触发
      flights.value = [makeFlight({ cleaning_end_time: '2026-07-22T08:30:00Z' })];
      await nextTick();
      expect(pulse.activePulse.value).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });
});
