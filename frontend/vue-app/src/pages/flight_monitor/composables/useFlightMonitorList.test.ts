import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ref } from 'vue';
import { useFlightMonitorList } from './useFlightMonitorList';
import { BASE_COLUMNS, DEFAULT_VISIBLE_COLUMN_KEYS } from '../../../components/flight-monitor/FlightList.vue';
import { DEFAULT_BUSINESS_FILTERS, DEFAULT_SEARCH_FIELDS } from '../../../composables/useFlightData';
import type { UseFlightMonitorListOptions } from './useFlightMonitorList';

function createMemoryStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() {
      return values.size;
    },
    clear: () => values.clear(),
    getItem: (key: string) => values.get(key) ?? null,
    key: (index: number) => Array.from(values.keys())[index] ?? null,
    removeItem: (key: string) => {
      values.delete(key);
    },
    setItem: (key: string, value: string) => {
      values.set(key, String(value));
    },
  };
}

function makeOptions(): UseFlightMonitorListOptions {
  const flightData = {
    originalFlights: ref([]),
    sortedFlights: ref([]),
    searchQuery: ref(''),
    businessFilters: ref({ ...DEFAULT_BUSINESS_FILTERS }),
    searchFields: ref({ ...DEFAULT_SEARCH_FIELDS }),
    sortConfig: ref({ field: 'scheduled_departure', direction: 'asc' }),
    setSortConfig: vi.fn(),
    setSearchQuery: vi.fn(),
    setSearchFields: vi.fn(),
    setBusinessFilters: vi.fn(),
    findFlightById: vi.fn(() => null),
  };
  const flightStream = {
    initialized: ref(true),
  };
  return {
    flightData: flightData as unknown as UseFlightMonitorListOptions['flightData'],
    flightStream: flightStream as unknown as UseFlightMonitorListOptions['flightStream'],
    selectedFlightId: ref(null),
    viewMode: ref('table'),
    alertPoolOpen: ref(false),
    searchOptionsExpanded: ref(false),
    businessFilterExpanded: ref(false),
    announce: vi.fn(),
  };
}

beforeEach(() => {
  vi.stubGlobal('localStorage', createMemoryStorage());
});

describe('useFlightMonitorList 列配置', () => {
  it('无保存配置时默认可见列对齐 legacy 9 列', () => {
    const list = useFlightMonitorList(makeOptions());
    expect(list.visibleColumns.value).toEqual([...DEFAULT_VISIBLE_COLUMN_KEYS]);
  });

  it('全部列都进入列配置弹窗候选', () => {
    const list = useFlightMonitorList(makeOptions());
    expect(list.columnConfigState.value.items).toEqual(BASE_COLUMNS.map((c) => c.key));
    // 打卡列与备注列可勾选
    expect(list.columnConfigState.value.items).toContain('cabin_door_open_time');
    expect(list.columnConfigState.value.items).toContain('load_planning_remarks');
    expect(list.columnConfigState.value.items).toContain('aircraft_check_remarks');
  });

  it('兼容旧 key 的保存配置：显式布尔值优先，未记录的新列按 legacy 默认', () => {
    // 旧版本配置：只有 16 个旧列的显式 true
    const legacySaved = {
      flight_number: true,
      route: true,
      status: true,
      scheduled_departure: true,
      scheduled_arrival: true,
      stand: true,
      cobt_time: true,
      boarding_allowed_time: true,
      start_boarding_time: true,
      end_boarding_time: true,
      on_blocks_time: true,
      off_blocks_time: true,
      baggage_carousel: true,
      aircraft_type: true,
      tags: true,
      remarks: true,
    };
    localStorage.setItem('flight_monitor_columns', JSON.stringify(legacySaved));
    const list = useFlightMonitorList(makeOptions());
    const visible = list.visibleColumns.value;
    // 旧配置显式 true 的列保持可见
    expect(visible).toContain('cobt_time');
    expect(visible).toContain('tags');
    expect(visible).toContain('remarks');
    // 旧配置没有的新列按 legacy 默认：flight_type/gate 默认可见，codt/打卡列默认隐藏
    expect(visible).toContain('flight_type');
    expect(visible).toContain('gate');
    expect(visible).not.toContain('codt');
    expect(visible).not.toContain('cabin_door_open_time');
    expect(visible).not.toContain('registration');
  });

  it('保存配置中的显式 false 被尊重', () => {
    localStorage.setItem('flight_monitor_columns', JSON.stringify({ flight_number: false, codt: true }));
    const list = useFlightMonitorList(makeOptions());
    expect(list.visibleColumns.value).not.toContain('flight_number');
    // 显式打开的新列可见
    expect(list.visibleColumns.value).toContain('codt');
  });

  it('损坏的配置回退到 legacy 默认', () => {
    localStorage.setItem('flight_monitor_columns', '{not-json');
    const list = useFlightMonitorList(makeOptions());
    expect(list.visibleColumns.value).toEqual([...DEFAULT_VISIBLE_COLUMN_KEYS]);
  });

  it('resetColumnConfig 恢复 legacy 默认可见列', () => {
    localStorage.setItem('flight_monitor_columns', JSON.stringify({ flight_number: false, codt: true }));
    const list = useFlightMonitorList(makeOptions());
    expect(list.visibleColumns.value).toContain('codt');
    list.resetColumnConfig();
    expect(list.visibleColumns.value).toEqual([...DEFAULT_VISIBLE_COLUMN_KEYS]);
  });

  it('handleColumnSave 仍写入 flight_monitor_columns key', () => {
    const list = useFlightMonitorList(makeOptions());
    list.handleColumnSave();
    const raw = localStorage.getItem('flight_monitor_columns');
    expect(raw).toBeTruthy();
    const saved = JSON.parse(raw as string) as Record<string, boolean>;
    expect(saved.flight_number).toBe(true);
    expect(saved.codt).toBe(false);
  });

  it('loadColumnOrder 读取 flight_monitor_columns_order', () => {
    const order = ['remarks', 'flight_number', 'stand'];
    localStorage.setItem('flight_monitor_columns_order', JSON.stringify(order));
    const list = useFlightMonitorList(makeOptions());
    const items = list.columnConfigState.value.items;
    expect(items[0]).toBe('remarks');
    expect(items.indexOf('flight_number')).toBeLessThan(items.indexOf('stand'));
    // 未列出的列仍会补齐
    expect(items).toContain('gate');
  });

  it('reorderColumnItems 按 legacy 规则重排', () => {
    const list = useFlightMonitorList(makeOptions());
    const before = [...list.columnConfigState.value.items];
    const a = before[0];
    const b = before[2];
    list.reorderColumnItems(a, b);
    const after = list.columnConfigState.value.items;
    // a 移到原 b 位置之后（from < to → after）
    expect(after.indexOf(a)).toBe(after.indexOf(b) + 1);
  });

  it('handleColumnSave 写入 columns_order', () => {
    const list = useFlightMonitorList(makeOptions());
    list.reorderColumnItems(list.columnConfigState.value.items[0], list.columnConfigState.value.items[1]);
    list.handleColumnSave();
    const raw = localStorage.getItem('flight_monitor_columns_order');
    expect(raw).toBeTruthy();
    expect(JSON.parse(raw as string)).toEqual(list.columnConfigState.value.items);
  });
});
