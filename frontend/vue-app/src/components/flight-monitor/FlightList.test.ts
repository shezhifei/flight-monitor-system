import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, type VueWrapper } from '@vue/test-utils';
import { ref } from 'vue';
import FlightList, { BASE_COLUMNS, DEFAULT_VISIBLE_COLUMN_KEYS } from './FlightList.vue';
import type { Flight } from '@/types/bindings';

const mockWriteTimeline = vi.fn<(flightId: string, field: string, options: { value: string | null | undefined }) => Promise<unknown>>();
const mockShowToast = vi.fn();
let mockUser: { permissions: string[] } | null = null;

vi.mock('../../composables/useFlightSync', async (importActual) => {
  const actual = await importActual<typeof import('../../composables/useFlightSync')>();
  return {
    ...actual,
    writeDispatchTimelineField: (...args: unknown[]) => mockWriteTimeline(...args as [string, string, { value: string | null | undefined }]),
  };
});

vi.mock('../../composables/useToast', () => ({
  useToast: () => ({ showToast: mockShowToast }),
}));

vi.mock('../../composables/useAuth', async (importActual) => {
  const actual = await importActual<typeof import('../../composables/useAuth')>();
  return {
    ...actual,
    useAuth: () => ({
      getUser: () => mockUser,
      apiBase: ref('/api/v2'),
      fetch: vi.fn(),
    }),
  };
});

function makeFlight(overrides: Record<string, unknown> = {}): Flight {
  return {
    flight_id: 'F100',
    flight_number: 'CZ3001',
    status: '计划中',
    scheduled_departure: '2026-07-22T08:00:00Z',
    scheduled_arrival: '2026-07-22T10:00:00Z',
    stand: 'A12',
    gate: 'G08',
    registration: 'B-1234',
    aircraft_type_detail: 'A320',
    has_boarding_restriction: false,
    is_quick_turnaround: false,
    is_commercial_signed: true,
    anomaly_summary: { has_open_anomaly: false, open_count: 0, acknowledged_count: 0 },
    business_cases: [],
    version: 3,
    ...overrides,
  } as unknown as Flight;
}

const ALL_COLUMN_KEYS = BASE_COLUMNS.map((c) => c.key);

function mountTable(flight: Flight, propOverrides: Record<string, unknown> = {}): VueWrapper {
  return mount(FlightList, {
    props: {
      flights: [flight],
      airportContext: { code: 'CAN', display_name: '广州白云', name_aliases: [] },
      selectedFlightId: null,
      viewMode: 'table',
      showAlertPool: false,
      hasActiveFilters: false,
      sortField: null,
      sortDirection: 'asc',
      visibleColumns: [...ALL_COLUMN_KEYS],
      canSelectCells: true,
      isCellSelected: () => false,
      canEditField: () => true,
      ...propOverrides,
    },
  });
}

function cell(wrapper: VueWrapper, field: string) {
  return wrapper.find(`td[data-field="${field}"]`);
}

beforeEach(() => {
  mockWriteTimeline.mockReset();
  mockWriteTimeline.mockResolvedValue({ items: [], cache: new Map() });
  mockShowToast.mockReset();
  mockUser = null;
});

afterEach(() => {
  document.body.innerHTML = '';
});

describe('BASE_COLUMNS 列体系', () => {
  it('默认可见列对齐 legacy 的 9 列', () => {
    expect(DEFAULT_VISIBLE_COLUMN_KEYS).toEqual([
      'flight_number',
      'status',
      'route',
      'scheduled_departure',
      'scheduled_arrival',
      'flight_type',
      'stand',
      'gate',
      'aircraft_type',
    ]);
  });

  it('覆盖 legacy 36 列中有数据的列', () => {
    const keys = new Set(ALL_COLUMN_KEYS);
    const expected = [
      'flight_number', 'status', 'route', 'scheduled_departure', 'scheduled_arrival',
      'flight_type', 'time_dep_sch', 'time_arr_sch', 'time_dep_est', 'time_arr_est',
      'time_dep_act', 'time_arr_act', 'stand', 'gate', 'baggage_carousel',
      'aircraft_type', 'missions', 'cobt_time', 'codt',
      'boarding_allowed_time', 'start_boarding_time', 'end_boarding_time',
      'passenger_ready_time', 'on_blocks_time', 'off_blocks_time',
      'cabin_door_open_time', 'deboarding_complete_time',
      'cleaning_start_time', 'cleaning_end_time',
      'cabin_door_close_time', 'cargo_door_close_time', 'loading_complete_time',
      'remarks', 'load_planning_remarks', 'aircraft_maintenance_remarks',
      'aircraft_check_remarks', 'registration',
    ];
    for (const key of expected) {
      expect(keys.has(key), `缺少列 ${key}`).toBe(true);
    }
  });

  it('按 visibleColumns 渲染表头与单元格', () => {
    const wrapper = mountTable(makeFlight(), {
      visibleColumns: ['flight_number', 'gate', 'registration'],
    });
    const headers = wrapper.findAll('thead th');
    expect(headers.map((h) => h.text())).toEqual(['航班号', '登机口', '机号']);
    const cells = wrapper.find('tbody tr[data-flight-id]').findAll('td');
    expect(cells[1].text()).toBe('G08');
    expect(cells[2].text()).toBe('B-1234');
    expect(wrapper.find('td[data-field="cobt_time"]').exists()).toBe(false);
  });

  it('表头与单元格遵循 visibleColumns 的拖拽顺序', () => {
    const wrapper = mountTable(makeFlight(), {
      visibleColumns: ['registration', 'flight_number', 'gate'],
    });
    const headers = wrapper.findAll('thead th');
    expect(headers.map((h) => h.text())).toEqual(['机号', '航班号', '登机口']);
    const cells = wrapper.find('tbody tr[data-flight-id]').findAll('td');
    expect(cells).toHaveLength(3);
    expect(cells[0].text()).toBe('B-1234');
    expect(cells[2].text()).toBe('G08');
  });

  it('时间细分列 HH:MM 渲染、空值 —', () => {
    const wrapper = mountTable(makeFlight({ estimated_departure: '2026-07-22T08:30:00Z' }));
    const estCell = wrapper.findAll('td.cell-time').find((td) => td.text() !== '—');
    expect(estCell).toBeDefined();
    // 空的时间列显示 —
    const flight = makeFlight({ codt: null });
    const wrapper2 = mountTable(flight);
    const codtCell = wrapper2.findAll('td').find((td) => td.classes().includes('cell-time') && td.text() === '—');
    expect(codtCell).toBeDefined();
  });
});

describe('交互式打卡单元格', () => {
  it('空值打卡列渲染可点击「+」占位', () => {
    const wrapper = mountTable(makeFlight({ cabin_door_open_time: null }));
    const placeholder = cell(wrapper, 'cabin_door_open_time').find('.cell-punch-placeholder');
    expect(placeholder.exists()).toBe(true);
    expect(placeholder.text()).toBe('+');
  });

  it('有值打卡列渲染时间而非「+」', () => {
    const wrapper = mountTable(makeFlight({ cabin_door_open_time: '2026-07-22T07:30:00Z' }));
    const target = cell(wrapper, 'cabin_door_open_time');
    expect(target.find('.cell-punch-placeholder').exists()).toBe(false);
    expect(target.find('.cell-punch-value').text()).toMatch(/\d{2}:\d{2}/);
  });

  it('单击空值未注册打卡列写入当前时间（dispatch-timeline API）', async () => {
    mockUser = { permissions: ['flight.timeline_edit'] };
    const flight = makeFlight({ cabin_door_open_time: null });
    const wrapper = mountTable(flight);
    await cell(wrapper, 'cabin_door_open_time').trigger('click');
    expect(mockWriteTimeline).toHaveBeenCalledTimes(1);
    const [flightId, field, options] = mockWriteTimeline.mock.calls[0];
    expect(flightId).toBe('F100');
    expect(field).toBe('cabin_door_open_time');
    expect(typeof options.value).toBe('string');
    expect(Number.isNaN(new Date(options.value as string).getTime())).toBe(false);
    // 乐观更新
    expect((flight as unknown as Record<string, unknown>).cabin_door_open_time).toBe(options.value);
  });

  it('单击空值已注册打卡列（如 on_blocks_time）同样写入当前时间', async () => {
    mockUser = { permissions: ['flight.timeline_edit'] };
    const flight = makeFlight({ on_blocks_time: null });
    const wrapper = mountTable(flight, { canEditField: () => true });
    await cell(wrapper, 'on_blocks_time').trigger('click');
    expect(mockWriteTimeline).toHaveBeenCalledTimes(1);
    expect(mockWriteTimeline.mock.calls[0][1]).toBe('on_blocks_time');
  });

  it('单击有值打卡列不重复打卡', async () => {
    mockUser = { permissions: ['flight.timeline_edit'] };
    const flight = makeFlight({ cabin_door_open_time: '2026-07-22T07:30:00Z' });
    const wrapper = mountTable(flight);
    await cell(wrapper, 'cabin_door_open_time').trigger('click');
    expect(mockWriteTimeline).not.toHaveBeenCalled();
  });

  it('无权限用户单击打卡被禁用并 toast', async () => {
    mockUser = null; // 无 flight.timeline_edit
    const flight = makeFlight({ cabin_door_open_time: null });
    const wrapper = mountTable(flight, { canEditField: () => false });
    await cell(wrapper, 'cabin_door_open_time').trigger('click');
    expect(mockWriteTimeline).not.toHaveBeenCalled();
    expect(mockShowToast).toHaveBeenCalledTimes(1);
    expect(mockShowToast.mock.calls[0][0]).toBe('warning');
  });

  it('无权限用户单击已注册打卡列走 canEditField 门控', async () => {
    mockUser = { permissions: ['flight.timeline_edit'] };
    const flight = makeFlight({ on_blocks_time: null });
    const wrapper = mountTable(flight, { canEditField: () => false });
    await cell(wrapper, 'on_blocks_time').trigger('click');
    expect(mockWriteTimeline).not.toHaveBeenCalled();
    expect(mockShowToast).toHaveBeenCalledTimes(1);
  });

  it('打卡失败回滚并 toast', async () => {
    mockUser = { permissions: ['flight.timeline_edit'] };
    mockWriteTimeline.mockRejectedValueOnce(new Error('写入时间线失败 (500)'));
    const flight = makeFlight({ cabin_door_open_time: null });
    const wrapper = mountTable(flight);
    await cell(wrapper, 'cabin_door_open_time').trigger('click');
    await vi.waitFor(() => {
      expect(mockShowToast).toHaveBeenCalled();
    });
    expect(mockShowToast.mock.calls[0][0]).toBe('error');
    expect((flight as unknown as Record<string, unknown>).cabin_door_open_time).toBeNull();
  });

  it('有值未注册打卡列右键弹出修改/撤销菜单', async () => {
    mockUser = { permissions: ['flight.timeline_edit'] };
    const flight = makeFlight({ cabin_door_open_time: '2026-07-22T07:30:00Z' });
    const wrapper = mountTable(flight);
    await cell(wrapper, 'cabin_door_open_time').trigger('contextmenu', { clientX: 100, clientY: 100 });
    const menu = document.body.querySelector('.punch-context-menu');
    expect(menu).not.toBeNull();
    const items = Array.from(menu!.querySelectorAll('.punch-context-menu-item')).map((el) => el.textContent?.trim());
    expect(items).toEqual(['修改', '撤销']);
  });

  it('空值打卡列右键不弹菜单', async () => {
    mockUser = { permissions: ['flight.timeline_edit'] };
    const flight = makeFlight({ cabin_door_open_time: null });
    const wrapper = mountTable(flight);
    await cell(wrapper, 'cabin_door_open_time').trigger('contextmenu', { clientX: 100, clientY: 100 });
    expect(document.body.querySelector('.punch-context-menu')).toBeNull();
  });

  it('撤销经 confirm 后置空（dispatch-timeline 删除）', async () => {
    mockUser = { permissions: ['flight.timeline_edit'] };
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);
    const flight = makeFlight({ cabin_door_open_time: '2026-07-22T07:30:00Z' });
    const wrapper = mountTable(flight);
    await cell(wrapper, 'cabin_door_open_time').trigger('contextmenu', { clientX: 100, clientY: 100 });
    const revokeBtn = Array.from(document.body.querySelectorAll<HTMLButtonElement>('.punch-context-menu-item'))
      .find((el) => el.textContent?.trim() === '撤销');
    revokeBtn?.click();
    await vi.waitFor(() => {
      expect(mockWriteTimeline).toHaveBeenCalledTimes(1);
    });
    expect(confirmSpy).toHaveBeenCalled();
    expect(mockWriteTimeline.mock.calls[0][2].value).toBeNull();
    expect((flight as unknown as Record<string, unknown>).cabin_door_open_time).toBeNull();
    confirmSpy.mockRestore();
  });

  it('无权限时撤销菜单项转静声并 toast 报错', async () => {
    mockUser = null;
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);
    const flight = makeFlight({ cabin_door_open_time: '2026-07-22T07:30:00Z' });
    const wrapper = mountTable(flight, { canEditField: () => false });
    await cell(wrapper, 'cabin_door_open_time').trigger('contextmenu', { clientX: 100, clientY: 100 });
    const revokeBtn = Array.from(document.body.querySelectorAll<HTMLButtonElement>('.punch-context-menu-item'))
      .find((el) => el.textContent?.trim() === '撤销');
    // 静声（mute）= 看得见、点得动，但点了会被拒并说明原因
    expect(revokeBtn?.getAttribute('data-tone')).toBe('mute');
    revokeBtn?.click();
    await vi.waitFor(() => {
      expect(mockShowToast).toHaveBeenCalled();
    });
    expect(mockShowToast.mock.calls[0][0]).toBe('error');
    expect(mockWriteTimeline).not.toHaveBeenCalled();
    confirmSpy.mockRestore();
  });

  it('已注册打卡列右键仍走父级 open-context-menu 菜单', async () => {
    mockUser = { permissions: ['flight.timeline_edit'] };
    const flight = makeFlight({ on_blocks_time: '2026-07-22T07:30:00Z' });
    const wrapper = mountTable(flight, { canEditField: () => true });
    await cell(wrapper, 'on_blocks_time').trigger('contextmenu', { clientX: 100, clientY: 100 });
    const events = wrapper.emitted('open-context-menu');
    expect(events).toBeTruthy();
    expect(events![0][2]).toBe('on_blocks_time');
    // 不开组件内菜单
    expect(document.body.querySelector('.punch-context-menu')).toBeNull();
  });
});

describe('备注类列', () => {
  it('配载/机务/复核备注双击触发 edit-field', async () => {
    const flight = makeFlight({
      load_planning_remarks: '配载注意',
      aircraft_maintenance_remarks: '机务注意',
      aircraft_check_remarks: 'B-1234',
    });
    const wrapper = mountTable(flight);
    const remarkCells = wrapper.findAll('td.cell-remarks');
    expect(remarkCells.length).toBeGreaterThanOrEqual(4);
    await remarkCells[1].trigger('dblclick');
    await remarkCells[2].trigger('dblclick');
    await remarkCells[3].trigger('dblclick');
    const events = wrapper.emitted('edit-field');
    expect(events).toBeTruthy();
    expect(events!.map((e) => e[1])).toEqual([
      'load_planning_remarks',
      'aircraft_maintenance_remarks',
      'aircraft_check_remarks',
    ]);
  });
});

describe('表头右键菜单（配置列）', () => {
  it('右键表头弹出菜单，点击「配置列...」发出 open-column-config 并关闭菜单', async () => {
    const wrapper = mountTable(makeFlight());
    await wrapper.find('thead').trigger('contextmenu', { clientX: 100, clientY: 100 });

    const menu = document.body.querySelector('#headerContextMenu');
    expect(menu).toBeTruthy();

    const item = document.body.querySelector<HTMLButtonElement>('#ctxConfigColumns');
    expect(item).toBeTruthy();
    item!.click();
    await wrapper.vm.$nextTick();

    expect(wrapper.emitted('open-column-config')).toBeTruthy();
    expect(document.body.querySelector('#headerContextMenu')).toBeNull();
  });

  it('按 Esc 关闭菜单且不发出事件', async () => {
    const wrapper = mountTable(makeFlight());
    await wrapper.find('thead').trigger('contextmenu', { clientX: 100, clientY: 100 });
    expect(document.body.querySelector('#headerContextMenu')).toBeTruthy();

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    await wrapper.vm.$nextTick();
    expect(document.body.querySelector('#headerContextMenu')).toBeNull();
    expect(wrapper.emitted('open-column-config')).toBeFalsy();
  });
});
