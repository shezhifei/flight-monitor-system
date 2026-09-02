import { describe, it, expect } from 'vitest';
import { findFlightById, getFlightRowId, resolveDirectionalFlightId, type Flight } from './useFlightData';
import { getFlightDomId } from '../components/flight-monitor/helpers';
import type { Flight as BindingFlight } from '@/types/bindings';

/** getFlightDomId 吃的是 bindings 层 FlightResponse；测试数据按该形状断言。 */
function domId(flight: Flight): string {
  return getFlightDomId(flight as unknown as BindingFlight);
}

/** 拆表后的过站监控行：row_id = 旧聚合 id（= 链 id），进/出港是全新方向航班 id。 */
function turnaroundRow(): Flight {
  return {
    row_id: 'OLD',
    link_id: 'OLD',
    kind: 'turnaround',
    flight_id: 'IN-NEW',
    inbound_flight_id: 'IN-NEW',
    outbound_flight_id: 'OUT-NEW',
    inbound_leg: { leg_type: 'inbound', flight_no: 'CA100' },
    outbound_leg: { leg_type: 'outbound', flight_no: 'CA101' },
    version: 2,
  };
}

/** 单边出港行：只有出港方向。 */
function singleOutboundRow(): Flight {
  return {
    row_id: 'OUT-1',
    kind: 'single',
    flight_id: 'OUT-1',
    outbound_flight_id: 'OUT-1',
    outbound_leg: { leg_type: 'outbound', flight_no: 'CA101' },
    version: 1,
  };
}

/** 拆表前 / 详情路径的旧聚合载荷：没有方向 id。 */
function legacyAggregate(): Flight {
  return {
    flight_id: 'LEGACY-1',
    inbound_leg: { leg_type: 'inbound', flight_no: 'CA100' },
    outbound_leg: { leg_type: 'outbound', flight_no: 'CA101' },
    version: 1,
  };
}

describe('监控行稳定键（列表 :key / selectedFlightId）', () => {
  it('列表 DOM id 用 row_id，不用会漂移的 flight_id', () => {
    const row = turnaroundRow();
    expect(domId(row)).toBe('OLD');
    expect(domId(row)).not.toBe(row.inbound_flight_id);
    expect(domId(row)).not.toBe(row.outbound_flight_id);
  });

  it('旧载荷（无 row_id）回退 flight_id', () => {
    expect(domId(legacyAggregate())).toBe('LEGACY-1');
    expect(getFlightRowId(legacyAggregate())).toBe('LEGACY-1');
  });

  it('getFlightRowId 是选中键的唯一来源', () => {
    expect(getFlightRowId(turnaroundRow())).toBe('OLD');
    expect(getFlightRowId(singleOutboundRow())).toBe('OUT-1');
  });

  it('findFlightById 按 row_id / 方向航班 id 都能命中间控行', () => {
    const row = turnaroundRow();
    expect(findFlightById('OLD', [row])).toBe(row);
    expect(findFlightById('IN-NEW', [row])).toBe(row);
    expect(findFlightById('OUT-NEW', [row])).toBe(row);
    expect(findFlightById('NOPE', [row])).toBeNull();
  });
});

describe('单元格方向解析（batch-cells / PATCH 的目标航班）', () => {
  it('进港字段打 inbound_flight_id，出港字段打 outbound_flight_id，绝不打 row_id', () => {
    const row = turnaroundRow();
    expect(resolveDirectionalFlightId(row, 'scheduled_arrival')).toBe('IN-NEW');
    expect(resolveDirectionalFlightId(row, 'estimated_arrival')).toBe('IN-NEW');
    expect(resolveDirectionalFlightId(row, 'scheduled_departure')).toBe('OUT-NEW');
    expect(resolveDirectionalFlightId(row, 'cobt_time')).toBe('OUT-NEW');
  });

  it('时间线里程碑按 leg_type 归方向：上轮挡→进港，撤轮挡/登机→出港', () => {
    const row = turnaroundRow();
    expect(resolveDirectionalFlightId(row, 'on_blocks_time')).toBe('IN-NEW');
    expect(resolveDirectionalFlightId(row, 'cabin_door_open_time')).toBe('IN-NEW');
    expect(resolveDirectionalFlightId(row, 'off_blocks_time')).toBe('OUT-NEW');
    expect(resolveDirectionalFlightId(row, 'start_boarding_time')).toBe('OUT-NEW');
  });

  it('行级字段（备注）进港侧优先；缺一侧不得借用对侧方向 id', () => {
    expect(resolveDirectionalFlightId(turnaroundRow(), 'flight_remarks')).toBe('IN-NEW');
    // 单边出港行的进港字段：没有进港航班，宁可 null 也不打在出港航班上。
    expect(resolveDirectionalFlightId(singleOutboundRow(), 'scheduled_arrival')).toBeNull();
    expect(resolveDirectionalFlightId(singleOutboundRow(), 'flight_remarks')).toBe('OUT-1');
  });

  it('旧聚合载荷（无方向 id）回退 flight_id，行为与拆表前一致', () => {
    expect(resolveDirectionalFlightId(legacyAggregate(), 'scheduled_arrival')).toBe('LEGACY-1');
    expect(resolveDirectionalFlightId(legacyAggregate(), 'off_blocks_time')).toBe('LEGACY-1');
  });
});
