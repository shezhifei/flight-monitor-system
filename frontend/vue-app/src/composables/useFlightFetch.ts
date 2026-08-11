import type {
  AirportContext,
  AuthFetchOptions,
  FetchFlightsPageDataOptions,
  Flight,
  LoadFlightsPagedDataOptions,
  RetryOptions,
} from './useFlightDataTypes';
import { normalizeAirportContextV2, normalizeFlightId, preprocessFlightTimes } from './useFlightField';

function wait(delayMs: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, delayMs);
  });
}

export async function fetchWithRetry(
  url: string,
  options: RequestInit = {},
  retryOptions: AuthFetchOptions,
): Promise<Response> {
  const { authFetch } = retryOptions;
  const retries = typeof retryOptions.retries === 'number' && Number.isInteger(retryOptions.retries) ? retryOptions.retries : 2;
  const retryDelayMs = typeof retryOptions.retryDelayMs === 'number' && Number.isInteger(retryOptions.retryDelayMs)
    ? retryOptions.retryDelayMs
    : 600;

  let lastError: unknown = null;
  for (let attempt = 0; attempt <= retries; attempt += 1) {
    try {
      return await authFetch(url, options);
    } catch (error) {
      lastError = error;
      if (attempt >= retries) break;
      await wait(retryDelayMs * (attempt + 1));
    }
  }

  throw lastError instanceof Error ? lastError : new Error('Request failed');
}

export async function runWithRetry<T>(task: () => Promise<T>, retryOptions: RetryOptions = {}): Promise<T> {
  const retries = typeof retryOptions.retries === 'number' && Number.isInteger(retryOptions.retries) ? retryOptions.retries : 2;
  const retryDelayMs = typeof retryOptions.retryDelayMs === 'number' && Number.isInteger(retryOptions.retryDelayMs)
    ? retryOptions.retryDelayMs
    : 600;

  let lastError: unknown = null;
  for (let attempt = 0; attempt <= retries; attempt += 1) {
    try {
      return await task();
    } catch (error) {
      lastError = error;
      if (attempt >= retries) break;
      await wait(retryDelayMs * (attempt + 1));
    }
  }

  throw lastError instanceof Error ? lastError : new Error('Task failed');
}

export async function fetchFlightsPageData(page: number, options: FetchFlightsPageDataOptions): Promise<Flight[]> {
  const apiBase = String(options.apiBase ?? '').trim();
  const pageSize = typeof options.pageSize === 'number' && Number.isInteger(options.pageSize) ? options.pageSize : 500;
  const protobufTransport = options.protobufTransport ?? null;
  const url = `${apiBase}/flights?page=${page}&page_size=${pageSize}`;

  return runWithRetry(async () => {
    if (protobufTransport && typeof protobufTransport.requestWithFallback === 'function') {
      const protobufResult = await protobufTransport.requestWithFallback(url, {}, 'flights');
      if (!protobufResult.ok) {
        throw new Error(protobufResult.error || `http_${protobufResult.status ?? 0}`);
      }
      return Array.isArray(protobufResult.data) ? (protobufResult.data as Flight[]) : [];
    }

    const response = await fetchWithRetry(url, {}, {
      authFetch: options.authFetch,
      retries: options.retries,
      retryDelayMs: options.retryDelayMs,
    });

    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }

    const result = (await response.json()) as { data?: unknown };
    return Array.isArray(result.data) ? (result.data as Flight[]) : [];
  }, {
    retries: options.retries,
    retryDelayMs: options.retryDelayMs,
  });
}

export async function loadFlightsPagedData(options: LoadFlightsPagedDataOptions): Promise<Flight[]> {
  const pageSize = typeof options.pageSize === 'number' && Number.isInteger(options.pageSize) ? options.pageSize : 500;
  const merged: Flight[] = [];
  const seen = new Set<string>();
  let page = 1;

  while (true) {
    const pageItems = await fetchFlightsPageData(page, {
      ...options,
      pageSize,
    });

    for (const flight of pageItems) {
      const nextFlight = options.preprocess === false
        ? flight
        : preprocessFlightTimes(flight, { dispatchTimelineCache: options.dispatchTimelineCache });
      const flightId = normalizeFlightId(nextFlight?.flight_id);
      if (!flightId) {
        merged.push(nextFlight);
        continue;
      }
      if (seen.has(flightId)) {
        continue;
      }
      seen.add(flightId);
      merged.push(nextFlight);
    }

    if (pageItems.length < pageSize) {
      break;
    }

    page += 1;
  }

  return merged;
}

export function getSampleFlights(): Flight[] {
  return [
    {
      flight_id: '1',
      flight_number: 'CZ5678',
      inbound_leg: {
        leg_type: 'inbound',
        flight_no: 'CZ1234',
        flight_type: 'domestic',
        mission: 20,
        origin_stations: [{ code: 'PEK', name: '北京' }],
        destination_stations: [],
        is_vip: false,
      },
      outbound_leg: {
        leg_type: 'outbound',
        flight_no: 'CZ5678',
        flight_type: 'domestic',
        mission: 20,
        origin_stations: [],
        destination_stations: [{ code: 'SHA', name: '上海' }],
        is_vip: true,
      },
      status: '正在登机',
      scheduled_departure: '2025-10-29T15:30:00Z',
      scheduled_arrival: '2025-10-29T14:20:00Z',
      estimated_departure: '2025-10-29T15:35:00Z',
      estimated_arrival: '2025-10-29T14:25:00Z',
      actual_arrival: '2025-10-29T14:18:00Z',
      actual_departure: null,
      start_boarding_time: '2025-10-29T14:45:00Z',
      end_boarding_time: null,
      cobt_time: '2025-10-29T14:50:00Z',
      aircraft_type_detail: 'A320',
      stand: 'A12',
      gate: 'B15',
      has_boarding_restriction: false,
      is_quick_turnaround: true,
      business_cases: [],
    },
    {
      flight_id: '2',
      flight_number: 'MU9876',
      inbound_leg: {
        leg_type: 'inbound',
        flight_no: 'MU9876',
        flight_type: 'domestic',
        mission: 20,
        origin_stations: [{ code: 'CTU', name: '成都' }],
        destination_stations: [],
        is_vip: false,
      },
      outbound_leg: null,
      status: '前方起飞',
      scheduled_departure: '2025-10-29T13:20:00Z',
      actual_departure: '2025-10-29T13:22:00Z',
      start_boarding_time: '2025-10-29T12:50:00Z',
      end_boarding_time: '2025-10-29T13:15:00Z',
      aircraft_type_detail: 'B737',
      stand: 'C08',
      gate: 'A20',
      has_boarding_restriction: true,
      is_quick_turnaround: false,
      business_cases: [],
    },
    {
      flight_id: '3',
      flight_number: 'CA2468',
      inbound_leg: null,
      outbound_leg: {
        leg_type: 'outbound',
        flight_no: 'CA2468',
        flight_type: 'domestic',
        mission: 20,
        origin_stations: [],
        destination_stations: [{ code: 'PEK', name: '北京' }],
        is_vip: true,
      },
      status: '计划中',
      scheduled_departure: '2025-10-29T18:00:00Z',
      aircraft_type_detail: 'A330',
      stand: 'B05',
      gate: 'C12',
      has_boarding_restriction: false,
      is_quick_turnaround: true,
      business_cases: [],
    },
  ];
}

export async function loadAirportContextV2(options: {
  apiBase: string;
  authFetch: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;
  fallbackContext?: Partial<AirportContext>;
}): Promise<AirportContext> {
  try {
    const response = await options.authFetch(`${options.apiBase}/system/airport-context`);
    const payload = (await response.json().catch(() => ({}))) as Partial<AirportContext> & { detail?: string; message?: string };
    if (!response.ok) {
      throw new Error(payload.detail || payload.message || `HTTP ${response.status}`);
    }
    return normalizeAirportContextV2(payload);
  } catch {
    return normalizeAirportContextV2(options.fallbackContext);
  }
}
