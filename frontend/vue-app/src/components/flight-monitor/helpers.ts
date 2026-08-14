import { ref } from 'vue';
import {
  EMPTY_DISPLAY_TEXT,
  formatMissionLabel,
  formatTimeValue,
  getAirportDisplayValue,
  getAnomalyCountForFlight,
  getFlightNumberDisplay,
  getLegField,
  getMissionSummary,
  getRouteEndpoint,
  hasVipMarker,
  isDelayedFlight,
  isWideBodyAircraft,
  normalizeFlightId,
  normalizeSignedFlag,
  getLegVipFlag,
  getLegPayload,
  normalizeFlightTypeCode,
  getFlightTypeSummary,
  type AirportContext,
  type Flight as FlightModel,
  type SearchFields,
} from '../../composables/useFlightData';
import type { Flight } from '@/types/bindings';
import type {
  BusinessCaseAiExtractionConfig,
  BusinessCaseProperties,
  BusinessCaseWorkflowReceiptProjection,
} from '../../types/backend';

function asFlightModel(flight: Flight): FlightModel {
  return flight as unknown as FlightModel;
}

export type FlightViewMode = 'card' | 'table';
export type TimeTone = 'scheduled' | 'estimated' | 'actual';

export interface SearchFieldOption {
  key: keyof SearchFields;
  id: keyof SearchFields;
  label: string;
  ariaLabel: string;
}

export interface TimeDisplay {
  value: string;
  tone: TimeTone;
}

export const SEARCH_FIELD_OPTIONS: readonly SearchFieldOption[] = [
  { key: 'searchFlightNo', id: 'searchFlightNo', label: '航班号', ariaLabel: '按航班号搜索' },
  { key: 'searchDestination', id: 'searchDestination', label: '目的地', ariaLabel: '按目的地搜索' },
  { key: 'searchDestinationName', id: 'searchDestinationName', label: '目的地名称', ariaLabel: '按目的地名称搜索' },
  { key: 'searchOrigin', id: 'searchOrigin', label: '出发地', ariaLabel: '按出发地搜索' },
  { key: 'searchOriginName', id: 'searchOriginName', label: '出发地名称', ariaLabel: '按出发地名称搜索' },
  { key: 'searchStatus', id: 'searchStatus', label: '状态', ariaLabel: '按状态搜索' },
  { key: 'searchAircraftType', id: 'searchAircraftType', label: '机型', ariaLabel: '按机型搜索' },
  { key: 'searchStand', id: 'searchStand', label: '机位', ariaLabel: '按机位搜索' },
  { key: 'searchGate', id: 'searchGate', label: '登机口', ariaLabel: '按登机口搜索' },
  { key: 'searchMission', id: 'searchMission', label: '任务类型', ariaLabel: '按任务类型搜索' },
  { key: 'searchFlightType', id: 'searchFlightType', label: '航班类型', ariaLabel: '按航班类型搜索' },
] as const;

function getFormattedField(flight: Flight, field: string): string | null {
  const model = asFlightModel(flight);
  const cached = model._fmt?.[field];
  return cached ?? formatTimeValue(model[field]) ?? null;
}

export function normalizeStatusToken(status: unknown): string {
  const text = String(status ?? '').trim().toLowerCase();
  if (text.includes('取消')) return 'cancelled';
  if (text.includes('延误')) return 'delayed';
  if (text.includes('下站到达') || text.includes('到下站')) return 'next-arrived';
  if (text.includes('已起飞')) return 'departed';
  if (text.includes('前方起飞')) return 'prev-departed';
  if (text.includes('到达')) return 'arrived';
  if (text.includes('值机结束')) return 'checkin-end';
  if (text.includes('催促登机')) return 'boarding-urge';
  if (text.includes('登机结束') || text.includes('结束登机')) return 'boarding-ended';
  if (text.includes('登机')) return 'boarding';
  return 'scheduled';
}

export function getStatusClassName(status: unknown): string {
  return `status-${normalizeStatusToken(status)}`;
}

export function getStatusRowClassName(status: unknown): string {
  return `row-${normalizeStatusToken(status)}`;
}

export function deriveOperationDateLabel(flight: Flight): string {
  const candidates = [
    flight.scheduled_departure,
    flight.estimated_departure,
    flight.scheduled_arrival,
    flight.estimated_arrival,
  ];
  for (const value of candidates) {
    if (!value) continue;
    const date = new Date(String(value));
    if (!Number.isNaN(date.getTime())) {
      return date.toLocaleDateString('zh-CN');
    }
  }
  return '--';
}

export function getFlightNumbers(flight: Flight): { inbound: string; outbound: string; combined: string } {
  const model = asFlightModel(flight);
  const inbound = getLegField(model, 'inbound', 'flight_no');
  const outbound = getLegField(model, 'outbound', 'flight_no');
  return {
    inbound,
    outbound,
    combined: getFlightNumberDisplay(model) || EMPTY_DISPLAY_TEXT,
  };
}

const ROUTE_DISPLAY_MODE_STORAGE_KEY = 'routeDisplayMode';

function loadRouteDisplayMode(): 'code' | 'name' {
  try {
    if (typeof localStorage !== 'undefined' && localStorage.getItem(ROUTE_DISPLAY_MODE_STORAGE_KEY) === 'name') {
      return 'name';
    }
  } catch {
    // ignore storage access failures; fall back to default
  }
  return 'code';
}

export const routeDisplayMode = ref<'code' | 'name'>(loadRouteDisplayMode());

export const toggleRouteDisplayMode = () => {
  routeDisplayMode.value = routeDisplayMode.value === 'code' ? 'name' : 'code';
  try {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(ROUTE_DISPLAY_MODE_STORAGE_KEY, routeDisplayMode.value);
    }
  } catch {
    // ignore storage access failures; toggle still applies for this session
  }
};

export function getFlightEndpoints(
  flight: Flight,
  airportContext: Partial<AirportContext> | null | undefined,
  fieldMode: 'code' | 'name' = routeDisplayMode.value,
): { origin: string; airport: string; destination: string; hasInbound: boolean; hasOutbound: boolean } {
  const model = asFlightModel(flight);
  const airport = getAirportDisplayValue(airportContext, fieldMode) || '本站';
  const hasInbound = Boolean(getLegField(model, 'inbound', 'flight_no'));
  const hasOutbound = Boolean(getLegField(model, 'outbound', 'flight_no'));

  let origin: string;
  let destination: string;

  if (hasInbound && hasOutbound) {
    // 双腿：来源 → [本站] → 目的
    origin = getRouteEndpoint(model, 'inbound', fieldMode) || airport;
    destination = getRouteEndpoint(model, 'outbound', fieldMode) || airport;
  } else if (hasInbound) {
    // 仅进港：来源 → 本站
    origin = getRouteEndpoint(model, 'inbound', fieldMode) || '--';
    destination = airport;
  } else {
    // 仅出港：本站 → 目的
    origin = airport;
    destination = getRouteEndpoint(model, 'outbound', fieldMode) || '--';
  }

  return { origin: origin || '--', airport, destination: destination || '--', hasInbound, hasOutbound };
}

export function getTimeDisplay(flight: Flight, kind: 'arrival' | 'departure'): TimeDisplay {
  const fields = kind === 'arrival'
    ? [
      ['actual_arrival', 'actual'],
      ['estimated_arrival', 'estimated'],
      ['scheduled_arrival', 'scheduled'],
    ]
    : [
      ['actual_departure', 'actual'],
      ['estimated_departure', 'estimated'],
      ['scheduled_departure', 'scheduled'],
    ];

  for (const [field, tone] of fields) {
    const value = getFormattedField(flight, field);
    if (value) {
      return { value, tone: tone as TimeTone };
    }
  }

  return { value: EMPTY_DISPLAY_TEXT, tone: 'scheduled' };
}

/**
 * Raw time field formatted as HH:MM (legacy FIELD_MAP time columns),
 * `--` when empty. Uses the preprocessed `_fmt` cache when available.
 */
export function getTimeFieldDisplay(flight: Flight, field: string): string {
  return getFormattedField(flight, field) || EMPTY_DISPLAY_TEXT;
}

/** Raw (unformatted) value of a time field, '' when empty. */
export function getTimeFieldRawValue(flight: Flight, field: string): string {
  const raw = asFlightModel(flight)[field];
  return raw === null || raw === undefined ? '' : String(raw);
}

/** 属性 column (legacy FIELD_MAP `flight_type`). */
export function getFlightTypeColumnDisplay(flight: Flight): string {
  return getFlightTypeSummary(asFlightModel(flight)) || EMPTY_DISPLAY_TEXT;
}

export function getTimeToneClass(tone: TimeTone): string {
  if (tone === 'actual') return 'actual-time';
  if (tone === 'estimated') return 'estimated-time';
  return 'scheduled-time';
}

export function getMissionDisplay(flight: Flight): string {
  const model = asFlightModel(flight);
  return getMissionSummary(model)
    || formatMissionLabel(model.outbound_leg?.mission)
    || formatMissionLabel(model.inbound_leg?.mission)
    || EMPTY_DISPLAY_TEXT;
}

export function getFlightTypeDisplay(flight: Flight): string {
  const inbound = String(flight?.inbound_leg?.flight_type ?? '').trim();
  const outbound = String(flight?.outbound_leg?.flight_type ?? '').trim();
  if ([inbound, outbound].some((value) => /intl|international|国际/i.test(value))) return '国际';
  if ([inbound, outbound].some((value) => /region|地区/i.test(value))) return '地区';
  if (inbound || outbound) return '国内';
  return '未分类';
}

export function getAircraftBodyLabel(flight: Flight): string {
  return isWideBodyAircraft(flight?.aircraft_type_detail) ? '宽体机' : '窄体机';
}

export function getStandGateDisplay(flight: Flight): string {
  const stand = String(flight?.stand ?? '').trim();
  const standDisplay = stand ? stand : '--';
  
  if (flight?.outbound_leg) {
    const gate = String(flight?.gate ?? '').trim();
    const gateDisplay = gate ? gate : '--';
    return `机位 ${standDisplay} / 登机口 ${gateDisplay}`;
  }
  return `机位 ${standDisplay}`;
}

export function getFlightNumberStyleClass(flight: Flight, legType: 'inbound' | 'outbound'): string {
  const model = asFlightModel(flight);
  const isVip = getLegVipFlag(model, legType);
  const payload = getLegPayload(model, legType);
  const typeCode = normalizeFlightTypeCode(payload?.flight_type);
  const isIntl = typeCode === 'intl' || typeCode === 'region';

  if (isVip && isIntl) return 'text-flight-vip-intl';
  if (isVip) return 'text-flight-vip';
  if (isIntl) return 'text-flight-intl';
  return '';
}

export function getCommercialSignedLabel(flight: Flight): string {
  return normalizeSignedFlag(flight?.is_commercial_signed) ? '已签约' : '--';
}

export function getVipLabel(flight: Flight): string {
  return hasVipMarker(asFlightModel(flight)) ? 'VIP' : '--';
}

export function getAnomalySeverity(flight: Flight): 'high' | 'medium' | 'low' {
  const model = asFlightModel(flight);
  const count = getAnomalyCountForFlight(model);
  if (count >= 2 || isDelayedFlight(model)) return 'high';
  if (count === 1) return 'medium';
  return 'low';
}

export function getAnomalyBadgeClass(flight: Flight): string {
  const severity = getAnomalySeverity(flight);
  return severity === 'high' ? 'badge-high' : severity === 'medium' ? 'badge-medium' : 'badge-low';
}

export function getFlightDomId(flight: Flight): string {
  return normalizeFlightId(flight?.flight_id);
}

export function getCaseReceiptProjection(
  caseData: { workflow_receipt?: BusinessCaseWorkflowReceiptProjection | null } | null | undefined,
): BusinessCaseWorkflowReceiptProjection | null {
  // Single canonical shape: workflow_receipt is a first-class field.
  // Do NOT fall back to context.workflow_receipt — that was a legacy
  // dual-shape shim that caused drift.
  if (!caseData) {
    return null;
  }
  return caseData.workflow_receipt ?? null;
}

/**
 * Minimal field-config shape used by the UI to validate case drafts.
 * Covers both `ai_extraction_config.fields` and `extra_info_schema.fields`.
 */
export interface CaseFieldConfig {
  type?: string | null;
  label?: string | null;
  required?: boolean;
  enum_values?: string[];
  aliases?: string[];
  examples?: string[];
  display_in_notification?: boolean;
}

/**
 * Resolved case-type config: the AI extraction config with `fields`
 * potentially overridden by `extra_info_schema.fields`.
 */
export type CaseTypeResolvedConfig = Omit<BusinessCaseAiExtractionConfig, 'fields'> & {
  fields: Record<string, CaseFieldConfig>;
};

/**
 * Resolve the effective field set for a case type.
 *
 * Prefers `case_properties.extra_info_schema.fields` when present and non-empty;
 * otherwise falls back to `ai_extraction_config.fields`.
 *
 * Replaces the former dual-shape cast access in AutoCopilotVoicePanel.
 */
export function resolveExtraInfoFields(
  caseProperties: BusinessCaseProperties | null | undefined,
  aiConfig: BusinessCaseAiExtractionConfig | null | undefined,
): Record<string, CaseFieldConfig> {
  const extraInfoFields = caseProperties?.extra_info_schema?.fields;
  if (extraInfoFields && Object.keys(extraInfoFields).length > 0) {
    return extraInfoFields;
  }
  return aiConfig?.fields ?? {};
}

/**
 * Resolve the full case-type config, merging `ai_extraction_config` with
 * the effective `fields` from `resolveExtraInfoFields`.
 *
 * Returns `null` when `aiConfig` is absent (no AI extraction enabled).
 */
export function resolveCaseTypeConfig(
  caseProperties: BusinessCaseProperties | null | undefined,
  aiConfig: BusinessCaseAiExtractionConfig | null | undefined,
): CaseTypeResolvedConfig | null {
  if (!aiConfig) {
    return null;
  }
  const fields = resolveExtraInfoFields(caseProperties, aiConfig);
  const { fields: _omitted, ...rest } = aiConfig;
  return { ...rest, fields };
}
