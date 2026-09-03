function formatDispatchNotifyDateTime(rawValue, fallback = '') {
    const value = String(rawValue || '').trim();
    if (!value) {
        return fallback;
    }
    const parsed = new Date(value);
    if (Number.isNaN(parsed.getTime())) {
        return value;
    }
    return parsed.toLocaleString('zh-CN', {
        hour12: false,
        month: '2-digit',
        day: '2-digit',
        hour: '2-digit',
        minute: '2-digit',
    });
}

async function loadDispatchNotifyReceiptGroup(receiptGroupId) {
    const normalized = String(receiptGroupId || '').trim();
    if (!normalized) {
        dispatchNotifyModalState.receiptGroup = null;
        renderDispatchNotifyReceiptGroup();
        return;
    }
    const response = await Auth.fetch(`${window.location.origin}/api/v2/notifications/receipt-groups/${encodeURIComponent(normalized)}`);
    const payload = await response.json().catch(() => ({}));
    if (!response.ok) {
        throw new Error(extractApiErrorMessage(payload, `加载回执情况失败 (HTTP ${response.status})`));
    }
    dispatchNotifyModalState.receiptGroup = payload;
    renderDispatchNotifyReceiptGroup();
}

function normalizeFlightId(value) {
    if (value === null || value === undefined) {
        return '';
    }
    return String(value);
}

function isSameFlightId(left, right) {
    const leftId = normalizeFlightId(left);
    const rightId = normalizeFlightId(right);
    return leftId !== '' && leftId === rightId;
}

function findFlightById(flightId) {
    const targetId = normalizeFlightId(flightId);
    if (!targetId) {
        return null;
    }
    return flights.find((flight) => normalizeFlightId(flight.flight_id) === targetId)
        || originalFlights.find((flight) => normalizeFlightId(flight.flight_id) === targetId)
        || null;
}

function updateDispatchNotifyReceiptControl() {
    const receiptRequiredInput = document.getElementById('dispatchNotifyReceiptRequiredInput');
    if (!(receiptRequiredInput instanceof HTMLInputElement)) {
        return;
    }
    receiptRequiredInput.disabled = false;
    receiptRequiredInput.title = '所有等级的通知都可按需开启确认回执';
}

function isCompactFlightMonitorLayout() {
    return window.innerWidth <= 1024;
}

function setDetailDrawerOpen(open) {
    isDetailDrawerOpen = !!open;
    const detailPanel = document.querySelector('.flight-detail-panel');
    if (detailPanel && isCompactFlightMonitorLayout()) {
        detailPanel.classList.toggle('detail-open', isDetailDrawerOpen);
    }
}

function getAirportDisplayValueV2(fieldMode) {
    if (fieldMode === 'name') {
        return airportContext.display_name || airportContext.code || '本站';
    }
    return airportContext.code || airportContext.display_name || '本站';
}

function normalizeRouteStationV2(rawStation) {
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

function normalizeRouteStationsV2(stations) {
    if (!Array.isArray(stations)) {
        return [];
    }
    const seen = new Set();
    const normalized = [];
    stations.forEach((station) => {
        const nextStation = normalizeRouteStationV2(station);
        if (!nextStation) {
            return;
        }
        const dedupeKey = `${nextStation.code}::${nextStation.name || ''}`;
        if (seen.has(dedupeKey)) {
            return;
        }
        seen.add(dedupeKey);
        normalized.push(nextStation);
    });
    return normalized;
}

function getLegStationsV2(flight, legType, fieldName) {
    const leg = getLegPayloadV2(flight, legType);
    if (!leg) {
        return [];
    }
    return normalizeRouteStationsV2(leg[fieldName]);
}

function getStationDisplayValueV2(station, fieldMode) {
    if (!station || typeof station !== 'object') {
        return '';
    }
    if (fieldMode === 'name') {
        return String(station.name || station.code || '').trim();
    }
    return String(station.code || station.name || '').trim();
}

function getStationListDisplayV2(flight, legType, fieldName, fieldMode) {
    return getLegStationsV2(flight, legType, fieldName)
        .map((station) => getStationDisplayValueV2(station, fieldMode))
        .filter(Boolean)
        .join(', ');
}

function parseLegPayloadV2(rawLeg, expectedLegType) {
    if (!rawLeg || typeof rawLeg !== 'object') {
        return null;
    }
    const next = { ...rawLeg };
    const normalizedType = String(next.leg_type || '').trim().toLowerCase();
    if (normalizedType !== expectedLegType) {
        return null;
    }
    next.leg_type = normalizedType;
    next.flight_no = String(next.flight_no || '').trim().toUpperCase();
    if (!next.flight_no) {
        return null;
    }
    next.flight_type = String(next.flight_type || 'domestic').trim().toLowerCase();
    next.origin_stations = normalizeRouteStationsV2(next.origin_stations);
    next.destination_stations = normalizeRouteStationsV2(next.destination_stations);
    return next;
}

function getLegPayloadV2(flight, legType) {
    if (!flight || typeof flight !== 'object') {
        return null;
    }
    const key = legType === 'inbound' ? 'inbound_leg' : 'outbound_leg';
    return parseLegPayloadV2(flight[key], legType);
}

function getLegFieldV2(flight, legType, fieldName) {
    const leg = getLegPayloadV2(flight, legType);
    if (!leg) {
        return '';
    }
    return String(leg[fieldName] || '').trim();
}

function normalizeFlightTypeLabelV2(rawType) {
    const type = String(rawType || '').trim().toLowerCase();
    if (type === 'intl' || type === 'international') {
        return '国际';
    }
    if (type === 'region') {
        return '地区';
    }
    if (type === 'domestic') {
        return '国内';
    }
    return '';
}

function getPrimaryFlightNoV2(flight) {
    const outbound = getLegFieldV2(flight, 'outbound', 'flight_no');
    const inbound = getLegFieldV2(flight, 'inbound', 'flight_no');
    return outbound || inbound || String(flight?.flight_number || flight?.flight_id || '').trim();
}

function getRouteDisplayTextV2(flight) {
    const inboundOrigin = getStationListDisplayV2(flight, 'inbound', 'origin_stations', 'name');
    const inboundDestination = getStationListDisplayV2(flight, 'inbound', 'destination_stations', 'name');
    const outboundOrigin = getStationListDisplayV2(flight, 'outbound', 'origin_stations', 'name');
    const outboundDestination = getStationListDisplayV2(flight, 'outbound', 'destination_stations', 'name');
    const inboundNo = getLegFieldV2(flight, 'inbound', 'flight_no');
    const outboundNo = getLegFieldV2(flight, 'outbound', 'flight_no');
    const airportName = getAirportDisplayValueV2('name');

    if (inboundNo && outboundNo) {
        return `${inboundOrigin || '-'} -> ${airportName} -> ${outboundDestination || '-'}`;
    }
    return `${inboundOrigin || outboundOrigin || '-'} -> ${outboundDestination || inboundDestination || '-'}`;
}

function normalizeFlightTypeCodeV2(rawType) {
    const type = String(rawType || '').trim().toLowerCase();
    if (type === 'intl' || type === 'international' || type === '国际') {
        return 'intl';
    }
    if (type === 'region' || type === '地区') {
        return 'region';
    }
    if (type === 'domestic' || type === '国内') {
        return 'domestic';
    }
    return 'domestic';
}

function getLegFlightTypeLabelV2(flight, legType) {
    const leg = getLegPayloadV2(flight, legType);
    if (!leg) {
        return '';
    }
    return normalizeFlightTypeLabelV2(leg.flight_type);
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

function parseMissionNumericInput(rawMission) {
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

function collectMissionSearchTerms(rawMission) {
    const parsed = parseMissionValue(rawMission);
    return Array.from(new Set([
        parsed.raw,
        parsed.key,
        parsed.label,
        parsed.suffix,
    ].filter(Boolean)));
}

function collectRawMissionValuesV2(flight) {
    return ['inbound', 'outbound']
        .map((legType) => {
            const leg = getLegPayloadV2(flight, legType);
            return leg ? String(leg.mission || '').trim() : '';
        })
        .filter(Boolean);
}

function getMissionSummaryV2(flight) {
    const missions = collectRawMissionValuesV2(flight)
        .map((mission) => formatMissionLabel(mission))
        .filter(Boolean);
    if (!missions.length) {
        return '';
    }
    return Array.from(new Set(missions)).join(' | ');
}

function getMissionSearchTextV2(flight) {
    const terms = collectRawMissionValuesV2(flight)
        .flatMap((mission) => collectMissionSearchTerms(mission));
    if (!terms.length) {
        return '';
    }
    return Array.from(new Set(terms)).join(' ');
}

function getFlightTypeSummaryV2(flight) {
    const types = [getLegFlightTypeLabelV2(flight, 'inbound'), getLegFlightTypeLabelV2(flight, 'outbound')]
        .filter(Boolean);
    if (!types.length) {
        return '';
    }
    return Array.from(new Set(types)).join('|');
}

function getLegVipFlagV2(flight, legType) {
    const leg = getLegPayloadV2(flight, legType);
    return Boolean(leg && leg.is_vip);
}

function getFlightNumberByLegV2(flight, legType) {
    return getLegFieldV2(flight, legType, 'flight_no');
}

function hasLegFlightV2(flight, legType) {
    return Boolean(getFlightNumberByLegV2(flight, legType));
}

function getFlightNumberDisplayV2(flight) {
    const inboundFlightNo = getFlightNumberByLegV2(flight, 'inbound');
    const outboundFlightNo = getFlightNumberByLegV2(flight, 'outbound');
    if (inboundFlightNo && outboundFlightNo) {
        return `${inboundFlightNo}|${outboundFlightNo}`;
    }
    return outboundFlightNo || inboundFlightNo || '';
}

function getFlightTypeLabelsV2(flight) {
    return {
        inbound: getLegFlightTypeLabelV2(flight, 'inbound'),
        outbound: getLegFlightTypeLabelV2(flight, 'outbound'),
    };
}

function getFlightNumberTextClassV2(flight, legType) {
    const isVip = getLegVipFlagV2(flight, legType);
    const flightType = getLegFlightTypeLabelV2(flight, legType);
    const isIntl = flightType === '国际' || flightType === '地区';

    if (isVip && isIntl) {
        return 'text-vip-intl';
    }
    if (isVip) {
        return 'text-vip';
    }
    if (isIntl) {
        return 'text-intl';
    }
    return '';
}

function getPrimaryFlightTypeLabelV2(flight) {
    const labels = getFlightTypeLabelsV2(flight);
    return labels.outbound || labels.inbound || '';
}

function getMissionInputValueV2(flight) {
    return collectRawMissionValuesV2(flight).join(',');
}

function getRouteEndpointV2(flight, legType, fieldMode) {
    const fieldName = legType === 'inbound' ? 'origin_stations' : 'destination_stations';
    return getStationListDisplayV2(flight, legType, fieldName, fieldMode);
}

function getRouteSummaryV2(flight, fieldMode) {
    const inboundNo = getLegFieldV2(flight, 'inbound', 'flight_no');
    const outboundNo = getLegFieldV2(flight, 'outbound', 'flight_no');
    const origin = getRouteEndpointV2(flight, 'inbound', fieldMode)
        || getStationListDisplayV2(flight, 'outbound', 'origin_stations', fieldMode);
    const destination = getRouteEndpointV2(flight, 'outbound', fieldMode)
        || getStationListDisplayV2(flight, 'inbound', 'destination_stations', fieldMode);
    const airportLabel = getAirportDisplayValueV2(fieldMode);
    if (inboundNo && outboundNo) {
        return `${origin || EMPTY_DISPLAY_TEXT} -> ${airportLabel} -> ${destination || EMPTY_DISPLAY_TEXT}`;
    }
    if (inboundNo) {
        return `${origin || EMPTY_DISPLAY_TEXT} -> ${airportLabel}`;
    }
    if (outboundNo) {
        return `${airportLabel} -> ${destination || EMPTY_DISPLAY_TEXT}`;
    }
    return `${origin || EMPTY_DISPLAY_TEXT} -> ${destination || EMPTY_DISPLAY_TEXT}`;
}

const DISPATCH_TIMELINE_FIELD_META = {
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
    cobt_time: { leg_type: 'outbound' },
};

const DISPATCH_TIMELINE_FIELDS = new Set(Object.keys(DISPATCH_TIMELINE_FIELD_META));

const dispatchTimelineCache = new Map();

function isDispatchTimelineField(field) {
    return DISPATCH_TIMELINE_FIELDS.has(String(field || '').trim());
}

function getTimelineFieldValueFromCache(flightId, field) {
    const cache = dispatchTimelineCache.get(normalizeFlightId(flightId));
    if (!cache || !cache.byMilestone) {
        return null;
    }
    const event = cache.byMilestone.get(String(field || '').trim());
    return event ? event.occurred_at || null : null;
}

function syncFlightTimelineFieldsFromCache(flight) {
    if (!flight || typeof flight !== 'object') {
        return;
    }
    const flightId = normalizeFlightId(flight.flight_id);
    if (!flightId) {
        return;
    }
    DISPATCH_TIMELINE_FIELDS.forEach((field) => {
        const cached = getTimelineFieldValueFromCache(flightId, field);
        if (cached) {
            flight[field] = cached;
        } else if (flight[field] === undefined) {
            flight[field] = null;
        }
    });
}

function updateDispatchTimelineCache(flightId, items) {
    const normalizedId = normalizeFlightId(flightId);
    const list = Array.isArray(items) ? items : [];
    const byMilestone = new Map();
    list.forEach((item) => {
        const code = String(item?.milestone_code || '').trim();
        if (!code) {
            return;
        }
        const existing = byMilestone.get(code);
        const occurredAt = item?.occurred_at ? new Date(item.occurred_at).getTime() : 0;
        const existingAt = existing?.occurred_at ? new Date(existing.occurred_at).getTime() : 0;
        if (!existing || occurredAt >= existingAt) {
            byMilestone.set(code, item);
        }
    });
    dispatchTimelineCache.set(normalizedId, { byMilestone, rawItems: list });

    const targets = [findFlightById(normalizedId), originalFlights.find((f) => isSameFlightId(f.flight_id, normalizedId))].filter(Boolean);
    targets.forEach((target) => {
        syncFlightTimelineFieldsFromCache(target);
        target._timesFormatted = false;
        preprocessFlightTimes(target);
    });
}

async function loadDispatchTimelineForFlight(flightId, force = false) {
    const normalizedId = normalizeFlightId(flightId);
    if (!normalizedId) {
        return [];
    }
    if (!force && dispatchTimelineCache.has(normalizedId)) {
        return dispatchTimelineCache.get(normalizedId).rawItems || [];
    }
    const response = await Auth.fetch(`${API_BASE}/flights/${encodeURIComponent(normalizedId)}/dispatch-timeline`);
    if (!response.ok) {
        throw new Error(`获取时间线失败 (${response.status})`);
    }
    const payload = await response.json();
    const items = payload?.data?.items || [];
    updateDispatchTimelineCache(normalizedId, items);
    return items;
}

function getLatestDispatchTimelineEvent(flightId, milestoneCode) {
    const cache = dispatchTimelineCache.get(normalizeFlightId(flightId));
    if (!cache || !cache.byMilestone) {
        return null;
    }
    return cache.byMilestone.get(String(milestoneCode || '').trim()) || null;
}

async function writeDispatchTimelineField(flightId, field, value) {
    const normalizedId = normalizeFlightId(flightId);
    if (!normalizedId) {
        throw new Error('航班标识缺失');
    }
    const milestoneCode = String(field || '').trim();
    const legType = DISPATCH_TIMELINE_FIELD_META[milestoneCode]?.leg_type || null;

    if (!value) {
        await loadDispatchTimelineForFlight(normalizedId);
        const existing = getLatestDispatchTimelineEvent(normalizedId, milestoneCode);
        if (!existing || !existing.timeline_id) {
            updateDispatchTimelineCache(normalizedId, (dispatchTimelineCache.get(normalizedId)?.rawItems || []).filter((item) => item?.milestone_code !== milestoneCode));
            return;
        }
        const response = await Auth.fetch(
            `${API_BASE}/flights/${encodeURIComponent(normalizedId)}/dispatch-timeline/events/${encodeURIComponent(existing.timeline_id)}`,
            { method: 'DELETE' },
        );
        if (!response.ok) {
            const errText = await response.text();
            throw new Error(errText || `撤销时间线失败 (${response.status})`);
        }
    } else {
        const body = {
            milestone_code: milestoneCode,
            occurred_at: value,
            leg_type: legType,
            source: 'flight_monitor_manual',
            payload: {},
        };
        const response = await Auth.fetch(`${API_BASE}/flights/${encodeURIComponent(normalizedId)}/dispatch-timeline/events`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(body),
        });
        if (!response.ok) {
            let message = `写入时间线失败 (${response.status})`;
            try {
                const err = await response.json();
                message = err?.detail || err?.message || message;
            } catch (_err) {
                const text = await response.text();
                if (text) {
                    message = text;
                }
            }
            throw new Error(message);
        }
    }

    await loadDispatchTimelineForFlight(normalizedId, true);
}

function compactDiagnosisText(value, maxLength) {
    if (value === null || value === undefined) {
        return '';
    }
    const raw = String(value).trim();
    if (!raw) {
        return '';
    }
    if (raw.length <= maxLength) {
        return raw;
    }
    return `${raw.slice(0, Math.max(0, maxLength - 1))}…`;
}

async function executeAITool(toolName, toolArgs) {
    const response = await Auth.fetch('/api/v2/ai/tools/execute', {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({
            tool_name: toolName,
            tool_args: toolArgs || {},
        }),
    });

    let payload = null;
    try {
        payload = await response.json();
    } catch (_error) {
        payload = null;
    }

    const semanticStatus = String(payload?.status || '').toLowerCase();
    const requestAccepted = Boolean(
        payload?.accepted === true
        || payload?.success === true
        || semanticStatus === 'pending_approval',
    );

    if (!response.ok || !payload || !requestAccepted) {
        const errorMessage = payload?.data?.error || payload?.detail || payload?.message || `请求失败 (${response.status})`;
        throw new Error(errorMessage);
    }

    if (semanticStatus === 'pending_approval' || payload?.approval_required === true) {
        return {
            pendingApproval: true,
            approvalId: String(payload?.approval_id || payload?.result_data?.action_id || '').trim(),
            payload,
            resultText: '',
        };
    }

    const resultPayload = payload?.result_data ?? payload?.data?.result;
    return {
        pendingApproval: false,
        approvalId: '',
        payload,
        resultText: buildDiagnosisResultText(resultPayload),
    };
}

function parseFlightDate(value) {
    if (!value) {
        return null;
    }
    const parsed = new Date(value);
    if (Number.isNaN(parsed.getTime())) {
        return null;
    }
    return parsed;
}

function resolveDelayMinutes(flight) {
    const estimatedDeparture = parseFlightDate(flight.estimated_departure || flight.time_dep_est || flight.estimated_departure_time_outbound);
    const scheduledDeparture = parseFlightDate(flight.scheduled_departure || flight.time_dep_sch || flight.scheduled_departure_time_outbound);
    if (!estimatedDeparture || !scheduledDeparture) {
        return null;
    }
    return Math.round((estimatedDeparture.getTime() - scheduledDeparture.getTime()) / 60000);
}

function resolveUrgencyByFlight(flight) {
    const delayMinutes = resolveDelayMinutes(flight);
    const status = String(flight.status || '').toLowerCase();

    if (delayMinutes !== null) {
        if (delayMinutes >= 60) {
            return '紧急';
        }
        if (delayMinutes >= 30) {
            return '高';
        }
    }

    if (status.includes('延误') || status.includes('异常') || status.includes('取消')) {
        return '高';
    }

    if (status.includes('登机')) {
        return '中';
    }

    return '低';
}

function setAIDiagnoseButtonLoading(isLoading) {
    const button = document.getElementById('aiDiagnoseBtn');
    if (!button) {
        return;
    }
    button.disabled = Boolean(isLoading);
    button.textContent = isLoading ? '诊断中...' : 'AI诊断';
    button.setAttribute('aria-busy', isLoading ? 'true' : 'false');
}

window.diagnoseSelectedFlight = diagnoseSelectedFlight;

function translateAICapabilityReason(reason) {
    switch (String(reason || '').trim()) {
        case 'NO_AI_CONFIG':
            return '未配置可用 AI 模型';
        case 'NO_AI_EXECUTE_PERMISSION':
            return '当前账号缺少 ai:execute 权限';
        case 'NO_AI_CHAT_PERMISSION':
            return '当前账号缺少 ai:chat 权限';
        default:
            return reason || 'AI 能力不可用';
    }
}

function getAICapabilityHintText() {
    if (!aiCapabilityState.loaded) {
        return '正在检查 AI 能力...';
    }
    if (aiCapabilityState.aiReady && aiCapabilityState.aiExecutePermission) {
        return '';
    }
    if (aiCapabilityState.error) {
        return `AI 能力检查失败: ${aiCapabilityState.error}`;
    }
    const reasons = (aiCapabilityState.missingReasons || []).map(translateAICapabilityReason);
    if (reasons.length > 0) {
        return `AI 不可用: ${reasons.join('；')}`;
    }
    return 'AI 能力不可用';
}

function isFlightInsightActionEnabled() {
    return Boolean(aiCapabilityState.loaded && aiCapabilityState.aiReady && aiCapabilityState.aiExecutePermission);
}

function isFlightChatActionEnabled() {
    return Boolean(aiCapabilityState.loaded && aiCapabilityState.aiReady && aiCapabilityState.aiChatPermission);
}

function getAIChatCapabilityHintText() {
    if (!aiCapabilityState.loaded) {
        return '正在检查 AI 能力...';
    }
    if (isFlightChatActionEnabled()) {
        return '';
    }
    if (aiCapabilityState.error) {
        return `AI 能力检查失败: ${aiCapabilityState.error}`;
    }

    const reasons = [];
    if (!aiCapabilityState.aiReady) {
        reasons.push('NO_AI_CONFIG');
    }
    if (!aiCapabilityState.aiChatPermission) {
        reasons.push('NO_AI_CHAT_PERMISSION');
    }
    return reasons.length > 0
        ? `AI 对话不可用: ${reasons.map(translateAICapabilityReason).join('；')}`
        : 'AI 对话不可用';
}

function setFlightInsightButtonLoading(kind, isLoading) {
    const key = kind === 'journey' ? 'journey' : 'history';
    flightInsightLoadingState[key] = Boolean(isLoading);

    const historyBtn = document.getElementById('generateHistoryReportBtn');
    const journeyBtn = document.getElementById('generateEventJourneyBtn');

    if (historyBtn) {
        historyBtn.disabled = !isFlightInsightActionEnabled() || !selectedFlightId || flightInsightLoadingState.history || flightInsightLoadingState.journey;
        historyBtn.textContent = flightInsightLoadingState.history ? '生成中...' : '生成动态报表';
    }

    if (journeyBtn) {
        journeyBtn.disabled = !isFlightInsightActionEnabled() || !selectedFlightId || flightInsightLoadingState.history || flightInsightLoadingState.journey;
        journeyBtn.textContent = flightInsightLoadingState.journey ? '生成中...' : '生成事件经过';
    }
}

function updateBusinessInsightActionState() {
    const historyBtn = document.getElementById('generateHistoryReportBtn');
    const journeyBtn = document.getElementById('generateEventJourneyBtn');
    const chatBtn = document.getElementById('openFlightChatBadgeBtn');
    const hintEl = document.getElementById('businessAiCapabilityHint');

    const actionEnabled = isFlightInsightActionEnabled();
    const chatEnabled = isFlightChatActionEnabled();
    const hasFlightSelected = Boolean(selectedFlightId);
    const shouldDisable = !actionEnabled || !hasFlightSelected || flightInsightLoadingState.history || flightInsightLoadingState.journey;

    if (historyBtn) {
        historyBtn.disabled = shouldDisable;
    }
    if (journeyBtn) {
        journeyBtn.disabled = shouldDisable;
    }
    if (chatBtn) {
        chatBtn.disabled = !chatEnabled;
        chatBtn.setAttribute('aria-disabled', chatEnabled ? 'false' : 'true');
        const hintText = chatEnabled ? '' : getAIChatCapabilityHintText();
        if (hintText) {
            chatBtn.title = hintText;
        } else {
            chatBtn.removeAttribute('title');
        }
    }

    if (hintEl) {
        const hintText = !actionEnabled ? getAICapabilityHintText() : (!hasFlightSelected ? '请先在左侧选择航班' : '');
        if (hintText) {
            hintEl.hidden = false;
            hintEl.textContent = hintText;
        } else {
            hintEl.hidden = true;
            hintEl.textContent = '';
        }
    }

    updateFlightChatMeta();
    syncDispatchNotifyModalWithContext();
}

function getCurrentPermissionSet() {
    const user = (window.Auth && typeof Auth.getUser === 'function') ? Auth.getUser() : null;
    const rawPermissions = Array.isArray(user?.permissions) ? user.permissions : [];
    return new Set(
        rawPermissions
            .map((permission) => String(permission || '').trim().toLowerCase())
            .filter(Boolean),
    );
}

function canViewDispatchNotifications() {
    const user = (window.Auth && typeof Auth.getUser === 'function') ? Auth.getUser() : null;
    if (user?.is_admin) {
        return true;
    }

    const permissions = getCurrentPermissionSet();
    return permissions.has('dispatch:view') || permissions.has('dispatch:manage');
}

function canManageDispatchNotifications() {
    const user = (window.Auth && typeof Auth.getUser === 'function') ? Auth.getUser() : null;
    if (user?.is_admin) {
        return true;
    }
    return getCurrentPermissionSet().has('dispatch:manage');
}

function hasPendingDispatchReceiptNotifications() {
    return dispatchNotifyModalState.pendingReceipts.some((item) => String(item?.ack_status || '').trim().toLowerCase() === 'pending');
}

function hasSentDispatchReceiptGroups() {
    return dispatchNotifyModalState.sentReceiptGroups.length > 0;
}

function getAiFloatButtonMetrics() {
    const floatButton = document.querySelector('.flight-monitor-page .ant-float-btn');
    if (!(floatButton instanceof HTMLElement) || !isFloatingBadgeVisible(floatButton)) {
        return null;
    }

    const rect = floatButton.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) {
        return null;
    }

    return {
        bottom: Math.max(12, Math.round(window.innerHeight - rect.bottom)),
        height: Math.max(0, Math.round(rect.height)),
    };
}

function updateDispatchNotifyEntryState() {
    const badgeBtn = document.getElementById('dispatchNotifyBadgeBtn');
    const canView = canViewDispatchNotifications();
    const canManage = canManageDispatchNotifications();
    const hasPendingReceipts = hasPendingDispatchReceiptNotifications();
    const hasSentReceipts = hasSentDispatchReceiptGroups();
    const canOpen = canView || hasPendingReceipts || hasSentReceipts;

    const applyState = (button, visibleDisplay) => {
        if (!(button instanceof HTMLButtonElement)) {
            return;
        }
        if (!canOpen) {
            button.style.display = 'none';
            button.setAttribute('aria-hidden', 'true');
            return;
        }

        button.style.display = visibleDisplay;
        button.removeAttribute('aria-hidden');
        button.disabled = false;
        if (hasPendingReceipts && !canManage) {
            button.title = `打开调度通知中心，处理 ${dispatchNotifyModalState.pendingReceipts.length} 条待确认回执`;
        } else if (hasSentReceipts && !canManage) {
            button.title = `打开调度通知中心，查看 ${dispatchNotifyModalState.sentReceiptGroups.length} 个已发回执批次`;
        } else if (canManage) {
            button.title = '向在线账号发送调度通知';
        } else {
            button.title = '打开调度通知中心';
        }
        button.setAttribute('aria-disabled', 'false');
    };

    applyState(badgeBtn, 'inline-flex');
    syncFloatingBadgeLayout();
}

function resolveDispatchNotifyFlightContext() {
    const selectedFlight = getSelectedFlightForInsight();
    if (!selectedFlight) {
        return {
            flightId: '',
            flightNo: '',
            label: '当前未选择航班，将按全局调度通知发送。',
        };
    }

    const flightNo = getPrimaryFlightNoV2(selectedFlight);
    const flightId = String(selectedFlight.flight_id || '').trim();
    return {
        flightId,
        flightNo,
        label: `当前航班上下文: ${flightNo || flightId || '--'}（发送时会自动附带航班标识）`,
    };
}

function setDispatchNotifyLoadingUsers(isLoading) {
    dispatchNotifyModalState.loadingUsers = Boolean(isLoading);
    const reloadBtn = document.getElementById('dispatchNotifyReloadUsersBtn');
    const selectAllBtn = document.getElementById('dispatchNotifySelectAllBtn');
    const clearSelectBtn = document.getElementById('dispatchNotifyClearSelectBtn');
    const searchInput = document.getElementById('dispatchNotifySearchInput');
    if (reloadBtn) {
        reloadBtn.disabled = dispatchNotifyModalState.loadingUsers;
        reloadBtn.textContent = dispatchNotifyModalState.loadingUsers ? '刷新中...' : '刷新';
    }
    if (selectAllBtn) {
        selectAllBtn.disabled = dispatchNotifyModalState.loadingUsers;
    }
    if (clearSelectBtn) {
        clearSelectBtn.disabled = dispatchNotifyModalState.loadingUsers;
    }
    if (searchInput) {
        searchInput.disabled = dispatchNotifyModalState.loadingUsers;
    }
    renderDispatchNotifyUserList();
    updateDispatchNotifySendState();
}

function setDispatchNotifySending(isSending) {
    dispatchNotifyModalState.sending = Boolean(isSending);
    const sendBtn = document.getElementById('dispatchNotifySendBtn');
    if (sendBtn) {
        sendBtn.disabled = dispatchNotifyModalState.sending;
        sendBtn.textContent = dispatchNotifyModalState.sending ? '发送中...' : '发送通知';
    }
    updateDispatchNotifySendState();
}

function normalizeDispatchNotifyUser(item) {
    if (!item || typeof item !== 'object') {
        return null;
    }
    const userId = String(item.user_id || item.id || '').trim();
    if (!userId) {
        return null;
    }
    return {
        user_id: userId,
        username: String(item.username || userId).trim() || userId,
        job_title: String(item.job_title || '').trim(),
        department: String(item.department || '').trim(),
        status: String(item.status || 'online').trim().toLowerCase() || 'online',
        login_time: String(item.login_time || '').trim(),
        last_heartbeat: String(item.last_heartbeat || '').trim(),
    };
}

function getDispatchNotifyUserStatusMeta(rawStatus) {
    const normalized = String(rawStatus || 'online').trim().toLowerCase() || 'online';
    const statusMetaMap = {
        online: { tone: 'online', label: '在线' },
        active: { tone: 'online', label: '活跃' },
        idle: { tone: 'idle', label: '空闲' },
        busy: { tone: 'busy', label: '忙碌' },
        away: { tone: 'away', label: '离开' },
        offline: { tone: 'offline', label: '离线' },
    };
    const mapped = statusMetaMap[normalized];
    if (mapped) {
        return mapped;
    }
    return {
        tone: 'default',
        label: normalized,
    };
}

function formatDispatchNotifyUserTime(rawValue) {
    const value = String(rawValue || '').trim();
    if (!value) {
        return '';
    }
    const parsed = new Date(value);
    if (Number.isNaN(parsed.getTime())) {
        return value;
    }
    return parsed.toLocaleString('zh-CN', {
        hour12: false,
        month: '2-digit',
        day: '2-digit',
        hour: '2-digit',
        minute: '2-digit',
    });
}

function toggleDispatchNotifyUserSelection(userId, selected) {
    if (!userId) {
        return;
    }
    if (selected) {
        dispatchNotifyModalState.selectedUserIds.add(userId);
    } else {
        dispatchNotifyModalState.selectedUserIds.delete(userId);
    }
    renderDispatchNotifyUserList();
    renderDispatchNotifySelectedUsers();
    updateDispatchNotifySendState();
}

function focusDispatchNotifySelectionArea() {
    const firstVisibleUser = document.querySelector('#dispatchOnlineUserList .dispatch-online-user-item');
    if (firstVisibleUser instanceof HTMLElement) {
        firstVisibleUser.focus();
        return;
    }
    const searchInput = document.getElementById('dispatchNotifySearchInput');
    if (searchInput instanceof HTMLElement) {
        searchInput.focus();
        return;
    }
    const selectAllBtn = document.getElementById('dispatchNotifySelectAllBtn');
    if (selectAllBtn instanceof HTMLElement) {
        selectAllBtn.focus();
    }
}

function getDispatchNotifySendValidationState() {
    const titleInput = document.getElementById('dispatchNotifyTitleInput');
    const bodyInput = document.getElementById('dispatchNotifyBodyInput');
    const selectedCount = dispatchNotifyModalState.selectedUserIds.size;
    const title = String(titleInput?.value || '').trim();
    const body = String(bodyInput?.value || '').trim();

    if (!canManageDispatchNotifications()) {
        return {
            valid: false,
            code: 'no_permission',
            message: '当前账号缺少 dispatch:manage 权限',
            tone: 'error',
            focusTarget: null,
        };
    }
    if (dispatchNotifyModalState.sending) {
        return {
            valid: false,
            code: 'sending',
            message: '正在发送通知...',
            tone: 'info',
            focusTarget: '#dispatchNotifySendBtn',
        };
    }
    if (dispatchNotifyModalState.loadingUsers) {
        return {
            valid: false,
            code: 'loading_users',
            message: '在线账号加载中，暂不可发送',
            tone: 'warning',
            focusTarget: '#dispatchNotifyReloadUsersBtn',
        };
    }
    if (dispatchNotifyModalState.loadError && selectedCount <= 0) {
        return {
            valid: false,
            code: 'load_error',
            message: dispatchNotifyModalState.loadError,
            tone: 'error',
            focusTarget: '#dispatchNotifyReloadUsersBtn',
        };
    }
    if (selectedCount <= 0) {
        return {
            valid: false,
            code: 'missing_targets',
            message: '请先选择至少一个在线账号',
            tone: 'warning',
            focusTarget: '#dispatchNotifySearchInput',
        };
    }
    if (!title) {
        return {
            valid: false,
            code: 'missing_title',
            message: '请输入通知标题',
            tone: 'warning',
            focusTarget: '#dispatchNotifyTitleInput',
        };
    }
    if (!body) {
        return {
            valid: false,
            code: 'missing_body',
            message: '请输入通知正文',
            tone: 'warning',
            focusTarget: '#dispatchNotifyBodyInput',
        };
    }
    return {
        valid: true,
        code: 'ready',
        message: '按 Enter 发送，正文可用 Shift+Enter 换行',
        tone: 'info',
        focusTarget: null,
    };
}

function updateDispatchNotifyStatusHint() {
    const hintEl = document.getElementById('dispatchNotifyStatusHint');
    if (!hintEl) {
        return;
    }

    let message = '消息将按账号定向推送到 SSE 统一消息流。';
    let tone = 'info';
    if (dispatchNotifyModalState.activeTab === 'pending') {
        message = '收到需回执的通知后，可在这里统一确认或拒绝。';
    } else if (dispatchNotifyModalState.activeTab === 'sent') {
        message = '实时查看本人已发送批次的确认回执与超时提醒。';
    } else {
        const validationState = getDispatchNotifySendValidationState();
        message = validationState.message;
        tone = validationState.tone;
    }

    hintEl.textContent = message;
    hintEl.className = `dispatch-notify-tip dispatch-notify-footer-tip is-${tone}`;
}

function focusDispatchNotifyValidationTarget(validationState) {
    if (!validationState?.focusTarget) {
        return;
    }
    if (validationState.code === 'missing_targets') {
        focusDispatchNotifySelectionArea();
        return;
    }
    const target = document.querySelector(validationState.focusTarget);
    if (target instanceof HTMLElement) {
        target.focus();
    }
}

function validateDispatchNotifyBeforeSend(options = {}) {
    const { showFeedback = false } = options;
    const validationState = getDispatchNotifySendValidationState();
    if (validationState.valid) {
        return validationState;
    }
    if (showFeedback) {
        const toastType = validationState.tone === 'error'
            ? 'error'
            : (validationState.tone === 'info' ? 'info' : 'warning');
        showToast(validationState.message, toastType);
        focusDispatchNotifyValidationTarget(validationState);
    }
    return validationState;
}

async function handleDispatchNotifyEditorKeydown(event) {
    if (!(event?.target instanceof HTMLElement)) {
        return;
    }
    if (dispatchNotifyModalState.activeTab !== 'send' || event.key !== 'Enter') {
        return;
    }
    if (event.isComposing || event.keyCode === 229) {
        return;
    }
    if (event.target.id === 'dispatchNotifyBodyInput' && event.shiftKey) {
        return;
    }
    event.preventDefault();
    await sendDispatchNotification();
}

function updateDispatchNotifySendState() {
    const sendBtn = document.getElementById('dispatchNotifySendBtn');
    if (!sendBtn) {
        return;
    }

    const validationState = getDispatchNotifySendValidationState();
    sendBtn.disabled = dispatchNotifyModalState.sending;
    sendBtn.title = validationState.valid ? '发送通知' : validationState.message;
    sendBtn.setAttribute('aria-describedby', 'dispatchNotifyStatusHint');
    sendBtn.setAttribute('aria-disabled', validationState.valid ? 'false' : 'true');
    updateDispatchNotifyStatusHint();
}

async function loadDispatchNotifyOnlineUsers(options = {}) {
    const { preserveSelection = true } = options;
    setDispatchNotifyLoadingUsers(true);
    try {
        const query = new URLSearchParams({ limit: '300' });
        const response = await Auth.fetch(`${DISPATCH_NOTIFY_API_BASE}/online-users?${query.toString()}`);
        const payload = await response.json().catch(() => ({}));
        if (!response.ok || !payload?.success) {
            throw new Error(extractApiErrorMessage(payload, `加载在线账号失败 (HTTP ${response.status})`));
        }

        const items = Array.isArray(payload?.data?.items) ? payload.data.items : [];
        dispatchNotifyModalState.users = items
            .map((item) => normalizeDispatchNotifyUser(item))
            .filter(Boolean);
        dispatchNotifyModalState.loadedOnce = true;
        dispatchNotifyModalState.loadError = '';

        if (preserveSelection) {
            const validUserIds = new Set(dispatchNotifyModalState.users.map((item) => item.user_id));
            dispatchNotifyModalState.selectedUserIds = new Set(
                Array.from(dispatchNotifyModalState.selectedUserIds).filter((userId) => validUserIds.has(userId)),
            );
        } else {
            dispatchNotifyModalState.selectedUserIds = new Set();
        }
    } catch (error) {
        dispatchNotifyModalState.loadError = error?.message || '加载在线账号失败';
        if (!dispatchNotifyModalState.loadedOnce) {
            dispatchNotifyModalState.users = [];
            dispatchNotifyModalState.filteredUsers = [];
        }
        showToast(dispatchNotifyModalState.loadError, 'error', 4200);
    } finally {
        setDispatchNotifyLoadingUsers(false);
        applyDispatchNotifyUserFilter();
        renderDispatchNotifySelectedUsers();
        updateDispatchNotifySendState();
    }
}

async function sendDispatchNotification() {
    const validationState = validateDispatchNotifyBeforeSend({ showFeedback: true });
    if (!validationState.valid) {
        return;
    }

    const selectedUserIds = Array.from(dispatchNotifyModalState.selectedUserIds);

    const titleInput = document.getElementById('dispatchNotifyTitleInput');
    const bodyInput = document.getElementById('dispatchNotifyBodyInput');
    const severitySelect = document.getElementById('dispatchNotifySeveritySelect');
    const title = String(titleInput?.value || '').trim();
    const body = String(bodyInput?.value || '').trim();
    const severity = String(severitySelect?.value || 'warning').trim().toLowerCase();

    const context = resolveDispatchNotifyFlightContext();
    const requestBody = {
        recipient_user_ids: selectedUserIds,
        title,
        body,
        severity: ['info', 'warning', 'critical'].includes(severity) ? severity : 'warning',
    };
    if (context.flightId) {
        requestBody.flight_id = context.flightId;
    }
    if (context.flightNo) {
        requestBody.flight_no = context.flightNo;
    }
    const receiptRequiredInput = document.getElementById('dispatchNotifyReceiptRequiredInput');
    requestBody.receipt_required = !(receiptRequiredInput instanceof HTMLInputElement) || receiptRequiredInput.checked;

    setDispatchNotifySending(true);
    try {
        const response = await Auth.fetch(`${DISPATCH_NOTIFY_API_BASE}/send`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(requestBody),
        });
        const payload = await response.json().catch(() => ({}));
        if (!response.ok || !payload?.success) {
            throw new Error(extractApiErrorMessage(payload, `发送通知失败 (HTTP ${response.status})`));
        }

        const sentCount = Number(payload?.data?.sent_count || 0);
        const failedCount = Number(payload?.data?.failed_count || 0);
        const receiptGroupId = String(payload?.data?.receipt_group_id || '').trim();
        showToast(
            failedCount > 0
                ? `通知发送完成: 成功 ${sentCount}，失败 ${failedCount}`
                : `通知发送成功，共 ${sentCount} 人`,
            failedCount > 0 ? 'warning' : 'success',
            3800,
        );

        if (receiptGroupId) {
            try {
                const groupPayload = await refreshSentReceiptGroupStatus(receiptGroupId);
                dispatchNotifyModalState.receiptGroup = groupPayload;
                renderDispatchNotifyReceiptGroup();
            } catch (refreshError) {
                console.warn('刷新已发回执缓存失败:', refreshError);
                scheduleSentReceiptReminderRetry(receiptGroupId);
                if (!dispatchNotifyModalState.sentReceiptGroupsLoading) {
                    void loadDispatchNotifySentReceiptGroups({ preserveSelection: true }).catch((loadError) => {
                        console.warn('刷新已发回执列表失败:', loadError);
                    });
                }
            }
        }

        if (failedCount <= 0) {
            if (bodyInput) {
                bodyInput.value = '';
            }
            dispatchNotifyModalState.selectedUserIds = new Set();
            renderDispatchNotifyUserList();
            renderDispatchNotifySelectedUsers();
            if (receiptGroupId) {
                try {
                    await selectDispatchNotifySentReceiptGroup(receiptGroupId);
                } catch (refreshError) {
                    console.warn('刷新已发回执详情失败:', refreshError);
                }
            }
            const modal = document.getElementById('dispatchNotifyModal');
            const entryButton = document.getElementById('dispatchNotifyBadgeBtn');
            if (entryButton instanceof HTMLElement) {
                activeModalRestoreTarget = entryButton;
            }
            if (modal) {
                closeManagedModal(modal);
            }
        }
    } catch (error) {
        showToast(error?.message || '发送通知失败', 'error', 4200);
    } finally {
        setDispatchNotifySending(false);
        updateDispatchNotifySendState();
    }
}

function normalizeDispatchPendingReceipt(item) {
    const normalized = normalizeNotificationItem(item);
    if (!normalized || !normalized.receipt_required || normalized.ack_status !== 'pending') {
        return null;
    }
    return normalized;
}

function normalizeSentReceiptGroup(item) {
    if (!item || typeof item !== 'object') {
        return null;
    }
    const receiptGroupId = String(item.receipt_group_id || '').trim();
    if (!receiptGroupId) {
        return null;
    }
    return {
        receipt_group_id: receiptGroupId,
        title: String(item.title || '未命名通知').trim() || '未命名通知',
        severity: String(item.severity || 'info').trim().toLowerCase() || 'info',
        origin_type: String(item.origin_type || 'manual').trim().toLowerCase() || 'manual',
        flight_id: String(item.flight_id || '').trim() || null,
        dispatch_order_id: String(item.dispatch_order_id || '').trim() || null,
        group_id: String(item.group_id || '').trim() || null,
        created_at: String(item.created_at || '').trim() || null,
        latest_updated_at: String(item.latest_updated_at || '').trim() || null,
        remind_after_at: String(item.remind_after_at || '').trim() || null,
        is_overdue: Boolean(item.is_overdue),
        total_count: Number(item.total_count || 0),
        pending_count: Number(item.pending_count || 0),
        acknowledged_count: Number(item.acknowledged_count || 0),
        rejected_count: Number(item.rejected_count || 0),
    };
}

async function setDispatchNotifyActiveTab(tabName) {
    const normalized = ['send', 'pending', 'sent'].includes(tabName) ? tabName : 'send';
    const nextTab = normalized === 'send' && !canManageDispatchNotifications()
        ? (hasPendingDispatchReceiptNotifications() ? 'pending' : 'sent')
        : normalized;
    dispatchNotifyModalState.activeTab = nextTab;

    document.querySelectorAll('[data-tab]').forEach((button) => {
        const isActive = button.dataset.tab === nextTab;
        button.classList.toggle('is-active', isActive);
        button.setAttribute('aria-selected', isActive ? 'true' : 'false');
    });
    document.querySelectorAll('[data-tab-panel]').forEach((panel) => {
        const isActive = panel.getAttribute('data-tab-panel') === nextTab;
        panel.classList.toggle('is-active', isActive);
        panel.hidden = !isActive;
    });

    const sendBtn = document.getElementById('dispatchNotifySendBtn');
    if (sendBtn) {
        sendBtn.style.display = nextTab === 'send' ? '' : 'none';
    }

    if (nextTab === 'pending' && !dispatchNotifyModalState.pendingReceiptsLoaded) {
        await loadDispatchNotifyPendingReceipts();
    }
    if (nextTab === 'sent' && !dispatchNotifyModalState.sentReceiptGroupsLoaded) {
        await loadDispatchNotifySentReceiptGroups({ preserveSelection: true });
    }
    updateDispatchNotifySendState();
}

async function loadDispatchNotifyPendingReceipts() {
    dispatchNotifyModalState.pendingReceiptsLoading = true;
    dispatchNotifyModalState.pendingReceiptsError = '';
    renderDispatchNotifyPendingReceipts();
    try {
        const response = await Auth.fetch(`${window.location.origin}/api/v2/notifications?limit=100&offset=0`);
        const payload = await response.json().catch(() => ({}));
        if (!response.ok) {
            throw new Error(extractApiErrorMessage(payload, `加载待确认回执失败 (HTTP ${response.status})`));
        }
        const items = Array.isArray(payload?.items) ? payload.items : [];
        dispatchNotifyModalState.pendingReceipts = items
            .map((item) => normalizeDispatchPendingReceipt(item))
            .filter(Boolean)
            .sort((left, right) => {
                const leftTime = new Date(left.created_at || 0).getTime();
                const rightTime = new Date(right.created_at || 0).getTime();
                return rightTime - leftTime;
            });
        dispatchNotifyModalState.pendingReceiptsLoaded = true;
    } catch (error) {
        dispatchNotifyModalState.pendingReceiptsError = error?.message || '加载待确认回执失败';
    } finally {
        dispatchNotifyModalState.pendingReceiptsLoading = false;
        renderDispatchNotifyPendingReceipts();
        updateDispatchNotifyEntryState();
    }
}

function removeSentReceiptReminderQueue(receiptGroupId) {
    const normalized = String(receiptGroupId || '').trim();
    if (!normalized) {
        return;
    }
    dispatchNotifyModalState.sentReceiptReminderQueue = dispatchNotifyModalState.sentReceiptReminderQueue
        .filter((item) => item !== normalized);
}

function clearSentReceiptReminderRetry(receiptGroupId) {
    const normalized = String(receiptGroupId || '').trim();
    if (!normalized) {
        return;
    }
    const retryTimer = dispatchNotifyModalState.sentReceiptReminderRetryTimers.get(normalized);
    if (retryTimer) {
        clearTimeout(retryTimer);
        dispatchNotifyModalState.sentReceiptReminderRetryTimers.delete(normalized);
    }
    dispatchNotifyModalState.sentReceiptReminderAttemptCounts.delete(normalized);
}

function scheduleSentReceiptReminderRetry(receiptGroupId) {
    const normalized = String(receiptGroupId || '').trim();
    if (!normalized) {
        return;
    }
    const nextAttempt = Number(dispatchNotifyModalState.sentReceiptReminderAttemptCounts.get(normalized) || 0) + 1;
    if (nextAttempt > SENT_RECEIPT_REMINDER_RETRY_MAX_ATTEMPTS) {
        dispatchNotifyModalState.sentReceiptReminderAttemptCounts.delete(normalized);
        return;
    }
    const existing = dispatchNotifyModalState.sentReceiptReminderRetryTimers.get(normalized);
    if (existing) {
        clearTimeout(existing);
    }
    dispatchNotifyModalState.sentReceiptReminderAttemptCounts.set(normalized, nextAttempt);
    const retryDelay = Math.min(
        SENT_RECEIPT_REMINDER_RETRY_MAX_MS,
        SENT_RECEIPT_REMINDER_RETRY_BASE_MS * (2 ** (nextAttempt - 1)),
    );
    const retryTimer = window.setTimeout(() => {
        dispatchNotifyModalState.sentReceiptReminderRetryTimers.delete(normalized);
        void queueSentReceiptReminder(normalized);
    }, retryDelay);
    dispatchNotifyModalState.sentReceiptReminderRetryTimers.set(normalized, retryTimer);
}

function scheduleDispatchNotifyReminderPresentRetry() {
    if (dispatchNotifyModalState.sentReceiptReminderPresentRetryTimer) {
        return;
    }
    dispatchNotifyModalState.sentReceiptReminderPresentRetryTimer = window.setTimeout(() => {
        dispatchNotifyModalState.sentReceiptReminderPresentRetryTimer = null;
        void presentDispatchNotifyReminderIfNeeded();
    }, SENT_RECEIPT_REMINDER_PRESENT_RETRY_MS);
}

function applySentReceiptGroupSnapshot(payload) {
    if (!payload || typeof payload !== 'object') {
        return null;
    }
    const receiptGroupId = String(payload.receipt_group_id || '').trim();
    if (!receiptGroupId) {
        return null;
    }
    const summary = payload.summary && typeof payload.summary === 'object' ? payload.summary : {};
    const nextGroup = normalizeSentReceiptGroup({
        receipt_group_id: receiptGroupId,
        title: payload.title,
        severity: payload.severity,
        origin_type: payload.origin_type,
        flight_id: payload.flight_id,
        dispatch_order_id: payload.dispatch_order_id,
        group_id: payload.group_id,
        created_at: payload.created_at,
        latest_updated_at: summary.latest_updated_at,
        remind_after_at: payload.remind_after_at,
        is_overdue: payload.is_overdue,
        total_count: summary.total_count,
        pending_count: summary.pending_count,
        acknowledged_count: summary.acknowledged_count,
        rejected_count: summary.rejected_count,
    });
    if (!nextGroup) {
        return null;
    }

    let matched = false;
    dispatchNotifyModalState.sentReceiptGroups = dispatchNotifyModalState.sentReceiptGroups.map((item) => {
        if (item.receipt_group_id !== receiptGroupId) {
            return item;
        }
        matched = true;
        return {
            ...item,
            ...nextGroup,
        };
    });
    if (!matched) {
        dispatchNotifyModalState.sentReceiptGroups.unshift(nextGroup);
    }
    scheduleSentReceiptReminder(nextGroup);
    if (Number(nextGroup.pending_count || 0) <= 0) {
        removeSentReceiptReminderQueue(receiptGroupId);
        clearSentReceiptReminderRetry(receiptGroupId);
    }
    if (dispatchNotifyModalState.selectedSentReceiptGroupId === receiptGroupId) {
        dispatchNotifyModalState.sentReceiptGroupDetail = payload;
    }
    syncDispatchNotifyReminderModal(nextGroup);
    renderDispatchNotifySentReceiptGroups();
    updateDispatchNotifyEntryState();
    return nextGroup;
}

async function refreshSentReceiptGroupStatus(receiptGroupId) {
    const normalized = String(receiptGroupId || '').trim();
    if (!normalized) {
        return null;
    }
    const response = await Auth.fetch(`${window.location.origin}/api/v2/notifications/receipt-groups/${encodeURIComponent(normalized)}`);
    const payload = await response.json().catch(() => ({}));
    if (!response.ok) {
        throw new Error(extractApiErrorMessage(payload, `加载回执详情失败 (HTTP ${response.status})`));
    }
    applySentReceiptGroupSnapshot(payload);
    return payload;
}

function scheduleSentReceiptReminder(group) {
    const receiptGroupId = String(group?.receipt_group_id || '').trim();
    if (!receiptGroupId) {
        return;
    }
    const existing = dispatchNotifyModalState.sentReceiptReminderTimers.get(receiptGroupId);
    if (existing) {
        clearTimeout(existing);
        dispatchNotifyModalState.sentReceiptReminderTimers.delete(receiptGroupId);
    }
    if (Number(group?.pending_count || 0) <= 0) {
        return;
    }
    const remindAfterText = String(group?.remind_after_at || '').trim();
    const remindAt = remindAfterText ? new Date(remindAfterText).getTime() : Number.NaN;
    if (Number.isNaN(remindAt)) {
        return;
    }
    const reminderKey = `dispatchNotifyReminderShown:${receiptGroupId}`;
    if (window.sessionStorage?.getItem(reminderKey) === '1') {
        return;
    }
    const now = Date.now();
    if (remindAt <= now) {
        void queueSentReceiptReminder(receiptGroupId);
        return;
    }
    const timer = window.setTimeout(() => {
        dispatchNotifyModalState.sentReceiptReminderTimers.delete(receiptGroupId);
        void queueSentReceiptReminder(receiptGroupId);
    }, Math.max(0, (remindAt + SENT_RECEIPT_REMINDER_GRACE_MS) - now));
    dispatchNotifyModalState.sentReceiptReminderTimers.set(receiptGroupId, timer);
}

async function queueSentReceiptReminder(receiptGroupId) {
    const normalized = String(receiptGroupId || '').trim();
    if (!normalized) {
        return;
    }
    const reminderKey = `dispatchNotifyReminderShown:${normalized}`;
    if (window.sessionStorage?.getItem(reminderKey) === '1') {
        return;
    }
    try {
        const payload = await refreshSentReceiptGroupStatus(normalized);
        const pendingCount = Number(payload?.summary?.pending_count || 0);
        if (pendingCount <= 0) {
            clearSentReceiptReminderRetry(normalized);
            return;
        }
        clearSentReceiptReminderRetry(normalized);
    } catch (error) {
        console.warn('校验超时回执状态失败:', error);
        scheduleSentReceiptReminderRetry(normalized);
        return;
    }
    if (!dispatchNotifyModalState.sentReceiptReminderQueue.includes(normalized)) {
        dispatchNotifyModalState.sentReceiptReminderQueue.push(normalized);
    }
    void presentDispatchNotifyReminderIfNeeded();
}

async function presentDispatchNotifyReminderIfNeeded() {
    if (activeModal && activeModal.id && activeModal.id !== 'dispatchNotifyReminderModal' && activeModal.id !== 'dispatchNotifyModal') {
        scheduleDispatchNotifyReminderPresentRetry();
        return;
    }
    const receiptGroupId = dispatchNotifyModalState.sentReceiptReminderQueue.shift();
    if (!receiptGroupId) {
        return;
    }
    let payload;
    try {
        payload = await refreshSentReceiptGroupStatus(receiptGroupId);
        clearSentReceiptReminderRetry(receiptGroupId);
    } catch (error) {
        console.warn('刷新超时回执状态失败:', error);
        scheduleSentReceiptReminderRetry(receiptGroupId);
        return;
    }
    const latestPendingCount = Number(payload?.summary?.pending_count || 0);
    if (latestPendingCount <= 0) {
        return;
    }
    const group = dispatchNotifyModalState.sentReceiptGroups.find((item) => item.receipt_group_id === receiptGroupId);
    if (!group || Number(group.pending_count || 0) <= 0) {
        return;
    }
    const reminderKey = `dispatchNotifyReminderShown:${receiptGroupId}`;
    if (window.sessionStorage?.getItem(reminderKey) === '1') {
        return;
    }
    window.sessionStorage?.setItem(reminderKey, '1');
    const modal = ensureDispatchNotifyReminderModal();
    modal.setAttribute('data-receipt-group-id', receiptGroupId);
    renderDispatchNotifyReminderBody(group);
    openManagedModal(modal, '#dispatchNotifyReminderViewBtn');
}

async function selectDispatchNotifySentReceiptGroup(receiptGroupId) {
    const normalized = String(receiptGroupId || '').trim();
    dispatchNotifyModalState.selectedSentReceiptGroupId = normalized;
    renderDispatchNotifySentReceiptGroups();
    if (!normalized) {
        dispatchNotifyModalState.sentReceiptGroupDetail = null;
        renderDispatchNotifySentReceiptDetail();
        return;
    }
    const response = await Auth.fetch(`${window.location.origin}/api/v2/notifications/receipt-groups/${encodeURIComponent(normalized)}`);
    const payload = await response.json().catch(() => ({}));
    if (!response.ok) {
        throw new Error(extractApiErrorMessage(payload, `加载回执详情失败 (HTTP ${response.status})`));
    }
    applySentReceiptGroupSnapshot(payload);
}

async function loadDispatchNotifySentReceiptGroups(options = {}) {
    const { preserveSelection = true } = options;
    dispatchNotifyModalState.sentReceiptGroupsLoading = true;
    dispatchNotifyModalState.sentReceiptGroupsError = '';
    renderDispatchNotifySentReceiptGroups();
    try {
        const response = await Auth.fetch(`${window.location.origin}/api/v2/notifications/sent-receipt-groups?limit=50&offset=0`);
        const payload = await response.json().catch(() => ({}));
        if (!response.ok) {
            throw new Error(extractApiErrorMessage(payload, `加载已发回执失败 (HTTP ${response.status})`));
        }
        dispatchNotifyModalState.sentReceiptGroups = (Array.isArray(payload?.items) ? payload.items : [])
            .map((item) => normalizeSentReceiptGroup(item))
            .filter(Boolean);
        dispatchNotifyModalState.sentReceiptGroupsLoaded = true;
        if (preserveSelection) {
            const selected = dispatchNotifyModalState.sentReceiptGroups.find((item) => item.receipt_group_id === dispatchNotifyModalState.selectedSentReceiptGroupId);
            dispatchNotifyModalState.selectedSentReceiptGroupId = selected
                ? selected.receipt_group_id
                : String(dispatchNotifyModalState.sentReceiptGroups[0]?.receipt_group_id || '');
        } else {
            dispatchNotifyModalState.selectedSentReceiptGroupId = String(dispatchNotifyModalState.sentReceiptGroups[0]?.receipt_group_id || '');
        }
        dispatchNotifyModalState.sentReceiptGroups.forEach((group) => {
            scheduleSentReceiptReminder(group);
        });
        if (dispatchNotifyModalState.selectedSentReceiptGroupId) {
            await selectDispatchNotifySentReceiptGroup(dispatchNotifyModalState.selectedSentReceiptGroupId);
        } else {
            dispatchNotifyModalState.sentReceiptGroupDetail = null;
        }
    } catch (error) {
        dispatchNotifyModalState.sentReceiptGroupsError = error?.message || '加载已发回执失败';
        dispatchNotifyModalState.sentReceiptGroupDetail = null;
    } finally {
        dispatchNotifyModalState.sentReceiptGroupsLoading = false;
        renderDispatchNotifySentReceiptGroups();
        updateDispatchNotifyEntryState();
    }
}

function normalizeNotificationItem(rawItem) {
    if (!rawItem || typeof rawItem !== 'object') {
        return null;
    }

    const notificationId = String(rawItem.notification_id || '').trim();
    if (!notificationId) {
        return null;
    }

    const severity = String(rawItem.severity || 'info').trim().toLowerCase() || 'info';
    const category = String(rawItem.category || 'system').trim().toLowerCase() || 'system';
    const relatedEntityType = String(rawItem.related_entity_type || '').trim().toLowerCase() || null;
    const relatedEntityId = String(rawItem.related_entity_id || '').trim() || null;
    const ackStatus = String(rawItem.ack_status || 'pending').trim().toLowerCase() || 'pending';
    const flightId = String(rawItem.flight_id || '').trim() || (relatedEntityType === 'flight' ? relatedEntityId : '');
    const timestamp = String(rawItem.created_at || rawItem.delivered_at || rawItem.timestamp || '').trim();
    const parsedTimestamp = timestamp ? new Date(timestamp) : null;

    return {
        notification_id: notificationId,
        user_id: String(rawItem.user_id || '').trim(),
        title: String(rawItem.title || '通知').trim() || '通知',
        body: String(rawItem.body || '').trim(),
        category,
        severity,
        is_read: Boolean(rawItem.is_read),
        read_status: String(rawItem.read_status || (rawItem.is_read ? 'read' : 'unread')).trim().toLowerCase() || 'unread',
        delivery_status: String(rawItem.delivery_status || 'sent').trim().toLowerCase() || 'sent',
        delivered_at: String(rawItem.delivered_at || '').trim() || null,
        origin_type: String(rawItem.origin_type || 'manual').trim().toLowerCase() || 'manual',
        receipt_required: Boolean(rawItem.receipt_required),
        receipt_group_id: String(rawItem.receipt_group_id || '').trim() || null,
        ack_status: ackStatus,
        ack_at: String(rawItem.ack_at || '').trim() || null,
        ack_note: String(rawItem.ack_note || '').trim() || null,
        sender_user_id: String(rawItem.sender_user_id || '').trim() || null,
        sender_username: String(rawItem.sender_username || '').trim() || null,
        related_entity_type: relatedEntityType,
        related_entity_id: relatedEntityId,
        flight_id: flightId || null,
        created_at: timestamp || null,
        timestamp: parsedTimestamp && !Number.isNaN(parsedTimestamp.getTime())
            ? parsedTimestamp.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', second: '2-digit' })
            : new Date().toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', second: '2-digit' }),
    };
}

function getNotificationSeverityMeta(severity) {
    const normalized = String(severity || 'info').trim().toLowerCase();
    if (normalized === 'critical') {
        return { label: 'CRITICAL', color: '#b91c1c', background: 'rgba(254, 226, 226, 0.95)' };
    }
    if (normalized === 'warning' || normalized === 'high') {
        return { label: 'WARNING', color: '#b45309', background: 'rgba(254, 243, 199, 0.95)' };
    }
    return { label: 'INFO', color: '#0f766e', background: 'rgba(204, 251, 241, 0.95)' };
}

function getNotificationFlightLabel(notification) {
    const relatedFlightId = String(notification?.flight_id || notification?.related_entity_id || '').trim();
    if (!relatedFlightId) {
        return '';
    }
    const matchedFlight = findFlightById(relatedFlightId);
    const flightNo = matchedFlight ? getPrimaryFlightNoV2(matchedFlight) : '';
    return flightNo || relatedFlightId;
}

function isPendingCriticalNotification(notification) {
    if (!notification) {
        return false;
    }
    return notification.severity === 'critical' && notification.ack_status === 'pending';
}

function pushUpdatePanelMessage(entry, countAsUnread = true) {
    updateMessages.unshift(entry);
    if (updateMessages.length > 100) {
        updateMessages.length = 100;
    }

    if (!isPanelOpen && countAsUnread) {
        unreadCount += 1;
        updateBadge();
    }
    renderUpdatePanel();
}

function addNotificationUpdateMessage(notification, options = {}) {
    const { countAsUnread = true } = options;
    const entryId = `notification:${notification.notification_id}`;
    if (notificationPanelEntryIds.has(entryId)) {
        return;
    }
    notificationPanelEntryIds.add(entryId);
    const relatedFlightLabel = getNotificationFlightLabel(notification);
    pushUpdatePanelMessage({
        id: entryId,
        kind: 'user_notification',
        time: notification.timestamp,
        title: notification.title,
        body: notification.body,
        severity: notification.severity,
        originType: notification.origin_type,
        relatedFlightLabel,
        notificationId: notification.notification_id,
        isRead: Boolean(notification.is_read || notification.read_status === 'read'),
        receiptRequired: notification.receipt_required,
        ackStatus: notification.ack_status,
    }, countAsUnread);
}

function ensureNotificationToastHost() {
    let host = document.getElementById('flightMonitorNotificationToastHost');
    if (host) {
        return host;
    }
    host = document.createElement('div');
    host.id = 'flightMonitorNotificationToastHost';
    host.setAttribute('aria-live', 'polite');
    host.style.position = 'fixed';
    host.style.top = '88px';
    host.style.right = '20px';
    host.style.display = 'flex';
    host.style.flexDirection = 'column';
    host.style.gap = '12px';
    host.style.width = 'min(420px, calc(100vw - 24px))';
    host.style.zIndex = '1400';
    host.style.pointerEvents = 'none';
    document.body.appendChild(host);
    return host;
}

async function markNotificationAsRead(notificationId) {
    const response = await Auth.fetch(`${window.location.origin}/api/v2/notifications/${encodeURIComponent(notificationId)}/read`, {
        method: 'POST',
    });
    const payload = await response.json().catch(() => ({}));
    if (!response.ok || !payload?.success) {
        throw new Error(extractApiErrorMessage(payload, `通知已读失败 (HTTP ${response.status})`));
    }
}

async function acknowledgeNotificationReceipt(notificationId, action, note = '') {
    const response = await Auth.fetch(`${window.location.origin}/api/v2/notifications/${encodeURIComponent(notificationId)}/ack`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
            action,
            note: String(note || '').trim() || undefined,
        }),
    });
    const payload = await response.json().catch(() => ({}));
    if (!response.ok || !payload?.success) {
        throw new Error(extractApiErrorMessage(payload, `通知处理失败 (HTTP ${response.status})`));
    }
    return payload?.data || null;
}

function removeNotificationToast(notificationId) {
    const host = document.getElementById('flightMonitorNotificationToastHost');
    const node = host?.querySelector(`[data-notification-id="${notificationId}"]`);
    if (node) {
        node.remove();
    }
    notificationToastIds.delete(notificationId);
}

async function dismissNotificationToast(notification) {
    const toastEl = document.querySelector(`[data-notification-id="${notification.notification_id}"]`);
    if (toastEl instanceof HTMLElement) {
        toastEl.style.opacity = '0.6';
    }
    try {
        await markNotificationAsRead(notification.notification_id);
        removeNotificationToast(notification.notification_id);
        announce(`已处理通知：${notification.title}`);
    } catch (error) {
        if (toastEl instanceof HTMLElement) {
            toastEl.style.opacity = '1';
        }
        showToast(error?.message || '通知已读失败', 'error', 4200);
    }
}

function showStandardNotificationToast(notification) {
    if (!notification || notificationToastIds.has(notification.notification_id)) {
        return;
    }

    notificationToastIds.add(notification.notification_id);
    const host = ensureNotificationToastHost();
    const severityMeta = getNotificationSeverityMeta(notification.severity);
    const relatedFlightLabel = getNotificationFlightLabel(notification);
    const card = document.createElement('section');
    card.dataset.notificationId = notification.notification_id;
    card.style.pointerEvents = 'auto';
    card.style.borderRadius = '16px';
    card.style.padding = '14px 16px';
    card.style.background = 'rgba(15, 23, 42, 0.94)';
    card.style.color = '#f8fafc';
    card.style.boxShadow = '0 18px 40px rgba(15, 23, 42, 0.28)';
    card.style.border = `1px solid ${severityMeta.color}`;
    card.innerHTML = `
        <div style="display:flex;align-items:flex-start;justify-content:space-between;gap:12px;">
            <div style="display:flex;flex-direction:column;gap:8px;min-width:0;">
                <div style="display:flex;align-items:center;gap:8px;flex-wrap:wrap;">
                    <span style="display:inline-flex;align-items:center;padding:2px 8px;border-radius:999px;font-size:11px;font-weight:700;letter-spacing:0.06em;background:${severityMeta.background};color:${severityMeta.color};">${severityMeta.label}</span>
                    ${renderOriginBadge(notification.origin_type)}
                    ${relatedFlightLabel ? `<span style="font-size:12px;color:#cbd5e1;">航班 ${escapeHtml(relatedFlightLabel)}</span>` : ''}
                </div>
                <div style="font-size:15px;font-weight:700;line-height:1.4;">${escapeHtml(notification.title)}</div>
                <div style="font-size:13px;line-height:1.6;color:#e2e8f0;white-space:pre-wrap;">${escapeHtml(notification.body || '暂无正文')}</div>
                <div style="font-size:12px;color:#94a3b8;">${escapeHtml(notification.timestamp)}</div>
                ${notification.receipt_required && notification.ack_status === 'pending' && notification.severity !== 'critical' ? `
                    <div style="display:flex;gap:8px;flex-wrap:wrap;">
                        <button type="button" data-role="open-receipt" style="border:none;border-radius:999px;background:rgba(59,130,246,0.18);color:#dbeafe;padding:7px 12px;font-size:12px;cursor:pointer;">去确认</button>
                    </div>
                ` : ''}
            </div>
            <button type="button" data-role="dismiss" style="border:none;background:transparent;color:#f8fafc;font-size:18px;cursor:pointer;line-height:1;padding:0 2px;" aria-label="关闭通知">×</button>
        </div>
    `;

    const dismissBtn = card.querySelector('[data-role="dismiss"]');
    if (dismissBtn) {
        dismissBtn.addEventListener('click', async () => {
            await dismissNotificationToast(notification);
        });
    }

    const openReceiptBtn = card.querySelector('[data-role="open-receipt"]');
    if (openReceiptBtn) {
        openReceiptBtn.addEventListener('click', async () => {
            try {
                await markNotificationAsRead(notification.notification_id);
            } catch (_error) {
                // ignore read failures here; user can still enter pending tab
            }
            removeNotificationToast(notification.notification_id);
            const modal = ensureDispatchNotifyModal();
            openManagedModal(modal, '#dispatchNotifyTabPendingBtn');
            await setDispatchNotifyActiveTab('pending');
            if (!dispatchNotifyModalState.pendingReceiptsLoaded) {
                await loadDispatchNotifyPendingReceipts();
            }
        });
    }

    host.prepend(card);
}

function presentNextCriticalNotification() {
    if (activeCriticalNotificationId) {
        return;
    }
    if (activeModal && activeModal.id !== 'criticalNotificationModal') {
        return;
    }
    const nextNotification = pendingCriticalNotifications.shift();
    if (!nextNotification) {
        return;
    }
    openCriticalNotificationModal(nextNotification);
}

function enqueueCriticalNotification(notification) {
    if (!notification || notificationCriticalQueueIds.has(notification.notification_id) || activeCriticalNotificationId === notification.notification_id) {
        return;
    }
    notificationCriticalQueueIds.add(notification.notification_id);
    pendingCriticalNotifications.push(notification);
    presentNextCriticalNotification();
}

function upsertDispatchPendingReceipt(notification) {
    if (!notification || !notification.receipt_required || notification.ack_status !== 'pending') {
        return;
    }
    const nextItems = dispatchNotifyModalState.pendingReceipts
        .filter((item) => item.notification_id !== notification.notification_id);
    nextItems.unshift(notification);
    dispatchNotifyModalState.pendingReceipts = nextItems
        .sort((left, right) => new Date(right.created_at || 0).getTime() - new Date(left.created_at || 0).getTime());
    renderDispatchNotifyPendingReceipts();
    updateDispatchNotifyEntryState();
}

function applySenderReceiptUpdate(payload) {
    const receiptGroupId = String(payload?.receipt_group_id || '').trim();
    if (!receiptGroupId) {
        return;
    }
    const recipientLabel = String(payload?.recipient_username || payload?.recipient_user_id || '对方').trim() || '对方';
    const ackStatus = String(payload?.ack_status || '').trim().toLowerCase();
    const title = String(payload?.title || '通知').trim() || '通知';
    if (ackStatus === 'acknowledged') {
        showToast(`${recipientLabel} 已确认《${title}》`, 'success', 3200);
    } else if (ackStatus === 'rejected') {
        const suffix = payload?.ack_note ? `：${String(payload.ack_note).trim()}` : '';
        showToast(`${recipientLabel} 已拒绝《${title}》${suffix}`, 'warning', 4200);
    }

    const summary = payload?.summary && typeof payload.summary === 'object' ? payload.summary : {};
    let matchedGroup = false;
    dispatchNotifyModalState.sentReceiptGroups = dispatchNotifyModalState.sentReceiptGroups.map((item) => {
        if (item.receipt_group_id !== receiptGroupId) {
            return item;
        }
        matchedGroup = true;
        const nextGroup = {
            ...item,
            title: String(payload?.title || item.title || '通知').trim() || item.title,
            severity: String(payload?.severity || item.severity || 'info').trim().toLowerCase() || 'info',
            origin_type: String(payload?.origin_type || item.origin_type || 'manual').trim().toLowerCase() || 'manual',
            flight_id: String(payload?.flight_id || item.flight_id || '').trim() || item.flight_id,
            total_count: Number(summary.total_count ?? item.total_count ?? 0),
            pending_count: Number(summary.pending_count ?? item.pending_count ?? 0),
            acknowledged_count: Number(summary.acknowledged_count ?? item.acknowledged_count ?? 0),
            rejected_count: Number(summary.rejected_count ?? item.rejected_count ?? 0),
            latest_updated_at: String(summary.latest_updated_at || item.latest_updated_at || '').trim() || item.latest_updated_at,
            remind_after_at: String(summary.remind_after_at || item.remind_after_at || '').trim() || item.remind_after_at,
            is_overdue: Boolean(summary.is_overdue ?? item.is_overdue),
        };
        scheduleSentReceiptReminder(nextGroup);
        return nextGroup;
    });
    if (!matchedGroup && !dispatchNotifyModalState.sentReceiptGroupsLoading) {
        void loadDispatchNotifySentReceiptGroups({ preserveSelection: true }).catch((error) => {
            console.warn('刷新已发回执列表失败:', error);
        });
    }
    renderDispatchNotifySentReceiptGroups();
    updateDispatchNotifyEntryState();
    if (dispatchNotifyModalState.selectedSentReceiptGroupId === receiptGroupId) {
        void selectDispatchNotifySentReceiptGroup(receiptGroupId).catch((error) => {
            console.warn('刷新已发回执详情失败:', error);
        });
    }
}

function processNotificationForFlightMonitor(rawNotification, options = {}) {
    const { fromInitial = false } = options;
    const notification = normalizeNotificationItem(rawNotification);
    if (!notification) {
        return;
    }
    if (notificationSeenIds.has(notification.notification_id)) {
        return;
    }
    notificationSeenIds.add(notification.notification_id);

    const isUnread = !notification.is_read && notification.read_status !== 'read';
    if (isUnread) {
        addNotificationUpdateMessage(notification, { countAsUnread: true });
    }
    if (notification.receipt_required && notification.ack_status === 'pending') {
        upsertDispatchPendingReceipt(notification);
    }

    if (isPendingCriticalNotification(notification)) {
        enqueueCriticalNotification(notification);
        return;
    }

    if (!fromInitial && isUnread) {
        showStandardNotificationToast(notification);
    }
}

function handleNotificationInitialPayload(payload) {
    const items = Array.isArray(payload?.items) ? payload.items : [];
    items.forEach((item) => {
        const normalized = normalizeNotificationItem(item);
        if (!normalized) {
            return;
        }
        if (
            !normalized.is_read
            || isPendingCriticalNotification(normalized)
            || (normalized.receipt_required && normalized.ack_status === 'pending')
        ) {
            processNotificationForFlightMonitor(normalized, { fromInitial: true });
        } else if (!notificationSeenIds.has(normalized.notification_id)) {
            notificationSeenIds.add(normalized.notification_id);
        }
    });
}

function handleRealtimeNotificationPayload(payload) {
    if (!payload) {
        return;
    }
    if (payload.type === 'sender_receipt_update') {
        applySenderReceiptUpdate(payload);
        return;
    }
    if (payload.type !== 'user_notification') {
        return;
    }
    processNotificationForFlightMonitor(payload.notification || payload, { fromInitial: false });
}

function scheduleNotificationReconnect() {
    // No-op: reconnection is handled by SSEHub
}

function closeNotificationStream() {
    // No-op: SSEHub manages the connection lifecycle
}

async function connectToNotificationStream() {
    if (!await Auth.requireAuthAsync()) {
        return;
    }

    function handleNotificationStreamEvent(event) {
        try {
            var payload = JSON.parse(event.data);
            // Skip heartbeat / connected payloads
            if (payload && (payload.type === 'connected' || payload.type === 'heartbeat')) {
                return;
            }
            handleRealtimeNotificationPayload(payload);
        } catch (error) {
            console.error('Notification payload parse error:', error, 'raw=', event.data && event.data.substring && event.data.substring(0, 300));
        }
    }

    // Register notification event listeners on the shared SSEHub.
    // The backend automatically adds user_notifications_{uid} to the connection's
    // topic subscriptions, so we just listen for the events it pushes.
    SSEHub.on('message', handleNotificationStreamEvent);
    SSEHub.on('user_notification', handleNotificationStreamEvent);
    SSEHub.on('sender_receipt_update', handleNotificationStreamEvent);

    // Load initial unread notifications via REST (SSEHub doesn't send initial snapshots)
    loadInitialNotifications();
}

async function loadInitialNotifications() {
    try {
        const response = await Auth.fetch('/api/v2/notifications?unread_only=true&limit=50');
        if (response.ok) {
            const payload = await response.json();
            handleNotificationInitialPayload(payload);
        }
    } catch (error) {
        console.warn('获取初始通知失败:', error);
    }
}

async function loadAICapabilities() {
    try {
        const response = await Auth.fetch('/api/v2/ai/capabilities');
        const payload = await response.json();

        if (!response.ok || !payload?.success) {
            throw new Error(payload?.detail || payload?.message || `HTTP ${response.status}`);
        }

        const data = payload.data || {};
        aiCapabilityState = {
            loaded: true,
            aiReady: Boolean(data.ai_ready),
            aiExecutePermission: Boolean(data.ai_execute_permission),
            aiChatPermission: Boolean(data.ai_chat_permission),
            missingReasons: Array.isArray(data.missing_reasons) ? data.missing_reasons : [],
            error: '',
        };
    } catch (error) {
        aiCapabilityState = {
            loaded: true,
            aiReady: false,
            aiExecutePermission: false,
            aiChatPermission: false,
            missingReasons: ['NO_AI_CONFIG', 'NO_AI_EXECUTE_PERMISSION', 'NO_AI_CHAT_PERMISSION'],
            error: error?.message || '能力探测失败',
        };
    } finally {
        updateBusinessInsightActionState();
    }
}

function getSelectedFlightForInsight() {
    if (selectedFlightId === null || selectedFlightId === undefined) {
        return null;
    }
    return flights.find((item) => String(item.flight_id) === String(selectedFlightId)) || null;
}

function extractApiErrorMessage(payload, fallbackMessage) {
    const detail = payload?.detail;
    if (typeof detail === 'string' && detail.trim()) {
        return detail;
    }
    if (detail && typeof detail === 'object') {
        const code = detail.code ? `[${detail.code}] ` : '';
        const msg = detail.message || detail.detail || '';
        if (msg) {
            return `${code}${msg}`;
        }
    }
    if (typeof payload?.message === 'string' && payload.message.trim()) {
        return payload.message;
    }
    return fallbackMessage;
}

function downloadInsightContent(content, filename, mimeType = 'text/plain;charset=utf-8') {
    const blob = new Blob([content], { type: mimeType });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = filename;
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
    URL.revokeObjectURL(url);
}

function resolveSelectedFlightNo() {
    const flight = getSelectedFlightForInsight();
    if (!flight) {
        return '';
    }
    return getPrimaryFlightNoV2(flight);
}

function updateFlightChatMeta() {
    const conversationMetaEl = document.getElementById('flightChatConversationMeta');
    const scopeMetaEl = document.getElementById('flightChatScopeMeta');
    if (conversationMetaEl) {
        conversationMetaEl.textContent = flightChatConversationId
            ? `会话: ${flightChatConversationId}`
            : '会话: 新会话';
    }

    if (scopeMetaEl) {
        if (selectedFlightId) {
            const flightNo = resolveSelectedFlightNo();
            const flightLabel = flightNo || String(selectedFlightId);
            scopeMetaEl.textContent = `范围: 优先选中航班（${flightLabel}），无选中时走全局`;
        } else {
            scopeMetaEl.textContent = '范围: 全局（未选中航班时请提供航班号）';
        }
    }
}

function setFlightChatSending(isSending) {
    flightChatSending = Boolean(isSending);
    const sendBtn = document.getElementById('flightChatSendBtn');
    const inputEl = document.getElementById('flightChatInput');
    if (sendBtn) {
        sendBtn.disabled = flightChatSending;
        sendBtn.textContent = flightChatSending ? '发送中...' : '发送';
    }
    if (inputEl) {
        inputEl.disabled = flightChatSending;
    }
}

function clearFlightChatMessages(options = {}) {
    const { silent = false } = options;
    const messagesEl = document.getElementById('flightChatMessages');
    if (!messagesEl) {
        return;
    }

    resetFlightChatActiveStreamState();
    messagesEl.innerHTML = '';
    flightChatInsightPayloads.clear();

    if (!silent) {
        appendFlightChatMessage(
            'assistant',
            '已清空当前聊天窗口。可继续提问，我会自动识别当前是否选中了航班。',
        );
    } else {
        appendFlightChatMessage(
            'assistant',
            '你好，我可以帮你查询航班信息、生成动态报表和事件经过。若已选中航班，会优先使用该航班作为上下文。',
        );
    }
}

function appendFlightChatMessage(role, text) {
    const messagesEl = document.getElementById('flightChatMessages');
    if (!messagesEl) {
        return null;
    }

    const bubble = document.createElement('div');
    bubble.className = `ai-chat-message ${role === 'user' ? 'user' : 'assistant'}`;

    const textEl = document.createElement('div');
    textEl.className = 'ai-chat-message-text';
    textEl.textContent = String(text || '-');
    bubble.appendChild(textEl);

    messagesEl.appendChild(bubble);
    messagesEl.scrollTop = messagesEl.scrollHeight;
    return bubble;
}

function generateClientRequestId() {
    if (window.crypto && typeof window.crypto.randomUUID === 'function') {
        return window.crypto.randomUUID();
    }
    return `req_${Date.now()}_${Math.random().toString(16).slice(2, 10)}`;
}

function normalizeFlightChatExecutionStatus(rawStatus) {
    const normalized = String(rawStatus || '').trim().toLowerCase();
    if (!normalized) return 'in_progress';
    if (normalized === 'completed' || normalized === 'done') return 'success';
    if (normalized === 'failed' || normalized === 'cancelled') return 'error';
    return normalized;
}

function isFlightChatTerminalExecutionStatus(status) {
    const normalized = normalizeFlightChatExecutionStatus(status);
    return [
        'success',
        'error',
        'timeout',
        'not_found',
        'permission_denied',
        'validation_error',
    ].includes(normalized);
}

function stopFlightChatExecutionPolling() {
    if (flightChatExecutionPollTimer) {
        window.clearInterval(flightChatExecutionPollTimer);
        flightChatExecutionPollTimer = null;
    }
    flightChatLastPollFingerprint = '';
}

async function pollFlightChatExecutionStatusOnce() {
    if (!flightChatActiveExecutionId || !flightChatActiveProgressPanel) {
        return;
    }
    try {
        const response = await Auth.fetch(`/api/v2/ai/executions/${encodeURIComponent(flightChatActiveExecutionId)}`);
        if (!response.ok) {
            return;
        }
        const payload = await response.json().catch(() => ({}));
        const execution = payload?.data || {};
        const status = normalizeFlightChatExecutionStatus(execution.status);
        const code = String(execution.code || '');
        const message = String(execution.message || execution.error_message || '').trim();
        const statusMeta = window.AIStatusMapper
            ? window.AIStatusMapper.getStatusMeta(status, code)
            : { tone: 'info', label: status || '处理中' };

        const fingerprint = `${status}|${code}|${message}`;
        if (fingerprint && fingerprint !== flightChatLastPollFingerprint) {
            appendFlightChatProgress(
                `[轮询] ${statusMeta.label || status}${message ? ` - ${message}` : ''}`,
                statusMeta.tone || 'info',
            );
            flightChatLastPollFingerprint = fingerprint;
        }

        if (isFlightChatTerminalExecutionStatus(status)) {
            stopFlightChatExecutionPolling();
        }
    } catch (_error) {
        // 网络波动时保持轮询即可
    }
}

function startFlightChatExecutionPolling() {
    if (flightChatExecutionPollTimer || !flightChatActiveExecutionId) {
        return;
    }
    pollFlightChatExecutionStatusOnce();
    flightChatExecutionPollTimer = window.setInterval(() => {
        pollFlightChatExecutionStatusOnce();
    }, FLIGHT_CHAT_EXECUTION_POLL_INTERVAL_MS);
}

function ensureFlightChatEventStream() {
    if (flightChatEventSource) {
        return;
    }

    // Mark as initialized to prevent duplicate registration
    flightChatEventSource = { _sseHub: true };

    SSEHub.on('ai_execution', function (event) {
        var parsed = null;
        try {
            parsed = JSON.parse(event.data);
        } catch (_error) {
            parsed = null;
        }
        if (!parsed || typeof parsed !== 'object') {
            return;
        }
        handleFlightChatAIStreamEvent(parsed);
    });

    // When SSEHub connects/reconnects, stop polling (SSE is live)
    SSEHub.onStatusChange(function (newStatus) {
        if (newStatus === 'online') {
            stopFlightChatExecutionPolling();
        } else if (newStatus === 'offline' || newStatus === 'reconnecting') {
            startFlightChatExecutionPolling();
        }
    });
}

function disconnectFlightChatEventStream() {
    // SSEHub manages the connection; just clear the sentinel
    flightChatEventSource = null;
}

function createFlightChatAssistantPlaceholder() {
    const bubble = appendFlightChatMessage('assistant', '正在分析中...');
    if (!bubble) {
        return null;
    }

    const progress = document.createElement('div');
    progress.className = 'ai-chat-viz';
    progress.style.marginTop = '10px';
    progress.style.padding = '8px';
    progress.style.border = '1px dashed rgba(15, 23, 42, 0.15)';
    progress.style.borderRadius = '8px';
    progress.style.background = 'rgba(248, 250, 252, 0.7)';
    progress.dataset.role = 'tool-log';
    bubble.appendChild(progress);
    return {
        bubble,
        progress,
    };
}

function appendFlightChatTextDelta(delta) {
    if (!flightChatActiveAssistantBubble) {
        return;
    }
    const text = String(delta || '');
    if (!text) {
        return;
    }

    const textBlock = flightChatActiveAssistantBubble.querySelector('.ai-chat-message-text');
    if (!textBlock) {
        return;
    }
    const current = String(textBlock.textContent || '');
    if (!current || current === '正在分析中...' || current === '-') {
        textBlock.textContent = text;
    } else {
        textBlock.textContent = `${current}${text}`;
    }

    const messagesEl = document.getElementById('flightChatMessages');
    if (messagesEl) {
        messagesEl.scrollTop = messagesEl.scrollHeight;
    }
}

function appendFlightChatProgress(message, tone = 'info') {
    if (!flightChatActiveProgressPanel) {
        return;
    }

    const row = document.createElement('div');
    row.style.fontSize = '12px';
    row.style.lineHeight = '1.5';
    row.style.marginBottom = '4px';
    row.style.color = tone === 'error'
        ? '#b91c1c'
        : tone === 'warning'
            ? '#9a3412'
            : tone === 'success'
                ? '#166534'
                : '#334155';
    row.textContent = `• ${String(message || '')}`;
    flightChatActiveProgressPanel.appendChild(row);

    const messagesEl = document.getElementById('flightChatMessages');
    if (messagesEl) {
        messagesEl.scrollTop = messagesEl.scrollHeight;
    }
}

function safeParseFlightChatJson(value) {
    if (!value) {
        return null;
    }
    try {
        const parsed = JSON.parse(value);
        return parsed && typeof parsed === 'object' ? parsed : null;
    } catch (_error) {
        return null;
    }
}

async function requestFlightChatStream(endpoint, payload) {
    const response = await Auth.fetch(endpoint, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
            'Accept': 'text/event-stream',
        },
        body: JSON.stringify(payload),
    });
    if (!response.ok) {
        const text = await response.text();
        throw new Error(text || `HTTP ${response.status}`);
    }

    let finalResult = null;
    let streamError = '';
    flightChatActiveRequestBodyStream = true;
    try {
        await consumeFlightChatSSEStream(response, (eventName, rawData) => {
            const event = String(eventName || '').trim().toLowerCase();
            const payloadObj = safeParseFlightChatJson(rawData) || {};

            if (event === 'connected' || event === 'completed' || event === 'heartbeat') {
                return;
            }
            if (event === 'final_result') {
                finalResult = payloadObj;
                return;
            }
            if (event === 'error' && !payloadObj.scene) {
                streamError = String(payloadObj.message || payloadObj.detail || rawData || '流式请求失败');
                appendFlightChatProgress(streamError, 'error');
                return;
            }

            handleFlightChatAIStreamEvent({
                type: event,
                payload: payloadObj,
            });

            if (event === 'error') {
                streamError = String(payloadObj.message || payloadObj.detail || payloadObj.error || '处理失败');
            }
        });
    } finally {
        flightChatActiveRequestBodyStream = false;
    }

    if (finalResult && typeof finalResult === 'object') {
        return finalResult;
    }
    if (streamError) {
        throw new Error(streamError);
    }
    throw new Error('流式响应未返回最终结果');
}

function handleFlightChatAIStreamEvent(message) {
    const payload = message.payload || {};
    const type = String(message.type || '').toLowerCase();
    const semanticEvent = String(payload.event || type || '').toLowerCase();
    const semanticStatus = String(payload.status || '').toLowerCase();
    if (payload.execution_id) {
        flightChatActiveExecutionId = String(payload.execution_id);
    }
    const statusMeta = window.AIStatusMapper
        ? window.AIStatusMapper.getStatusMeta(semanticStatus, payload.code)
        : { tone: 'info', label: semanticStatus || '处理中' };
    if (!flightChatActiveRequestId || payload.request_id !== flightChatActiveRequestId) {
        return;
    }
    if (String(payload.scene || '').toLowerCase() !== 'flight_monitor') {
        return;
    }
    if (!flightChatActiveProgressPanel) {
        return;
    }

    if (type === 'text_delta' || semanticEvent === 'text_delta') {
        appendFlightChatTextDelta(payload.delta || payload.text || '');
        return;
    }
    if (type === 'progress' || semanticEvent === 'tool_progress') {
        appendFlightChatProgress(payload.message || payload.stage || '处理中...', 'info');
        return;
    }
    if (type === 'tool_start' || semanticEvent === 'tool_start') {
        flightChatToolCallSeen = true;
        appendFlightChatProgress(`工具开始: ${payload.tool_name || payload.tool_call_id || 'unknown'}`, statusMeta.tone || 'info');
        return;
    }
    if (type === 'tool_end' || semanticEvent === 'tool_end') {
        flightChatToolCallSeen = true;
        const status = String(payload.status || 'unknown');
        appendFlightChatProgress(
            `工具结束: ${payload.tool_name || payload.tool_call_id || 'unknown'} (${status})`,
            statusMeta.tone || (status === 'success' ? 'success' : 'warning'),
        );
        return;
    }
    if (type === 'error' || semanticEvent === 'execution_end' && semanticStatus === 'error') {
        appendFlightChatProgress(payload.message || '处理失败', 'error');
        return;
    }
    if (type === 'done' || semanticEvent === 'execution_end') {
        appendFlightChatProgress(payload.message || '处理完成', statusMeta.tone || 'success');
        stopFlightChatExecutionPolling();
    }
}

function resetFlightChatActiveStreamState() {
    stopFlightChatExecutionPolling();
    flightChatActiveRequestId = null;
    flightChatActiveExecutionId = null;
    flightChatActiveAssistantBubble = null;
    flightChatActiveProgressPanel = null;
    flightChatToolCallSeen = false;
}

function extractFlightChatInsightPayload(payload) {
    if (!payload || typeof payload !== 'object') {
        return null;
    }

    const queue = [payload];
    const seen = new Set();

    while (queue.length > 0) {
        const current = queue.shift();
        if (!current || typeof current !== 'object') {
            continue;
        }
        if (seen.has(current)) {
            continue;
        }
        seen.add(current);

        if (Array.isArray(current)) {
            current.forEach((item) => queue.push(item));
            continue;
        }

        const reportMarkdown = String(current.report_markdown || '').trim();
        const journeyMarkdown = String(current.journey_markdown || '').trim();
        if (reportMarkdown || current.report_json) {
            return {
                kind: 'history_report',
                title: '航班动态报表',
                markdown: reportMarkdown,
                jsonPayload: current.report_json || {},
                flightNo: current.flight_number || current.flight_id || '',
                generatedAt: current.generated_at || '',
                model: current.model || '',
            };
        }
        if (journeyMarkdown || current.journey_json) {
            return {
                kind: 'event_journey',
                title: '航班事件经过',
                markdown: journeyMarkdown,
                jsonPayload: current.journey_json || {},
                flightNo: current.flight_number || current.flight_id || '',
                generatedAt: current.generated_at || '',
                model: current.model || '',
            };
        }

        if (Array.isArray(current.tool_calls)) {
            current.tool_calls.forEach((item) => {
                if (item && typeof item === 'object') {
                    if ('result' in item) {
                        queue.push(item.result);
                    } else {
                        queue.push(item);
                    }
                }
            });
        }
        if (current.result && typeof current.result === 'object') {
            queue.push(current.result);
        }
        if (current.data && typeof current.data === 'object') {
            queue.push(current.data);
        }
    }

    return null;
}

function appendFlightChatAssistantResult(result, options = {}) {
    const bubble = options.bubble || appendFlightChatMessage('assistant', result.summary || '已完成查询。');
    if (!bubble) {
        return;
    }
    const preserveToolLog = Boolean(options.preserveToolLog);

    const textEl = bubble.querySelector('.ai-chat-message-text');
    if (textEl) {
        textEl.textContent = result.summary || '已完成查询。';
    }
    Array.from(bubble.children).forEach((child) => {
        if (child.classList && child.classList.contains('ai-chat-message-text')) {
            return;
        }
        if (
            preserveToolLog
            && child instanceof HTMLElement
            && child.dataset.role === 'tool-log'
            && child.childElementCount > 0
        ) {
            return;
        }
        bubble.removeChild(child);
    });

    const insightPayload = extractFlightChatInsightPayload(result.structured_data);
    if (insightPayload) {
        bubble.appendChild(renderFlightChatInsightCard(insightPayload));
        return;
    }

    const viz = renderFlightChatVisualization(result.visualization_hint, result.structured_data);
    if (viz) {
        bubble.appendChild(viz);
    }
}

async function sendFlightChatMessage() {
    if (flightChatSending) {
        return;
    }
    if (!isFlightChatActionEnabled()) {
        showToast(getAIChatCapabilityHintText() || 'AI 对话不可用', 'warning');
        return;
    }

    ensureFlightChatModal();
    const inputEl = document.getElementById('flightChatInput');
    if (!inputEl) {
        return;
    }

    const question = String(inputEl.value || '').trim();
    if (!question) {
        return;
    }

    appendFlightChatMessage('user', question);
    inputEl.value = '';
    setFlightChatSending(true);
    updateFlightChatMeta();

    const requestId = generateClientRequestId();
    const placeholder = createFlightChatAssistantPlaceholder();
    flightChatActiveRequestId = requestId;
    flightChatActiveExecutionId = requestId;
    flightChatActiveAssistantBubble = placeholder ? placeholder.bubble : null;
    flightChatActiveProgressPanel = placeholder ? placeholder.progress : null;
    flightChatToolCallSeen = false;
    appendFlightChatProgress('请求已发送，等待处理...', 'info');

    const endpoint = flightChatConversationId
        ? `${NL_QUERY_API_BASE}/followup/stream`
        : `${NL_QUERY_API_BASE}/stream`;
    const contextPayload = buildFlightChatContextPayload();
    const requestPayload = flightChatConversationId
        ? {
            question,
            conversation_id: flightChatConversationId,
            context: contextPayload,
            request_id: requestId,
        }
        : {
            question,
            context: contextPayload,
            request_id: requestId,
        };

    try {
        const data = await requestFlightChatStream(endpoint, requestPayload);
        flightChatConversationId = data.conversation_id || flightChatConversationId;
        updateFlightChatMeta();
        if (flightChatActiveAssistantBubble) {
            appendFlightChatAssistantResult(data, {
                bubble: flightChatActiveAssistantBubble,
                preserveToolLog: flightChatToolCallSeen,
            });
        } else {
            appendFlightChatAssistantResult(data);
        }
    } catch (error) {
        if (flightChatActiveAssistantBubble) {
            appendFlightChatAssistantResult(
                { summary: `查询失败：${error?.message || '请稍后重试'}`, structured_data: null, visualization_hint: null },
                {
                    bubble: flightChatActiveAssistantBubble,
                    preserveToolLog: flightChatToolCallSeen,
                },
            );
        } else {
            appendFlightChatMessage('assistant', `查询失败：${error?.message || '请稍后重试'}`);
        }
    } finally {
        setFlightChatSending(false);
        resetFlightChatActiveStreamState();
    }
}

async function endFlightChatConversation() {
    if (flightChatSending) {
        return;
    }
    if (!flightChatConversationId) {
        clearFlightChatMessages({ silent: true });
        showToast('当前没有进行中的会话', 'info');
        return;
    }

    try {
        const response = await Auth.fetch(`${NL_QUERY_API_BASE}/${encodeURIComponent(flightChatConversationId)}`, {
            method: 'DELETE',
        });
        const payload = await response.json().catch(() => ({}));
        if (!response.ok || !payload?.success) {
            throw new Error(extractApiErrorMessage(payload, `结束会话失败 (HTTP ${response.status})`));
        }

        flightChatConversationId = null;
        updateFlightChatMeta();
        clearFlightChatMessages({ silent: true });
        appendFlightChatMessage('assistant', '会话已结束。你可以开始新的问题。');
    } catch (error) {
        showToast(error?.message || '结束会话失败，请稍后重试', 'error', 4200);
    }
}

window.openFlightChatModal = openFlightChatModal;

async function requestFlightInsight(kind) {
    if (!selectedFlightId) {
        showToast('请先选择一个航班', 'info');
        return;
    }
    if (!isFlightInsightActionEnabled()) {
        showToast(getAICapabilityHintText() || 'AI 不可用', 'warning');
        return;
    }

    const flight = getSelectedFlightForInsight();
    if (!flight) {
        showToast('未找到当前航班，请刷新后重试', 'error');
        return;
    }

    const endpoint = kind === 'journey'
        ? `${API_BASE}/flights/${encodeURIComponent(flight.flight_id)}/event-journey?hours=24`
        : `${API_BASE}/flights/${encodeURIComponent(flight.flight_id)}/history-report?hours=24`;

    try {
        setFlightInsightButtonLoading(kind, true);
        if (window.FM_AI_BRIDGE && typeof window.FM_AI_BRIDGE.generateInsight === 'function') {
            const flightNoLabel = getPrimaryFlightNoV2(flight) || String(flight.flight_id || '');
            const bridged = await window.FM_AI_BRIDGE.generateInsight(kind, {
                flight_id: String(flight.flight_id || ''),
                flight_number: String(flight.flight_number || flight.flight_id || ''),
                summary: `${flightNoLabel} ${flight.status || ''}`,
            });
            const markdown = bridged && typeof bridged.markdown === 'string' ? bridged.markdown : '';
            const jsonPayload = bridged && bridged.jsonPayload ? bridged.jsonPayload : {};
            openFlightInsightModal({
                kind,
                title: kind === 'journey' ? '航班事件经过' : '航班动态报表',
                flightId: flight.flight_id,
                flightNo: flightNoLabel,
                generatedAt: new Date().toISOString(),
                model: '',
                markdown,
                jsonPayload,
            });
            showToast(kind === 'journey' ? '事件经过生成成功' : '动态报表生成成功', 'success');
            return;
        }
        const response = await Auth.fetch(endpoint);
        const payload = await response.json();
        if (!response.ok || !payload?.success) {
            throw new Error(extractApiErrorMessage(payload, '洞察生成失败'));
        }

        const data = payload.data || {};
        const markdown = kind === 'journey' ? (data.journey_markdown || '') : (data.report_markdown || '');
        const jsonPayload = kind === 'journey' ? (data.journey_json || {}) : (data.report_json || {});
        openFlightInsightModal({
            kind,
            title: kind === 'journey' ? '航班事件经过' : '航班动态报表',
            flightId: data.flight_id || flight.flight_id,
            flightNo: data.flight_number || flight.flight_number || flight.flight_id,
            generatedAt: data.generated_at || '',
            model: data.model || '',
            markdown,
            jsonPayload,
        });
        showToast(kind === 'journey' ? '事件经过生成成功' : '动态报表生成成功', 'success');
    } catch (error) {
        showToast(error?.message || '洞察生成失败，请稍后重试', 'error', 5000);
    } finally {
        setFlightInsightButtonLoading(kind, false);
        updateBusinessInsightActionState();
    }
}

async function generateSelectedFlightHistoryReport() {
    await requestFlightInsight('history');
}

async function generateSelectedFlightEventJourney() {
    await requestFlightInsight('journey');
}

window.generateSelectedFlightHistoryReport = generateSelectedFlightHistoryReport;

window.generateSelectedFlightEventJourney = generateSelectedFlightEventJourney;

function announce(message) {
    const announcer = document.getElementById('ariaAnnouncer');
    if (!announcer) return;
    announcer.textContent = '';
    requestAnimationFrame(() => {
        announcer.textContent = String(message || '');
    });
}

function setFlightListBusy(isBusy) {
    const list = document.getElementById('flightList');
    if (list) {
        list.setAttribute('aria-busy', String(!!isBusy));
    }
}

function setGlobalLoaderVisible(visible) {
    const loader = document.getElementById('globalLoader');
    if (!loader) return;
    loader.classList.toggle('active', !!visible);
    loader.style.display = visible ? 'flex' : 'none';
    setFlightListBusy(visible);
}

function setRefreshButtonLoading(isLoading) {
    const btn = document.getElementById('refreshBtn');
    if (!btn) return;
    btn.disabled = !!isLoading;
    btn.textContent = isLoading ? '刷新中...' : '刷新数据';
    btn.setAttribute('aria-label', isLoading ? '正在刷新航班数据' : '刷新航班数据');
    btn.setAttribute('aria-busy', String(!!isLoading));
}

function ensureConnectionStatusElement() {
    const primary = document.getElementById('connectionStatusPill');
    if (primary) {
        return primary;
    }
    const existing = document.getElementById('connectionStatus');
    if (existing) return existing;

    const title = document.querySelector('.flight-panel-meta') || document.querySelector('.panel-title');
    if (!title) return null;

    const indicator = document.createElement('span');
    indicator.id = 'connectionStatus';
    indicator.className = 'connection-status connecting';
    indicator.setAttribute('role', 'status');
    indicator.setAttribute('aria-live', 'polite');
    indicator.textContent = CONNECTION_STATUS.connecting.text;

    const lastUpdated = document.getElementById('lastUpdated');
    if (lastUpdated && lastUpdated.parentNode === title) {
        title.insertBefore(indicator, lastUpdated);
    } else {
        title.appendChild(indicator);
    }
    return indicator;
}

function setConnectionStatus(statusKey) {
    const meta = CONNECTION_STATUS[statusKey] || CONNECTION_STATUS.offline;
    const indicator = ensureConnectionStatusElement();
    if (!indicator) return;

    const baseClass = indicator.id === 'connectionStatusPill'
        ? 'flight-connection-pill connection-status'
        : 'connection-status';
    indicator.className = `${baseClass} ${meta.cls}`;
    indicator.textContent = meta.text;

    if (currentConnectionStatusKey !== statusKey) {
        currentConnectionStatusKey = statusKey;
        announce(meta.text);
    }
}

function getFocusableElements(container) {
    if (!container) return [];
    return Array.from(container.querySelectorAll('a[href], button:not([disabled]), textarea, input, select, [tabindex]:not([tabindex="-1"])'))
        .filter((el) => !el.hasAttribute('hidden') && el.offsetParent !== null);
}

function normalizeFlightMonitorBaseView(view) {
    return view === 'table' ? 'table' : 'card';
}

let currentView = normalizeFlightMonitorBaseView(localStorage.getItem('flightMonitorView'));

let lastNonAlertView = currentView;

let routeDisplayMode = localStorage.getItem('routeDisplayMode') || 'code';

const DEFAULT_VISIBLE_COLUMNS = ['flight_no', 'status', 'route', 'smart_departure', 'smart_arrival', 'flight_type', 'stand', 'gate', 'aircraft'];

const DEFAULT_COLUMN_ORDER = ['flight_no', 'status', 'route', 'smart_departure', 'smart_arrival', 'flight_type', 'time_dep_sch', 'time_arr_sch', 'time_dep_est', 'time_arr_est', 'time_dep_act', 'time_arr_act', 'stand', 'gate', 'baggage_carousel', 'aircraft', 'missions', 'cobt_time', 'codt', 'boarding_allowed_time', 'start_boarding_time', 'end_boarding_time', 'passenger_ready_time', 'on_blocks_time', 'off_blocks_time', 'cabin_door_open_time', 'deboarding_complete_time', 'cleaning_start_time', 'cleaning_end_time', 'cabin_door_close_time', 'cargo_door_close_time', 'loading_complete_time', 'flight_remarks', 'load_planning_remarks', 'aircraft_maintenance_remarks', 'aircraft_check_remarks'];

let tableConfig = {
    visibleColumns: [...DEFAULT_VISIBLE_COLUMNS],
    columnOrder: [...DEFAULT_COLUMN_ORDER],
    columnWidths: {} // { columnId: widthInPx }
};

let currentRenderTaskId = 0;

let tableHeaderRenderKey = '';

const TABLE_SYNC_RENDER_THRESHOLD = 0;

const TABLE_VIRTUAL_ROW_HEIGHT = 40;

const TABLE_VIRTUAL_BUFFER = 10;

const CARD_CHUNK_RENDER_THRESHOLD = 120;

const FLIGHT_LIST_PANEL_WIDTH_KEY = 'flightListPanelWidth';

const FLIGHT_LIST_PANEL_MIN_WIDTH = 300;

const FLIGHT_DETAIL_PANEL_MIN_WIDTH = 300;

let pendingViewRenderTimer = null;

let pendingViewRenderRafId = null;

let tableVirtualScroller = null;

function getMaxFlightListPanelWidth() {
    return Math.max(FLIGHT_LIST_PANEL_MIN_WIDTH, window.innerWidth - FLIGHT_DETAIL_PANEL_MIN_WIDTH);
}

function setSavedFlightListPanelWidth(width) {
    localStorage.setItem(FLIGHT_LIST_PANEL_WIDTH_KEY, String(width));
}

function getClampedFlightListPanelWidth() {
    const raw = localStorage.getItem(FLIGHT_LIST_PANEL_WIDTH_KEY);
    if (!raw) {
        return null;
    }

    const parsed = Number(raw);
    if (!Number.isFinite(parsed)) {
        localStorage.removeItem(FLIGHT_LIST_PANEL_WIDTH_KEY);
        return null;
    }

    const clamped = Math.min(Math.max(parsed, FLIGHT_LIST_PANEL_MIN_WIDTH), getMaxFlightListPanelWidth());
    if (clamped !== parsed) {
        setSavedFlightListPanelWidth(clamped);
    }
    return clamped;
}

function applySavedFlightListPanelWidth(panelEl) {
    if (!panelEl) {
        return;
    }
    const width = getClampedFlightListPanelWidth();
    panelEl.style.width = width ? `${width}px` : '';
}

function syncFlightMonitorLayout(view = currentView) {
    const detailPanel = document.querySelector('.flight-detail-panel');
    const resizer = document.getElementById('resizer');
    const listPanel = document.querySelector('.flight-list-panel');
    const compactLayout = isCompactFlightMonitorLayout();

    if (!listPanel) {
        return;
    }

    if (view === 'table') {
        listPanel.style.width = '100%';
        if (detailPanel) {
            detailPanel.style.display = compactLayout ? '' : 'none';
        }
        if (resizer) {
            resizer.style.display = 'none';
        }
        setDetailDrawerOpen(false);
        return;
    }

    listPanel.style.width = '';
    applySavedFlightListPanelWidth(listPanel);
    if (detailPanel) {
        detailPanel.style.display = '';
    }
    if (resizer) {
        resizer.style.display = compactLayout ? 'none' : '';
    }
    if (!compactLayout) {
        setDetailDrawerOpen(false);
    }
}

class VirtualScroller {
    constructor(options) {
        this.container = options.container;
        this.itemHeight = options.itemHeight;
        this.renderFn = options.renderFn;
        this.buffer = options.buffer || 5;
        this.data = [];
        this.startIndex = 0;
        this.endIndex = 0;
        this.visibleCount = 0;
        this.scrollTop = 0;
        this.totalHeight = 0;
        this.contentEl = null;
        this.spacerEl = null;
        this.scrollHandler = null;
        this.resizeObserver = null;
        this.isInitialized = false;
    }

    init() {
        if (this.isInitialized) return;

        // 保存原始内容并清空容器
        this.container.innerHTML = '';
        this.container.style.position = 'relative';
        this.container.style.overflowY = 'auto';

        // 创建内容层
        this.contentEl = document.createElement('div');
        this.contentEl.className = 'virtual-scroll-content';
        this.contentEl.style.position = 'absolute';
        this.contentEl.style.left = '0';
        this.contentEl.style.right = '0';
        this.container.appendChild(this.contentEl);

        // 创建撑开层（用于产生滚动条）
        this.spacerEl = document.createElement('div');
        this.spacerEl.className = 'virtual-scroll-spacer';
        this.spacerEl.style.width = '1px';
        this.spacerEl.style.visibility = 'hidden';
        this.container.appendChild(this.spacerEl);

        // 绑定滚动事件
        this.scrollHandler = this.onScroll.bind(this);
        this.container.addEventListener('scroll', this.scrollHandler, { passive: true });

        // 监听容器大小变化
        if (window.ResizeObserver) {
            this.resizeObserver = new ResizeObserver(() => {
                this.calculateVisibleCount();
                this.updateVisibleRange();
            });
            this.resizeObserver.observe(this.container);
        }

        // 计算可见数量
        this.calculateVisibleCount();
        this.isInitialized = true;
    }

    calculateVisibleCount() {
        const containerHeight = this.container.clientHeight;
        this.visibleCount = Math.ceil(containerHeight / this.itemHeight) + this.buffer * 2;
    }

    onScroll() {
        this.scrollTop = this.container.scrollTop;
        this.updateVisibleRange();
    }

    updateVisibleRange() {
        const newStartIndex = Math.max(0, Math.floor(this.scrollTop / this.itemHeight) - this.buffer);
        const newEndIndex = Math.min(
            this.data.length,
            newStartIndex + this.visibleCount
        );

        // 如果范围没有变化，不重新渲染
        if (newStartIndex === this.startIndex && newEndIndex === this.endIndex) {
            return;
        }

        this.startIndex = newStartIndex;
        this.endIndex = newEndIndex;
        this.render();
    }

    render() {
        if (!this.contentEl) return;

        // 更新撑开元素高度
        this.totalHeight = this.data.length * this.itemHeight;
        this.spacerEl.style.height = this.totalHeight + 'px';

        // 使用 transform 定位内容层
        const offsetY = this.startIndex * this.itemHeight;
        this.contentEl.style.transform = `translateY(${offsetY}px)`;

        // 清空当前内容
        this.contentEl.innerHTML = '';

        // 批量渲染可见项
        const fragment = document.createDocumentFragment();
        for (let i = this.startIndex; i < this.endIndex; i++) {
            if (i < this.data.length) {
                const item = this.renderFn(this.data[i], i);
                if (item) {
                    fragment.appendChild(item);
                }
            }
        }
        this.contentEl.appendChild(fragment);
    }

    setData(data) {
        this.data = data || [];
        this.calculateVisibleCount();
        this.updateVisibleRange();
    }

    refresh() {
        this.render();
    }

    scrollToIndex(index) {
        const targetScrollTop = index * this.itemHeight;
        this.container.scrollTop = targetScrollTop;
    }

    scrollToItem(predicate) {
        const index = this.data.findIndex(predicate);
        if (index !== -1) {
            this.scrollToIndex(index);
        }
    }

    destroy() {
        if (this.resizeObserver) {
            this.resizeObserver.disconnect();
            this.resizeObserver = null;
        }
        if (this.container && this.scrollHandler) {
            this.container.removeEventListener('scroll', this.scrollHandler);
        }
        this.contentEl = null;
        this.spacerEl = null;
        this.scrollHandler = null;
        this.isInitialized = false;
    }
}

class TableVirtualScroller {
    constructor(options) {
        this.wrapper = options.wrapper;
        this.tbody = options.tbody;
        this.rowHeight = options.rowHeight || TABLE_VIRTUAL_ROW_HEIGHT;
        this.buffer = options.buffer || TABLE_VIRTUAL_BUFFER;
        this.renderRowHtml = options.renderRowHtml;

        this.data = [];
        this.columnIds = [];
        this.lastStart = -1;
        this.lastEnd = -1;
        this.rafPending = false;
        this.forcePending = false;
        this.scrollHandler = null;
        this.resizeObserver = null;
        this.isInitialized = false;
    }

    init() {
        if (this.isInitialized || !this.wrapper || !this.tbody) return;

        this.scrollHandler = () => this.scheduleRender(false);
        this.wrapper.addEventListener('scroll', this.scrollHandler, { passive: true });

        if (window.ResizeObserver) {
            this.resizeObserver = new ResizeObserver(() => this.scheduleRender(true));
            this.resizeObserver.observe(this.wrapper);
        }

        this.isInitialized = true;
    }

    setData(data, columnIds) {
        this.data = Array.isArray(data) ? data : [];
        this.columnIds = Array.isArray(columnIds) ? columnIds : [];
        this.lastStart = -1;
        this.lastEnd = -1;
        this.scheduleRender(true);
    }

    scheduleRender(force = false) {
        this.forcePending = this.forcePending || force;
        if (this.rafPending) return;

        this.rafPending = true;
        requestAnimationFrame(() => {
            this.rafPending = false;
            const forceNow = this.forcePending;
            this.forcePending = false;
            this.render(forceNow);
        });
    }

    getVisibleRange() {
        const total = this.data.length;
        if (total === 0) {
            return { start: 0, end: 0 };
        }

        const viewportHeight = this.wrapper.clientHeight || 0;
        const visibleCount = Math.ceil(viewportHeight / this.rowHeight) + this.buffer * 2;
        const start = Math.max(0, Math.floor(this.wrapper.scrollTop / this.rowHeight) - this.buffer);
        const end = Math.min(total, start + visibleCount);

        return { start, end };
    }

    render(force = false) {
        if (!this.tbody) return;

        const total = this.data.length;
        if (total === 0) {
            this.tbody.innerHTML = '';
            this.lastStart = -1;
            this.lastEnd = -1;
            return;
        }

        const { start, end } = this.getVisibleRange();
        if (!force && start === this.lastStart && end === this.lastEnd) {
            return;
        }

        this.lastStart = start;
        this.lastEnd = end;

        const topHeight = start * this.rowHeight;
        const bottomHeight = (total - end) * this.rowHeight;
        const colspan = Math.max(this.columnIds.length, 1);

        let html = '';

        if (topHeight > 0) {
            html += `<tr class="virtual-spacer-row" aria-hidden="true"><td colspan="${colspan}" style="height:${topHeight}px;"></td></tr>`;
        }

        for (let i = start; i < end; i++) {
            html += this.renderRowHtml(this.data[i], this.columnIds);
        }

        if (bottomHeight > 0) {
            html += `<tr class="virtual-spacer-row" aria-hidden="true"><td colspan="${colspan}" style="height:${bottomHeight}px;"></td></tr>`;
        }

        this.tbody.innerHTML = html;
    }

    destroy() {
        if (this.resizeObserver) {
            this.resizeObserver.disconnect();
            this.resizeObserver = null;
        }

        if (this.wrapper && this.scrollHandler) {
            this.wrapper.removeEventListener('scroll', this.scrollHandler);
        }

        this.scrollHandler = null;
        this.rafPending = false;
        this.forcePending = false;
        this.isInitialized = false;
    }
}

let cardVirtualScroller = null;

const timeFormatter = new Intl.DateTimeFormat('zh-CN', {
    hour: '2-digit',
    minute: '2-digit',
    hour12: false
});

const TIME_FIELDS = [
    'scheduled_departure', 'scheduled_arrival',
    'estimated_departure', 'estimated_arrival',
    'actual_departure', 'actual_arrival',
    'cobt_time', 'codt', 'start_boarding_time', 'end_boarding_time',
    'boarding_allowed_time', 'passenger_ready_time',
    'off_blocks_time', 'cabin_door_open_time',
    'cleaning_start_time', 'cleaning_end_time',
    'on_blocks_time', 'deboarding_complete_time',
    'cabin_door_close_time', 'cargo_door_close_time',
    'loading_complete_time'
];

function normalizeAnomalySummaryV2(rawSummary) {
    const summary = rawSummary && typeof rawSummary === 'object' ? rawSummary : {};
    return {
        has_open_anomaly: Boolean(summary.has_open_anomaly),
        open_count: Number(summary.open_count || 0),
        acknowledged_count: Number(summary.acknowledged_count || 0),
    };
}

function hydrateFlightLegViewV2(flight) {
    if (!flight || typeof flight !== 'object') {
        return flight;
    }

    const inboundLeg = getLegPayloadV2(flight, 'inbound');
    const outboundLeg = getLegPayloadV2(flight, 'outbound');
    flight.inbound_leg = inboundLeg;
    flight.outbound_leg = outboundLeg;
    flight.anomaly_summary = normalizeAnomalySummaryV2(flight.anomaly_summary);
    syncFlightTimelineFieldsFromCache(flight);

    return flight;
}

function formatTimeValue(isoString) {
    if (!isoString) return null;
    try {
        return timeFormatter.format(new Date(isoString));
    } catch (e) {
        return null;
    }
}

function preprocessFlightTimes(flight) {
    hydrateFlightLegViewV2(flight);
    if (!flight._timesFormatted) {
        flight._fmt = {};
        for (const field of TIME_FIELDS) {
            flight._fmt[field] = formatTimeValue(flight[field]);
        }
        flight._timesFormatted = true;
    }
    return flight;
}

function preprocessFlightsBatch(flightsArray) {
    for (let i = 0; i < flightsArray.length; i++) {
        preprocessFlightTimes(flightsArray[i]);
    }
    return flightsArray;
}

function throttleRAF(fn) {
    let scheduled = false;
    return function (...args) {
        if (!scheduled) {
            scheduled = true;
            requestAnimationFrame(() => {
                fn.apply(this, args);
                scheduled = false;
            });
        }
    };
}

function debounce(fn, delay = 150) {
    let timeoutId;
    return function (...args) {
        clearTimeout(timeoutId);
        timeoutId = setTimeout(() => fn.apply(this, args), delay);
    };
}

let flightWorker = null;

function initFlightWorker() {
    if (window.Worker && !flightWorker) {
        try {
            flightWorker = new Worker('/frontend/js/flight_worker.js');
            flightWorker.onmessage = handleWorkerMessage;
            flightWorker.onerror = (e) => {
                console.warn('Flight Worker error, falling back to sync:', e);
                flightWorker = null;
                applyCurrentFilters();
            };
        } catch (e) {
            console.warn('Failed to create Flight Worker:', e);
        }
    }
}

function handleWorkerMessage(e) {
    const { type, data, requestId } = e.data;

    if (typeof requestId === 'number') {
        if (requestId < latestWorkerRequestId) {
            return;
        }
        latestWorkerResponseId = requestId;
    }

    if (type === 'filterResult' || type === 'filterAndSortResult') {
        flights = data;
        renderFlights();
            updateBusinessFilterSummary(flights.length, originalFlights.length);
        setFlightListBusy(false);
        announce(`筛选后显示 ${flights.length} 条航班`);
    } else if (type === 'sortResult') {
        flights = data;
        renderFlights();
        setFlightListBusy(false);
        announce(`排序完成，共 ${flights.length} 条航班`);
    } else if (type === 'error') {
        console.warn('Flight Worker returned error, falling back to sync mode');
        flightWorker = null;
        setFlightListBusy(false);
        applyCurrentFilters();
    }
}

const DEFAULT_COLUMNS = {
    flight_no: { label: '航班号', minWidth: 100 },
    status: { label: '状态', minWidth: 80 },
    route: { label: '航线', minWidth: 150 },
    smart_departure: { label: '起飞时间', minWidth: 100 },
    smart_arrival: { label: '到达时间', minWidth: 100 },
    time_dep_sch: { label: '计划起飞', minWidth: 90 },
    time_arr_sch: { label: '计划到达', minWidth: 90 },
    time_dep_est: { label: '预计起飞', minWidth: 90 },
    time_arr_est: { label: '预计到达', minWidth: 90 },
    time_dep_act: { label: '实际起飞', minWidth: 90 },
    time_arr_act: { label: '实际到达', minWidth: 90 },
    stand: { label: '机位', minWidth: 60 },
    gate: { label: '登机口', minWidth: 60 },
    aircraft: { label: '机型', minWidth: 80 },
    flight_type: { label: '属性', minWidth: 60 },
    missions: { label: '任务', minWidth: 80 },
    // Read-only Time Fields
    cobt_time: { label: 'COBT', minWidth: 80 },
    codt: { label: 'CODT', minWidth: 80 },
    start_boarding_time: { label: '开始登机', minWidth: 90 },
    end_boarding_time: { label: '结束登机', minWidth: 90 },
    // Interactive Time Fields
    on_blocks_time: { label: '上轮挡', minWidth: 90 },
    cabin_door_open_time: { label: '开舱门', minWidth: 90 },
    deboarding_complete_time: { label: '下客完成', minWidth: 90 },
    cleaning_start_time: { label: '清洁开始', minWidth: 90 },
    cleaning_end_time: { label: '清洁结束', minWidth: 90 },
    cabin_door_close_time: { label: '关客舱门', minWidth: 90 },
    cargo_door_close_time: { label: '关货舱门', minWidth: 90 },
    loading_complete_time: { label: '装载完成', minWidth: 90 },
    // Remarks
    flight_remarks: { label: '航班备注', minWidth: 120 },
    load_planning_remarks: { label: '配载备注', minWidth: 120 },
    aircraft_maintenance_remarks: { label: '机务备注', minWidth: 120 },
    registration: { label: '机号', minWidth: 90 }, // Added registration
    // New Fields
    off_blocks_time: { label: '撤轮挡', minWidth: 90 },
    passenger_ready_time: { label: '人齐', minWidth: 90 },
    boarding_allowed_time: { label: '允许登机', minWidth: 90 },
    baggage_carousel: { label: '行李转盘', minWidth: 80 },
    aircraft_check_remarks: { label: '复核机号', minWidth: 100 }
};

const formatTimeSimple = (timeStr) => {
    return timeStr ? new Date(timeStr).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' }) : '';
};

function getCachedTimeForField(flight, field) {
    if (flight && flight._fmt && flight._fmt[field]) {
        return flight._fmt[field];
    }
    return formatTimeSimple(flight ? flight[field] : null);
}

const FIELD_MAP = {
    flight_no: f => renderTableFlightNumberV2(f) || escapeHtml(String(f.flight_id || '-')),
    registration: f => f.registration || '-',
    status: f => `<span class="cell-status-pill ${getStatusClass(f.status)}">${f.status || '-'}</span>`,
    route: f => {
        const inboundNo = getLegFieldV2(f, 'inbound', 'flight_no');
        const outboundNo = getLegFieldV2(f, 'outbound', 'flight_no');
        const useName = routeDisplayMode === 'name';
        const origin = getRouteEndpointV2(f, 'inbound', useName ? 'name' : 'code') || EMPTY_DISPLAY_TEXT;
        const dest = getRouteEndpointV2(f, 'outbound', useName ? 'name' : 'code') || EMPTY_DISPLAY_TEXT;
        const airportLabel = getAirportDisplayValueV2(routeDisplayMode);
        if (inboundNo && outboundNo) {
            return `<div class="cell-route"><span>${origin}</span><span class="route-arrow">→</span><span>${airportLabel}</span><span class="route-arrow">→</span><span>${dest}</span></div>`;
        } else if (inboundNo) {
            return `<div class="cell-route"><span>${origin}</span><span class="route-arrow">→</span><span>${airportLabel}</span></div>`;
        } else {
            return `<div class="cell-route"><span>${airportLabel}</span><span class="route-arrow">→</span><span>${dest}</span></div>`;
        }
    },
    // Split Time Columns - 使用预格式化时间字段提升性能
    time_dep_sch: f => {
        const t = (f._fmt && f._fmt.scheduled_departure) || '-';
        return `<span class="cell-time">${t}</span>`;
    },
    time_arr_sch: f => {
        const t = (f._fmt && f._fmt.scheduled_arrival) || '-';
        return `<span class="cell-time">${t}</span>`;
    },
    time_dep_est: f => {
        const t = (f._fmt && f._fmt.estimated_departure) || '-';
        return `<span class="cell-time">${t}</span>`;
    },
    time_arr_est: f => {
        const t = (f._fmt && f._fmt.estimated_arrival) || '-';
        return `<span class="cell-time">${t}</span>`;
    },
    time_dep_act: f => {
        const t = (f._fmt && f._fmt.actual_departure) || '-';
        return `<span class="cell-time">${t}</span>`;
    },
    time_arr_act: f => {
        const t = (f._fmt && f._fmt.actual_arrival) || '-';
        return `<span class="cell-time">${t}</span>`;
    },
    smart_departure: f => {
        let timeStr = '';
        let sourceClass = 'time-source-scheduled';

        const act = f._fmt && f._fmt.actual_departure;
        const est = f._fmt && f._fmt.estimated_departure;
        const sch = f._fmt && f._fmt.scheduled_departure;

        if (act) {
            timeStr = act;
            sourceClass = 'time-source-actual';
        } else if (est) {
            timeStr = est;
            sourceClass = 'time-source-estimated';
        } else {
            timeStr = sch || '-';
        }
        return `<span class="cell-time ${sourceClass}">${timeStr}</span>`;
    },
    smart_arrival: f => {
        let timeStr = '';
        let sourceClass = 'time-source-scheduled';

        const act = f._fmt && f._fmt.actual_arrival;
        const est = f._fmt && f._fmt.estimated_arrival;
        const sch = f._fmt && f._fmt.scheduled_arrival;

        if (act) {
            timeStr = act;
            sourceClass = 'time-source-actual';
        } else if (est) {
            timeStr = est;
            sourceClass = 'time-source-estimated';
        } else {
            timeStr = sch || '-';
        }
        return `<span class="cell-time ${sourceClass}">${timeStr}</span>`;
    },
    stand: f => f.stand || '-',
    gate: f => f.gate || '-',
    aircraft: f => f.aircraft_type_detail || '-',
    missions: f => {
        return getMissionSummaryV2(f) || '-';
    },
    flight_type: f => {
        return getFlightTypeSummaryV2(f) || '-';
    },
    // Read-Only Time Columns
    cobt_time: f => getCachedTimeForField(f, 'cobt_time') || '-',
    codt: f => getCachedTimeForField(f, 'codt') || '-',
    start_boarding_time: f => getCachedTimeForField(f, 'start_boarding_time') || '-',
    end_boarding_time: f => getCachedTimeForField(f, 'end_boarding_time') || '-',

    // Interactive Time Columns
    on_blocks_time: f => renderInteractiveTimeCell(f, 'on_blocks_time'),
    cabin_door_open_time: f => renderInteractiveTimeCell(f, 'cabin_door_open_time'),
    deboarding_complete_time: f => renderInteractiveTimeCell(f, 'deboarding_complete_time'),
    cleaning_start_time: f => renderInteractiveTimeCell(f, 'cleaning_start_time'),
    cleaning_end_time: f => renderInteractiveTimeCell(f, 'cleaning_end_time'),
    cabin_door_close_time: f => renderInteractiveTimeCell(f, 'cabin_door_close_time'),
    cargo_door_close_time: f => renderInteractiveTimeCell(f, 'cargo_door_close_time'),
    loading_complete_time: f => renderInteractiveTimeCell(f, 'loading_complete_time'),

    // Interactive Remarks
    flight_remarks: f => renderInteractiveRemarkCell(f, 'flight_remarks'),
    load_planning_remarks: f => renderInteractiveRemarkCell(f, 'load_planning_remarks'),
    aircraft_maintenance_remarks: f => renderInteractiveRemarkCell(f, 'aircraft_maintenance_remarks'),
    aircraft_check_remarks: f => renderInteractiveRemarkCell(f, 'aircraft_check_remarks'),

    // New Interactive Times
    off_blocks_time: f => renderInteractiveTimeCell(f, 'off_blocks_time'),
    passenger_ready_time: f => renderInteractiveTimeCell(f, 'passenger_ready_time'),
    boarding_allowed_time: f => renderInteractiveTimeCell(f, 'boarding_allowed_time'),

    // Readonly Info
    baggage_carousel: f => f.baggage_carousel || '-'
};

window.handleInteractiveTimeClick = function (el) {
    const flightId = el.dataset.flightId;
    const field = el.dataset.field;
    const value = el.dataset.value;

    if (!value) {
        // Empty: Set to current time
        updateFlightField(flightId, field, new Date().toISOString());
    }
};

window.handleInteractiveTimeContext = function (e, el) {
    e.preventDefault();
    const flightId = el.dataset.flightId;
    const field = el.dataset.field;
    const value = el.dataset.value;
    const filler = el.dataset.filler;

    if (!value) return; // Ignore if empty

    showTimeContextMenu(e, flightId, field, filler, value);
};

window.handleInteractiveRemarkDblClick = function (el) {
    const flightId = el.dataset.flightId;
    const field = el.dataset.field;
    const currentVal = el.innerText === '...' ? '' : el.innerText; // crude but works, better to use dataset if stored

    showRemarkEditModal(flightId, field, currentVal);
};

async function callApi(endpoint, method = 'GET', body = null) {
    const headers = {
        'Content-Type': 'application/json'
    };

    const options = {
        method,
        headers
    };

    if (body) {
        options.body = JSON.stringify(body);
    }

    // Handle full URL or relative path
    const url = endpoint.startsWith('http') ? endpoint : `${API_BASE}${endpoint.startsWith('/') ? '' : '/'}${endpoint}`;

    const response = await Auth.fetch(url, options);

    if (!response.ok) {
        let errorMessage = `API Error ${response.status}`;
        try {
            const errorData = await response.json();
            errorMessage = errorData.detail || errorData.message || errorMessage;
        } catch (e) {
            errorMessage = await response.text() || errorMessage;
        }
        throw new Error(errorMessage);
    }

    // Return JSON if possible, otherwise text
    const contentType = response.headers.get("content-type");
    if (contentType && contentType.includes("application/json")) {
        return await response.json();
    }
    return null;
}

function escapeHtml(text) {
    if (text === null || text === undefined) return '';
    const str = String(text);
    const div = document.createElement('div');
    div.textContent = str;
    return div.innerHTML;
}

function debounce(fn, delay) {
    let timer;
    return (...args) => {
        clearTimeout(timer);
        timer = setTimeout(() => fn(...args), delay);
    };
}

let pendingUpdates = [];

let rafId = null;

function queueFlightUpdate(flightId, flightData, changedFields, oldFlight) {
    if (oldFlight && changedFields) {
        addUpdateMessage(oldFlight, { ...oldFlight, ...flightData, flight_id: flightData.flight_id || flightId }, changedFields);
    }
    pendingUpdates.push({ flightId, flightData, changedFields });
    if (!rafId) {
        rafId = requestAnimationFrame(flushFlightUpdates);
    }
}

function flushFlightUpdates() {
    pendingUpdates.forEach(({ flightId, flightData, changedFields }) => {
        try {
            updateFlightLocally(flightId, flightData, changedFields);
        } catch (e) {
            console.error(`Error updating flight ${flightId}:`, e);
        }
    });

    if (hasActiveSearchOrBusinessFilters()) {
        applyCurrentFilters();
    }

    // Update time once per batch
    updateLastUpdated();

    pendingUpdates = [];
    rafId = null;
}

async function init() {
    // Require authentication (with auto-refresh)
    if (!await Auth.requireAuthAsync()) return;

    await loadAirportContextV2();
    ensureConnectionStatusElement();
    setConnectionStatus('connecting');
    observeFloatingBadgeLayout();
    window.addEventListener('resize', throttleRAF(syncFloatingBadgeLayout), { passive: true });
    await loadAICapabilities();
    updateDispatchNotifyEntryState();
    syncFloatingBadgeLayout();

    setupEventListeners();
    setupFlightListEventDelegation(); // 事件委托初始化
    setupPageUnloadHandler(); // 页面卸载时关闭 SSE 连接
    setupScrollOptimization(); // 滚动性能优化
    initFlightWorker(); // Web Worker 初始化
    updateLastUpdated();
    updateAnomalyFloatingButton();


    // Initialize Table View
    loadTableConfig();
    setupColumnConfig();
    toggleView(currentView);

    if (window.innerWidth <= 768) {
        toggleView('card');
    }

    // Connect to SSE stream for real-time flight updates
    if (typeof SSEHub !== 'undefined' && typeof SSEHub.connect === 'function') {
        SSEHub.connect();
    }
    connectToFlightSSE();
    connectToNotificationStream();
    void loadDispatchNotifyPendingReceipts().catch((error) => {
        console.warn('预加载待确认回执失败:', error);
    });
    void loadDispatchNotifySentReceiptGroups({ preserveSelection: true }).catch((error) => {
        console.warn('预加载已发回执失败:', error);
    });
    loadFlights({ silent: true, showLoading: false });
}

function setupScrollOptimization() {
    // 航班列表滚动优化
    if (flightListElement) {
        flightListElement.addEventListener('scroll', throttleRAF(() => {
            // 滚动时的优化处理（未来可用于虚拟滚动）
        }), { passive: true });
    }

    // 表格滚动优化
    const tableWrapper = document.querySelector('.table-scroll-wrapper');
    if (tableWrapper) {
        tableWrapper.addEventListener('scroll', throttleRAF(() => {
            // 表格滚动时的优化处理
        }), { passive: true });
    }
}

function setupPageUnloadHandler() {
    window.addEventListener('beforeunload', () => {
        suppressFlightReconnect = true;
        if (window.flightStreamSource) {
            window.flightStreamSource.close();
            window.flightStreamSource = null;
        }
        closeNotificationStream();
        if (windowAnomalyStreamSource) {
            windowAnomalyStreamSource.close();
            windowAnomalyStreamSource = null;
        }
        disconnectFlightChatEventStream();
        if (sseReconnectTimer) {
            clearTimeout(sseReconnectTimer);
            sseReconnectTimer = null;
        }
        if (anomalyReconnectTimer) {
            clearTimeout(anomalyReconnectTimer);
            anomalyReconnectTimer = null;
        }
    });

    // 页面隐藏时也关闭（移动端切换标签页）
    document.addEventListener('visibilitychange', () => {
        if (document.visibilityState === 'hidden') {
            suppressFlightReconnect = true;
            if (window.flightStreamSource) {
                window.flightStreamSource.close();
                window.flightStreamSource = null;
                setConnectionStatus('offline');
            }
            closeNotificationStream();
            disconnectFlightChatEventStream();
            if (sseReconnectTimer) {
                clearTimeout(sseReconnectTimer);
                sseReconnectTimer = null;
            }
            if (anomalyReconnectTimer) {
                clearTimeout(anomalyReconnectTimer);
                anomalyReconnectTimer = null;
            }
        } else if (document.visibilityState === 'visible') {
            suppressFlightReconnect = false;
            // 页面重新可见时重新连接
            if (!window.flightStreamSource) {
                connectToFlightStream();
            }
            if (!window.notificationStreamSource) {
                connectToNotificationStream();
            }
        }
    });
}

async function connectToFlightStream() {
    // Ensure we have a valid token before connecting
    if (!await Auth.requireAuthAsync()) return;

    if (typeof Auth.refreshSSEToken === 'function') {
        await Auth.refreshSSEToken();
    }

    if (sseReconnectTimer) {
        clearTimeout(sseReconnectTimer);
        sseReconnectTimer = null;
    }

    setConnectionStatus('connecting');

    // Close any existing connection
    if (window.flightStreamSource) {
        suppressFlightReconnect = true;
        try {
            window.flightStreamSource.close();
        } catch (_error) {
            // ignore close errors
        }
        window.flightStreamSource = null;
    }
    suppressFlightReconnect = false;

    // 全面转向 SSE，移除 WebSocket
    connectToFlightSSE();
}

function handleFlightRealtimePayload(data) {
    if (!data || typeof data !== 'object') {
        return;
    }

    if (data.type === 'heartbeat') {
        setConnectionStatus('online');
        return;
    }

    if (data.type === 'initial_data') {
        const initialFlights = preprocessFlightsBatch(data.flights || []);
        flights = initialFlights;
        originalFlights = [...flights];
        applyCurrentFilters();
        updateEditButtonState();
        announce(`已加载 ${flights.length} 条航班数据`);
        // 实时流首包可能是裁剪快照，始终再做一次分页全量对账，避免切回页面后退化成 300 条。
        if (realtimeFullSyncTimer) {
            clearTimeout(realtimeFullSyncTimer);
        }
        realtimeFullSyncTimer = setTimeout(() => {
            realtimeFullSyncTimer = null;
            loadFlights({ silent: true, showLoading: false });
        }, 0);
        return;
    }

    const incrementalPayload = data.patch || data.data || data.flight_data || data.flight;
    const flightId = normalizeFlightId(data.flight_id || (incrementalPayload && incrementalPayload.flight_id));
    if (!flightId || !incrementalPayload) {
        return;
    }

    const changedFields = Array.isArray(data.changed_fields)
        ? data.changed_fields.map(field => String(field))
        : Object.keys(incrementalPayload).filter(field => !['flight_id', 'version', 'updated_at'].includes(field));
    const patch = { ...incrementalPayload, flight_id: incrementalPayload.flight_id || flightId };
    const oldFlight = flights.find(f => String(f.flight_id) === String(flightId));
    queueFlightUpdate(flightId, patch, changedFields, oldFlight);
}

function handleFlightLabelsChanged(data) {
    const { flight_id, leg_type, action, code, labels } = data;

    const flightId = normalizeFlightId(flight_id);
    if (!flightId) {
        console.debug('[LabelSync] Unknown flight_id:', flight_id);
        return;
    }

    const flight = flights.find(f => String(f.flight_id) === String(flightId));
    if (!flight) {
        console.debug('[LabelSync] Flight not found:', flightId);
        return;
    }

    if (leg_type) {
        const leg = leg_type === 'inbound' ? flight.inbound_leg : flight.outbound_leg;
        if (leg) {
            leg.labels = labels;
        }
    } else {
        flight.labels = labels;
    }

    if (window.currentOpenFlightId === flightId) {
        updateFlightLabelsInPanel(flightId);
    }

    console.log(`[LabelSync] Flight ${flightId} ${action} label: ${code}`);
}

function updateFlightLabelsInPanel(flightId) {
    const flight = flights.find(f => String(f.flight_id) === String(flightId));
    if (!flight) return;

    const labelCard = document.querySelector('.labels-card');
    if (labelCard && typeof renderFlightLabelsSection === 'function') {
        labelCard.outerHTML = renderFlightLabelsSection(flight);
    }
}

let anomalyData = {};

let windowAnomalyStreamSource = null;

async function connectToAnomalyStream() {
    const token = Auth.getToken();
    if (!token) {
        return;
    }

    if (typeof Auth.refreshSSEToken === 'function') {
        await Auth.refreshSSEToken();
    }

    if (windowAnomalyStreamSource) {
        try {
            windowAnomalyStreamSource.close();
        } catch (_error) {
            // ignore
        }
        windowAnomalyStreamSource = null;
    }
    if (anomalyReconnectTimer) {
        clearTimeout(anomalyReconnectTimer);
        anomalyReconnectTimer = null;
    }

    // 全面转向 SSE，移除 WebSocket
    connectToAnomalySSE(token);
}

function handleAnomalyRealtimePayload(data) {
    if (!data || typeof data !== 'object') {
        return;
    }

    if (data.type === 'heartbeat') {
        return;
    }

    if (data.type === 'initial_data') {
        const initialAnomalies = data.data || data.items || [];
        anomalyData = {};
        initialAnomalies.forEach(a => {
            const fid = String(a.flight_id);
            if (!anomalyData[fid]) anomalyData[fid] = { anomalies: [] };
            anomalyData[fid].anomalies.push(a);
        });

        updateAnomalyFloatingButton();
        if (currentView === 'alert') {
            scheduleViewRender(renderAlertPoolView, 0);
        }
        return;
    }

    const anomaly = data.data || data.anomaly || null;
    if (!anomaly) {
        return;
    }
    const fid = String(anomaly.flight_id);
    if (!anomalyData[fid]) anomalyData[fid] = { anomalies: [] };

    const anomalyId = String(anomaly.anomaly_id || anomaly.id || '');
    const existingIndex = anomalyData[fid].anomalies.findIndex((a) => String(a.anomaly_id || a.id || '') === anomalyId);
    if (data.status === 'resolved' || anomaly.status === 'resolved') {
        if (existingIndex > -1) {
            anomalyData[fid].anomalies.splice(existingIndex, 1);
        }
    } else {
        if (existingIndex > -1) {
            anomalyData[fid].anomalies[existingIndex] = anomaly;
        } else {
            anomalyData[fid].anomalies.push(anomaly);
        }
    }

    if (anomalyData[fid].anomalies.length === 0) {
        delete anomalyData[fid];
    }

    updateAnomalyFloatingButton();
    if (currentView === 'alert') {
        scheduleViewRender(renderAlertPoolView, 0);
    }
}

const updateMessages = [];

let unreadCount = 0;

let isPanelOpen = false;

const FIELD_NAMES = {
    status: '状态', inbound_leg: '进港航段', outbound_leg: '出港航段',
    stand: '机位', gate: '登机口', flight_number: '主航班号',
    scheduled_departure: '计划起飞', scheduled_arrival: '计划到达',
    estimated_departure: '预计起飞', estimated_arrival: '预计到达',
    actual_departure: '实际起飞', actual_arrival: '实际到达',
    start_boarding_time: '开始登机', end_boarding_time: '结束登机',
    aircraft_type_detail: '机型', cobt_time: 'COBT',
    has_boarding_restriction: '登机限制', is_quick_turnaround: '快速过站',

    // Extended Time Fields
    on_blocks_time: '上轮挡', cabin_door_open_time: '开舱门',
    deboarding_complete_time: '下客完成', cleaning_start_time: '清洁开始',
    cleaning_end_time: '清洁结束', cabin_door_close_time: '关客舱门',
    cargo_door_close_time: '关货舱门', loading_complete_time: '装载完成',
    off_blocks_time: '撤轮挡', passenger_ready_time: '人齐',
    boarding_allowed_time: '允许登机',

    // Other Fields
    baggage_carousel: '行李转盘',
    flight_remarks: '航班备注', load_planning_remarks: '配载备注',
    aircraft_maintenance_remarks: '机务备注', aircraft_check_remarks: '复核机号'
};

function getFlightNo(flight) {
    return getPrimaryFlightNoV2(flight) || flight.flight_id;
}

function formatValue(val) {
    if (val === null || val === undefined) return '空';
    if (Array.isArray(val)) return val.join(', ');
    if (typeof val === 'boolean') return val ? '是' : '否';
    if (typeof val === 'string' && val.includes('T')) {
        try { return new Date(val).toLocaleString('zh-CN'); } catch { return val; }
    }
    return String(val);
}

function addUpdateMessage(oldFlight, newData, changedFields) {
    const flightNo = getFlightNo(oldFlight);
    const now = new Date().toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', second: '2-digit' });

    let addedCount = 0;
    changedFields.forEach(field => {
        const fieldName = FIELD_NAMES[field] || field;
        const oldVal = formatValue(oldFlight[field]);
        const newVal = formatValue(newData[field]);

        if (oldVal === newVal) {
            // console.log(`DEBUG: Ignoring identical update for ${fieldName}: ${oldVal} ->${newVal}`);
            return;
        }

        updateMessages.unshift({
            id: `flight:${flightNo}:${field}:${now}:${addedCount}`,
            kind: 'flight_update',
            time: now,
            flightNo,
            field: fieldName,
            oldValue: oldVal,
            newValue: newVal
        });
        addedCount++;

        // --- EP-04: 关键保障节点完成 → 强视觉通知 ---
        if (newVal !== '空' && newVal !== '--') {
            const milestoneFields = ['cleaning_end_time', 'boarding_allowed_time'];
            if (milestoneFields.includes(field)) {
                triggerMilestonePulse(flightNo, fieldName);
            }
        }
    });

    // 最多保留100条
    if (updateMessages.length > 100) updateMessages.length = 100;

    if (!isPanelOpen) {
        unreadCount += addedCount;
        updateBadge();
    }
    renderUpdatePanel();
}

function syncUpdatePanelLayout() {
    const panel = document.getElementById('updatePanel');
    if (!(panel instanceof HTMLElement) || panel.hidden) {
        return;
    }

    const stack = document.getElementById('floatingBadgeStack');
    const viewportWidth = Math.max(window.innerWidth || 0, 320);
    const viewportHeight = Math.max(window.innerHeight || 0, 320);
    const isMobile = viewportWidth <= 768;
    const margin = isMobile ? 12 : 24;
    const gap = 12;

    panel.style.left = '';
    panel.style.right = '';
    panel.style.top = '';
    panel.style.bottom = '';
    panel.style.width = '';
    panel.style.maxHeight = '';

    const stackRect = stack instanceof HTMLElement ? stack.getBoundingClientRect() : null;
    const stackVisible = Boolean(
        stackRect
        && stackRect.width > 0
        && stackRect.height > 0
        && isFloatingBadgeVisible(stack),
    );

    if (!stackVisible) {
        panel.style.right = `${margin}px`;
        panel.style.bottom = `${isMobile ? 72 : 80}px`;
        panel.style.width = `${Math.min(isMobile ? viewportWidth - margin * 2 : 380, viewportWidth - margin * 2)}px`;
        panel.style.maxHeight = `${Math.max(240, viewportHeight - margin * 2)}px`;
        return;
    }

    const availableLeft = Math.max(0, stackRect.left - gap - margin);
    if (!isMobile && availableLeft >= 320) {
        panel.style.right = `${Math.max(margin, Math.round(viewportWidth - stackRect.left + gap))}px`;
        panel.style.bottom = `${Math.max(margin, Math.round(viewportHeight - stackRect.bottom))}px`;
        panel.style.width = `${Math.min(380, availableLeft)}px`;
        panel.style.maxHeight = `${Math.max(260, viewportHeight - margin * 2)}px`;
        return;
    }

    const availableAbove = Math.max(0, stackRect.top - gap - margin);
    if (availableAbove >= 260) {
        panel.style.right = `${margin}px`;
        panel.style.bottom = `${Math.max(margin, Math.round(viewportHeight - stackRect.top + gap))}px`;
        panel.style.width = `${Math.min(isMobile ? viewportWidth - margin * 2 : 380, viewportWidth - margin * 2)}px`;
        panel.style.maxHeight = `${Math.max(240, Math.round(availableAbove))}px`;
        return;
    }

    panel.style.left = `${margin}px`;
    panel.style.right = `${margin}px`;
    panel.style.bottom = `${margin}px`;
    panel.style.width = 'auto';
    panel.style.maxHeight = `${Math.max(220, viewportHeight - margin * 2)}px`;
}

function getUnreadStandardNotificationEntries() {
    return updateMessages.filter((message) => (
        message?.kind === 'user_notification'
        && !message?.receiptRequired
        && !message?.isRead
    ));
}

function markStandardNotificationEntriesReadLocally() {
    const unreadIds = new Set(
        getUnreadStandardNotificationEntries()
            .map((message) => String(message.notificationId || '').trim())
            .filter(Boolean),
    );
    if (unreadIds.size <= 0) {
        return;
    }
    for (let index = 0; index < updateMessages.length; index += 1) {
        const message = updateMessages[index];
        if (
            message?.kind === 'user_notification'
            && unreadIds.has(String(message.notificationId || '').trim())
        ) {
            updateMessages[index] = {
                ...message,
                isRead: true,
            };
        }
    }
    unreadIds.forEach((notificationId) => {
        removeNotificationToast(notificationId);
    });
    renderUpdatePanel();
}

async function markStandardNotificationsReadOnOpen() {
    if (getUnreadStandardNotificationEntries().length <= 0) {
        return;
    }
    const response = await Auth.fetch(`${window.location.origin}/api/v2/notifications/read-all`, {
        method: 'POST',
    });
    const payload = await response.json().catch(() => ({}));
    if (!response.ok || !payload?.success) {
        throw new Error(extractApiErrorMessage(payload, `批量已读失败 (HTTP ${response.status})`));
    }
    markStandardNotificationEntriesReadLocally();
}

function toggleUpdatePanel() {
    const badge = document.getElementById('updateBadge');
    const panel = document.getElementById('updatePanel');
    if (!panel) return;
    isPanelOpen = !isPanelOpen;
    panel.hidden = !isPanelOpen;
    if (badge) {
        badge.setAttribute('aria-expanded', isPanelOpen ? 'true' : 'false');
    }
    if (isPanelOpen) {
        unreadCount = 0;
        updateBadge();
        syncUpdatePanelLayout();
        const closeBtn = document.getElementById('closeUpdatePanelBtn');
        if (closeBtn) closeBtn.focus();
        void markStandardNotificationsReadOnOpen().catch((error) => {
            console.warn('同步普通通知已读失败:', error);
            showToast(error?.message || '同步通知已读失败', 'error', 3200);
        });
        announce('更新消息面板已打开');
    } else {
        announce('更新消息面板已关闭');
    }
}

function updateFlightRow(flightId, flightData, changedFields) {
    if (currentView !== 'table') return;

    const tr = document.querySelector(`tr[data-flight-id="${flightId}"]`);
    if (!tr) {
        // Row not found? Maybe new flight. Fallback to add row or full render if needed.
        // For simplicity, if not found and should be visible, maybe trigger full render?
        // But usually updates come for existing flights.
        // Let's check filter criteria. If passed filters but not in DOM, we might need to insert.
        // For now, assume update existing.
        return;
    }

    if (!changedFields || changedFields.length === 0) return;

    changedFields.forEach(field => {
        // Find cell by data-field
        // Note: Field names in changedFields might need mapping if using aliases?
        // Our DEFAULT_COLUMNS keys match DB fields mostly.
        const td = tr.querySelector(`td[data-field="${field}"]`);
        if (td) {
            const renderer = FIELD_MAP[field];
            const newValue = renderer ? renderer(flightData) : (flightData[field] || '-');

            // Only update if content changed (renderer might return same HTML)
            if (td.innerHTML !== newValue) {
                td.innerHTML = newValue;
                // Add highlight effect
                td.classList.add('flash-update');
                setTimeout(() => td.classList.remove('flash-update'), 1000);
            }
        }
    });
}

let flightListDelegationSetup = false;

function setupFlightListEventDelegation() {
    if (flightListDelegationSetup) return; // 避免重复绑定
    flightListDelegationSetup = true;

    // Card 视图事件委托
    if (flightListElement) {
        flightListElement.addEventListener('click', (e) => {
            const flightItem = e.target.closest('.flight-item');
            if (!flightItem) return;

            const flightId = flightItem.dataset.flightId || flightItem.dataset.flight_id;
            if (!flightId) return;

            if (isSameFlightId(selectedFlightId, flightId)) {
                // 已选中 - 切换航线显示模式
                toggleRouteDisplayMode();
            } else {
                selectFlight(flightId);
            }
        });

        flightListElement.addEventListener('keydown', (e) => {
            const flightItem = e.target.closest('.flight-item');
            if (!flightItem) return;

            const flightId = flightItem.dataset.flightId || flightItem.dataset.flight_id;
            if (!flightId) return;

            if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                selectFlight(flightId);
                return;
            }

            if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
                e.preventDefault();
                const items = Array.from(flightListElement.querySelectorAll('.flight-item'));
                const index = items.indexOf(flightItem);
                if (index < 0) return;
                const delta = e.key === 'ArrowDown' ? UI_CONSTANTS.listKeyboardStep : -UI_CONSTANTS.listKeyboardStep;
                const nextIndex = Math.min(items.length - 1, Math.max(0, index + delta));
                const nextItem = items[nextIndex];
                if (nextItem) {
                    nextItem.focus();
                    const nextFlightId = nextItem.dataset.flightId || nextItem.dataset.flight_id;
                    if (nextFlightId) {
                        selectFlight(nextFlightId);
                    }
                }
            }
        });
    }

    // Table 视图事件委托
    const tableBody = document.getElementById('flightTableBody');
    if (tableBody) {
        tableBody.addEventListener('click', (e) => {
            const row = e.target.closest('tr[data-flight-id]');
            if (!row) return;

            const flightId = row.dataset.flightId;
            if (!flightId) return;

            selectFlight(flightId);
        });

        tableBody.addEventListener('keydown', (e) => {
            const row = e.target.closest('tr[data-flight-id]');
            if (!row) return;

            const rows = Array.from(tableBody.querySelectorAll('tr[data-flight-id]'));
            const index = rows.indexOf(row);
            if (index < 0) return;

            if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                const flightId = row.dataset.flightId;
                if (flightId) selectFlight(flightId);
                return;
            }

            if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
                e.preventDefault();
                const delta = e.key === 'ArrowDown' ? 1 : -1;
                const nextIndex = Math.min(rows.length - 1, Math.max(0, index + delta));
                const nextRow = rows[nextIndex];
                if (!nextRow) return;
                nextRow.focus();
                const flightId = nextRow.dataset.flightId;
                if (flightId) selectFlight(flightId);
            }
        });
    }
}

function updateFlightCard(flightId, flightData, changedFields) {
    if (currentView === 'table') return;

    const cardEl = document.querySelector(`.flight-item[data-flight-id="${flightId}"], .flight-item[data-flight_id="${flightId}"]`);
    if (!cardEl) return;

    // 如果没有指定变更字段，执行完整重渲染
    if (!changedFields || changedFields.length === 0) {
        renderFlightList();
        return;
    }

    // 增量更新：只更新变化的字段
    // 状态更新
    if (changedFields.includes('status')) {
        const statusEl = cardEl.querySelector('.flight-status');
        if (statusEl) {
            statusEl.className = `flight-status ${getStatusClass(flightData.status)}`;
            statusEl.textContent = flightData.status || '计划中';
            statusEl.classList.add('flash-update');
            setTimeout(() => statusEl.classList.remove('flash-update'), 1000);
        }
    }

    // 机位更新
    if (changedFields.includes('stand')) {
        // 需要重新渲染以更新机位显示
        renderFlightList();
        return;
    }

    // 登机口更新
    if (changedFields.includes('gate')) {
        renderFlightList();
        return;
    }

    // 时间字段更新 - 这些变化较复杂，需要重渲染
    const timeFields = ['scheduled_departure', 'scheduled_arrival', 'estimated_departure',
        'estimated_arrival', 'actual_departure', 'actual_arrival'];
    if (changedFields.some(f => timeFields.includes(f))) {
        renderFlightList();
        return;
    }

    // 其他重要字段变化也触发重渲染
    const criticalFields = ['flight_number', 'inbound_leg', 'outbound_leg'];
    if (changedFields.some(f => criticalFields.includes(f))) {
        renderFlightList();
        return;
    }
}

function updateFlightInUI(flightId, flightData, changedFields) {
    // Deprecated by delta updates inline
}

function normalizeSignedFlag(value) {
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

function updateBusinessResetButtonState() {
    const resetBtn = document.getElementById('resetBusinessFiltersBtn');
    if (!resetBtn) return;

    const shouldHide = isBusinessFilterDefaultState();
    resetBtn.classList.toggle('is-hidden', shouldHide);
    resetBtn.disabled = shouldHide;
}

function isWideBodyAircraft(aircraftTypeDetail) {
    const code = String(aircraftTypeDetail || '').toUpperCase().replace(/[^A-Z0-9]/g, '');
    if (!code) {
        return false;
    }
    return /^(A330|A340|A350|A380|B747|B767|B777|B787)/.test(code);
}

function deriveOperationDateLabel(flight) {
    const candidates = [
        flight.scheduled_departure,
        flight.estimated_departure,
        flight.scheduled_arrival,
        flight.estimated_arrival,
    ];
    for (let i = 0; i < candidates.length; i++) {
        const value = candidates[i];
        if (!value) {
            continue;
        }
        const date = new Date(value);
        if (!Number.isNaN(date.getTime())) {
            return date.toLocaleDateString('zh-CN');
        }
    }
    return EMPTY_DISPLAY_TEXT;
}

function getAnomalyCountForFlight(flight) {
    const flightId = normalizeFlightId(flight?.flight_id);
    const anomalyItems = flightId && anomalyData[flightId] && Array.isArray(anomalyData[flightId].anomalies)
        ? anomalyData[flightId].anomalies
        : [];
    const summaryOpenCount = Number(flight?.anomaly_summary?.open_count || 0);
    return Math.max(Number(flight?.anomaly_count || 0), summaryOpenCount, anomalyItems.length);
}

function getAnomaliesForFlight(flight) {
    const flightId = normalizeFlightId(flight?.flight_id);
    if (!flightId || !anomalyData[flightId] || !Array.isArray(anomalyData[flightId].anomalies)) {
        return [];
    }
    return anomalyData[flightId].anomalies;
}

function getAnomalySeverityWeight(severity) {
    switch (String(severity || '').trim().toLowerCase()) {
        case 'critical':
            return 10;
        case 'high':
            return 5;
        case 'medium':
            return 2;
        case 'low':
            return 1;
        default:
            return 0;
    }
}

function getFlightHighestAnomalySeverity(flight) {
    const anomalies = Array.isArray(flight?.anomalies) ? flight.anomalies : getAnomaliesForFlight(flight);
    const highestWeight = anomalies.reduce((maxWeight, anomaly) => {
        return Math.max(maxWeight, getAnomalySeverityWeight(anomaly?.severity));
    }, 0);

    if (highestWeight >= 10) return 'critical';
    if (highestWeight >= 5) return 'high';
    if (highestWeight >= 2) return 'medium';
    if (getAnomalyCountForFlight(flight) > 0) return 'low';
    return 'none';
}

function getAbnormalFlightCount(sourceFlights = flights) {
    return buildAbnormalFlightList(sourceFlights).length;
}

function getHighestAnomalySeverity(sourceFlights = flights) {
    return buildAbnormalFlightList(sourceFlights).reduce((highest, flight) => {
        const next = getFlightHighestAnomalySeverity(flight);
        return getAnomalySeverityWeight(next) > getAnomalySeverityWeight(highest) ? next : highest;
    }, 'none');
}

function updateAnomalyFloatingButton(sourceFlights = flights) {
    const button = document.getElementById('openAnomalyBadgeBtn');
    const countEl = document.getElementById('openAnomalyBadgeCount');
    if (!(button instanceof HTMLButtonElement) || !(countEl instanceof HTMLElement)) {
        return;
    }

    const abnormalCount = getAbnormalFlightCount(sourceFlights);
    const highestSeverity = getHighestAnomalySeverity(sourceFlights);
    const isVisible = abnormalCount > 0;

    countEl.textContent = String(abnormalCount);
    button.hidden = !isVisible;
    button.style.display = isVisible ? 'inline-flex' : 'none';
    button.dataset.severity = highestSeverity;
    button.setAttribute('aria-hidden', isVisible ? 'false' : 'true');
    button.setAttribute(
        'aria-label',
        isVisible ? `打开异常告警视图，当前有 ${abnormalCount} 个异常航班` : '当前没有异常航班',
    );
    button.title = isVisible ? `异常航班 ${abnormalCount}` : '当前没有异常航班';

    syncFloatingBadgeLayout();
}

function hasVipMarker(flight) {
    return Boolean(getLegVipFlagV2(flight, 'inbound') || getLegVipFlagV2(flight, 'outbound'));
}

function isCommercialSignedFlight(flight) {
    const raw = flight?.commercial_signed ?? flight?.is_commercial_signed;
    if (typeof raw === 'boolean') {
        return raw;
    }
    const normalized = String(raw ?? '').trim().toLowerCase();
    if (!normalized) {
        return true;
    }
    if (normalized === 'yes' || normalized === 'true' || normalized === '1') {
        return true;
    }
    if (normalized === 'no' || normalized === 'false' || normalized === '0') {
        return false;
    }
    return true;
}

function isWideBodyFlight(flight) {
    const raw = String(flight?.aircraft_type_detail || flight?.aircraft_type || '').toLowerCase();
    return raw.includes('wide') || raw.includes('330') || raw.includes('350') || raw.includes('777') || raw.includes('787') || raw.includes('747');
}

function isDelayedFlight(flight) {
    const status = String(flight?.status || '').toLowerCase();
    if (status.includes('延误')) {
        return true;
    }
    const delayPairs = [
        [flight?.estimated_departure, flight?.scheduled_departure],
        [flight?.estimated_arrival, flight?.scheduled_arrival],
    ];
    return delayPairs.some(([estimate, schedule]) => {
        if (!estimate || !schedule) {
            return false;
        }
        const diffMs = new Date(estimate).getTime() - new Date(schedule).getTime();
        return Number.isFinite(diffMs) && diffMs >= 15 * 60 * 1000;
    });
}

function getCurrentSearchQuery() {
    const searchInput = document.getElementById('searchInput');
    return searchInput ? searchInput.value.trim() : '';
}

function filterFlights(query, options = {}) {
    const { sourceFlights = originalFlights } = options;
    const searchFields = getSearchFieldFilters();
    const filteredByBusinessRules = applyBusinessFiltersLocal(sourceFlights, businessFilters);

    if (!query || !query.trim()) {
        return filteredByBusinessRules;
    }

    const normalizedQuery = query.toLowerCase().trim();
    const searchMatched = filteredByBusinessRules.filter(flight => {
        const inboundFlightNo = getLegFieldV2(flight, 'inbound', 'flight_no').toLowerCase();
        const outboundFlightNo = getLegFieldV2(flight, 'outbound', 'flight_no').toLowerCase();
        const destination = getRouteEndpointV2(flight, 'outbound', 'code').toLowerCase();
        const destinationName = getRouteEndpointV2(flight, 'outbound', 'name').toLowerCase();
        const origin = getRouteEndpointV2(flight, 'inbound', 'code').toLowerCase();
        const originName = getRouteEndpointV2(flight, 'inbound', 'name').toLowerCase();

        const status = flight.status ? flight.status.toLowerCase() : '';
        const aircraftType = flight.aircraft_type_detail ? flight.aircraft_type_detail.toLowerCase() : '';
        const stand = flight.stand ? flight.stand.toLowerCase() : '';
        const gate = flight.gate ? flight.gate.toLowerCase() : '';
        const mission = getMissionSearchTextV2(flight).toLowerCase();
        const flightType = getFlightTypeSummaryV2(flight).toLowerCase();

        return (searchFields.searchFlightNo && (inboundFlightNo.includes(normalizedQuery) || outboundFlightNo.includes(normalizedQuery))) ||
            (searchFields.searchDestination && destination.includes(normalizedQuery)) ||
            (searchFields.searchDestinationName && destinationName.includes(normalizedQuery)) ||
            (searchFields.searchOrigin && origin.includes(normalizedQuery)) ||
            (searchFields.searchOriginName && originName.includes(normalizedQuery)) ||
            (searchFields.searchStatus && status.includes(normalizedQuery)) ||
            (searchFields.searchAircraftType && aircraftType.includes(normalizedQuery)) ||
            (searchFields.searchStand && stand.includes(normalizedQuery)) ||
            (searchFields.searchGate && gate.includes(normalizedQuery)) ||
            (searchFields.searchMission && mission.includes(normalizedQuery)) ||
            (searchFields.searchFlightType && flightType.includes(normalizedQuery));
    });
    return searchMatched;
}

let isRefreshing = false;

async function runWithRetry(task, retries = 2, retryDelayMs = 600) {
    let lastError = null;
    for (let attempt = 0; attempt <= retries; attempt += 1) {
        try {
            return await task();
        } catch (error) {
            lastError = error;
            if (attempt >= retries) {
                break;
            }
            await new Promise((resolve) => setTimeout(resolve, retryDelayMs * (attempt + 1)));
        }
    }
    throw lastError;
}

async function loadFlightsPagedData() {
    const merged = [];
    const seen = new Set();
    let page = 1;

    while (true) {
        const pageItems = await fetchFlightsPageData(page);
        for (const flight of pageItems) {
            const flightId = String((flight && flight.flight_id) || '').trim();
            if (!flightId) {
                merged.push(flight);
                continue;
            }
            if (seen.has(flightId)) {
                continue;
            }
            seen.add(flightId);
            merged.push(flight);
        }

        if (pageItems.length < FLIGHT_LIST_PAGE_SIZE) {
            break;
        }

        page += 1;
    }

    return merged;
}

async function loadFlights(options = {}) {
    const silent = options && options.silent === true;
    const showLoading = !(options && options.showLoading === false);

    // 幂等性：如果正在刷新，直接返回
    if (isRefreshing) {
        return;
    }

    isRefreshing = true;
    if (showLoading) {
        setRefreshButtonLoading(true);
    }

    try {
        // 确保标签定义已加载（首次调用时异步拉取，后续命中缓存）
        await loadLabelDefinitions();
        const allFlights = await loadFlightsPagedData();
        flights = preprocessFlightsBatch(allFlights || []);
        originalFlights = [...flights]; // Store original flights
        if (typeof invalidateAllDispatchOrderCaches === 'function') {
            invalidateAllDispatchOrderCaches();
        }
        applyCurrentFilters();
        updateAnomalyFloatingButton();
        updateEditButtonState(); // 更新编辑按钮状态
        if (!silent) {
            announce(`航班数据刷新完成，共 ${flights.length} 条`);
        }
    } catch (error) {
        console.error('Error loading flights:', error);
        // Fallback to sample data if API is not available
        flights = preprocessFlightsBatch(getSampleFlights());
        originalFlights = [...flights]; // Store original flights
        if (typeof invalidateAllDispatchOrderCaches === 'function') {
            invalidateAllDispatchOrderCaches();
        }
        applyCurrentFilters();
        updateAnomalyFloatingButton();
        updateEditButtonState(); // 更新编辑按钮状态
        if (!silent) {
            announce('航班数据加载失败，已切换为示例数据');
        }
    } finally {
        // 重置幂等性标志和按钮状态
        isRefreshing = false;
        if (showLoading) {
            setRefreshButtonLoading(false);
        }
    }
}

function getSampleFlights() {
    return [
        {
            flight_id: "1",
            flight_number: "CZ5678",
            inbound_leg: {
                leg_type: "inbound",
                flight_no: "CZ1234",
                flight_type: "domestic",
                mission: 20,
                origin_stations: [{ code: "PEK", name: "北京" }],
                destination_stations: [],
                is_vip: false
            },
            outbound_leg: {
                leg_type: "outbound",
                flight_no: "CZ5678",
                flight_type: "domestic",
                mission: 20,
                origin_stations: [],
                destination_stations: [{ code: "SHA", name: "上海" }],
                is_vip: true
            },
            status: "正在登机",
            scheduled_departure: "2025-10-29T15:30:00Z",
            scheduled_arrival: "2025-10-29T14:20:00Z",
            estimated_departure: "2025-10-29T15:35:00Z",
            estimated_arrival: "2025-10-29T14:25:00Z",
            actual_arrival: "2025-10-29T14:18:00Z",
            actual_departure: null,
            start_boarding_time: "2025-10-29T14:45:00Z",
            end_boarding_time: null,
            cobt_time: "2025-10-29T14:50:00Z",
            aircraft_type_detail: "A320",
            stand: "A12",
            gate: "B15",
            has_boarding_restriction: false,
            is_quick_turnaround: true,
            business_cases: [
                {
                    case_id: 101,
                    case_type: "开始登机",
                    description: "航班CZ5678开始登机",
                    status: "SUCCESS",
                    flight_no: "CZ5678",
                    stand_no: "A12",
                    gate_no: "B15",
                    created_at: "2025-10-29T14:45:00Z",
                    created_by: "系统管理员",
                    case_creator: "系统管理员",
                    last_updated_by: "系统管理员",
                    on_update_time: "2025-10-29T14:45:00Z",
                    on_finish_time: null,
                    on_cancel_time: null,
                    log: ["登机口B15已开放", "开始办理登机手续"]
                }
            ]
        },
        {
            flight_id: "2",
            flight_number: "MU9876",
            inbound_leg: {
                leg_type: "inbound",
                flight_no: "MU9876",
                flight_type: "domestic",
                mission: 20,
                origin_stations: [{ code: "CTU", name: "成都" }],
                destination_stations: [],
                is_vip: false
            },
            outbound_leg: null,
            status: "前方起飞",
            scheduled_departure: "2025-10-29T13:20:00Z",
            scheduled_arrival: null,
            estimated_departure: null,
            estimated_arrival: null,
            actual_arrival: null,
            actual_departure: "2025-10-29T13:22:00Z",
            start_boarding_time: "2025-10-29T12:50:00Z",
            end_boarding_time: "2025-10-29T13:15:00Z",
            cobt_time: null,
            aircraft_type_detail: "B737",
            stand: "C08",
            gate: "A20",
            has_boarding_restriction: true,
            is_quick_turnaround: false,
            business_cases: [
                {
                    case_id: 102,
                    case_type: "航班起飞",
                    description: "航班MU9876已起飞",
                    status: "SUCCESS",
                    flight_no: "MU9876",
                    stand_no: "C08",
                    gate_no: "A20",
                    created_at: "2025-10-29T13:22:00Z",
                    created_by: "系统管理员",
                    case_creator: "系统管理员",
                    last_updated_by: "系统管理员",
                    on_update_time: "2025-10-29T13:22:00Z",
                    on_finish_time: "2025-10-29T13:22:00Z",
                    on_cancel_time: null,
                    log: ["航班正常起飞", "预计到达时间16:30"]
                }
            ]
        },
        {
            flight_id: "3",
            flight_number: "CA2468",
            inbound_leg: null,
            outbound_leg: {
                leg_type: "outbound",
                flight_no: "CA2468",
                flight_type: "domestic",
                mission: 20,
                origin_stations: [],
                destination_stations: [{ code: "PEK", name: "北京" }],
                is_vip: true
            },
            status: "计划中",
            scheduled_departure: "2025-10-29T18:00:00Z",
            scheduled_arrival: null,
            estimated_departure: null,
            estimated_arrival: null,
            actual_arrival: null,
            actual_departure: null,
            start_boarding_time: null,
            end_boarding_time: null,
            cobt_time: null,
            aircraft_type_detail: "A330",
            stand: "B05",
            gate: "C12",
            has_boarding_restriction: false,
            is_quick_turnaround: true,
            business_cases: []
        }
    ];
}

function highlightText(text, query) {
    // First escape the text to prevent XSS
    const safeText = escapeHtml(text);
    if (!query) return safeText;
    const normalizedQuery = query.toLowerCase().trim();
    if (!normalizedQuery) return safeText;

    // Escape special regex characters in the query
    const escapedQuery = normalizedQuery.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const regex = new RegExp(`(${escapedQuery})`, 'gi');
    return safeText.replace(regex, '<span class="highlight">$1</span>');
}

function decorateFlightCardElement(flightElement, flight) {
    if (!flightElement || !flight) return;
    const flightNo = getPrimaryFlightNoV2(flight) || flight.flight_id || '未知航班';
    const status = flight.status || '状态未知';
    flightElement.setAttribute('role', 'listitem');
    flightElement.setAttribute('tabindex', '0');
    flightElement.setAttribute('aria-selected', String(isSameFlightId(selectedFlightId, flight.flight_id)));
    flightElement.setAttribute('aria-label', `${flightNo}，当前状态 ${status}`);
}

function createFlightCardElement(flight, query) {
    const flightElement = document.createElement('div');
    const isQuickTurnaround = flight.is_quick_turnaround || false;

    flightElement.className = `flight-item ${isSameFlightId(selectedFlightId, flight.flight_id) ? 'selected' : ''} ${isQuickTurnaround ? 'quick-turnaround' : ''}`;
    const flightIdText = normalizeFlightId(flight.flight_id);
    flightElement.dataset.flight_id = flightIdText;
    flightElement.dataset.flightId = flightIdText;
    decorateFlightCardElement(flightElement, flight);

    const scheduledDeparture = flight.scheduled_departure ? new Date(flight.scheduled_departure) : null;
    const estimatedDeparture = flight.estimated_departure ? new Date(flight.estimated_departure) : null;
    const scheduledArrival = flight.scheduled_arrival ? new Date(flight.scheduled_arrival) : null;
    const estimatedArrival = flight.estimated_arrival ? new Date(flight.estimated_arrival) : null;
    const actualArrival = flight.actual_arrival ? new Date(flight.actual_arrival) : null;
    const actualDeparture = flight.actual_departure ? new Date(flight.actual_departure) : null;

    const statusClass = getStatusClass(flight.status);
    const inboundFlightNo = getFlightNumberByLegV2(flight, 'inbound');
    const outboundFlightNo = getFlightNumberByLegV2(flight, 'outbound');
    const hasInboundFlight = Boolean(inboundFlightNo);
    const hasOutboundFlight = Boolean(outboundFlightNo);
    const flightNumberDisplay = getFlightNumberDisplayV2(flight) || EMPTY_DISPLAY_TEXT;
    const flightType = getPrimaryFlightTypeLabelV2(flight) || '未知';
    const missionType = getMissionSummaryV2(flight) || '未知';

    const inboundFlightNoClass = hasInboundFlight ? getFlightNumberTextClassV2(flight, 'inbound') : '';
    const outboundFlightNoClass = hasOutboundFlight ? getFlightNumberTextClassV2(flight, 'outbound') : '';

    const routeFieldMode = routeDisplayMode === 'name' ? 'name' : 'code';
    const originRoute = getRouteEndpointV2(flight, 'inbound', routeFieldMode)
        || getStationListDisplayV2(flight, 'outbound', 'origin_stations', routeFieldMode)
        || EMPTY_DISPLAY_TEXT;
    const destinationRoute = getRouteEndpointV2(flight, 'outbound', routeFieldMode)
        || getStationListDisplayV2(flight, 'inbound', 'destination_stations', routeFieldMode)
        || EMPTY_DISPLAY_TEXT;
    const airportLabel = getAirportDisplayValueV2(routeDisplayMode);

    if (hasInboundFlight && !hasOutboundFlight) {
        flightElement.innerHTML = `
            <div class="flight-header"><div class="flight-number inbound ${inboundFlightNoClass}">${highlightText(flightNumberDisplay, query)}</div><div class="flight-status ${statusClass}">${highlightText(flight.status, query)}</div></div><div class="flight-info"><div class="flight-type">航班类型: ${highlightText(flightType, query)}</div><div class="mission-type">任务类型: ${highlightText(missionType, query)}</div></div><div class="flight-route centered"><div class="flight-origin">${highlightText(originRoute, query)}</div><div class="flight-arrow">→</div><div class="flight-destination">${airportLabel}</div></div><div class="flight-times-single"><div class="time-left">${scheduledArrival ? `<span class="scheduled-time">计划到达: ${scheduledArrival.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}</span>` : `计划到达: ${EMPTY_DISPLAY_TEXT}`}
                    ${actualArrival ? `<span class="actual-time">| 实际: ${actualArrival.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}</span>` : (estimatedArrival ? `<span class="estimated-time">| 预计: ${estimatedArrival.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}</span>` : '')}</div></div>`;
    } else if (!hasInboundFlight && hasOutboundFlight) {
        flightElement.innerHTML = `
            <div class="flight-header"><div class="flight-number outbound ${outboundFlightNoClass}">${highlightText(flightNumberDisplay, query)}</div><div class="flight-status ${statusClass}">${highlightText(flight.status, query)}</div></div><div class="flight-info"><div class="flight-type">航班类型: ${highlightText(flightType, query)}</div><div class="mission-type">任务类型: ${highlightText(missionType, query)}</div></div><div class="flight-route centered"><div class="flight-origin">${airportLabel}</div><div class="flight-arrow">→</div><div class="flight-destination">${highlightText(destinationRoute, query)}</div></div><div class="flight-times-single"><div class="time-right">${scheduledDeparture ? `<span class="scheduled-time">计划起飞: ${scheduledDeparture.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}</span>` : `计划起飞: ${EMPTY_DISPLAY_TEXT}`}
                    ${actualDeparture ? `<span class="actual-time">| 实际: ${actualDeparture.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}</span>` : (estimatedDeparture ? `<span class="estimated-time">| 预计: ${estimatedDeparture.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}</span>` : '')}</div></div>`;
    } else {
        flightElement.innerHTML = `
            <div class="flight-header has-both"><div class="flight-number inbound ${inboundFlightNoClass}">${highlightText(inboundFlightNo, query)}</div><div class="flight-status ${statusClass}">${highlightText(flight.status, query)}</div><div class="flight-number outbound ${outboundFlightNoClass}">${highlightText(outboundFlightNo, query)}</div></div><div class="flight-info"><div class="flight-type">航班类型: ${highlightText(flightType, query)}</div><div class="mission-type">任务类型: ${highlightText(missionType, query)}</div></div><div class="flight-route centered"><div class="flight-origin">${highlightText(originRoute, query)}</div><div class="flight-arrow">→</div><div class="flight-destination">${airportLabel}</div><div class="flight-arrow">→</div><div class="flight-destination">${highlightText(destinationRoute, query)}</div></div><div class="flight-times"><div>${scheduledArrival ? `<span class="scheduled-time">计划到达: ${scheduledArrival.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}</span>` : `计划到达: ${EMPTY_DISPLAY_TEXT}`}
                    ${actualArrival ? `<span class="actual-time">| 实际: ${actualArrival.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}</span>` : (estimatedArrival ? `<span class="estimated-time">| 预计: ${estimatedArrival.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}</span>` : '')}</div><div>${scheduledDeparture ? `<span class="scheduled-time">计划起飞: ${scheduledDeparture.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}</span>` : `计划起飞: ${EMPTY_DISPLAY_TEXT}`}
                    ${actualDeparture ? `<span class="actual-time">| 实际: ${actualDeparture.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}</span>` : (estimatedDeparture ? `<span class="estimated-time">| 预计: ${estimatedDeparture.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}</span>` : '')}</div></div>`;
    }

    return flightElement;
}

window.alertVirtualScroller = null;

function selectFlight(flightId) {
    const previousSelectedId = selectedFlightId;
    selectedFlightId = normalizeFlightId(flightId) || null;
    selectedCaseId = null; // Reset case selection when switching flights

    // 优化：只更新选中状态变化的卡片，而不是重新渲染整个列表
    if (currentView === 'card') {
        updateCardSelectionState(previousSelectedId, selectedFlightId);
    } else {
        updateTableSelectionState(previousSelectedId, selectedFlightId);
    }

    renderFlightDetail();
    updateEditButtonState();
    if (isCompactFlightMonitorLayout()) {
        setDetailDrawerOpen(true);
    }
}

function updateCardSelectionState(previousId, newId) {
    // 移除之前选中卡片的选中样式
    if (previousId) {
        const previousCard = flightListElement.querySelector(`.flight-item[data-flight-id="${previousId}"], .flight-item[data-flight_id="${previousId}"]`);
        if (previousCard) {
            previousCard.classList.remove('selected');
            previousCard.setAttribute('aria-selected', 'false');
        }
    }

    // 添加新选中卡片的选中样式
    if (newId) {
        const newCard = flightListElement.querySelector(`.flight-item[data-flight-id="${newId}"], .flight-item[data-flight_id="${newId}"]`);
        if (newCard) {
            newCard.classList.add('selected');
            newCard.setAttribute('aria-selected', 'true');
            // 如果卡片不在视口中，滚动到可见位置
            if (!isElementInViewport(newCard)) {
                newCard.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
            }
        }
    }
}

function updateTableSelectionState(previousId, newId) {
    const tableBody = document.getElementById('flightTableBody');
    if (!tableBody) return;

    // 移除之前选中行的选中样式
    if (previousId) {
        const previousRow = tableBody.querySelector(`tr[data-flight-id="${previousId}"]`);
        if (previousRow) {
            previousRow.classList.remove('row-selected');
            previousRow.setAttribute('aria-selected', 'false');
        }
    }

    // 添加新选中行的选中样式
    if (newId) {
        const newRow = tableBody.querySelector(`tr[data-flight-id="${newId}"]`);
        if (newRow) {
            newRow.classList.add('row-selected');
            newRow.setAttribute('aria-selected', 'true');
            // 如果行不在视口中，滚动到可见位置
            if (!isElementInViewport(newRow)) {
                newRow.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
            }
        }
    }
}

function isElementInViewport(el) {
    const rect = el.getBoundingClientRect();
    const containerRect = el.closest('.table-scroll-wrapper, #flightList')?.getBoundingClientRect();

    if (!containerRect) {
        return rect.top >= 0 && rect.bottom <= window.innerHeight;
    }

    return rect.top >= containerRect.top && rect.bottom <= containerRect.bottom;
}

function updateEditButtonState() {
    const aiDiagnoseBtn = document.getElementById('aiDiagnoseBtn');
    const editBtn = document.getElementById('editBtn');
    const saveBtn = document.getElementById('saveBtn');
    const cancelBtn = document.getElementById('cancelBtn');
    const editControls = document.getElementById('editControls');

    if (selectedFlightId === null) {
        // 没有选择航班时隐藏编辑控件
        if (editControls) editControls.style.display = 'none';
        if (aiDiagnoseBtn) aiDiagnoseBtn.disabled = true;
        return;
    }

    // 显示编辑控件
    if (editControls) editControls.style.display = 'flex';
    if (aiDiagnoseBtn) aiDiagnoseBtn.disabled = false;

    const isEditMode = editMode[selectedFlightId] || false;

    if (isEditMode) {
        // 编辑模式下显示保存和取消按钮
        if (editBtn) editBtn.style.display = 'none';
        if (saveBtn) saveBtn.style.display = 'inline-block';
        if (cancelBtn) cancelBtn.style.display = 'inline-block';
    } else {
        // 非编辑模式下显示编辑按钮 (允许所有登录用户，但在表单中限制字段)
        if (editBtn) {
            editBtn.style.display = 'inline-block';
        }
        if (saveBtn) saveBtn.style.display = 'none';
        if (cancelBtn) cancelBtn.style.display = 'none';
    }
}

function collectRouteStationsFromListV2(listId) {
    const listElement = document.getElementById(listId);
    if (!listElement) {
        return [];
    }
    return Array.from(listElement.querySelectorAll('.route-station-item'))
        .map((item) => {
            const codeInput = item.querySelector('.station-code-input');
            const nameInput = item.querySelector('.station-name-input');
            return {
                code: codeInput ? String(codeInput.value || '').trim().toUpperCase() : '',
                name: nameInput ? String(nameInput.value || '').trim() : '',
            };
        })
        .filter((station) => station.code || station.name)
        .map((station) => ({
            code: station.code,
            name: station.name || null,
        }))
        .filter((station) => station.code);
}

function addRouteStation(listId) {
    const listElement = document.getElementById(listId);
    if (!listElement) {
        return;
    }
    const item = document.createElement('div');
    item.className = 'route-station-item';
    item.style.display = 'flex';
    item.style.gap = '8px';
    item.style.marginBottom = '8px';
    item.style.alignItems = 'center';

    const codeInput = document.createElement('input');
    codeInput.type = 'text';
    codeInput.className = 'station-code-input';
    codeInput.placeholder = '代码';

    const nameInput = document.createElement('input');
    nameInput.type = 'text';
    nameInput.className = 'station-name-input';
    nameInput.placeholder = '名称（选填）';

    const removeButton = document.createElement('button');
    removeButton.type = 'button';
    removeButton.textContent = '×';
    removeButton.onclick = () => removeRouteStation(removeButton);

    item.appendChild(codeInput);
    item.appendChild(nameInput);
    item.appendChild(removeButton);
    listElement.appendChild(item);
}

function removeRouteStation(button) {
    if (button && button.parentElement) {
        button.parentElement.remove();
    }
}

function createEditForm(flight) {
    const formatDateTime = (dateTimeStr) => {
        if (!dateTimeStr) return '';
        const date = new Date(dateTimeStr);
        return date.toISOString().slice(0, 16); // YYYY-MM-DDTHH:MM 格式
    };

    const inboundOriginStations = getLegStationsV2(flight, 'inbound', 'origin_stations');
    const outboundDestinationStations = getLegStationsV2(flight, 'outbound', 'destination_stations');
    const inboundFlightNo = getFlightNumberByLegV2(flight, 'inbound');
    const outboundFlightNo = getFlightNumberByLegV2(flight, 'outbound');
    const inboundType = getLegFlightTypeLabelV2(flight, 'inbound') || '国内';
    const outboundType = getLegFlightTypeLabelV2(flight, 'outbound') || '国内';
    const haveInboundVIP = getLegVipFlagV2(flight, 'inbound');
    const haveOutboundVIP = getLegVipFlagV2(flight, 'outbound');
    const missionInputValue = getMissionInputValueV2(flight);

    // 确定航班类型
    const hasInboundFlight = Boolean(inboundFlightNo);
    const hasOutboundFlight = Boolean(outboundFlightNo);

    // Permission Check
    const isAdmin = Auth.isAdmin();
    // Fields restricted for non-admins:
    // status, leg fields, route stations, aircraft_type_detail,
    // stand, gate, terminal, position, scheduled_*, estimated_*, actual_*, cobt_time
    const coreDisabled = isAdmin ? '' : 'disabled';
    const coreReadonlyClass = isAdmin ? '' : 'readonly-field'; // Style for visual feedback

    return `
        <div class="edit-form" id="editForm"><div class="form-row">${hasInboundFlight ? `
                        <div class="form-group"><label>进港航班号</label><input type="text" id="editFlightNoInbound" value="${inboundFlightNo || ''}" class="readonly-field" readonly></div>` : ''}
                        ${hasOutboundFlight ? `
                        <div class="form-group"><label>出港航班号</label><input type="text" id="editFlightNoOutbound" value="${outboundFlightNo || ''}" class="readonly-field" readonly></div>` : ''}
                        <div class="form-group"><label>状态</label><select id="editStatus" ${coreDisabled}><option value="计划中" ${flight.status === '计划中' ? 'selected' : ''}>计划中</option><option value="前站起飞" ${flight.status === '前站起飞' ? 'selected' : ''}>前站起飞</option><option value="到达本站" ${flight.status === '到达本站' ? 'selected' : ''}>到达本站</option><option value="值机结束" ${flight.status === '值机结束' ? 'selected' : ''}>值机结束</option><option value="登机" ${flight.status === '登机' ? 'selected' : ''}>登机</option><option value="催促登机" ${flight.status === '催促登机' ? 'selected' : ''}>催促登机</option><option value="结束登机" ${flight.status === '结束登机' ? 'selected' : ''}>结束登机</option><option value="已起飞" ${flight.status === '已起飞' ? 'selected' : ''}>已起飞</option><option value="到下站" ${flight.status === '到下站' ? 'selected' : ''}>到下站</option><option value="取消" ${flight.status === '取消' ? 'selected' : ''}>取消</option><option value="延误" ${flight.status === '延误' ? 'selected' : ''}>延误</option></select></div></div><div class="form-row">${hasInboundFlight ? `
                        <div class="form-group"><label>前序站点</label><div><div id="inboundOriginStationList">${renderRouteStationEditorRowsV2(inboundOriginStations, coreDisabled)}</div><div class="add-destination" style="display: ${isAdmin ? 'flex' : 'none'}"><button type="button" onclick="addRouteStation('inboundOriginStationList')">添加站点</button></div></div></div>` : ''}
                        ${hasOutboundFlight ? `
                        <div class="form-group"><label>后序站点</label><div><div id="outboundDestinationStationList">${renderRouteStationEditorRowsV2(outboundDestinationStations, coreDisabled)}</div><div class="add-destination" style="display: ${isAdmin ? 'flex' : 'none'}"><button type="button" onclick="addRouteStation('outboundDestinationStationList')">添加站点</button></div></div></div>` : ''}</div><div class="form-row">${hasInboundFlight ? `
                        <div class="form-group"><label>计划到达时间</label><input type="datetime-local" id="editScheduledArrivalTime" value="${formatDateTime(flight.scheduled_arrival)}" ${coreDisabled}></div>` : ''}
                        ${hasOutboundFlight ? `
                        <div class="form-group"><label>计划起飞时间</label><input type="datetime-local" id="editScheduledDepartureTime" value="${formatDateTime(flight.scheduled_departure)}" ${coreDisabled}></div>` : ''}
                        ${hasInboundFlight ? `
                        <div class="form-group"><label>预计到达时间</label><input type="datetime-local" id="editEstimatedArrivalTime" value="${formatDateTime(flight.estimated_arrival)}" ${coreDisabled}></div>` : ''}
                    </div><div class="form-row">${hasOutboundFlight ? `
                        <div class="form-group"><label>预计起飞时间</label><input type="datetime-local" id="editEstimatedDepartureTime" value="${formatDateTime(flight.estimated_departure)}" ${coreDisabled}></div>` : ''}
                        ${hasInboundFlight ? `
                        <div class="form-group"><label>实际到达时间</label><input type="datetime-local" id="editActualArrivalTime" value="${formatDateTime(flight.actual_arrival)}" ${coreDisabled}></div>` : ''}
                        ${hasOutboundFlight ? `
                        <div class="form-group"><label>实际起飞时间</label><input type="datetime-local" id="editActualDepartureTime" value="${formatDateTime(flight.actual_departure)}" ${coreDisabled}></div>` : ''}
                    </div><div class="form-row">${hasOutboundFlight ? `
                        <div class="form-group"><label>开始登机时间</label><input type="datetime-local" id="editStartBoardingTime" value="${formatDateTime(flight.start_boarding_time)}" ${coreDisabled}></div>` : ''}
                        ${hasOutboundFlight ? `
                        <div class="form-group"><label>结束登机时间</label><input type="datetime-local" id="editEndBoardingTime" value="${formatDateTime(flight.end_boarding_time)}" ${coreDisabled}></div>` : ''}
                        ${hasOutboundFlight ? `
                        <div class="form-group"><label>COBT</label><input type="datetime-local" id="editCOBT" value="${formatDateTime(flight.cobt_time)}" ${coreDisabled}></div>` : ''}
                    </div><div class="form-row"><div class="form-group"><label>机型</label><input type="text" id="editAircraftType" value="${flight.aircraft_type_detail || ''}" ${coreDisabled}></div><div class="form-group"><label>机位</label><input type="text" id="editStand" value="${flight.stand || ''}" ${coreDisabled}></div>${hasOutboundFlight ? `
                        <div class="form-group"><label>登机口</label><input type="text" id="editGate" value="${flight.gate || ''}" ${coreDisabled}></div>` : ''}
                        <div class="form-group"><label>停机位置</label><input type="text" id="editPosition" value="${flight.position || ''}" ${coreDisabled}></div><div class="form-group"><label>航站楼</label><input type="text" id="editTerminal" value="${flight.terminal || ''}" ${coreDisabled}></div></div><div class="form-row">${hasOutboundFlight ? `
                        <div class="form-group"><label>登机限制</label><select id="editBoardingRestriction"><option value="false" ${!flight.has_boarding_restriction ? 'selected' : ''}>否</option><option value="true" ${flight.has_boarding_restriction ? 'selected' : ''}>是</option></select></div>` : ''}
                        <div class="form-group"><label>任务类型数值</label><input type="text" id="editMissions" value="${missionInputValue}" placeholder="例如: 20, 21" title="${escapeHtmlAttribute(getMissionSummaryV2(flight) || '按任务类型数值录入')}" ${coreDisabled}></div></div><div class="form-row">${hasInboundFlight ? `
                        <div class="form-group"><label>进港航班类别</label><select id="editInboundType" ${coreDisabled}><option value="国内" ${inboundType === '国内' ? 'selected' : ''}>国内</option><option value="国际" ${inboundType === '国际' ? 'selected' : ''}>国际</option><option value="地区" ${inboundType === '地区' ? 'selected' : ''}>地区</option></select></div>` : ''}
                        ${hasOutboundFlight ? `
                        <div class="form-group"><label>出港航班类别</label><select id="editOutboundType" ${coreDisabled}><option value="国内" ${outboundType === '国内' ? 'selected' : ''}>国内</option><option value="国际" ${outboundType === '国际' ? 'selected' : ''}>国际</option><option value="地区" ${outboundType === '地区' ? 'selected' : ''}>地区</option></select></div>` : ''}
                    </div><div class="form-row">${hasInboundFlight ? `
                        <div class="form-group"><label>进港重要旅客</label><select id="editInboundVIP" ${coreDisabled}><option value="false" ${!haveInboundVIP ? 'selected' : ''}>否</option><option value="true" ${haveInboundVIP ? 'selected' : ''}>是</option></select></div>` : ''}
                        ${hasOutboundFlight ? `
                        <div class="form-group"><label>出港重要旅客</label><select id="editOutboundVIP" ${coreDisabled}><option value="false" ${!haveOutboundVIP ? 'selected' : ''}>否</option><option value="true" ${haveOutboundVIP ? 'selected' : ''}>是</option></select></div>` : ''}
                    </div>${hasInboundFlight && hasOutboundFlight ? `
                    <div class="form-row"><div class="form-group"><label>快速过站</label><select id="editIsQuickTurnaround"><option value="false" ${!flight.is_quick_turnaround ? 'selected' : ''}>否</option><option value="true" ${flight.is_quick_turnaround ? 'selected' : ''}>是</option></select></div></div>` : ''
        }

                    ${hasOutboundFlight ? `
                    <div class="form-row"><div class="form-group"><label>允许登机</label><input type="datetime-local" id="editBoardingAllowedTime" value="${formatDateTime(flight.boarding_allowed_time)}"></div><div class="form-group"><label>人齐</label><input type="datetime-local" id="editPassengerReadyTime" value="${formatDateTime(flight.passenger_ready_time)}"></div><div class="form-group"><label>撤轮挡</label><input type="datetime-local" id="editOffBlocksTime" value="${formatDateTime(flight.off_blocks_time)}"></div></div>` : ''
        }

    <div id="saveStatus"></div></div>`;
}

function valuesEqual(oldVal, newVal) {
    // 都为空视为相等
    if ((oldVal === null || oldVal === undefined || oldVal === '') &&
        (newVal === null || newVal === undefined || newVal === '')) {
        return true;
    }
    // 数组比较
    if (Array.isArray(oldVal) && Array.isArray(newVal)) {
        if (oldVal.length !== newVal.length) return false;
        return oldVal.every((v, i) => valuesEqual(v, newVal[i]));
    }
    // 对象比较（递归）
    if (oldVal && newVal && typeof oldVal === 'object' && typeof newVal === 'object') {
        const oldKeys = Object.keys(oldVal).sort();
        const newKeys = Object.keys(newVal).sort();
        if (!valuesEqual(oldKeys, newKeys)) {
            return false;
        }
        return oldKeys.every((key) => valuesEqual(oldVal[key], newVal[key]));
    }
    // 日期字符串比较（忽略毫秒差异）
    if (typeof oldVal === 'string' && typeof newVal === 'string' &&
        oldVal.includes('T') && newVal.includes('T')) {
        try {
            return new Date(oldVal).getTime() === new Date(newVal).getTime();
        } catch { return oldVal === newVal; }
    }
    return oldVal === newVal;
}

function cloneLegForPatchV2(leg, legType) {
    const parsed = parseLegPayloadV2(leg, legType);
    if (!parsed) {
        return null;
    }
    return {
        leg_type: legType,
        flight_no: String(parsed.flight_no || '').trim().toUpperCase(),
        flight_type: normalizeFlightTypeCodeV2(parsed.flight_type),
        mission: parseMissionNumericInput(parsed.mission),
        origin_stations: normalizeRouteStationsV2(parsed.origin_stations),
        destination_stations: normalizeRouteStationsV2(parsed.destination_stations),
        is_vip: Boolean(parsed.is_vip),
        stand_type: parsed.stand_type ? String(parsed.stand_type).trim() : null,
    };
}

function pickArrayValueByLeg(values, legType, hasInbound, hasOutbound) {
    if (!Array.isArray(values) || values.length === 0) {
        return null;
    }
    const inboundIndex = hasInbound ? 0 : -1;
    const outboundIndex = hasInbound && hasOutbound ? 1 : 0;
    const targetIndex = legType === 'inbound' ? inboundIndex : outboundIndex;
    if (targetIndex >= 0 && values[targetIndex] !== undefined && values[targetIndex] !== null && String(values[targetIndex]).trim() !== '') {
        return values[targetIndex];
    }
    return null;
}

function normalizeFlightEditPatchV2(diff) {
    const patch = { ...diff };
    const timelineUpdates = [];

    DISPATCH_TIMELINE_FIELDS.forEach((field) => {
        if (!Object.prototype.hasOwnProperty.call(patch, field)) {
            return;
        }
        timelineUpdates.push({ field, value: patch[field] });
        delete patch[field];
    });

    return { patch, timelineUpdates };
}

function getStatusText(status) {
    switch (status) {
        case 'INITIAL':
            return '初始';
        case 'PENDING':
            return '待处理';
        case 'PROCESSING':
            return '处理中';
        case 'COMPLETED':
            return '已完成';
        case 'SUCCESS':
            return '成功';
        case 'FAILED':
            return '失败';
        case 'RETRYING':
            return '重试中';
        case 'DEAD_LETTER':
            return '死信';
        case 'CANCELED':
            return '已取消';
        default:
            return status;
    }
}

function getUserIdFromToken() {
    const token = Auth.getToken();
    if (!token) return null;
    try {
        const base64Url = token.split('.')[1];
        const base64 = base64Url.replace(/-/g, '+').replace(/_/g, '/');
        const jsonPayload = decodeURIComponent(window.atob(base64).split('').map(function (c) {
            return '%' + ('00' + c.charCodeAt(0).toString(16)).slice(-2);
        }).join(''));
        return JSON.parse(jsonPayload).sub;
    } catch (e) {
        return null;
    }
}

const timeContextMenu = document.getElementById('timeContextMenu');

const headerContextMenu = document.getElementById('headerContextMenu');

const ctxModify = document.getElementById('ctxModify');

const ctxRevoke = document.getElementById('ctxRevoke');

let contextMenuState = {
    visible: false,
    flightId: null,
    field: null,
    currentValue: null,
};

document.addEventListener('click', (e) => {
    if (timeContextMenu && (timeContextMenu.style.display === 'block' || timeContextMenu.style.display === 'flex')) {
        timeContextMenu.style.display = 'none';
        contextMenuState.visible = false;
    }
    if (headerContextMenu && headerContextMenu.style.display === 'block') {
        headerContextMenu.style.display = 'none';
    }
});

function showTimeContextMenu(e, flightId, field, filler, currentValue) {
    const currentUser = getUserIdFromToken();

    // Check permission: Only filler can modify/revoke
    // If filler is not recorded (e.g. old data), strict mode might block or allow?
    // User requirement: "Only allow filler account".
    // If filler is missing, assume System or unknown, so block unless admin?
    // Let's implement strict check.

    const canEdit = currentUser && (filler === currentUser);

    if (canEdit) {
        ctxModify.classList.remove('disabled');
        ctxRevoke.classList.remove('disabled');
    } else {
        ctxModify.classList.add('disabled');
        ctxRevoke.classList.add('disabled');
    }

    // Position menu
    if (!timeContextMenu) return;
    timeContextMenu.style.display = 'flex';
    timeContextMenu.style.left = `${e.pageX} px`;
    timeContextMenu.style.top = `${e.pageY} px`;

    contextMenuState.visible = true;
    contextMenuState.flightId = flightId;
    contextMenuState.field = field;
    contextMenuState.currentValue = currentValue || '';
}

const remarkInput = document.getElementById('remarkInput');

const saveRemarkBtn = document.getElementById('saveRemarkBtn');

let remarkTarget = null;

window.closeRemarkModal = function () {
    closeManagedModal(remarkEditModal);
    remarkTarget = null;
};

if (saveRemarkBtn) saveRemarkBtn.onclick = () => {
    if (remarkTarget) {
        const { flightId, field } = remarkTarget;
        const val = remarkInput.value;
        updateFlightField(flightId, field, val);
        closeRemarkModal();
    }
};

function getDispatchChatCurrentUserId() {
    const user = (window.Auth && typeof Auth.getUser === 'function') ? Auth.getUser() : null;
    return String(user?.sub || user?.id || user?.user_id || getUserIdFromToken() || '').trim();
}

function ensureDispatchChatPanel() {
    if (dispatchChatPanel) {
        return dispatchChatPanel;
    }

    const modal = document.getElementById('dispatchChatModal');
    if (!(modal instanceof HTMLElement)) {
        return null;
    }

    if (!window.DispatchChatPanel || typeof window.DispatchChatPanel.create !== 'function') {
        console.warn('DispatchChatPanel 模块未加载');
        return null;
    }

    dispatchChatPanel = window.DispatchChatPanel.create({
        root: modal,
        showToast,
        isOpen: () => activeModal === modal && modal.getAttribute('aria-hidden') === 'false',
        getCurrentUserId: getDispatchChatCurrentUserId,
    });
    dispatchChatPanel.initialize();

    const closeBtn = document.getElementById('dispatchChatCloseBtn');
    if (closeBtn instanceof HTMLButtonElement) {
        closeBtn.addEventListener('click', () => {
            closeFlightMonitorDispatchChatDrawer();
        });
    }

    return dispatchChatPanel;
}

async function openFlightMonitorDispatchChatDrawer() {
    if (!canViewDispatchNotifications()) {
        showToast('当前账号缺少 dispatch:view 权限', 'warning');
        return;
    }

    const modal = document.getElementById('dispatchChatModal');
    if (!(modal instanceof HTMLElement)) {
        showToast('群聊抽屉尚未就绪，请刷新页面后重试', 'warning');
        return;
    }

    const panel = ensureDispatchChatPanel();
    if (!panel) {
        showToast('群聊模块加载失败，请刷新页面后重试', 'warning');
        return;
    }

    panel.setEntryMeta(buildDispatchChatEntryMeta(selectedFlightId));
    openManagedModal(modal, '#dispatchChatCloseBtn');
    await panel.open({
        flightId: normalizeFlightId(selectedFlightId),
        fallbackToFirstGroup: true,
    });
}

function closeFlightMonitorDispatchChatDrawer() {
    const modal = document.getElementById('dispatchChatModal');
    if (dispatchChatPanel) {
        dispatchChatPanel.close();
    }
    if (modal instanceof HTMLElement) {
        closeManagedModal(modal);
    }
}

function setupEventListeners() {
    refreshBtn.addEventListener('click', () => {
        loadFlights();
        updateLastUpdated();
        announce('已触发刷新');
    });

    const openFlightChatBadgeBtn = document.getElementById('openFlightChatBadgeBtn');
    if (openFlightChatBadgeBtn instanceof HTMLButtonElement) {
        openFlightChatBadgeBtn.addEventListener('click', () => {
            openFlightChatModal();
        });
    }

    const openDispatchChatBadgeBtn = document.getElementById('openDispatchChatBadgeBtn');
    if (openDispatchChatBadgeBtn instanceof HTMLButtonElement) {
        openDispatchChatBadgeBtn.addEventListener('click', () => {
            openFlightMonitorDispatchChatDrawer();
        });
    }

    const bindDispatchNotifyTrigger = (button) => {
        if (!(button instanceof HTMLButtonElement)) {
            return;
        }
        button.addEventListener('click', async () => {
            await openDispatchNotifyModal();
        });
    };

    const dispatchNotifyBadgeBtn = document.getElementById('dispatchNotifyBadgeBtn');
    bindDispatchNotifyTrigger(dispatchNotifyBadgeBtn);

    loadBusinessFiltersFromStorage();
    syncBusinessFilterUI();
    updateBusinessFilterSummary(flights.length, originalFlights.length);
    setBusinessFilterPanelExpanded(false);

    const businessFilterToggle = document.getElementById('businessFilterToggle');
    const businessFilterBar = document.getElementById('businessFilterBar');
    if (businessFilterToggle && businessFilterBar) {
        businessFilterToggle.addEventListener('click', (event) => {
            event.stopPropagation();
            setBusinessFilterPanelExpanded(!isBusinessFilterPanelOpen);
        });

        businessFilterBar.addEventListener('click', (event) => {
            event.stopPropagation();
        });

        document.addEventListener('click', (event) => {
            if (!isBusinessFilterPanelOpen) return;

            const target = event.target;
            if (businessFilterBar.contains(target) || businessFilterToggle.contains(target)) {
                return;
            }
            setBusinessFilterPanelExpanded(false);
        });
    }

    // Add search event listeners
    const searchInput = document.getElementById('searchInput');
    const searchBtn = document.getElementById('searchBtn');
    const clearSearchBtn = document.getElementById('clearSearchBtn');

    if (searchInput && searchBtn && clearSearchBtn) {
        // Search button click event
        searchBtn.addEventListener('click', performSearch);

        // Clear search button click event
        clearSearchBtn.addEventListener('click', clearSearch);

        // Enter key in search input
        searchInput.addEventListener('keyup', (e) => {
            if (e.key === 'Enter') {
                performSearch();
            }
        });

        // Show/hide clear button based on input content + debounced search
        const debouncedSearch = debounce(() => performSearch(), UI_CONSTANTS.searchDebounceMs);
        searchInput.addEventListener('input', () => {
            clearSearchBtn.style.display = searchInput.value ? 'flex' : 'none';
            debouncedSearch();
        });
    }

    const aircraftBodyFilter = document.getElementById('aircraftBodyFilter');
    if (aircraftBodyFilter) {
        aircraftBodyFilter.addEventListener('change', () => {
            updateBusinessFiltersFromUI(true);
            syncBusinessFilterUI();
            performSearch();
        });
    }

    const commercialSignedFilter = document.getElementById('commercialSignedFilter');
    if (commercialSignedFilter) {
        commercialSignedFilter.addEventListener('change', () => {
            updateBusinessFiltersFromUI(true);
            syncBusinessFilterUI();
            performSearch();
        });
    }

    // 4 new filter dropdowns (anomaly, delay, vip, quickTurn)
    ['anomalyFilter', 'delayFilter', 'vipFilter', 'quickTurnFilter'].forEach(filterId => {
        const el = document.getElementById(filterId);
        if (el) {
            el.addEventListener('change', () => {
                updateBusinessFiltersFromUI(true);
                syncBusinessFilterUI();
                performSearch();
            });
        }
    });

    const resetBusinessFiltersBtn = document.getElementById('resetBusinessFiltersBtn');
    if (resetBusinessFiltersBtn) {
        resetBusinessFiltersBtn.addEventListener('click', () => {
            businessFilters = { ...DEFAULT_BUSINESS_FILTERS };
            syncBusinessFilterUI();
            saveBusinessFiltersToStorage();
            performSearch();
        });
    }

    const clearAllFiltersBtn = document.getElementById('clearAllFiltersBtn');
    if (clearAllFiltersBtn) {
        clearAllFiltersBtn.addEventListener('click', () => {
            const searchInputEl = document.getElementById('searchInput');
            const clearBtnEl = document.getElementById('clearSearchBtn');
            businessFilters = { ...DEFAULT_BUSINESS_FILTERS };
            syncBusinessFilterUI();
            saveBusinessFiltersToStorage();
            if (searchInputEl) {
                searchInputEl.value = '';
            }
            if (clearBtnEl) {
                clearBtnEl.style.display = 'none';
            }
            performSearch({ preserveSelection: true });
        });
    }

    const focusSelectedFlightBtn = document.getElementById('focusSelectedFlightBtn');
    if (focusSelectedFlightBtn) {
        focusSelectedFlightBtn.addEventListener('click', () => {
            if (!selectedFlightId) {
                showToast('当前没有选中的航班', 'info');
                return;
            }
            const selectedEl = document.querySelector(`.flight-item[data-flight-id="${selectedFlightId}"], .flight-item[data-flight_id="${selectedFlightId}"], tr[data-flight-id="${selectedFlightId}"]`);
            if (selectedEl) {
                selectedEl.scrollIntoView({ behavior: 'smooth', block: 'center' });
            }
        });
    }

    const closeDetailDrawerBtn = document.getElementById('closeDetailDrawerBtn');
    if (closeDetailDrawerBtn) {
        closeDetailDrawerBtn.addEventListener('click', () => {
            setDetailDrawerOpen(false);
        });
    }

    // Search options panel toggle
    const optionsToggle = document.getElementById('searchOptionsToggle');
    const optionsPanel = document.getElementById('searchOptionsPanel');
    if (optionsToggle && optionsPanel) {
        optionsToggle.addEventListener('click', () => {
            const isExpanded = optionsPanel.classList.toggle('expanded');
            optionsToggle.classList.toggle('expanded', isExpanded);
            optionsToggle.setAttribute('aria-expanded', isExpanded ? 'true' : 'false');
            announce(isExpanded ? '搜索选项已展开' : '搜索选项已收起');
        });
    }

    // 联动出发地/目的地与其名称选项
    const searchOriginCheckbox = document.getElementById('searchOrigin');
    const searchOriginNameCheckbox = document.getElementById('searchOriginName');
    const searchDestinationCheckbox = document.getElementById('searchDestination');
    const searchDestinationNameCheckbox = document.getElementById('searchDestinationName');

    if (searchOriginCheckbox && searchOriginNameCheckbox) {
        searchOriginCheckbox.addEventListener('change', () => {
            searchOriginNameCheckbox.checked = searchOriginCheckbox.checked;
        });
    }
    if (searchDestinationCheckbox && searchDestinationNameCheckbox) {
        searchDestinationCheckbox.addEventListener('change', () => {
            searchDestinationNameCheckbox.checked = searchDestinationCheckbox.checked;
        });
    }

    const searchOptions = document.getElementById('searchOptions');
    if (searchOptions) {
        searchOptions.addEventListener('change', () => {
            performSearch();
        });
    }


    // View Switcher logic
    const viewCardBtn = document.getElementById('viewCardBtn');
    const viewTableBtn = document.getElementById('viewTableBtn');
    if (viewCardBtn) viewCardBtn.addEventListener('click', () => toggleView('card'));
    if (viewTableBtn) viewTableBtn.addEventListener('click', () => toggleView('table'));

    const openAnomalyBadgeBtn = document.getElementById('openAnomalyBadgeBtn');
    if (openAnomalyBadgeBtn instanceof HTMLButtonElement) {
        openAnomalyBadgeBtn.addEventListener('click', () => toggleView('alert'));
    }

    const updateBadgeBtn = document.getElementById('updateBadge');
    const closeUpdatePanelBtn = document.getElementById('closeUpdatePanelBtn');
    if (updateBadgeBtn) updateBadgeBtn.addEventListener('click', toggleUpdatePanel);
    if (closeUpdatePanelBtn) closeUpdatePanelBtn.addEventListener('click', toggleUpdatePanel);

    document.addEventListener('click', (event) => {
        if (!isPanelOpen) return;
        const panel = document.getElementById('updatePanel');
        if (!panel || panel.hidden) return;
        const target = event.target;
        if (panel.contains(target) || (updateBadgeBtn && updateBadgeBtn.contains(target))) {
            return;
        }
        toggleUpdatePanel();
    });

    document.addEventListener('click', (event) => {
        if (!activeModal) return;
        if (event.target !== activeModal) return;
        if (isBlockingManagedModal(activeModal)) {
            return;
        }
        const closeBtn = activeModal.querySelector('.close, .close-modal, #cancelEventBtn, #closeColumnConfig');
        if (closeBtn) {
            closeBtn.click();
            return;
        }
        closeManagedModal(activeModal);
    });

    // Keyboard shortcut for view toggle
    document.addEventListener('keydown', (e) => {
        if (activeModal) {
            if (e.key === 'Escape') {
                if (isBlockingManagedModal(activeModal)) {
                    return;
                }
                const closeBtn = activeModal.querySelector('.close, .close-modal, #cancelEventBtn, #closeColumnConfig');
                if (closeBtn) {
                    closeBtn.click();
                }
                return;
            }
            handleModalFocusTrap(e);
            return;
        }

        if (e.key === 'Escape') {
            if (isBusinessFilterPanelOpen) {
                setBusinessFilterPanelExpanded(false);
                return;
            }

            const optionsPanel = document.getElementById('searchOptionsPanel');
            const optionsToggle = document.getElementById('searchOptionsToggle');
            if (optionsPanel && optionsPanel.classList.contains('expanded')) {
                optionsPanel.classList.remove('expanded');
                if (optionsToggle) {
                    optionsToggle.classList.remove('expanded');
                    optionsToggle.setAttribute('aria-expanded', 'false');
                    optionsToggle.focus();
                }
                return;
            }

            if (isPanelOpen) {
                toggleUpdatePanel();
                return;
            }
        }

        if (e.key === '/' && !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey) {
            const target = e.target;
            if (target && target.matches('input, textarea, [contenteditable="true"]')) {
                return;
            }
            e.preventDefault();
            const searchInput = document.getElementById('searchInput');
            if (searchInput) {
                searchInput.focus();
                searchInput.select();
            }
            return;
        }

        if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key.toLowerCase() === 'v') {
            e.preventDefault();
            toggleView(currentView === 'card' ? 'table' : 'card');
        }
    });
}

function performSearch(options = {}) {
    const { preserveSelection = false } = options;
    const searchInput = document.getElementById('searchInput');
    const query = searchInput ? searchInput.value : '';

    updateBusinessFiltersFromUI(false);

    setFlightListBusy(true);

    if (flightWorker) {
        latestWorkerRequestId += 1;
        flightWorker.postMessage({
            type: 'filter',
            flights: originalFlights,
            query,
            filters: {
                ...getSearchFieldFilters(),
                ...businessFilters,
            },
            airportContext,
            requestId: latestWorkerRequestId
        });
        return;
    }

    flights = filterFlights(query);
    renderFlights();
    updateBusinessFilterSummary(flights.length, originalFlights.length);
    setFlightListBusy(false);
    announce(`筛选后显示 ${flights.length} 条航班`);
}

function clearSearch() {
    const searchInput = document.getElementById('searchInput');
    const clearBtn = document.getElementById('clearSearchBtn');
    if (searchInput) {
        searchInput.value = '';
        if (clearBtn) clearBtn.style.display = 'none';
        performSearch();
    }
}

function updateLastUpdated() {
    const now = new Date();
    lastUpdatedElement.textContent = `最后更新: ${now.toLocaleTimeString('zh-CN')} `;
}

document.addEventListener('DOMContentLoaded', init);

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', function () {
        setupEventCreationModal();
    });
} else {
    setupEventCreationModal();
}

function selectCase(case_id) {

    selectedCaseId = case_id;
    renderFlightDetail();
}

function setupEventListClick() {

    // Use event delegation to handle clicks on business case list items
    document.addEventListener('click', function (e) {
        // Check if the click is on a business case list item or its children
        const eventListItem = e.target.closest('[data-case-id]');
        if (eventListItem) {

            // Get the business case ID from the data attribute
            const case_id = eventListItem.dataset.caseId;

            if (case_id) {
                selectCase(case_id);
            } else {
                console.warn('Invalid business case ID:', case_id);
            }
        }
    });

}

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', function () {
        setupEventListClick();
    });
} else {
    setupEventListClick();
}

document.addEventListener('DOMContentLoaded', function () {
    const resizer = document.getElementById('resizer');
    const flightListPanel = document.querySelector('.flight-list-panel');

    if (!resizer || !flightListPanel) {
        return;
    }

    let isResizing = false;
    let startX = 0;
    let startWidth = 0;

    // Apply initial layout without polluting saved panel width.
    syncFlightMonitorLayout(currentView);

    // Mouse events
    resizer.addEventListener('mousedown', initResize);
    document.addEventListener('mousemove', doResize);
    document.addEventListener('mouseup', stopResize);

    // Touch events for mobile
    resizer.addEventListener('touchstart', initResize);
    document.addEventListener('touchmove', doResize);
    document.addEventListener('touchend', stopResize);

    function initResize(e) {
        if (currentView === 'table') {
            return;
        }
        isResizing = true;
        startX = e.type.includes('mouse') ? e.clientX : e.touches[0].clientX;
        startWidth = parseInt(document.defaultView.getComputedStyle(flightListPanel).width, 10);

        // Prevent text selection while resizing
        document.body.style.userSelect = 'none';
        document.body.style.cursor = 'col-resize';

        e.preventDefault();
    }

    function doResize(e) {
        if (!isResizing) return;

        const currentX = e.type.includes('mouse') ? e.clientX : e.touches[0].clientX;
        const width = startWidth + currentX - startX;

        // Set minimum and maximum widths
        const minWidth = FLIGHT_LIST_PANEL_MIN_WIDTH;
        const maxWidth = getMaxFlightListPanelWidth();

        if (width >= minWidth && width <= maxWidth) {
            flightListPanel.style.width = width + 'px';

            // Save to localStorage
            setSavedFlightListPanelWidth(width);
        }
    }

    function stopResize() {
        if (!isResizing) return;

        isResizing = false;

        // Restore text selection and cursor
        document.body.style.userSelect = '';
        document.body.style.cursor = '';
    }

    // Handle window resize
    window.addEventListener('resize', function () {
        syncFlightMonitorLayout(currentView);
        if (currentView === 'table') {
            syncFlightMonitorLayout('table');
            return;
        }

        const clampedWidth = getClampedFlightListPanelWidth();
        if (clampedWidth) {
            flightListPanel.style.width = `${clampedWidth}px`;
        } else {
            flightListPanel.style.width = '';
        }
        if (!isCompactFlightMonitorLayout()) {
            setDetailDrawerOpen(false);
        }
    });
});

function getStatusClass(status) {
    const statusMap = {
        '计划中': 'status-scheduled',
        '前站起飞': 'status-prev-departed',
        '到达本站': 'status-arrived',
        '值机结束': 'status-checkin-end',
        '登机': 'status-boarding',
        '催促登机': 'status-boarding-urge',
        '结束登机': 'status-boarding-ended',
        '已起飞': 'status-departed',
        '到下站': 'status-next-arrived',
        '取消': 'status-cancelled',
        '延误': 'status-delayed'
    };
    return statusMap[status] || 'status-scheduled';
}

function getRowStatusClass(status) {
    const rowStatusMap = {
        '计划中': 'row-scheduled',           // 0 - 白色
        '前站起飞': 'row-prev-departed',      // 1 - 浅橙黄色
        '到达本站': 'row-arrived',            // 2 - 天蓝色
        '值机结束': 'row-checkin-end',        // 3 - 浅水绿蓝色
        '登机': 'row-boarding',               // 4 - 浅灰蓝色
        '催促登机': 'row-boarding-urge',      // 5 - 浅灰蓝色
        '结束登机': 'row-boarding-ended',     // 6 - 绿色
        '已起飞': 'row-departed',             // 7 - 灰色
        '到下站': 'row-next-arrived',         // 8 - 灰色
        '取消': 'row-cancelled',              // 9 - 灰色条纹
        '延误': 'row-delayed'                 // 10 - 品红色
    };
    return rowStatusMap[status] || 'row-scheduled';
}

function cancelPendingViewRender() {
    if (pendingViewRenderTimer !== null) {
        clearTimeout(pendingViewRenderTimer);
        pendingViewRenderTimer = null;
    }

    if (pendingViewRenderRafId !== null) {
        cancelAnimationFrame(pendingViewRenderRafId);
        pendingViewRenderRafId = null;
    }
}

function scheduleViewRender(renderFn, delayMs = 0) {
    cancelPendingViewRender();

    if (delayMs <= 0) {
        pendingViewRenderRafId = requestAnimationFrame(() => {
            pendingViewRenderRafId = null;
            renderFn();
        });
        return;
    }

    pendingViewRenderTimer = window.setTimeout(() => {
        pendingViewRenderTimer = null;
        pendingViewRenderRafId = requestAnimationFrame(() => {
            pendingViewRenderRafId = null;
            renderFn();
        });
    }, Math.max(0, delayMs));
}

function toggleView(view) {
    const nextView = view === 'alert' ? 'alert' : normalizeFlightMonitorBaseView(view);
    const glider = document.getElementById('viewGlider');
    if (nextView === currentView && (!glider || glider.style.transform)) return; // Skip if already active and glider positioned

    // Cancel any in-flight chunk render tasks when switching view
    currentRenderTaskId += 1;
    cancelPendingViewRender();

    if (nextView === 'alert') {
        if (currentView !== 'alert') {
            lastNonAlertView = normalizeFlightMonitorBaseView(currentView);
        }
    } else {
        lastNonAlertView = nextView;
        localStorage.setItem('flightMonitorView', nextView);
    }

    currentView = nextView;

    // Update UI elements
    const cardBtn = document.getElementById('viewCardBtn');
    const tableBtn = document.getElementById('viewTableBtn');
    const switcherView = currentView === 'alert' ? lastNonAlertView : currentView;

    // Switcher Container data-active
    const switcherContainer = document.querySelector('.view-switcher');
    if (switcherContainer) {
        switcherContainer.setAttribute('data-active', switcherView);
    }

    const listContainer = document.getElementById('flightList');
    const alertContainer = document.getElementById('alertPoolContainer');
    const tableContainer = document.getElementById('flightTableContainer');

    // --- Glider Animation ---
    const activeBtn = switcherView === 'table' ? tableBtn : cardBtn;
    if (glider && activeBtn) {
        // Update button active state (visual only, glider handles bg)
        if (cardBtn) cardBtn.classList.toggle('active', switcherView === 'card');
        if (tableBtn) tableBtn.classList.toggle('active', switcherView === 'table');

        if (cardBtn) cardBtn.setAttribute('aria-pressed', switcherView === 'card' ? 'true' : 'false');
        if (tableBtn) tableBtn.setAttribute('aria-pressed', switcherView === 'table' ? 'true' : 'false');
    }

    // --- View Transition ---

    // Helper to transition
    const switchContent = () => {
        // 1. Destroy scrollers of unused views safely
        if (nextView !== 'card' && cardVirtualScroller) {
            cardVirtualScroller.destroy();
            cardVirtualScroller = null;
        }
        if (nextView !== 'alert' && window.alertVirtualScroller) {
            window.alertVirtualScroller.destroy();
            window.alertVirtualScroller = null;
        }
        if (nextView !== 'table') {
            destroyTableVirtualScroller();
        }

        // 2. Hide all view containers
        if (listContainer) { listContainer.style.display = 'none'; listContainer.setAttribute('aria-hidden', 'true'); }
        if (alertContainer) { alertContainer.style.display = 'none'; alertContainer.setAttribute('aria-hidden', 'true'); }
        if (tableContainer) { tableContainer.style.display = 'none'; tableContainer.setAttribute('aria-hidden', 'true'); }

        if (nextView === 'table') {
            // Enter Table View
            syncFlightMonitorLayout(nextView);

            tableContainer.style.display = 'flex';
            tableContainer.setAttribute('aria-hidden', 'false');

            scheduleViewRender(() => {
                renderFlightTable();
            }, 0);
        } else if (nextView === 'alert') {
            // Enter Alert View
            syncFlightMonitorLayout(nextView);

            alertContainer.style.display = 'flex';
            alertContainer.setAttribute('aria-hidden', 'false');

            scheduleViewRender(() => {
                renderAlertPoolView();
            }, 0);
        } else {
            // Enter Card View
            syncFlightMonitorLayout(nextView);

            listContainer.style.display = 'block';
            listContainer.setAttribute('aria-hidden', 'false');

            scheduleViewRender(() => {
                renderFlightList();
            }, 0);
        }
    };

    // Simple state switch for now - could wrap in requestAnimationFrame for smoother start
    requestAnimationFrame(() => {
        switchContent();
        updateAnomalyFloatingButton();
        announce(nextView === 'table' ? '已切换到表格视图' : (nextView === 'alert' ? '已切换到告警视图' : '已切换到卡片视图'));
    });
}

function getRenderableTableColumns() {
    return tableConfig.columnOrder.filter(colId => tableConfig.visibleColumns.includes(colId) && !!DEFAULT_COLUMNS[colId]);
}

function getTableHeaderRenderKey(columnIds) {
    const widthKey = columnIds.map(colId => `${colId}:${tableConfig.columnWidths[colId] || 'auto'}`).join('|');
    return `${columnIds.join('|')}::${widthKey}`;
}

function ensureTableHeader(table, columnIds, forceRebuild = false) {
    const thead = table.querySelector('thead');
    if (!thead) return;

    const headerKey = getTableHeaderRenderKey(columnIds);
    if (!forceRebuild && tableHeaderRenderKey === headerKey && thead.children.length > 0) {
        return;
    }

    tableHeaderRenderKey = headerKey;
    thead.innerHTML = '';

    const headerRow = document.createElement('tr');

    columnIds.forEach(colId => {
        const colDef = DEFAULT_COLUMNS[colId];
        if (!colDef) return;

        const th = document.createElement('th');
        th.dataset.columnId = colId;
        th.textContent = colDef.label;
        th.id = `col-${colId}`;
        th.scope = 'col';
        th.setAttribute('aria-sort', 'none');

        if (tableConfig.columnWidths[colId]) {
            th.style.width = tableConfig.columnWidths[colId] + 'px';
        }

        th.className = 'sortable';
        th.draggable = true;

        const resizer = document.createElement('div');
        resizer.className = 'resizer-handle';
        th.appendChild(resizer);

        if (colId === 'route') {
            th.style.cursor = 'pointer';
            th.title = '点击切换 显示代码/中文名称';
            th.onclick = function (e) {
                if (e.target.classList.contains('resizer') || e.target.closest('.column-menu-btn')) return;
                toggleRouteDisplayMode();
            };
        }

        headerRow.appendChild(th);
    });

    headerRow.addEventListener('contextmenu', (e) => {
        e.preventDefault();
        const menu = document.getElementById('headerContextMenu');
        if (menu) {
            menu.style.display = 'block';
            menu.style.left = e.pageX + 'px';
            menu.style.top = e.pageY + 'px';
        }
    });

    thead.appendChild(headerRow);
    setupTableInteraction();
}

function ensureTableVirtualScroller(wrapper, tbody) {
    if (tableVirtualScroller && tableVirtualScroller.wrapper === wrapper && tableVirtualScroller.tbody === tbody) {
        return;
    }

    destroyTableVirtualScroller();

    tableVirtualScroller = new TableVirtualScroller({
        wrapper,
        tbody,
        rowHeight: TABLE_VIRTUAL_ROW_HEIGHT,
        buffer: TABLE_VIRTUAL_BUFFER,
        renderRowHtml: createTableRowHtml
    });
    tableVirtualScroller.init();
}

function destroyTableVirtualScroller() {
    if (!tableVirtualScroller) return;
    tableVirtualScroller.destroy();
    tableVirtualScroller = null;
}

function createTableRowHtml(flight, columnIds = getRenderableTableColumns()) {
    const rowClasses = [];

    const rowStatusClass = getRowStatusClass(flight.status);
    if (rowStatusClass) {
        rowClasses.push(rowStatusClass);
    }

    if (isSameFlightId(selectedFlightId, flight.flight_id)) {
        rowClasses.push('row-selected');
    }

    const classAttr = rowClasses.length > 0 ? ` class="${rowClasses.join(' ')}"` : '';
    const flightIdAttr = escapeHtml(String(flight.flight_id || ''));
    const isSelected = isSameFlightId(selectedFlightId, flight.flight_id);
    const flightNo = escapeHtml(String(getPrimaryFlightNoV2(flight) || flightIdAttr));
    const statusText = escapeHtml(String(flight.status || '状态未知'));

    let cellsHtml = '';
    for (let i = 0; i < columnIds.length; i++) {
        const colId = columnIds[i];
        if (!DEFAULT_COLUMNS[colId]) continue;

        const renderer = FIELD_MAP[colId];
        const rendered = renderer ? renderer(flight) : escapeHtml(String(flight[colId] ?? '-'));
        cellsHtml += `<td data-field="${colId}" headers="col-${colId}" role="gridcell">${rendered}</td>`;
    }

    return `<tr data-flight-id="${flightIdAttr}"${classAttr} role="row" tabindex="0" aria-selected="${isSelected}" aria-label="${flightNo}，当前状态 ${statusText}">${cellsHtml}</tr>`;
}

function setupTableInteraction() {
    const table = document.getElementById('flightTable');
    if (!table) return;

    // Resize Logic
    const headers = table.querySelectorAll('th');
    headers.forEach(th => {
        const resizer = th.querySelector('.resizer-handle');
        if (resizer) {
            initResize(resizer, th);
        }

        // Drag Logic
        th.addEventListener('dragstart', handleDragStart);
        th.addEventListener('dragover', handleDragOver);
        th.addEventListener('drop', handleDrop);
        th.addEventListener('dragend', handleDragEnd);
    });
}

let dragSrcEl = null;

function handleDragStart(e) {
    dragSrcEl = this;
    e.dataTransfer.effectAllowed = 'move';
    e.dataTransfer.setData('text/html', this.innerHTML);
    this.classList.add('dragging');
}

function handleDragOver(e) {
    if (e.preventDefault) {
        e.preventDefault();
    }
    e.dataTransfer.dropEffect = 'move';

    // Add visual indicator
    if (this !== dragSrcEl) {
        this.classList.add('drag-over');
    }
    return false;
}

function handleDrop(e) {
    if (e.stopPropagation) {
        e.stopPropagation();
    }

    if (dragSrcEl !== this) {
        // Swap columns in tableConfig.columnOrder
        const srcId = dragSrcEl.dataset.columnId;
        const targetId = this.dataset.columnId;

        const srcIdx = tableConfig.columnOrder.indexOf(srcId);
        const targetIdx = tableConfig.columnOrder.indexOf(targetId);

        if (srcIdx > -1 && targetIdx > -1) {
            // Remove src
            tableConfig.columnOrder.splice(srcIdx, 1);
            // Insert at target (adjust index if moving right)
            tableConfig.columnOrder.splice(targetIdx, 0, srcId);

            saveTableConfig();
            renderFlightTable();
        }
    }
    return false;
}

function handleDragEnd(e) {
    this.classList.remove('dragging');
    const headers = document.querySelectorAll('th');
    headers.forEach(h => h.classList.remove('drag-over'));
}

function initResize(resizer, th) {
    let startX, startWidth;

    resizer.addEventListener('mousedown', function (e) {
        startX = e.pageX;
        startWidth = th.offsetWidth;

        resizer.classList.add('resizing');
        document.body.style.cursor = 'col-resize';

        const onMouseMove = function (e) {
            const width = startWidth + (e.pageX - startX);
            if (width > 50) { // Min width
                th.style.width = width + 'px';
                // Save width
                const colId = th.dataset.columnId;
                tableConfig.columnWidths[colId] = width;
            }
        };

        const onMouseUp = function () {
            document.removeEventListener('mousemove', onMouseMove);
            document.removeEventListener('mouseup', onMouseUp);
            resizer.classList.remove('resizing');
            document.body.style.cursor = '';
            saveTableConfig();
        };

        document.addEventListener('mousemove', onMouseMove);
        document.addEventListener('mouseup', onMouseUp);
        e.preventDefault(); // Prevent text selection
        e.stopPropagation();
    });
}

function saveTableConfig() {
    localStorage.setItem('flightMonitorTableConfig', JSON.stringify(tableConfig));
}

function loadTableConfig() {
    const saved = localStorage.getItem('flightMonitorTableConfig');
    if (saved) {
        try {
            const parsed = JSON.parse(saved);
            tableConfig = { ...tableConfig, ...parsed };
            // Ensure all DEFAULT_COLUMNS are in columnOrder (handling upgrades)
            Object.keys(DEFAULT_COLUMNS).forEach(colId => {
                if (!tableConfig.columnOrder.includes(colId)) {
                    tableConfig.columnOrder.push(colId);
                }
            });
        } catch (e) {
            console.error('Failed to load table config', e);
        }
    }
}

function setupColumnConfig() {
    const modal = document.getElementById('columnConfigModal');
    if (!modal) return;
    // const btn = document.getElementById('columnConfigBtn'); // Removed
    const ctxItem = document.getElementById('ctxConfigColumns');
    const closeBtn = document.getElementById('closeColumnConfig');
    const saveBtn = document.getElementById('saveColumnsBtn');
    const resetBtn = document.getElementById('resetColumnsBtn');

    if (ctxItem) ctxItem.onclick = () => {
        renderColumnConfigList();
        openManagedModal(modal, '#columnConfigList .column-checkbox');
        // Hide context menu
        const menu = document.getElementById('headerContextMenu');
        if (menu) menu.style.display = 'none';
    }

    if (closeBtn) closeBtn.onclick = () => closeManagedModal(modal);

    window.addEventListener('click', (e) => {
        if (e.target === modal) closeManagedModal(modal);
    });

    if (saveBtn) saveBtn.onclick = () => {
        // Gather checked columns and order
        const list = document.getElementById('columnConfigList');
        const items = list.querySelectorAll('.column-config-item');
        const newVisible = [];
        const newOrder = [];

        items.forEach(item => {
            const colId = item.dataset.columnId;
            const checkbox = item.querySelector('.column-checkbox');
            newOrder.push(colId);
            if (checkbox.checked) {
                newVisible.push(colId);
            }
        });

        tableConfig.visibleColumns = newVisible;
        tableConfig.columnOrder = newOrder;
        saveTableConfig();
        renderFlightTable();
        closeManagedModal(modal);
    };

    if (resetBtn) resetBtn.onclick = () => {
        // Reset to defaults
        tableConfig.visibleColumns = [...DEFAULT_VISIBLE_COLUMNS];
        tableConfig.columnOrder = [...DEFAULT_COLUMN_ORDER];
        renderColumnConfigList();
    };
}

let configDragSrc = null;

function handleConfigDragStart(e) {
    configDragSrc = this;
    e.dataTransfer.effectAllowed = 'move';
    this.classList.add('dragging');
}

function handleConfigDragOver(e) {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    return false;
}

function handleConfigDrop(e) {
    e.stopPropagation();
    if (configDragSrc !== this) {
        const list = document.getElementById('columnConfigList');
        // Simple swap logic or insert before
        // Here we just insert before
        let allItems = Array.from(list.children);
        let srcIdx = allItems.indexOf(configDragSrc);
        let targetIdx = allItems.indexOf(this);

        if (srcIdx < targetIdx) {
            this.after(configDragSrc);
        } else {
            this.before(configDragSrc);
        }
    }
    return false;
}

window.handleTimeFieldContextMenu = function (e, flightId, field, currentValue) {
    // console.log('Context menu triggered:', flightId, field, currentValue);
    e.preventDefault();
    e.stopPropagation();

    contextMenuState = {
        visible: true,
        flightId: flightId,
        field: field,
        currentValue: currentValue === 'undefined' || currentValue === 'null' ? '' : currentValue
    };

    const menu = document.getElementById('timeContextMenu');
    if (menu) {
        menu.style.display = 'flex'; // Use flex for column layout
        // Adjust position to keep within viewport
        let top = e.clientY;
        let left = e.clientX;

        menu.style.top = `${top}px`;
        menu.style.left = `${left}px`;
        menu.style.zIndex = '99999'; // Force high z-index
    } else {
        console.error('Context menu element not found!');
    }

    // Hide other menus
    const headerMenu = document.getElementById('headerContextMenu');
    if (headerMenu) headerMenu.style.display = 'none';
}

window.handleTimeFieldClick = function (e, flightId, field, currentValue) {
    if (e) {
        e.preventDefault();
        e.stopPropagation();
    }

    // Quick Punch: Submit current server time immediately
    // This allows single-click timestamping (e.g., "Clean Start", "Door Open")
    // Manual editing is available via Right Click ->Modify
    const nowIso = new Date().toISOString();
    updateFlightField(flightId, field, nowIso);
}

const ctxModifyBtn = document.getElementById('ctxModify');

if (ctxModifyBtn) {
    ctxModifyBtn.addEventListener('click', function () {
        if (!contextMenuState.visible) return;
        if (ctxModifyBtn.classList.contains('disabled')) {
            showToast('仅允许操作人修改该时间字段', 'error');
            return;
        }

        showTimeEditModal(contextMenuState.flightId, contextMenuState.field, contextMenuState.currentValue);

        document.getElementById('timeContextMenu').style.display = 'none';
        contextMenuState.visible = false;
    });
}

const ctxRevokeBtn = document.getElementById('ctxRevoke');

if (ctxRevokeBtn) {
    ctxRevokeBtn.addEventListener('click', function () {
        if (!contextMenuState.visible) return;
        if (ctxRevokeBtn.classList.contains('disabled')) {
            showToast('仅允许操作人撤销该时间字段', 'error');
            return;
        }

        if (confirm(`确定要撤销此时间吗？撤销后将变更为${EMPTY_DISPLAY_TEXT}。`)) {
            updateFlightField(contextMenuState.flightId, contextMenuState.field, null);
        }

        document.getElementById('timeContextMenu').style.display = 'none';
        contextMenuState.visible = false;
    });
}

let timeEditState = {
    flightId: null,
    field: null
};

const saveTimeBtn = document.getElementById('saveTimeBtn');

if (saveTimeBtn) {
    saveTimeBtn.addEventListener('click', function () {
        const input = document.getElementById('timeInput');
        if (!input) return;

        const val = input.value;

        if (!val) {
            showToast('请选择时间', 'error');
            return;
        }

        // Convert to ISO string for backend
        const date = new Date(val);
        const isoString = date.toISOString();

        updateFlightField(timeEditState.flightId, timeEditState.field, isoString);
        closeTimeModal();
    });
}

function toggleRouteDisplayMode() {
    routeDisplayMode = routeDisplayMode === 'code' ? 'name' : 'code';
    localStorage.setItem('routeDisplayMode', routeDisplayMode);

    // 保存当前滚动位置
    let scrollContainer, scrollTop;
    if (currentView === 'table') {
        scrollContainer = document.querySelector('.table-scroll-wrapper');
    } else {
        scrollContainer = flightListElement;
    }
    if (scrollContainer) {
        scrollTop = scrollContainer.scrollTop;
    }

    // Re-render based on current view
    if (currentView === 'table') {
        renderFlightTable();
    } else {
        renderFlightList();
    }

    // 恢复滚动位置
    if (scrollContainer && scrollTop !== undefined) {
        // 使用 requestAnimationFrame 确保在 DOM 更新后恢复滚动位置
        requestAnimationFrame(() => {
            scrollContainer.scrollTop = scrollTop;
        });
    }
}

(function () {
    if (window.__flightMonitorFetchWrapped) {
        return;
    }
    window.__flightMonitorFetchWrapped = true;

    const originalFetch = window.fetch.bind(window);
    window.fetch = async function (...args) {
        const fetchArgs = [...args];
        let fetchOptions = fetchArgs.length > 1 && fetchArgs[1] && typeof fetchArgs[1] === 'object'
            ? { ...fetchArgs[1] }
            : null;
        const suppressGlobalLoader = Boolean(fetchOptions && fetchOptions.suppressGlobalLoader);
        if (fetchOptions && Object.prototype.hasOwnProperty.call(fetchOptions, 'suppressGlobalLoader')) {
            delete fetchOptions.suppressGlobalLoader;
            fetchArgs[1] = fetchOptions;
        }

        if (suppressGlobalLoader) {
            return await originalFetch(...fetchArgs);
        }

        beginNetworkActivity();
        try {
            return await originalFetch(...fetchArgs);
        } finally {
            endNetworkActivity();
        }
    };
})();

function triggerMilestonePulse(flightNo, fieldName) {
    if (Notification.permission === 'granted') {
        new Notification('航班关键节点完成', {
            body: flightNo + ': ' + fieldName + ' 已完成',
            icon: '/favicon.ico'
        });
    } else if (Notification.permission !== 'denied') {
        Notification.requestPermission().then(permission => {
            if (permission === 'granted') {
                new Notification('航班关键节点完成', {
                    body: flightNo + ': ' + fieldName + ' 已完成',
                    icon: '/favicon.ico'
                });
            }
        });
    }

    let pulseDiv = document.getElementById('milestonePulseLayer');
    if (!pulseDiv) {
        pulseDiv = document.createElement('div');
        pulseDiv.id = 'milestonePulseLayer';
        pulseDiv.className = 'milestone-pulse';
        document.body.appendChild(pulseDiv);
    }

    pulseDiv.innerHTML = '<div class="pulse-content"><h1>' + flightNo + '</h1><p>' + fieldName + '已就绪</p></div>';
    pulseDiv.classList.add('active');

    setTimeout(() => {
        pulseDiv.classList.remove('active');
    }, 4000);
}
