import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ref } from 'vue';
import type { UseFlightDataReturn } from './useFlightData';
import type { Flight } from './useFlightDataTypes';

// Task 12a (F1): Flight monitor must not use fabricated prototype data.
// When the initial snapshot load fails, the composable must NOT set
// usingFallbackData=true — there is no fallback data to show.
// It must set initializationError and leave flights empty.

// Mock dependencies that require browser APIs
vi.mock('./useAuth', () => ({
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

vi.mock('./useProtobuf', () => ({
  useProtobuf: () => ({
    isReady: ref(true),
    load: vi.fn().mockResolvedValue(undefined),
    decode: vi.fn(),
  }),
}));

vi.mock('./useToast', () => ({
  useToast: () => ({
    showToast: vi.fn(),
  }),
}));

vi.mock('./useNotification', () => ({
  useNotification: () => ({
    sentReceiptReminderQueue: ref<string[]>([]),
    updateUnreadCount: vi.fn(),
  }),
}));

vi.mock('./useSSE', () => ({
  useSSE: () => ({
    status: ref('idle' as const),
    connect: vi.fn().mockResolvedValue(undefined),
    disconnect: vi.fn(),
    on: vi.fn(),
  }),
}));

// Import after mocks are set up
const { useFlightStream } = await import('./useFlightStream');

function createMockFlightData(overrides?: Partial<UseFlightDataReturn>): UseFlightDataReturn {
  const flights = ref<Flight[]>([]);
  return {
    originalFlights: flights,
    setFlights: vi.fn((next: Flight[]) => {
      flights.value = next;
      return next;
    }),
    loadAirportContext: vi.fn().mockResolvedValue(undefined),
    loadFlightsPaged: vi.fn().mockResolvedValue([]),
    ...overrides,
  } as unknown as UseFlightDataReturn;
}

describe('useFlightStream — no fake data (Task 12a / F1)', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('does not set usingFallbackData when initial load fails', async () => {
    const loadError = new Error('DB connection refused');
    const flightData = createMockFlightData({
      loadFlightsPaged: vi.fn().mockRejectedValue(loadError),
    });

    const stream = useFlightStream({
      flightData,
      announce: vi.fn(),
    });

    // initialize() uses Promise.allSettled — it does not throw
    await stream.initialize();

    expect(stream.usingFallbackData.value).toBe(false);
    expect(stream.initializationError.value).toBeTruthy();
    expect(stream.initializationError.value).toContain('DB connection refused');
  });

  it('does not set usingFallbackData when refreshFlights fails on first load', async () => {
    const loadError = new Error('Network timeout');
    const flightData = createMockFlightData({
      loadFlightsPaged: vi.fn().mockRejectedValue(loadError),
    });

    const stream = useFlightStream({
      flightData,
      announce: vi.fn(),
    });

    await expect(stream.refreshFlights(false)).rejects.toThrow();

    expect(stream.usingFallbackData.value).toBe(false);
    expect(stream.initializationError.value).toContain('Network timeout');
  });

  it('starts with empty flights (no prototype data)', async () => {
    const flightData = createMockFlightData();

    useFlightStream({
      flightData,
      announce: vi.fn(),
    });

    // Before initialize, flights should be empty — not prototype/sample data
    expect(flightData.originalFlights.value).toEqual([]);
  });

  it('clears initializationError on successful load after failure', async () => {
    const flightData = createMockFlightData({
      loadFlightsPaged: vi.fn()
        .mockRejectedValueOnce(new Error('first attempt fails'))
        .mockResolvedValueOnce([{ flight_id: '1', flight_number: 'CA123' }]),
    });

    const stream = useFlightStream({
      flightData,
      announce: vi.fn(),
    });

    // First attempt fails
    await expect(stream.refreshFlights(false)).rejects.toThrow();
    expect(stream.initializationError.value).toBeTruthy();

    // Second attempt succeeds
    await stream.refreshFlights(false);

    expect(stream.usingFallbackData.value).toBe(false);
    expect(stream.initializationError.value).toBeNull();
    expect(stream.hasLoadedSnapshot.value).toBe(true);
  });
});
