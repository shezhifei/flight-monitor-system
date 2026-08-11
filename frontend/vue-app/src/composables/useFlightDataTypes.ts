import type {
  BusinessCaseSummary,
  BusinessCaseTypeDefinition,
  BusinessCaseVisibilityScope,
} from '../types/backend';

export type LegType = 'inbound' | 'outbound';
export type FlightSortDirection = 'asc' | 'desc';
export type AircraftBodyFilter = 'all' | 'wide' | 'narrow';
export type YesNoAllFilter = 'all' | 'yes' | 'no';
export type OnlyAllFilter = 'all' | 'only';

export interface Station {
  code: string;
  name: string | null;
  [key: string]: unknown;
}

export interface FlightLeg {
  leg_type: LegType;
  flight_no: string;
  flight_type: string;
  origin_stations: Station[];
  destination_stations: Station[];
  mission?: string | number | null;
  is_vip?: boolean;
  labels?: string[];
  [key: string]: unknown;
}

export interface AnomalySummary {
  has_open_anomaly: boolean;
  open_count: number;
  acknowledged_count: number;
}

export interface DispatchTimelineEvent {
  timeline_id?: string | number | null;
  milestone_code?: string | null;
  occurred_at?: string | null;
  leg_type?: LegType | null;
  source?: string | null;
  payload?: Record<string, unknown> | null;
  [key: string]: unknown;
}

export interface DispatchTimelineCacheEntry {
  byMilestone: Map<string, DispatchTimelineEvent>;
  rawItems: DispatchTimelineEvent[];
}

export type DispatchTimelineCache = Map<string, DispatchTimelineCacheEntry>;

export interface Flight {
  flight_id?: string | number | null;
  flight_number?: string | null;
  inbound_leg?: Partial<FlightLeg> | null;
  outbound_leg?: Partial<FlightLeg> | null;
  anomaly_summary?: Partial<AnomalySummary> | null;
  status?: string | null;
  stand?: string | null;
  gate?: string | null;
  aircraft_type_detail?: string | null;
  is_commercial_signed?: boolean | string | number | null;
  commercial_signed?: boolean | string | number | null;
  is_quick_turnaround?: boolean | null;
  labels?: string[];
  business_cases?: BusinessCaseSummary[] | null;
  scheduled_departure?: string | number | Date | null;
  scheduled_arrival?: string | number | Date | null;
  estimated_departure?: string | number | Date | null;
  estimated_arrival?: string | number | Date | null;
  actual_departure?: string | number | Date | null;
  actual_arrival?: string | number | Date | null;
  cobt_time?: string | number | Date | null;
  codt?: string | number | Date | null;
  start_boarding_time?: string | number | Date | null;
  end_boarding_time?: string | number | Date | null;
  boarding_allowed_time?: string | number | Date | null;
  passenger_ready_time?: string | number | Date | null;
  off_blocks_time?: string | number | Date | null;
  cabin_door_open_time?: string | number | Date | null;
  cleaning_start_time?: string | number | Date | null;
  cleaning_end_time?: string | number | Date | null;
  on_blocks_time?: string | number | Date | null;
  deboarding_complete_time?: string | number | Date | null;
  cabin_door_close_time?: string | number | Date | null;
  cargo_door_close_time?: string | number | Date | null;
  loading_complete_time?: string | number | Date | null;
  _fmt?: Record<string, string | null>;
  _timesFormatted?: boolean;
  [key: string]: unknown;
}

export interface AirportContext {
  code: string;
  display_name: string;
  name_aliases: readonly string[];
}

export interface BusinessFilters {
  aircraftBodyFilter: AircraftBodyFilter;
  commercialSignedFilter: YesNoAllFilter;
  anomalyFilter: OnlyAllFilter;
  delayFilter: OnlyAllFilter;
  vipFilter: OnlyAllFilter;
  quickTurnFilter: OnlyAllFilter;
}

export interface SearchFields {
  searchFlightNo: boolean;
  searchDestination: boolean;
  searchDestinationName: boolean;
  searchOrigin: boolean;
  searchOriginName: boolean;
  searchStatus: boolean;
  searchAircraftType: boolean;
  searchStand: boolean;
  searchGate: boolean;
  searchMission: boolean;
  searchFlightType: boolean;
}

export interface FlightSortConfig {
  field: string;
  direction: FlightSortDirection;
}

export interface RetryOptions {
  retries?: number;
  retryDelayMs?: number;
}

export interface AuthFetchOptions extends RetryOptions {
  authFetch: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;
}

export interface ProtobufTransportResult<T = unknown> {
  ok: boolean;
  status?: number;
  error?: string;
  data?: T;
}

export interface ProtobufTransportLike {
  requestWithFallback: (
    url: string,
    init?: RequestInit,
    resourceKey?: string,
  ) => Promise<ProtobufTransportResult<unknown>>;
}

export interface FetchFlightsPageDataOptions extends AuthFetchOptions {
  apiBase: string;
  pageSize?: number;
  protobufTransport?: ProtobufTransportLike | null;
}

export interface LoadFlightsPagedDataOptions extends FetchFlightsPageDataOptions {
  preprocess?: boolean;
  dispatchTimelineCache?: DispatchTimelineCache;
}

export interface FlightFilterHelperOptions {
  defaultBusinessFilters?: Partial<BusinessFilters>;
  getAnomalyCountForFlight?: (flight: Flight) => number;
  hasVipMarker?: (flight: Flight) => boolean;
}

export interface FilterFlightsOptions {
  sourceFlights?: Flight[];
  searchFields?: Partial<SearchFields>;
  businessFilters?: Partial<BusinessFilters>;
  helperOptions?: FlightFilterHelperOptions;
  getLegField?: (flight: Flight, legType: LegType, fieldName: keyof FlightLeg | string) => string;
  getRouteEndpoint?: (flight: Flight, legType: LegType, fieldMode?: 'code' | 'name') => string;
  getMissionSearchText?: (flight: Flight) => string;
  getFlightTypeSummary?: (flight: Flight) => string;
}

export interface DispatchTimelineUpdateOptions {
  cache?: DispatchTimelineCache;
  flights?: Flight[];
  originalFlights?: Flight[];
}

export interface DispatchTimelineRequestOptions extends DispatchTimelineUpdateOptions {
  apiBase: string;
  authFetch: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;
  force?: boolean;
}

export interface DispatchTimelineWriteOptions extends DispatchTimelineRequestOptions {
  value: string | null | undefined;
}

export interface UseFlightDataOptions {
  flights?: Flight[];
  originalFlights?: Flight[];
  airportContext?: Partial<AirportContext>;
  businessFilters?: Partial<BusinessFilters>;
  searchFields?: Partial<SearchFields>;
  searchQuery?: string;
  sortConfig?: Partial<FlightSortConfig>;
}

export interface UseFlightDataReturn {
  flights: Readonly<import('vue').Ref<readonly Flight[]>>;
  originalFlights: Readonly<import('vue').Ref<readonly Flight[]>>;
  airportContext: Readonly<import('vue').Ref<AirportContext>>;
  businessFilters: Readonly<import('vue').Ref<BusinessFilters>>;
  searchFields: Readonly<import('vue').Ref<SearchFields>>;
  searchQuery: Readonly<import('vue').Ref<string>>;
  sortConfig: Readonly<import('vue').Ref<FlightSortConfig>>;
  dispatchTimelineCache: Readonly<import('vue').Ref<ReadonlyMap<string, DispatchTimelineCacheEntry>>>;
  filteredFlights: import('vue').ComputedRef<Flight[]>;
  sortedFlights: import('vue').ComputedRef<Flight[]>;
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
  fetchEventJourney: (flightId: string | number, hours?: number) => Promise<Record<string, unknown>>;
  fetchHistoryReport: (flightId: string | number, hours?: number) => Promise<Record<string, unknown>>;
  appendBusinessCase: (caseId: string, appendData: { content: string; type?: string; mention_user_ids?: string[]; [key: string]: unknown }) => Promise<Record<string, unknown>>;
  acknowledgeBusinessCaseAppend: (caseId: string, appendId: string) => Promise<Record<string, unknown>>;
  fetchCollaborationGroupByFlight: (flightId: string | number) => Promise<Record<string, unknown>>;
}

export interface BusinessCaseCreatePayload {
  case_type: string;
  flight_id: string | number;
  description?: string | null;
  status?: string | null;
  visibility_scope?: BusinessCaseVisibilityScope | null;
  department_id?: string | null;
  department_name_snapshot?: string | null;
  context?: Record<string, unknown> | null;
  [key: string]: unknown;
}

export interface BusinessCaseVisibilityInfo {
  scope: BusinessCaseVisibilityScope | '';
  scopeLabel: string;
  departmentId: string | null;
  departmentName: string | null;
  isCommon: boolean;
}
