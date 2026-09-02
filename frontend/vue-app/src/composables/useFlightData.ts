import { computed, readonly, ref } from 'vue';
import type { ComputedRef, Ref } from 'vue';
import { useAuth } from './useAuth';
import type {
  AirportContext,
  BusinessCaseCreatePayload,
  BusinessFilters,
  DispatchTimelineCache,
  DispatchTimelineEvent,
  Flight,
  FlightSortConfig,
  LoadFlightsPagedDataOptions,
  SearchFields,
  UseFlightDataOptions,
} from './useFlightDataTypes';
import type { BusinessCaseAiExtractionConfig, BusinessCaseTypeDefinition } from '../types/backend';
import {
  DEFAULT_BUSINESS_FILTERS,
  DEFAULT_SORT_CONFIG,
} from './useFlightDataConstants';
import {
  findFlightById,
  normalizeAirportContext,
  normalizeFlightId,
  preprocessFlightsBatch,
  resolveDirectionalFlightId,
} from './useFlightField';
import {
  filterFlights,
  normalizeBusinessFilters,
  normalizeSearchFields,
  sortFlights,
} from './useFlightFilter';
import {
  createDispatchTimelineCache,
  loadDispatchTimelineForFlight,
  writeDispatchTimelineField,
} from './useFlightSync';
import {
  loadAirportContext as loadAirportContextRequest,
  loadFlightsPagedData,
} from './useFlightFetch';
import {
  appendBusinessCase,
  acknowledgeBusinessCaseAppend,
  createBusinessCase,
  fetchCollaborationGroupByFlight,
  fetchFlightEventJourney,
  fetchFlightHistoryReport,
  loadBusinessCaseTypes as loadBusinessCaseTypesRequest,
  patchFlightField,
  updateBusinessCaseStatusRequest,
  updateBusinessCaseTypeAiConfigRequest,
} from './useFlightCrud';

// 组合入口：本文件装配 useFlightData() 组合函数，拆分的纯逻辑子模块从这里统一对外。
export * from './useFlightDataTypes';
export * from './useFlightDataConstants';
export * from './useFlightField';
export * from './useFlightFilter';
export * from './useFlightSync';
export * from './useFlightCrud';
export * from './useFlightFetch';

export interface UseFlightDataReturn {
  flights: Readonly<Ref<readonly Flight[]>>;
  originalFlights: Readonly<Ref<readonly Flight[]>>;
  airportContext: Readonly<Ref<AirportContext>>;
  businessFilters: Readonly<Ref<BusinessFilters>>;
  searchFields: Readonly<Ref<SearchFields>>;
  searchQuery: Readonly<Ref<string>>;
  sortConfig: Readonly<Ref<FlightSortConfig>>;
  dispatchTimelineCache: Readonly<Ref<ReadonlyMap<string, import('./useFlightDataTypes').DispatchTimelineCacheEntry>>>;
  filteredFlights: ComputedRef<Flight[]>;
  sortedFlights: ComputedRef<Flight[]>;
  setFlights: (nextFlights: Flight[], syncOriginalFlights?: boolean) => Flight[];
  setOriginalFlights: (nextFlights: Flight[]) => Flight[];
  setAirportContext: (nextContext: Partial<AirportContext>) => AirportContext;
  setBusinessFilters: (nextFilters: Partial<BusinessFilters>) => BusinessFilters;
  setSearchFields: (nextFields: Partial<SearchFields>) => SearchFields;
  setSearchQuery: (nextQuery: string) => string;
  setSortConfig: (nextSortConfig: Partial<FlightSortConfig>) => FlightSortConfig;
  resetBusinessFilters: () => BusinessFilters;
  findFlightById: (flightId: string | number | null | undefined) => Flight | null;
  loadAirportContext: () => Promise<AirportContext>;
  loadFlightsPaged: (options?: Partial<LoadFlightsPagedDataOptions>) => Promise<Flight[]>;
  loadDispatchTimelineForFlight: (
    flightId: string | number | null | undefined,
    force?: boolean,
  ) => Promise<DispatchTimelineEvent[]>;
  writeDispatchTimelineField: (
    flightId: string | number | null | undefined,
    field: string,
    value: string | null | undefined,
  ) => Promise<DispatchTimelineEvent[]>;
  loadBusinessCaseTypes: () => Promise<BusinessCaseTypeDefinition[]>;
  submitBusinessCase: (caseData: BusinessCaseCreatePayload) => Promise<Record<string, unknown>>;
  updateBusinessCaseStatus: (caseId: string, status: string) => Promise<Record<string, unknown>>;
  updateBusinessCaseTypeAiConfig: (code: string, config: BusinessCaseAiExtractionConfig) => Promise<Record<string, unknown>>;
  fetchEventJourney: (flightId: string | number, hours?: number) => Promise<Record<string, unknown>>;
  fetchHistoryReport: (flightId: string | number, hours?: number) => Promise<Record<string, unknown>>;
  appendBusinessCase: (caseId: string, appendData: { content: string; type?: string; mention_user_ids?: string[]; [key: string]: unknown }) => Promise<Record<string, unknown>>;
  acknowledgeBusinessCaseAppend: (caseId: string, appendId: string) => Promise<Record<string, unknown>>;
  fetchCollaborationGroupByFlight: (flightId: string | number) => Promise<Record<string, unknown>>;
  updateFlightField: (flightId: string | number, field: string, value: unknown) => Promise<Record<string, unknown> | undefined>;
}

export function useFlightData(initialOptions: UseFlightDataOptions = {}): UseFlightDataReturn {
  const auth = useAuth();

  const flights = ref<Flight[]>(preprocessFlightsBatch(initialOptions.flights ?? []));
  const originalFlights = ref<Flight[]>(preprocessFlightsBatch(initialOptions.originalFlights ?? initialOptions.flights ?? []));
  const airportContext = ref<AirportContext>(normalizeAirportContext(initialOptions.airportContext));
  const businessFilters = ref<BusinessFilters>(normalizeBusinessFilters(initialOptions.businessFilters));
  const searchFields = ref<SearchFields>(normalizeSearchFields(initialOptions.searchFields));
  const searchQuery = ref(String(initialOptions.searchQuery ?? ''));
  const sortConfig = ref<FlightSortConfig>({
    ...DEFAULT_SORT_CONFIG,
    ...initialOptions.sortConfig,
  });
  const dispatchTimelineCache = ref<DispatchTimelineCache>(createDispatchTimelineCache());

  function setFlights(nextFlights: Flight[], syncOriginalFlights = false): Flight[] {
    const normalized = preprocessFlightsBatch(nextFlights, { dispatchTimelineCache: dispatchTimelineCache.value });
    flights.value = normalized;
    if (syncOriginalFlights) {
      originalFlights.value = [...normalized];
    }
    return normalized;
  }

  function setOriginalFlights(nextFlights: Flight[]): Flight[] {
    const normalized = preprocessFlightsBatch(nextFlights, { dispatchTimelineCache: dispatchTimelineCache.value });
    originalFlights.value = normalized;
    return normalized;
  }

  function setAirportContext(nextContext: Partial<AirportContext>): AirportContext {
    airportContext.value = normalizeAirportContext(nextContext);
    return airportContext.value;
  }

  function setBusinessFilters(nextFilters: Partial<BusinessFilters>): BusinessFilters {
    businessFilters.value = normalizeBusinessFilters(nextFilters);
    return businessFilters.value;
  }

  function setSearchFields(nextFields: Partial<SearchFields>): SearchFields {
    searchFields.value = normalizeSearchFields(nextFields);
    return searchFields.value;
  }

  function setSearchQuery(nextQuery: string): string {
    searchQuery.value = String(nextQuery ?? '');
    return searchQuery.value;
  }

  function setSortConfig(nextSortConfig: Partial<FlightSortConfig>): FlightSortConfig {
    sortConfig.value = {
      ...DEFAULT_SORT_CONFIG,
      ...sortConfig.value,
      ...nextSortConfig,
    };
    return sortConfig.value;
  }

  function resetBusinessFilters(): BusinessFilters {
    businessFilters.value = normalizeBusinessFilters(DEFAULT_BUSINESS_FILTERS);
    return businessFilters.value;
  }

  function findFlight(flightId: string | number | null | undefined): Flight | null {
    return findFlightById(flightId, flights.value, originalFlights.value);
  }

  async function loadAirportContext(): Promise<AirportContext> {
    const nextContext = await loadAirportContextRequest({
      apiBase: auth.apiBase.value,
      authFetch: auth.fetch,
      fallbackContext: airportContext.value,
    });
    airportContext.value = nextContext;
    return nextContext;
  }

  async function loadFlightsPaged(options: Partial<LoadFlightsPagedDataOptions> = {}): Promise<Flight[]> {
    const nextFlights = await loadFlightsPagedData({
      apiBase: auth.apiBase.value,
      authFetch: auth.fetch,
      dispatchTimelineCache: dispatchTimelineCache.value,
      ...options,
    });
    setFlights(nextFlights, true);
    return flights.value;
  }

  async function loadTimelineForFlight(
    flightId: string | number | null | undefined,
    force = false,
  ): Promise<DispatchTimelineEvent[]> {
    const result = await loadDispatchTimelineForFlight(flightId, {
      apiBase: auth.apiBase.value,
      authFetch: auth.fetch,
      cache: dispatchTimelineCache.value,
      flights: flights.value,
      originalFlights: originalFlights.value,
      force,
    });
    dispatchTimelineCache.value = result.cache;
    return result.items;
  }

  async function writeTimelineField(
    flightId: string | number | null | undefined,
    field: string,
    value: string | null | undefined,
  ): Promise<DispatchTimelineEvent[]> {
    const result = await writeDispatchTimelineField(flightId, field, {
      apiBase: auth.apiBase.value,
      authFetch: auth.fetch,
      cache: dispatchTimelineCache.value,
      flights: flights.value,
      originalFlights: originalFlights.value,
      value,
    });
    dispatchTimelineCache.value = result.cache;
    return result.items;
  }

  async function loadBusinessCaseTypes() {
    return loadBusinessCaseTypesRequest({
      apiBase: auth.apiBase.value,
      authFetch: auth.fetch,
    });
  }

  async function updateBusinessCaseTypeAiConfig(code: string, config: BusinessCaseAiExtractionConfig) {
    return updateBusinessCaseTypeAiConfigRequest(code, config, {
      apiBase: auth.apiBase.value,
      authFetch: auth.fetch,
    });
  }

  async function submitBusinessCase(caseData: BusinessCaseCreatePayload) {
    return createBusinessCase(caseData, {
      apiBase: auth.apiBase.value,
      authFetch: auth.fetch,
    });
  }

  async function updateBusinessCaseStatus(caseId: string, status: string) {
    return updateBusinessCaseStatusRequest(caseId, status, {
      apiBase: auth.apiBase.value,
      authFetch: auth.fetch,
    });
  }

  async function updateFlightField(flightId: string | number, field: string, value: unknown) {
    const normalizedId = normalizeFlightId(flightId);

    const flight = findFlight(normalizedId);
    // 拆表后过站行 = 链 id + 两班方向航班：单元格 PATCH 必须打在字段所属方向的
    // 航班上，不能用监控行 row_id（后端拒绝旧聚合 id 的写）。
    const patchTargetId = resolveDirectionalFlightId(flight, field) ?? normalizedId;
    let originalValue: unknown;
    let shouldRollback = false;
    let expectedVersion: number | null = null;
    if (flight) {
      if (flight && typeof flight === 'object' && (flight as Record<string, unknown>)[field] !== undefined) {
        // handled below
      }
      const flightRecord = flight as unknown as Record<string, unknown>;
      originalValue = flightRecord[field];
      shouldRollback = true;
      flightRecord[field] = value;
      const rawVersion = flightRecord.version;
      if (typeof rawVersion === 'number' && Number.isFinite(rawVersion)) {
        expectedVersion = rawVersion;
      } else if (typeof rawVersion === 'string' && rawVersion.trim() !== '' && Number.isFinite(Number(rawVersion))) {
        expectedVersion = Number(rawVersion);
      }
    }

    try {
      return await patchFlightField(patchTargetId, field, value, {
        apiBase: auth.apiBase.value,
        authFetch: auth.fetch,
        expectedVersion,
      });
    } catch (error) {
      if (flight && shouldRollback) {
        const flightRecord = flight as unknown as Record<string, unknown>;
        flightRecord[field] = originalValue;
      }
      throw error;
    }
  }

  async function fetchEventJourney(flightId: string | number, hours?: number) {
    return fetchFlightEventJourney(flightId, {
      apiBase: auth.apiBase.value,
      authFetch: auth.fetch,
      hours,
    });
  }

  async function fetchHistoryReport(flightId: string | number, hours?: number) {
    return fetchFlightHistoryReport(flightId, {
      apiBase: auth.apiBase.value,
      authFetch: auth.fetch,
      hours,
    });
  }

  async function appendBusinessCaseData(caseId: string, appendData: { content: string; type?: string; mention_user_ids?: string[]; [key: string]: unknown }) {
    return appendBusinessCase(caseId, appendData, {
      apiBase: auth.apiBase.value,
      authFetch: auth.fetch,
    });
  }

  async function acknowledgeBusinessCaseAppendData(caseId: string, appendId: string) {
    return acknowledgeBusinessCaseAppend(caseId, appendId, {
      apiBase: auth.apiBase.value,
      authFetch: auth.fetch,
    });
  }

  async function fetchCollaborationGroupByFlightId(flightId: string | number) {
    return fetchCollaborationGroupByFlight(flightId, {
      apiBase: auth.apiBase.value,
      authFetch: auth.fetch,
    });
  }

  const filteredFlights = computed(() => filterFlights(searchQuery.value, {
    sourceFlights: flights.value,
    searchFields: searchFields.value,
    businessFilters: businessFilters.value,
  }));

  const sortedFlights = computed(() => sortFlights(filteredFlights.value, sortConfig.value));

  return {
    flights: readonly(flights) as UseFlightDataReturn['flights'],
    originalFlights: readonly(originalFlights) as UseFlightDataReturn['originalFlights'],
    airportContext: readonly(airportContext),
    businessFilters: readonly(businessFilters),
    searchFields: readonly(searchFields),
    searchQuery: readonly(searchQuery),
    sortConfig: readonly(sortConfig),
    dispatchTimelineCache: readonly(dispatchTimelineCache) as UseFlightDataReturn['dispatchTimelineCache'],
    filteredFlights,
    sortedFlights,
    setFlights,
    setOriginalFlights,
    setAirportContext,
    setBusinessFilters,
    setSearchFields,
    setSearchQuery,
    setSortConfig,
    resetBusinessFilters,
    findFlightById: findFlight,
    loadAirportContext,
    loadFlightsPaged,
    loadDispatchTimelineForFlight: loadTimelineForFlight,
    writeDispatchTimelineField: writeTimelineField,
    loadBusinessCaseTypes,
    updateBusinessCaseTypeAiConfig,
    submitBusinessCase,
    updateBusinessCaseStatus,
    updateFlightField,
    fetchEventJourney,
    fetchHistoryReport,
    appendBusinessCase: appendBusinessCaseData,
    acknowledgeBusinessCaseAppend: acknowledgeBusinessCaseAppendData,
    fetchCollaborationGroupByFlight: fetchCollaborationGroupByFlightId,
  };
}
