import type {
  BusinessFilters,
  FilterFlightsOptions,
  Flight,
  FlightFilterHelperOptions,
  FlightSortConfig,
  SearchFields,
} from './useFlightDataTypes';
import { DEFAULT_BUSINESS_FILTERS, DEFAULT_SEARCH_FIELDS, DEFAULT_SORT_CONFIG } from './useFlightDataConstants';
import {
  getAnomalyCountForFlight,
  getLegFieldV2,
  getMissionSearchTextV2,
  getFlightTypeSummaryV2,
  getRouteEndpointV2,
  hasVipMarker,
  isDelayedFlight,
  isWideBodyAircraft,
  normalizeSignedFlag,
} from './useFlightField';

const zhCollator = new Intl.Collator('zh-CN');

function getLocalStorageSafe(): Storage | null {
  try {
    return typeof window !== 'undefined' ? window.localStorage : null;
  } catch {
    return null;
  }
}

export function normalizeBusinessFilters(
  raw: Partial<BusinessFilters> = {},
  defaultBusinessFilters: Partial<BusinessFilters> = DEFAULT_BUSINESS_FILTERS,
): BusinessFilters {
  const merged: BusinessFilters = {
    ...DEFAULT_BUSINESS_FILTERS,
    ...defaultBusinessFilters,
    ...raw,
  };

  if (!['all', 'wide', 'narrow'].includes(merged.aircraftBodyFilter)) {
    merged.aircraftBodyFilter = (defaultBusinessFilters.aircraftBodyFilter ?? DEFAULT_BUSINESS_FILTERS.aircraftBodyFilter);
  }
  if (!['all', 'yes', 'no'].includes(merged.commercialSignedFilter)) {
    merged.commercialSignedFilter = (defaultBusinessFilters.commercialSignedFilter ?? DEFAULT_BUSINESS_FILTERS.commercialSignedFilter);
  }
  if (!['all', 'only'].includes(merged.anomalyFilter)) {
    merged.anomalyFilter = defaultBusinessFilters.anomalyFilter ?? DEFAULT_BUSINESS_FILTERS.anomalyFilter;
  }
  if (!['all', 'only'].includes(merged.delayFilter)) {
    merged.delayFilter = defaultBusinessFilters.delayFilter ?? DEFAULT_BUSINESS_FILTERS.delayFilter;
  }
  if (!['all', 'only'].includes(merged.vipFilter)) {
    merged.vipFilter = defaultBusinessFilters.vipFilter ?? DEFAULT_BUSINESS_FILTERS.vipFilter;
  }
  if (!['all', 'only'].includes(merged.quickTurnFilter)) {
    merged.quickTurnFilter = defaultBusinessFilters.quickTurnFilter ?? DEFAULT_BUSINESS_FILTERS.quickTurnFilter;
  }

  return merged;
}

export function normalizeSearchFields(raw: Partial<SearchFields> = {}): SearchFields {
  return {
    ...DEFAULT_SEARCH_FIELDS,
    ...raw,
  };
}

export function loadBusinessFiltersFromStorage(
  storageKey: string,
  defaultBusinessFilters: Partial<BusinessFilters> = DEFAULT_BUSINESS_FILTERS,
): BusinessFilters {
  try {
    const saved = getLocalStorageSafe()?.getItem(storageKey);
    if (saved) {
      return normalizeBusinessFilters(JSON.parse(saved) as Partial<BusinessFilters>, defaultBusinessFilters);
    }
  } catch {
    // ignore storage exceptions
  }
  return normalizeBusinessFilters({}, defaultBusinessFilters);
}

export function saveBusinessFiltersToStorage(storageKey: string, businessFilters: Partial<BusinessFilters>): void {
  try {
    getLocalStorageSafe()?.setItem(storageKey, JSON.stringify(businessFilters ?? {}));
  } catch {
    // ignore storage exceptions
  }
}

export function isBusinessFilterDefaultState(
  businessFilters: Partial<BusinessFilters>,
  defaultBusinessFilters: Partial<BusinessFilters> = DEFAULT_BUSINESS_FILTERS,
): boolean {
  const current = normalizeBusinessFilters(businessFilters, defaultBusinessFilters);
  const defaults = normalizeBusinessFilters(defaultBusinessFilters);
  return Object.keys(defaults).every((key) => current[key as keyof BusinessFilters] === defaults[key as keyof BusinessFilters]);
}

export function hasActiveBusinessFilters(
  businessFilters: Partial<BusinessFilters>,
  defaultBusinessFilters: Partial<BusinessFilters> = DEFAULT_BUSINESS_FILTERS,
): boolean {
  return !isBusinessFilterDefaultState(businessFilters, defaultBusinessFilters);
}

export function hasActiveSearchOrBusinessFilters(
  query: string | null | undefined,
  businessFilters: Partial<BusinessFilters>,
  defaultBusinessFilters: Partial<BusinessFilters> = DEFAULT_BUSINESS_FILTERS,
): boolean {
  return Boolean(String(query ?? '').trim()) || hasActiveBusinessFilters(businessFilters, defaultBusinessFilters);
}

export function applyBusinessFilters(
  sourceFlights: Flight[] | null | undefined,
  filters: Partial<BusinessFilters> = {},
  options: FlightFilterHelperOptions = {},
): Flight[] {
  const safeFlights = Array.isArray(sourceFlights) ? sourceFlights : [];
  const defaultBusinessFilters = options.defaultBusinessFilters ?? DEFAULT_BUSINESS_FILTERS;
  const getAnomalyCount = options.getAnomalyCountForFlight ?? getAnomalyCountForFlight;
  const hasVip = options.hasVipMarker ?? hasVipMarker;
  const normalizedFilters = normalizeBusinessFilters(filters, defaultBusinessFilters);

  const allDefault = normalizedFilters.aircraftBodyFilter === 'all'
    && normalizedFilters.commercialSignedFilter === 'all'
    && normalizedFilters.anomalyFilter === 'all'
    && normalizedFilters.delayFilter === 'all'
    && normalizedFilters.vipFilter === 'all'
    && normalizedFilters.quickTurnFilter === 'all';

  if (allDefault) {
    return safeFlights;
  }

  return safeFlights.filter((flight) => {
    if (normalizedFilters.aircraftBodyFilter !== 'all') {
      const isWideBody = isWideBodyAircraft(flight?.aircraft_type_detail);
      if (normalizedFilters.aircraftBodyFilter === 'wide' && !isWideBody) return false;
      if (normalizedFilters.aircraftBodyFilter === 'narrow' && isWideBody) return false;
    }

    if (normalizedFilters.commercialSignedFilter !== 'all') {
      const signed = normalizeSignedFlag(flight?.is_commercial_signed ?? flight?.commercial_signed);
      if (normalizedFilters.commercialSignedFilter === 'yes' && signed !== true) return false;
      if (normalizedFilters.commercialSignedFilter === 'no' && signed !== false) return false;
    }

    if (normalizedFilters.anomalyFilter === 'only' && !(getAnomalyCount(flight) > 0)) return false;
    if (normalizedFilters.delayFilter === 'only' && !isDelayedFlight(flight)) return false;
    if (normalizedFilters.vipFilter === 'only' && !hasVip(flight)) return false;
    if (normalizedFilters.quickTurnFilter === 'only' && !flight?.is_quick_turnaround) return false;
    return true;
  });
}

export function filterFlights(query: string, options: FilterFlightsOptions = {}): Flight[] {
  const sourceFlights = Array.isArray(options.sourceFlights) ? options.sourceFlights : [];
  const searchFields = normalizeSearchFields(options.searchFields);
  const businessFilters = options.businessFilters ?? {};
  const helperOptions = options.helperOptions ?? {};
  const getLegField = options.getLegField ?? getLegFieldV2;
  const getRouteEndpoint = options.getRouteEndpoint ?? getRouteEndpointV2;
  const getMissionSearchText = options.getMissionSearchText ?? getMissionSearchTextV2;
  const getFlightTypeSummary = options.getFlightTypeSummary ?? getFlightTypeSummaryV2;
  const filteredByBusinessRules = applyBusinessFilters(sourceFlights, businessFilters, helperOptions);

  if (!query || !query.trim()) {
    return filteredByBusinessRules;
  }

  const normalizedQuery = query.toLowerCase().trim();
  return filteredByBusinessRules.filter((flight) => {
    const inboundFlightNo = getLegField(flight, 'inbound', 'flight_no').toLowerCase();
    const outboundFlightNo = getLegField(flight, 'outbound', 'flight_no').toLowerCase();
    const destination = getRouteEndpoint(flight, 'outbound', 'code').toLowerCase();
    const destinationName = getRouteEndpoint(flight, 'outbound', 'name').toLowerCase();
    const origin = getRouteEndpoint(flight, 'inbound', 'code').toLowerCase();
    const originName = getRouteEndpoint(flight, 'inbound', 'name').toLowerCase();
    const status = String(flight?.status ?? '').toLowerCase();
    const aircraftType = String(flight?.aircraft_type_detail ?? '').toLowerCase();
    const stand = String(flight?.stand ?? '').toLowerCase();
    const gate = String(flight?.gate ?? '').toLowerCase();
    const mission = getMissionSearchText(flight).toLowerCase();
    const flightType = getFlightTypeSummary(flight).toLowerCase();

    return (searchFields.searchFlightNo && (inboundFlightNo.includes(normalizedQuery) || outboundFlightNo.includes(normalizedQuery)))
      || (searchFields.searchDestination && destination.includes(normalizedQuery))
      || (searchFields.searchDestinationName && destinationName.includes(normalizedQuery))
      || (searchFields.searchOrigin && origin.includes(normalizedQuery))
      || (searchFields.searchOriginName && originName.includes(normalizedQuery))
      || (searchFields.searchStatus && status.includes(normalizedQuery))
      || (searchFields.searchAircraftType && aircraftType.includes(normalizedQuery))
      || (searchFields.searchStand && stand.includes(normalizedQuery))
      || (searchFields.searchGate && gate.includes(normalizedQuery))
      || (searchFields.searchMission && mission.includes(normalizedQuery))
      || (searchFields.searchFlightType && flightType.includes(normalizedQuery));
  });
}

export function toSortableTimestamp(value: unknown): number {
  if (value == null || value === '') return 0;
  if (typeof value === 'number' && Number.isFinite(value)) return value > 1e12 ? value : value * 1000;
  if (value instanceof Date) {
    const timestamp = value.getTime();
    return Number.isFinite(timestamp) ? timestamp : 0;
  }

  const raw = String(value).trim();
  if (!raw) return 0;
  if (/^\d+$/.test(raw)) {
    const numeric = Number(raw);
    if (Number.isFinite(numeric)) return numeric > 1e12 ? numeric : numeric * 1000;
  }

  const hasTimezone = /([zZ]|[+-]\d{2}:?\d{2})$/.test(raw);
  let normalized = raw;
  if (!hasTimezone) {
    normalized = normalized.replace(' ', 'T');
    normalized = `${normalized}Z`;
  }
  const parsed = Date.parse(normalized);
  return Number.isFinite(parsed) ? parsed : 0;
}

export function sortFlights(
  flights: Flight[] | null | undefined,
  sortConfig: Partial<FlightSortConfig> = DEFAULT_SORT_CONFIG,
): Flight[] {
  const { field, direction } = {
    ...DEFAULT_SORT_CONFIG,
    ...sortConfig,
  };
  const multiplier = direction === 'desc' ? -1 : 1;

  function getEffectiveTimestamp(flight: Flight): number {
    const isTimeSortField = field.includes('time') || field.includes('departure') || field.includes('arrival');
    if (!isTimeSortField) {
      return toSortableTimestamp(flight[field]);
    }
    const dep = toSortableTimestamp(flight.scheduled_departure);
    const arr = toSortableTimestamp(flight.scheduled_arrival);
    const primary = toSortableTimestamp(flight[field]);
    if (primary > 0) return primary;
    const fallback = [dep, arr].filter((t) => t > 0);
    return fallback.length > 0 ? Math.min(...fallback) : 0;
  }

  return [...(Array.isArray(flights) ? flights : [])].sort((left, right) => {
    const isTimeSortField = field.includes('time') || field.includes('departure') || field.includes('arrival');

    if (isTimeSortField) {
      const leftTs = getEffectiveTimestamp(left);
      const rightTs = getEffectiveTimestamp(right);
      if (leftTs === 0 && rightTs === 0) return 0;
      if (leftTs === 0) return 1;
      if (rightTs === 0) return -1;
      return (leftTs - rightTs) * multiplier;
    }

    let leftValue = left[field];
    let rightValue = right[field];
    if (Array.isArray(leftValue)) leftValue = leftValue.join(',');
    if (Array.isArray(rightValue)) rightValue = rightValue.join(',');
    if (leftValue == null && rightValue == null) return 0;
    if (leftValue == null) return 1 * multiplier;
    if (rightValue == null) return -1 * multiplier;
    if (typeof leftValue === 'string' && typeof rightValue === 'string') {
      return zhCollator.compare(leftValue, rightValue) * multiplier;
    }
    return (Number(leftValue) - Number(rightValue)) * multiplier;
  });
}
