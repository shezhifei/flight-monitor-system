import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ref } from 'vue';
import { useFlightBatchEdit } from './useFlightBatchEdit';
import { MAX_BATCH_CELL_EDIT, useFlightCellSelection } from './useFlightCellSelection';
import type { Flight } from '../../../composables/useFlightData';

const mockPatchBatchCells = vi.fn();
const mockShowToast = vi.fn();
const mockAnnounce = vi.fn();

vi.mock('../../../composables/useFlightCrud', () => ({
  patchFlightBatchCells: (...args: unknown[]) => mockPatchBatchCells(...args),
}));

vi.mock('../../../composables/useToast', () => ({
  useToast: () => ({ showToast: mockShowToast }),
}));

vi.mock('../../../composables/useAuth', async (importActual) => {
  const actual = await importActual<typeof import('../../../composables/useAuth')>();
  return {
    ...actual,
    useAuth: () => ({
      getUser: () => ({ user_id: 'u1', permissions: ['*'], is_admin: true }),
      apiBase: ref('/api/v2'),
      fetch: vi.fn(),
    }),
  };
});

/** 拆表后的过站行：选中键 row_id='OLD'，两班方向航班是 IN-NEW / OUT-NEW。 */
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
  } as Flight;
}

function makeBatchEdit(flight: Flight) {
  const flights = ref<Flight[]>([flight]);
  const flightData = {
    flights,
    originalFlights: flights,
    findFlightById: (id: string | number | null | undefined) =>
      [flight].find((item) =>
        [item.row_id, item.flight_id, item.inbound_flight_id, item.outbound_flight_id]
          .map((key) => String(key ?? ''))
          .includes(String(id ?? ''))) ?? null,
  };
  const cellSelection = useFlightCellSelection();
  const refreshFlights = vi.fn(async () => {});
  return {
    batchEdit: useFlightBatchEdit({
      flightData: flightData as never,
      cellSelection,
      announce: mockAnnounce,
      refreshFlights,
    }),
    cellSelection,
    refreshFlights,
  };
}

beforeEach(() => {
  mockPatchBatchCells.mockReset().mockResolvedValue({ updated_count: 1, results: [] });
  mockShowToast.mockReset();
  mockAnnounce.mockReset();
});

describe('useFlightBatchEdit 拆表后的方向目标', () => {
  it('出港字段的 batch-cells 目标是 outbound_flight_id，不是 row_id', async () => {
    const { batchEdit } = makeBatchEdit(turnaroundRow());
    batchEdit.openBatchEditForField('scheduled_departure', ['OLD']);
    expect(batchEdit.modalState.value.isOpen).toBe(true);
    batchEdit.setBatchValue('2026-08-28T08:00');
    await batchEdit.submitBatchEdit();

    expect(mockPatchBatchCells).toHaveBeenCalledTimes(1);
    const request = mockPatchBatchCells.mock.calls[0][0] as {
      field: string;
      targets: Array<{ flight_id: string }>;
    };
    expect(request.field).toBe('scheduled_departure');
    expect(request.targets).toHaveLength(1);
    expect(request.targets[0].flight_id).toBe('OUT-NEW');
    expect(request.targets[0].flight_id).not.toBe('OLD');
  });

  it('进港字段的 batch-cells 目标是 inbound_flight_id', async () => {
    const { batchEdit } = makeBatchEdit(turnaroundRow());
    batchEdit.openBatchEditForField('scheduled_arrival', ['OLD']);
    batchEdit.setBatchValue('2026-08-28T07:00');
    await batchEdit.submitBatchEdit();

    const request = mockPatchBatchCells.mock.calls[0][0] as {
      targets: Array<{ flight_id: string }>;
    };
    expect(request.targets[0].flight_id).toBe('IN-NEW');
    expect(request.targets[0].flight_id).not.toBe('OLD');
  });

  it('单边出港行的进港字段解析不出目标 → 拒绝提交，不打 row_id', async () => {
    const flight = {
      row_id: 'OUT-1',
      kind: 'single',
      flight_id: 'OUT-1',
      outbound_flight_id: 'OUT-1',
      version: 1,
    } as Flight;
    const { batchEdit } = makeBatchEdit(flight);
    batchEdit.openBatchEditForField('scheduled_arrival', ['OUT-1']);
    batchEdit.setBatchValue('2026-08-28T07:00');
    await batchEdit.submitBatchEdit();

    expect(mockPatchBatchCells).not.toHaveBeenCalled();
    expect(mockShowToast).toHaveBeenCalledWith('warning', expect.stringContaining('无法解析'), expect.anything());
  });

  it('多选仍受上限约束', () => {
    expect(MAX_BATCH_CELL_EDIT).toBeGreaterThan(0);
  });
});
