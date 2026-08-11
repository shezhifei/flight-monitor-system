import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ref } from 'vue';
import type { UseFlightDataReturn } from '../useFlightData';
import type { Flight } from '../useFlightDataTypes';

vi.mock('../useAuth', () => ({
  useAuth: () => ({
    requireAuthAsync: vi.fn().mockResolvedValue(true),
    refreshSSEToken: vi.fn().mockResolvedValue(undefined),
    isAuthenticated: ref(true),
    token: ref('test-token'),
    user: ref(null),
    login: vi.fn(),
    logout: vi.fn(),
    checkToken: vi.fn().mockResolvedValue(true),
  }),
}));

vi.mock('../useProtobuf', () => ({
  useProtobuf: () => ({
    isReady: ref(true),
    load: vi.fn().mockResolvedValue(undefined),
    decode: vi.fn(),
  }),
}));

vi.mock('../useToast', () => ({
  useToast: () => ({
    showToast: vi.fn(),
  }),
}));

vi.mock('../useNotification', () => ({
  useNotification: () => ({
    sentReceiptReminderQueue: ref<string[]>([]),
    updateUnreadCount: vi.fn(),
  }),
}));

const sseHandlers = new Map<string, (event: Event) => void>();

vi.mock('../useSSE', () => ({
  useSSE: () => ({
    status: ref('idle' as const),
    connect: vi.fn().mockResolvedValue(undefined),
    disconnect: vi.fn(),
    on: vi.fn((eventName: string, handler: (event: Event) => void) => {
      sseHandlers.set(eventName, handler);
    }),
  }),
}));

const { useFlightStream, coerceRecord } = await import('../useFlightStream');

function createMockFlightData(initial: Flight[] = []): {
  flightData: UseFlightDataReturn;
  flights: ReturnType<typeof ref<Flight[]>>;
} {
  const flights = ref<Flight[]>([...initial]);
  const flightData = {
    originalFlights: flights,
    setFlights: vi.fn((next: Flight[]) => {
      flights.value = next;
      return next;
    }),
    loadAirportContext: vi.fn().mockResolvedValue(undefined),
    loadFlightsPaged: vi.fn().mockResolvedValue([]),
  } as unknown as UseFlightDataReturn;
  return { flightData, flights };
}

describe('flight stream performance pipeline', () => {
  beforeEach(() => {
    sseHandlers.clear();
    vi.useFakeTimers({ toFake: ['requestAnimationFrame'] });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('flushFlightUpdates merges same flight_id updates and emits one flash event per flight', async () => {
    const existing: Flight = {
      flight_id: 'F1',
      status: 'scheduled',
      flight_number: 'CA100',
    } as Flight;
    const { flightData, flights } = createMockFlightData([existing]);
    const stream = useFlightStream({ flightData, announce: vi.fn() });

    await stream.handleFlightPayload({
      type: 'update',
      flight_id: 'F1',
      patch: { flight_id: 'F1', status: 'boarding' },
      changed_fields: ['status'],
    });
    await stream.handleFlightPayload({
      type: 'update',
      flight_id: 'F1',
      patch: { flight_id: 'F1', status: 'departed', stand: 'A1' },
      changed_fields: ['status', 'stand'],
    });

    await vi.runAllTimersAsync();

    const list = flights.value as Flight[];
    expect(list).toHaveLength(1);
    expect(list[0].status).toBe('departed');
    expect(list[0].stand).toBe('A1');
    const flashEvents = stream.flightFlashEvents.value;
    expect(flashEvents).toHaveLength(1);
    expect(flashEvents[0].flightId).toBe('F1');
  });

  it('flushFlightUpdates appends new flights without touching other flight object refs', async () => {
    const f1: Flight = { flight_id: 'F1', status: 'scheduled', flight_number: 'CA100' } as Flight;
    const f2: Flight = { flight_id: 'F2', status: 'boarding', flight_number: 'CA200' } as Flight;
    const { flightData, flights } = createMockFlightData([f1, f2]);
    const stream = useFlightStream({ flightData, announce: vi.fn() });
    const listBefore = flights.value as Flight[];
    const f1Ref = listBefore[0];
    const f2Ref = listBefore[1];

    await stream.handleFlightPayload({
      type: 'update',
      flight_id: 'F3',
      patch: { flight_id: 'F3', status: 'scheduled', flight_number: 'CA300' },
      changed_fields: ['status'],
    });

    await vi.runAllTimersAsync();

    const list = flights.value as Flight[];
    expect(list).toHaveLength(3);
    expect(list[0]).toBe(f1Ref);
    expect(list[1]).toBe(f2Ref);
    expect(list[2].flight_id).toBe('F3');
    expect(stream.flightFlashEvents.value.some((e) => e.flightId === 'F3')).toBe(true);
  });

  it('syncAnomalySummaries(affected set) only rewrites the target flight object', async () => {
    const f1: Flight = {
      flight_id: 'F1',
      status: 'scheduled',
      anomaly_summary: { has_open_anomaly: false, open_count: 0, acknowledged_count: 0 },
    } as Flight;
    const f2: Flight = {
      flight_id: 'F2',
      status: 'boarding',
      anomaly_summary: { has_open_anomaly: false, open_count: 0, acknowledged_count: 0 },
    } as Flight;
    const { flightData, flights } = createMockFlightData([f1, f2]);
    useFlightStream({ flightData, announce: vi.fn() });
    const f2Ref = (flights.value as Flight[])[1];

    const handler = sseHandlers.get('anomaly_alerts');
    expect(handler).toBeTruthy();

    handler!({
      data: JSON.stringify({
        type: 'update',
        data: {
          anomaly_id: 'A1',
          flight_id: 'F1',
          status: 'open',
        },
      }),
    } as MessageEvent);

    const list = flights.value as Flight[];
    expect(list[1]).toBe(f2Ref);
    expect(list[0]).not.toBe(f1);
    expect(Number(list[0].anomaly_summary?.open_count ?? 0)).toBe(1);
  });

  it('coerceRecord parses strings, passes objects through, and returns null for invalid input', () => {
    expect(coerceRecord('{"a":1}')).toEqual({ a: 1 });
    const obj = { b: 2 };
    expect(coerceRecord(obj)).toBe(obj);
    expect(coerceRecord('not-json')).toBeNull();
    expect(coerceRecord(null)).toBeNull();
    expect(coerceRecord(42)).toBeNull();
  });
});
