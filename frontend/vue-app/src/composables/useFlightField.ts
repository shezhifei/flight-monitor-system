import type {
  AirportContext,
  DispatchTimelineCache,
  Flight,
  FlightLeg,
  LegType,
  Station,
} from './useFlightDataTypes';
import { DISPATCH_TIMELINE_FIELDS, FLIGHT_MISSION_LABELS, TIME_FIELDS } from './useFlightDataConstants';

const timeFormatter = new Intl.DateTimeFormat('zh-CN', {
  hour: '2-digit',
  minute: '2-digit',
  hour12: false,
});

export function normalizeFlightId(value: string | number | null | undefined): string {
  if (value === null || value === undefined) {
    return '';
  }
  return String(value);
}

export function isSameFlightId(
  left: string | number | null | undefined,
  right: string | number | null | undefined,
): boolean {
  const leftId = normalizeFlightId(left);
  const rightId = normalizeFlightId(right);
  return leftId !== '' && leftId === rightId;
}

export function findFlightById(
  flightId: string | number | null | undefined,
  flights: Flight[] = [],
  originalFlights: Flight[] = [],
): Flight | null {
  const targetId = normalizeFlightId(flightId);
  if (!targetId) {
    return null;
  }

  return flights.find((flight) => normalizeFlightId(flight.flight_id) === targetId)
    ?? originalFlights.find((flight) => normalizeFlightId(flight.flight_id) === targetId)
    ?? null;
}

export function normalizeAirportContextV2(rawContext: Partial<AirportContext> | null | undefined): AirportContext {
  const context = rawContext && typeof rawContext === 'object' ? rawContext : {};
  const aliases = Array.isArray(context.name_aliases)
    ? context.name_aliases.map((alias) => String(alias ?? '').trim()).filter(Boolean)
    : [];

  return {
    code: String(context.code ?? '').trim().toUpperCase(),
    display_name: String(context.display_name ?? '').trim() || '本站',
    name_aliases: Array.from(new Set(aliases)),
  };
}

export function getAirportDisplayValueV2(
  airportContext: Partial<AirportContext> | null | undefined,
  fieldMode: 'code' | 'name' = 'code',
): string {
  const normalized = normalizeAirportContextV2(airportContext);
  if (fieldMode === 'name') {
    return normalized.display_name || normalized.code || '本站';
  }
  return normalized.code || normalized.display_name || '本站';
}

export function normalizeRouteStationV2(rawStation: unknown): Station | null {
  if (!rawStation || typeof rawStation !== 'object') {
    return null;
  }

  const code = String((rawStation as Record<string, unknown>).code ?? '').trim().toUpperCase();
  const name = String((rawStation as Record<string, unknown>).name ?? '').trim();
  if (!code && !name) {
    return null;
  }

  return { ...(rawStation as Record<string, unknown>), code, name: name || null } as Station;
}

export function normalizeRouteStationsV2(stations: unknown): Station[] {
  if (!Array.isArray(stations)) {
    return [];
  }

  const seen = new Set<string>();
  const normalized: Station[] = [];
  stations.forEach((station) => {
    const nextStation = normalizeRouteStationV2(station);
    if (!nextStation) {
      return;
    }
    const dedupeKey = `${nextStation.code}::${nextStation.name ?? ''}`;
    if (seen.has(dedupeKey)) {
      return;
    }
    seen.add(dedupeKey);
    normalized.push(nextStation);
  });
  return normalized;
}

export function parseLegPayloadV2(rawLeg: unknown, expectedLegType: LegType): FlightLeg | null {
  if (!rawLeg || typeof rawLeg !== 'object') {
    return null;
  }

  const next = { ...(rawLeg as Record<string, unknown>) };
  const normalizedType = String(next.leg_type ?? '').trim().toLowerCase();
  if (normalizedType !== expectedLegType) {
    return null;
  }

  const flightNo = String(next.flight_no ?? '').trim().toUpperCase();
  if (!flightNo) {
    return null;
  }

  return {
    ...next,
    leg_type: normalizedType as LegType,
    flight_no: flightNo,
    flight_type: String(next.flight_type ?? 'domestic').trim().toLowerCase(),
    origin_stations: normalizeRouteStationsV2(next.origin_stations),
    destination_stations: normalizeRouteStationsV2(next.destination_stations),
  } as FlightLeg;
}

export function getLegPayloadV2(flight: Flight | null | undefined, legType: LegType): FlightLeg | null {
  if (!flight || typeof flight !== 'object') {
    return null;
  }
  const key = legType === 'inbound' ? 'inbound_leg' : 'outbound_leg';
  return parseLegPayloadV2(flight[key], legType);
}

export function getLegStationsV2(
  flight: Flight | null | undefined,
  legType: LegType,
  fieldName: keyof Pick<FlightLeg, 'origin_stations' | 'destination_stations'>,
): Station[] {
  const leg = getLegPayloadV2(flight, legType);
  if (!leg) {
    return [];
  }
  return normalizeRouteStationsV2(leg[fieldName]);
}

export function getStationDisplayValueV2(station: Station | null | undefined, fieldMode: 'code' | 'name' = 'code'): string {
  if (!station || typeof station !== 'object') {
    return '';
  }
  if (fieldMode === 'name') {
    return String(station.name ?? station.code ?? '').trim();
  }
  return String(station.code ?? station.name ?? '').trim();
}

export function getStationListDisplayV2(
  flight: Flight | null | undefined,
  legType: LegType,
  fieldName: keyof Pick<FlightLeg, 'origin_stations' | 'destination_stations'>,
  fieldMode: 'code' | 'name' = 'code',
): string {
  return getLegStationsV2(flight, legType, fieldName)
    .map((station) => getStationDisplayValueV2(station, fieldMode))
    .filter(Boolean)
    .join(', ');
}

export function getLegFieldV2(
  flight: Flight | null | undefined,
  legType: LegType,
  fieldName: keyof FlightLeg | string,
): string {
  const leg = getLegPayloadV2(flight, legType);
  if (!leg) {
    return '';
  }
  return String((leg as Record<string, unknown>)[fieldName] ?? '').trim();
}

export function normalizeFlightTypeLabelV2(rawType: unknown): string {
  const type = String(rawType ?? '').trim().toLowerCase();
  if (type === 'intl' || type === 'international') return '国际';
  if (type === 'region') return '地区';
  if (type === 'domestic') return '国内';
  return '';
}

export function getPrimaryFlightNoV2(flight: Flight): string {
  const outbound = getLegFieldV2(flight, 'outbound', 'flight_no');
  const inbound = getLegFieldV2(flight, 'inbound', 'flight_no');
  return outbound || inbound || String(flight?.flight_number ?? flight?.flight_id ?? '').trim();
}

export function getRouteDisplayTextV2(
  flight: Flight,
  airportContext: Partial<AirportContext> | null | undefined,
): string {
  const inboundOrigin = getStationListDisplayV2(flight, 'inbound', 'origin_stations', 'name');
  const inboundDestination = getStationListDisplayV2(flight, 'inbound', 'destination_stations', 'name');
  const outboundOrigin = getStationListDisplayV2(flight, 'outbound', 'origin_stations', 'name');
  const outboundDestination = getStationListDisplayV2(flight, 'outbound', 'destination_stations', 'name');
  const inboundNo = getLegFieldV2(flight, 'inbound', 'flight_no');
  const outboundNo = getLegFieldV2(flight, 'outbound', 'flight_no');
  const airportName = getAirportDisplayValueV2(airportContext, 'name');

  if (inboundNo && outboundNo) {
    return `${inboundOrigin || '-'} -> ${airportName} -> ${outboundDestination || '-'}`;
  }
  return `${inboundOrigin || outboundOrigin || '-'} -> ${outboundDestination || inboundDestination || '-'}`;
}

export function normalizeFlightTypeCodeV2(rawType: unknown): 'intl' | 'region' | 'domestic' {
  const type = String(rawType ?? '').trim().toLowerCase();
  if (type === 'intl' || type === 'international' || type === '国际') return 'intl';
  if (type === 'region' || type === '地区') return 'region';
  return 'domestic';
}

export function getLegFlightTypeLabelV2(flight: Flight, legType: LegType): string {
  const leg = getLegPayloadV2(flight, legType);
  if (!leg) {
    return '';
  }
  return normalizeFlightTypeLabelV2(leg.flight_type);
}

export function normalizeMissionKey(rawMission: unknown): string {
  const mission = String(rawMission ?? '').trim();
  if (!mission) {
    return '';
  }
  return mission
    .replace(/[／]/g, '/')
    .replace(/\s*\/\s*/g, '/')
    .replace(/\s+/g, ' ')
    .toUpperCase();
}

export function parseMissionValue(rawMission: unknown): { raw: string; key: string; label: string; suffix: string } {
  const mission = String(rawMission ?? '').trim();
  if (!mission) {
    return { raw: '', key: '', label: '', suffix: '' };
  }

  const parts = mission.split(/[，,]/).map((part) => part.trim()).filter(Boolean);
  const primary = parts[0] || '';
  const suffixParts = parts.slice(1);
  const key = normalizeMissionKey(primary);
  return {
    raw: mission,
    key,
    label: FLIGHT_MISSION_LABELS[key as keyof typeof FLIGHT_MISSION_LABELS] || '',
    suffix: suffixParts.join('，'),
  };
}

export function formatMissionLabel(rawMission: unknown): string {
  const parsed = parseMissionValue(rawMission);
  if (!parsed.raw) {
    return '';
  }
  if (!parsed.label) {
    return parsed.raw;
  }
  return parsed.suffix ? `${parsed.label}（${parsed.suffix}）` : parsed.label;
}

export function parseMissionNumericInput(rawMission: unknown): number | null {
  const mission = String(rawMission ?? '').trim();
  if (!mission || !/^\d+$/.test(mission)) {
    return null;
  }
  const numericMission = Number(mission);
  if (!Number.isInteger(numericMission)) {
    return null;
  }
  return Object.prototype.hasOwnProperty.call(FLIGHT_MISSION_LABELS, String(numericMission))
    ? numericMission
    : null;
}

export function collectMissionSearchTerms(rawMission: unknown): string[] {
  const parsed = parseMissionValue(rawMission);
  return Array.from(new Set([parsed.raw, parsed.key, parsed.label, parsed.suffix].filter(Boolean)));
}

export function collectRawMissionValuesV2(flight: Flight): string[] {
  return ['inbound', 'outbound']
    .map((legType) => {
      const leg = getLegPayloadV2(flight, legType as LegType);
      return leg ? String(leg.mission ?? '').trim() : '';
    })
    .filter(Boolean);
}

export function getLegMissionLabelV2(flight: Flight, legType: LegType): string {
  const leg = getLegPayloadV2(flight, legType);
  if (!leg) {
    return '';
  }
  return formatMissionLabel(leg.mission);
}

export function getMissionSummaryV2(flight: Flight): string {
  const missions = collectRawMissionValuesV2(flight)
    .map((mission) => formatMissionLabel(mission))
    .filter(Boolean);
  if (!missions.length) {
    return '';
  }
  return Array.from(new Set(missions)).join(' | ');
}

export function getMissionSearchTextV2(flight: Flight): string {
  const terms = collectRawMissionValuesV2(flight).flatMap((mission) => collectMissionSearchTerms(mission));
  if (!terms.length) {
    return '';
  }
  return Array.from(new Set(terms)).join(' ');
}

export function getFlightTypeSummaryV2(flight: Flight): string {
  const types = [getLegFlightTypeLabelV2(flight, 'inbound'), getLegFlightTypeLabelV2(flight, 'outbound')].filter(Boolean);
  if (!types.length) {
    return '';
  }
  return Array.from(new Set(types)).join('|');
}

export function getLegVipFlagV2(flight: Flight, legType: LegType): boolean {
  const leg = getLegPayloadV2(flight, legType);
  return Boolean(leg?.is_vip);
}

export function getFlightNumberByLegV2(flight: Flight, legType: LegType): string {
  return getLegFieldV2(flight, legType, 'flight_no');
}

export function hasLegFlightV2(flight: Flight, legType: LegType): boolean {
  return Boolean(getFlightNumberByLegV2(flight, legType));
}

export function getFlightNumberDisplayV2(flight: Flight): string {
  const inboundFlightNo = getFlightNumberByLegV2(flight, 'inbound');
  const outboundFlightNo = getFlightNumberByLegV2(flight, 'outbound');
  if (inboundFlightNo && outboundFlightNo) {
    return `${inboundFlightNo}|${outboundFlightNo}`;
  }
  return outboundFlightNo || inboundFlightNo || '';
}

export function getFlightTypeLabelsV2(flight: Flight): { inbound: string; outbound: string } {
  return {
    inbound: getLegFlightTypeLabelV2(flight, 'inbound'),
    outbound: getLegFlightTypeLabelV2(flight, 'outbound'),
  };
}

export function getPrimaryFlightTypeLabelV2(flight: Flight): string {
  const labels = getFlightTypeLabelsV2(flight);
  return labels.outbound || labels.inbound || '';
}

export function getMissionInputValueV2(flight: Flight): string {
  return collectRawMissionValuesV2(flight).join(',');
}

export function getRouteEndpointV2(
  flight: Flight,
  legType: LegType,
  fieldMode: 'code' | 'name' = 'code',
): string {
  const fieldName = legType === 'inbound' ? 'origin_stations' : 'destination_stations';
  return getStationListDisplayV2(flight, legType, fieldName, fieldMode);
}

export function getRouteSummaryV2(
  flight: Flight,
  airportContext: Partial<AirportContext> | null | undefined,
  fieldMode: 'code' | 'name' = 'code',
): string {
  const inboundNo = getLegFieldV2(flight, 'inbound', 'flight_no');
  const outboundNo = getLegFieldV2(flight, 'outbound', 'flight_no');
  const origin = getRouteEndpointV2(flight, 'inbound', fieldMode)
    || getStationListDisplayV2(flight, 'outbound', 'origin_stations', fieldMode);
  const destination = getRouteEndpointV2(flight, 'outbound', fieldMode)
    || getStationListDisplayV2(flight, 'inbound', 'destination_stations', fieldMode);
  const airportLabel = getAirportDisplayValueV2(airportContext, fieldMode);

  if (inboundNo && outboundNo) {
    return `${origin || '--'} -> ${airportLabel} -> ${destination || '--'}`;
  }
  if (inboundNo) {
    return `${origin || '--'} -> ${airportLabel}`;
  }
  if (outboundNo) {
    return `${airportLabel} -> ${destination || '--'}`;
  }
  return `${origin || '--'} -> ${destination || '--'}`;
}

export function isWideBodyAircraft(aircraftTypeDetail: unknown): boolean {
  const code = String(aircraftTypeDetail ?? '').toUpperCase().replace(/[^A-Z0-9]/g, '');
  if (!code) {
    return false;
  }
  return /^(A330|A340|A350|A380|B747|B767|B777|B787)/.test(code);
}

export function normalizeSignedFlag(value: unknown): boolean {
  if (typeof value === 'boolean') return value;
  if (typeof value === 'string') {
    const normalized = value.toLowerCase().trim();
    if (normalized === 'true' || normalized === '1' || normalized === 'yes') return true;
    if (normalized === 'false' || normalized === '0' || normalized === 'no') return false;
  }
  if (typeof value === 'number') {
    if (value === 1) return true;
    if (value === 0) return false;
  }
  return true;
}

export function isDelayedFlight(flight: Flight): boolean {
  const status = String(flight?.status ?? '').toLowerCase();
  if (status.includes('延误')) {
    return true;
  }

  const delayPairs: Array<[unknown, unknown]> = [
    [flight?.estimated_departure, flight?.scheduled_departure],
    [flight?.estimated_arrival, flight?.scheduled_arrival],
  ];

  return delayPairs.some(([estimate, schedule]) => {
    if (!estimate || !schedule) {
      return false;
    }
    const diffMs = new Date(estimate as string | number | Date).getTime() - new Date(schedule as string | number | Date).getTime();
    return Number.isFinite(diffMs) && diffMs >= 15 * 60 * 1000;
  });
}

export function normalizeAnomalySummary(rawSummary: unknown): Flight['anomaly_summary'] {
  const summary = rawSummary && typeof rawSummary === 'object' ? (rawSummary as Record<string, unknown>) : {};
  return {
    has_open_anomaly: Boolean(summary.has_open_anomaly),
    open_count: Number(summary.open_count ?? 0),
    acknowledged_count: Number(summary.acknowledged_count ?? 0),
  };
}

export function getAnomalyCountForFlight(flight: Flight): number {
  const summaryCount = Number(flight?.anomaly_summary?.open_count ?? 0);
  if (Number.isFinite(summaryCount) && summaryCount > 0) {
    return summaryCount;
  }
  const anomalyCount = Number((flight as Record<string, unknown>).anomaly_count ?? 0);
  return Number.isFinite(anomalyCount) ? Math.max(0, anomalyCount) : 0;
}

export function hasVipMarker(flight: Flight): boolean {
  return Boolean(flight?.inbound_leg?.is_vip || flight?.outbound_leg?.is_vip);
}

export function isDispatchTimelineField(field: string): boolean {
  return DISPATCH_TIMELINE_FIELDS.has(String(field || '').trim());
}

export function getTimelineFieldValueFromCache(
  cache: DispatchTimelineCache | null | undefined,
  flightId: string | number | null | undefined,
  field: string,
): string | null {
  const entry = cache?.get(normalizeFlightId(flightId));
  if (!entry?.byMilestone) {
    return null;
  }
  const event = entry.byMilestone.get(String(field || '').trim());
  return typeof event?.occurred_at === 'string' ? event.occurred_at : null;
}

export function syncFlightTimelineFieldsFromCache(
  flight: Flight | null | undefined,
  cache: DispatchTimelineCache | null | undefined,
): Flight | null | undefined {
  if (!flight || typeof flight !== 'object') {
    return flight;
  }

  const flightId = normalizeFlightId(flight.flight_id);
  if (!flightId) {
    return flight;
  }

  DISPATCH_TIMELINE_FIELDS.forEach((field) => {
    const cached = getTimelineFieldValueFromCache(cache, flightId, field);
    if (cached) {
      flight[field] = cached;
    } else if (flight[field] === undefined) {
      flight[field] = null;
    }
  });

  return flight;
}

export function formatTimeValue(isoString: unknown): string | null {
  if (!isoString) return null;
  try {
    return timeFormatter.format(new Date(isoString as string | number | Date));
  } catch {
    return null;
  }
}

export function hydrateFlightLegViewV2(
  flight: Flight,
  options: { dispatchTimelineCache?: DispatchTimelineCache } = {},
): Flight {
  flight.inbound_leg = getLegPayloadV2(flight, 'inbound');
  flight.outbound_leg = getLegPayloadV2(flight, 'outbound');
  flight.anomaly_summary = normalizeAnomalySummary(flight.anomaly_summary);
  syncFlightTimelineFieldsFromCache(flight, options.dispatchTimelineCache);
  return flight;
}

export function preprocessFlightTimes(
  flight: Flight,
  options: { dispatchTimelineCache?: DispatchTimelineCache } = {},
): Flight {
  hydrateFlightLegViewV2(flight, options);
  if (!flight._timesFormatted) {
    flight._fmt = {};
    (TIME_FIELDS as readonly string[]).forEach((field) => {
      flight._fmt![field] = formatTimeValue(flight[field]);
    });
    flight._timesFormatted = true;
  }
  return flight;
}

export function preprocessFlightsBatch(
  flights: Flight[],
  options: { dispatchTimelineCache?: DispatchTimelineCache } = {},
): Flight[] {
  return flights.map((flight) => preprocessFlightTimes(flight, options));
}
