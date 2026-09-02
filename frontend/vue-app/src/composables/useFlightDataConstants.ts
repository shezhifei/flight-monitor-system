import type { BusinessFilters, FlightSortConfig, LegType, SearchFields } from './useFlightDataTypes';

export const DEFAULT_BUSINESS_FILTERS: BusinessFilters = {
  aircraftBodyFilter: 'all',
  commercialSignedFilter: 'yes',
  anomalyFilter: 'all',
  delayFilter: 'all',
  vipFilter: 'all',
  quickTurnFilter: 'all',
};

export const DEFAULT_SEARCH_FIELDS: SearchFields = {
  searchFlightNo: true,
  searchDestination: true,
  searchDestinationName: true,
  searchOrigin: true,
  searchOriginName: true,
  searchStatus: true,
  searchAircraftType: true,
  searchStand: true,
  searchGate: true,
  searchMission: true,
  searchFlightType: true,
};

export const DEFAULT_SORT_CONFIG: FlightSortConfig = {
  field: 'scheduled_departure',
  direction: 'asc',
};

export const DISPATCH_TIMELINE_FIELD_META = {
  on_blocks_time: { leg_type: 'inbound' },
  cabin_door_open_time: { leg_type: 'inbound' },
  deboarding_complete_time: { leg_type: 'inbound' },
  cleaning_start_time: { leg_type: 'inbound' },
  cleaning_end_time: { leg_type: 'inbound' },
  start_boarding_time: { leg_type: 'outbound' },
  end_boarding_time: { leg_type: 'outbound' },
  boarding_allowed_time: { leg_type: 'outbound' },
  passenger_ready_time: { leg_type: 'outbound' },
  cabin_door_close_time: { leg_type: 'outbound' },
  cargo_door_close_time: { leg_type: 'outbound' },
  loading_complete_time: { leg_type: 'outbound' },
  off_blocks_time: { leg_type: 'outbound' },
} as const satisfies Record<string, { leg_type: LegType }>;

export const DISPATCH_TIMELINE_FIELDS = new Set<string>(Object.keys(DISPATCH_TIMELINE_FIELD_META));

/**
 * 快照字段（非时间线）的方向归属：该字段属于进港还是出港方向航班。
 * 拆表后过站行 = 链 id + 两班方向航班；单元格 PATCH 必须打在对应方向航班上，
 * 不能用 row_id（过站行的 row_id = 链 id = 已软删聚合行，后端会拒绝写）。
 * 时间线字段的方向见 DISPATCH_TIMELINE_FIELD_META.leg_type。
 * 未列出的行级字段（备注等）按「进港优先」解析，见 resolveDirectionalFlightId。
 */
export const FLIGHT_FIELD_DIRECTION = {
  scheduled_departure: 'outbound',
  estimated_departure: 'outbound',
  actual_departure: 'outbound',
  codt: 'outbound',
  cobt_time: 'outbound',
  scheduled_arrival: 'inbound',
  estimated_arrival: 'inbound',
  actual_arrival: 'inbound',
} as const satisfies Record<string, 'inbound' | 'outbound'>;

export type FlightFieldDirection = 'inbound' | 'outbound';

export const TIME_FIELDS = [
  'scheduled_departure',
  'scheduled_arrival',
  'estimated_departure',
  'estimated_arrival',
  'actual_departure',
  'actual_arrival',
  'cobt_time',
  'codt',
  'start_boarding_time',
  'end_boarding_time',
  'boarding_allowed_time',
  'passenger_ready_time',
  'off_blocks_time',
  'cabin_door_open_time',
  'cleaning_start_time',
  'cleaning_end_time',
  'on_blocks_time',
  'deboarding_complete_time',
  'cabin_door_close_time',
  'cargo_door_close_time',
  'loading_complete_time',
] as const;

export const FLIGHT_MISSION_LABELS = Object.freeze({
  '1': '航线熟练飞行',
  '2': '播种飞行',
  '3': '专机飞行',
  '4': '旅客加班',
  '5': '展示飞行',
  '6': '带飞飞行',
  '7': '校验飞行',
  '8': '货运包机',
  '9': '货运加班',
  '10': '按专机保障的定期航班',
  '11': '本场训练飞行',
  '12': '旅客包机',
  '13': '调机飞行',
  '14': '试航飞行',
  '15': '试飞飞行',
  '16': '公务飞行',
  '17': '要客飞行',
  '18': '训练飞行',
  '19': '急救飞行',
  '20': '正班飞行',
  '21': '补班飞行',
  '22': '执法飞行',
  '23': '验证飞行',
  '24': '转场飞行',
  '25': '视察飞行（含巡线飞行）',
  '26': '航摄飞行',
  '27': '其他飞行',
  '28': '临时飞越',
  '31': '技术经停',
  'A/V': '航线熟练飞行',
  'B/F': '播种飞行',
  'B/W': '专机飞行',
  'C/B': '旅客加班',
  'D/M': '展示飞行',
  'D/Y': '带飞飞行',
  'F/J': '校验飞行',
  'H/G': '货运包机',
  'H/Y': '货运加班',
  'J/B': '按专机保障的定期航班',
  'K/L': '本场训练飞行',
  'L/W': '旅客包机',
  'N/M': '调机飞行',
  'R/Z': '试航飞行',
  'S/F': '试飞飞行',
  'U/H': '公务飞行',
  VIP: '要客飞行',
  'X/L': '训练飞行',
  'O/F': '急救飞行',
  '0/F': '急救飞行',
  'W/Z': '正班飞行',
  'Z/P': '补班飞行',
  'Z/F': '执法飞行',
  'Y/Z': '验证飞行',
  'W/A': '转场飞行',
  'S/Q': '视察飞行（含巡线飞行）',
  'H/F': '航摄飞行',
  'X/X': '其他飞行',
  OVERFLIGHT: '临时飞越',
  'TECH STOP': '技术经停',
} as const);
