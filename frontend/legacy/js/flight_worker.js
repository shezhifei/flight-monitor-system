/**
 * Flight Worker - 后台数据处理 Web Worker
 * 将搜索、过滤、排序逻辑移至后台线程，避免阻塞主线程
 */

self.airportContext = {
    code: '',
    display_name: '本站',
    name_aliases: [],
};

self.onmessage = function (e) {
    const { type, flights, query, filters, sortConfig, requestId, airportContext } = e.data;
    if (airportContext && typeof airportContext === 'object') {
        self.airportContext = normalizeAirportContext(airportContext);
    }

    switch (type) {
        case 'filter':
            const filtered = filterFlights(flights, query, filters);
            self.postMessage({ type: 'filterResult', data: filtered, requestId });
            break;
        case 'sort':
            const sorted = sortFlights(flights, sortConfig);
            self.postMessage({ type: 'sortResult', data: sorted, requestId });
            break;
        case 'filterAndSort':
            const result = sortFlights(filterFlights(flights, query, filters), sortConfig);
            self.postMessage({ type: 'filterAndSortResult', data: result, requestId });
            break;
        default:
            self.postMessage({ type: 'error', message: 'Unknown operation type' });
    }
};

function getLegPayload(flight, legType) {
    if (!flight || typeof flight !== 'object') {
        return null;
    }
    const key = legType === 'inbound' ? 'inbound_leg' : 'outbound_leg';
    const leg = flight[key];
    if (!leg || typeof leg !== 'object') {
        return null;
    }
    return leg;
}

function getLegField(flight, legType, fieldName) {
    const leg = getLegPayload(flight, legType);
    if (!leg) {
        return '';
    }
    return String(leg[fieldName] || '').trim();
}

function normalizeAirportContext(rawContext) {
    const context = rawContext && typeof rawContext === 'object' ? rawContext : {};
    const aliases = Array.isArray(context.name_aliases)
        ? context.name_aliases.map((alias) => String(alias || '').trim()).filter(Boolean)
        : [];
    return {
        code: String(context.code || '').trim().toUpperCase(),
        display_name: String(context.display_name || '').trim() || '本站',
        name_aliases: Array.from(new Set(aliases)),
    };
}

function normalizeRouteStation(rawStation) {
    if (!rawStation || typeof rawStation !== 'object') {
        return null;
    }
    const code = String(rawStation.code || '').trim().toUpperCase();
    const name = String(rawStation.name || '').trim();
    if (!code && !name) {
        return null;
    }
    return {
        code,
        name: name || null,
    };
}

function getLegStations(flight, legType, fieldName) {
    const leg = getLegPayload(flight, legType);
    if (!leg || !Array.isArray(leg[fieldName])) {
        return [];
    }
    return leg[fieldName]
        .map((station) => normalizeRouteStation(station))
        .filter(Boolean);
}

function collectLegRouteValues(flight, stationField, valueField) {
    const stations = [
        ...getLegStations(flight, 'inbound', stationField),
        ...getLegStations(flight, 'outbound', stationField),
    ];
    return Array.from(new Set(
        stations
            .map((station) => String(station[valueField] || '').trim())
            .filter(Boolean),
    ));
}

function normalizeFlightTypeLabel(value) {
    const raw = String(value || '').trim().toLowerCase();
    if (raw === 'intl' || raw === 'international') return '国际';
    if (raw === 'region') return '地区';
    if (raw === 'domestic') return '国内';
    return raw;
}

const FLIGHT_MISSION_LABELS = Object.freeze({
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
});

function normalizeMissionKey(rawMission) {
    const mission = String(rawMission || '').trim();
    if (!mission) {
        return '';
    }
    return mission
        .replace(/[／]/g, '/')
        .replace(/\s*\/\s*/g, '/')
        .replace(/\s+/g, ' ')
        .toUpperCase();
}

function parseMissionValue(rawMission) {
    const mission = String(rawMission || '').trim();
    if (!mission) {
        return {
            raw: '',
            key: '',
            label: '',
            suffix: '',
        };
    }
    const parts = mission.split(/[，,]/).map((part) => part.trim()).filter(Boolean);
    const [primary = '', ...suffixParts] = parts;
    const key = normalizeMissionKey(primary);
    return {
        raw: mission,
        key,
        label: FLIGHT_MISSION_LABELS[key] || '',
        suffix: suffixParts.join('，'),
    };
}

function collectMissionSearchTerms(rawMission) {
    const parsed = parseMissionValue(rawMission);
    return Array.from(new Set([
        parsed.raw,
        parsed.key,
        parsed.label,
        parsed.suffix,
    ].filter(Boolean)));
}

function collectRawMissionValues(flight) {
    return [getLegField(flight, 'inbound', 'mission'), getLegField(flight, 'outbound', 'mission')]
        .filter(Boolean);
}

/**
 * 过滤航班数据
 */
function filterFlights(flights, query, filters = {}) {
    const safeFlights = Array.isArray(flights) ? flights : [];
    const filteredByBusinessRules = applyBusinessFilters(safeFlights, filters);

    if (!query || !query.trim()) return filteredByBusinessRules;

    const normalizedQuery = query.toLowerCase().trim();

    // 默认搜索字段
    const searchFields = {
        flightNo: filters.searchFlightNo !== false,
        destination: filters.searchDestination !== false,
        destinationName: filters.searchDestinationName !== false,
        origin: filters.searchOrigin !== false,
        originName: filters.searchOriginName !== false,
        status: filters.searchStatus !== false,
        aircraftType: filters.searchAircraftType !== false,
        stand: filters.searchStand !== false,
        gate: filters.searchGate !== false,
        mission: filters.searchMission !== false,
        flightType: filters.searchFlightType !== false
    };

    return filteredByBusinessRules.filter(flight => {
        // 准备字段值
        const inboundFlightNo = getLegField(flight, 'inbound', 'flight_no').toLowerCase();
        const outboundFlightNo = getLegField(flight, 'outbound', 'flight_no').toLowerCase();

        const destinationRaw = collectLegRouteValues(flight, 'destination_stations', 'code');
        const destination = (Array.isArray(destinationRaw) ? destinationRaw.join(', ') : destinationRaw).toLowerCase();

        const destinationNameRaw = collectLegRouteValues(flight, 'destination_stations', 'name');
        const destinationName = (Array.isArray(destinationNameRaw) ? destinationNameRaw.join(', ') : destinationNameRaw).toLowerCase();

        const originRaw = collectLegRouteValues(flight, 'origin_stations', 'code');
        const origin = (Array.isArray(originRaw) ? originRaw.join(', ') : originRaw).toLowerCase();

        const originNameRaw = collectLegRouteValues(flight, 'origin_stations', 'name');
        const originName = (Array.isArray(originNameRaw) ? originNameRaw.join(', ') : originNameRaw).toLowerCase();

        const status = flight.status ? flight.status.toLowerCase() : '';
        const aircraftType = flight.aircraft_type_detail ? flight.aircraft_type_detail.toLowerCase() : '';
        const stand = flight.stand ? flight.stand.toLowerCase() : '';
        const gate = flight.gate ? flight.gate.toLowerCase() : '';
        const mission = collectRawMissionValues(flight)
            .flatMap((value) => collectMissionSearchTerms(value))
            .join(' ')
            .toLowerCase();
        const typeRaw = [
            normalizeFlightTypeLabel(getLegField(flight, 'inbound', 'flight_type')),
            normalizeFlightTypeLabel(getLegField(flight, 'outbound', 'flight_type')),
        ].filter(Boolean);
        const flightType = (Array.isArray(typeRaw) ? typeRaw.join(', ') : typeRaw).toLowerCase();

        // 匹配选中字段
        return (searchFields.flightNo && (inboundFlightNo.includes(normalizedQuery) || outboundFlightNo.includes(normalizedQuery))) ||
            (searchFields.destination && destination.includes(normalizedQuery)) ||
            (searchFields.destinationName && destinationName.includes(normalizedQuery)) ||
            (searchFields.origin && origin.includes(normalizedQuery)) ||
            (searchFields.originName && originName.includes(normalizedQuery)) ||
            (searchFields.status && status.includes(normalizedQuery)) ||
            (searchFields.aircraftType && aircraftType.includes(normalizedQuery)) ||
            (searchFields.stand && stand.includes(normalizedQuery)) ||
            (searchFields.gate && gate.includes(normalizedQuery)) ||
            (searchFields.mission && mission.includes(normalizedQuery)) ||
            (searchFields.flightType && flightType.includes(normalizedQuery));
    });
}

/**
 * 业务筛选（统一筛选：机型、签约、异常、延误、VIP、快速过站）
 */
function applyBusinessFilters(flights, filters = {}) {
    const bodyFilter = filters.aircraftBodyFilter || 'all';
    const signedFilter = filters.commercialSignedFilter || 'all';
    const anomalyFilter = filters.anomalyFilter || 'all';
    const delayFilter = filters.delayFilter || 'all';
    const vipFilter = filters.vipFilter || 'all';
    const quickTurnFilter = filters.quickTurnFilter || 'all';

    const allDefault = bodyFilter === 'all' && signedFilter === 'all'
        && anomalyFilter === 'all' && delayFilter === 'all'
        && vipFilter === 'all' && quickTurnFilter === 'all';
    if (allDefault) {
        return flights;
    }

    return flights.filter(flight => {
        if (bodyFilter !== 'all') {
            const isWideBody = isWideBodyAircraft(flight.aircraft_type_detail);
            if (bodyFilter === 'wide' && !isWideBody) return false;
            if (bodyFilter === 'narrow' && isWideBody) return false;
        }

        if (signedFilter !== 'all') {
            const signed = normalizeSignedValue(flight.is_commercial_signed ?? flight.commercial_signed);
            if (signedFilter === 'yes' && signed !== true) return false;
            if (signedFilter === 'no' && signed !== false) return false;
        }

        if (anomalyFilter === 'only' && !(Number(flight?.anomaly_count || 0) > 0)) return false;

        if (delayFilter === 'only' && !isDelayedFlight(flight)) return false;

        if (vipFilter === 'only' && !Boolean(flight?.inbound_leg?.is_vip || flight?.outbound_leg?.is_vip)) return false;

        if (quickTurnFilter === 'only' && !Boolean(flight?.is_quick_turnaround)) return false;

        return true;
    });
}

function isDelayedFlight(flight) {
    const status = String(flight?.status || '').toLowerCase();
    if (status.includes('延误')) {
        return true;
    }
    const pairs = [
        [flight?.estimated_departure, flight?.scheduled_departure],
        [flight?.estimated_arrival, flight?.scheduled_arrival],
    ];
    return pairs.some(([estimate, schedule]) => {
        if (!estimate || !schedule) {
            return false;
        }
        const diff = new Date(estimate).getTime() - new Date(schedule).getTime();
        return Number.isFinite(diff) && diff >= 15 * 60 * 1000;
    });
}

function isWideBodyAircraft(aircraftTypeDetail) {
    const code = String(aircraftTypeDetail || '').toUpperCase().replace(/[^A-Z0-9]/g, '');
    if (!code) {
        return false;
    }
    return /^(A330|A340|A350|A380|B747|B767|B777|B787)/.test(code);
}

function normalizeSignedValue(value) {
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
    // 后端默认签约为 true，字段缺失时按已签约处理
    return true;
}

/**
 * 排序航班数据
 */
function toSortableTimestamp(value) {
    if (value == null || value === '') {
        return 0;
    }

    if (typeof value === 'number' && Number.isFinite(value)) {
        return value > 1e12 ? value : value * 1000;
    }

    if (value instanceof Date) {
        const ts = value.getTime();
        return Number.isFinite(ts) ? ts : 0;
    }

    const raw = String(value).trim();
    if (!raw) {
        return 0;
    }

    if (/^\d+$/.test(raw)) {
        const numeric = Number(raw);
        if (Number.isFinite(numeric)) {
            return numeric > 1e12 ? numeric : numeric * 1000;
        }
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

function sortFlights(flights, sortConfig = { field: 'scheduled_departure', direction: 'asc' }) {
    const { field, direction } = sortConfig;
    const multiplier = direction === 'desc' ? -1 : 1;

    return [...flights].sort((a, b) => {
        let aVal = a[field];
        let bVal = b[field];

        // 处理日期字段
        if (field.includes('time') || field.includes('departure') || field.includes('arrival')) {
            aVal = toSortableTimestamp(aVal);
            bVal = toSortableTimestamp(bVal);
        }

        // 处理数组字段
        if (Array.isArray(aVal)) aVal = aVal.join(',');
        if (Array.isArray(bVal)) bVal = bVal.join(',');

        // 处理 null/undefined
        if (aVal == null && bVal == null) return 0;
        if (aVal == null) return 1 * multiplier;
        if (bVal == null) return -1 * multiplier;

        // 字符串比较
        if (typeof aVal === 'string' && typeof bVal === 'string') {
            return aVal.localeCompare(bVal, 'zh-CN') * multiplier;
        }

        // 数字比较
        return (aVal - bVal) * multiplier;
    });
}
