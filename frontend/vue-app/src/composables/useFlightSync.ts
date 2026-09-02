import type {
  DispatchTimelineCache,
  DispatchTimelineCacheEntry,
  DispatchTimelineEvent,
  DispatchTimelineRequestOptions,
  DispatchTimelineUpdateOptions,
  DispatchTimelineWriteOptions,
  Flight,
} from './useFlightDataTypes';
import { DISPATCH_TIMELINE_FIELD_META } from './useFlightDataConstants';
import { findFlightById, flightIdentityKeys, normalizeFlightId, resolveDirectionalFlightId, syncFlightTimelineFieldsFromCache } from './useFlightField';

function getLatestDispatchTimelineEvent(
  cache: DispatchTimelineCache | null | undefined,
  flightId: string | number | null | undefined,
  milestoneCode: string,
): DispatchTimelineEvent | null {
  const entry = cache?.get(normalizeFlightId(flightId));
  if (!entry?.byMilestone) {
    return null;
  }
  return entry.byMilestone.get(String(milestoneCode || '').trim()) ?? null;
}

function buildDispatchTimelineClientActionId(
  flightId: string,
  milestoneCode: string,
  occurredAt: string,
): string {
  const seed = `${flightId}|${milestoneCode}|${occurredAt}`;
  const readable = `fm-timeline:${seed}`;
  if (readable.length <= 128) {
    return readable;
  }

  let hash = 0x811c9dc5;
  for (let index = 0; index < seed.length; index += 1) {
    hash ^= seed.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return `fm-timeline-${(hash >>> 0).toString(36)}`;
}

export function createDispatchTimelineCache(): DispatchTimelineCache {
  return new Map<string, DispatchTimelineCacheEntry>();
}

export function updateDispatchTimelineCache(
  flightId: string | number | null | undefined,
  items: DispatchTimelineEvent[] | null | undefined,
  options: DispatchTimelineUpdateOptions = {},
): DispatchTimelineCache {
  const normalizedId = normalizeFlightId(flightId);
  const list = Array.isArray(items) ? items : [];
  const byMilestone = new Map<string, DispatchTimelineEvent>();

  list.forEach((item) => {
    const code = String(item?.milestone_code ?? '').trim();
    if (!code) {
      return;
    }
    const existing = byMilestone.get(code);
    // Last-write-wins: prefer the most recently recorded event (created_at),
    // not the maximum business occurred_at, so earlier corrections win.
    const createdRaw = item?.created_at;
    const existingCreatedRaw = existing?.created_at;
    const createdAt = typeof createdRaw === 'string' || typeof createdRaw === 'number'
      ? new Date(createdRaw).getTime()
      : 0;
    const existingCreated = typeof existingCreatedRaw === 'string' || typeof existingCreatedRaw === 'number'
      ? new Date(existingCreatedRaw).getTime()
      : 0;
    const timelineId = String(item?.timeline_id ?? '');
    const existingId = String(existing?.timeline_id ?? '');
    if (
      !existing
      || createdAt > existingCreated
      || (createdAt === existingCreated && timelineId > existingId)
    ) {
      byMilestone.set(code, item);
    }
  });

  const nextCache = new Map(options.cache ?? createDispatchTimelineCache());
  // 同一监控行的进/出港时间线并入同一缓存视图：行级字段名（里程碑按方向命名）
  // 不会冲突，且行对象按自身身份键读取时两侧都能命中。
  const cacheKeys = new Set<string>([normalizedId]);
  const targets = [
    findFlightById(normalizedId, options.flights ?? [], []),
    findFlightById(normalizedId, options.originalFlights ?? [], []),
  ].filter((target): target is Flight => Boolean(target));
  targets.forEach((target) => {
    flightIdentityKeys(target).forEach((key) => cacheKeys.add(key));
  });
  cacheKeys.forEach((key) => {
    nextCache.set(key, { byMilestone, rawItems: list });
  });

  targets.forEach((target) => {
    syncFlightTimelineFieldsFromCache(target, nextCache);
    target._timesFormatted = false;
  });

  return nextCache;
}

export async function loadDispatchTimelineForFlight(
  flightId: string | number | null | undefined,
  options: DispatchTimelineRequestOptions,
): Promise<{ items: DispatchTimelineEvent[]; cache: DispatchTimelineCache }> {
  const normalizedId = normalizeFlightId(flightId);
  if (!normalizedId) {
    return { items: [], cache: options.cache ?? createDispatchTimelineCache() };
  }

  if (!options.force && options.cache?.has(normalizedId)) {
    return {
      items: options.cache.get(normalizedId)?.rawItems ?? [],
      cache: options.cache,
    };
  }

  const response = await options.authFetch(`${options.apiBase}/flights/${encodeURIComponent(normalizedId)}/dispatch-timeline`);
  if (!response.ok) {
    throw new Error(`获取时间线失败 (${response.status})`);
  }
  const payload = (await response.json()) as { data?: { items?: DispatchTimelineEvent[] } };
  const items = payload?.data?.items ?? [];
  return {
    items,
    cache: updateDispatchTimelineCache(normalizedId, items, options),
  };
}

export async function writeDispatchTimelineField(
  flightId: string | number | null | undefined,
  field: string,
  options: DispatchTimelineWriteOptions,
): Promise<{ items: DispatchTimelineEvent[]; cache: DispatchTimelineCache }> {
  const normalizedId = normalizeFlightId(flightId);
  if (!normalizedId) {
    throw new Error('航班标识缺失');
  }

  const milestoneCode = String(field || '').trim();
  const legType = DISPATCH_TIMELINE_FIELD_META[milestoneCode as keyof typeof DISPATCH_TIMELINE_FIELD_META]?.leg_type ?? null;

  // 拆表后时间线事件按方向航班存储：监控行 row_id 要先解析到字段所属方向
  // （DISPATCH_TIMELINE_FIELD_META.leg_type）的那班航班，row_id 本身（= 链 id）
  // 不是时间线的写入目标。行不在传入列表（旧载荷/未知 id）时按原 id 直写。
  const flight = findFlightById(normalizedId, options.flights ?? [], options.originalFlights ?? []);
  const targetId = flight ? resolveDirectionalFlightId(flight, milestoneCode) ?? normalizedId : normalizedId;

  if (!options.value) {
    const loaded = await loadDispatchTimelineForFlight(targetId, options);
    const existing = getLatestDispatchTimelineEvent(loaded.cache, targetId, milestoneCode);
    if (!existing?.timeline_id) {
      const rawItems = loaded.cache.get(targetId)?.rawItems ?? [];
      return {
        items: rawItems.filter((item) => item?.milestone_code !== milestoneCode),
        cache: updateDispatchTimelineCache(
          targetId,
          rawItems.filter((item) => item?.milestone_code !== milestoneCode),
          { ...options, cache: loaded.cache },
        ),
      };
    }

    const deleteResponse = await options.authFetch(
      `${options.apiBase}/flights/${encodeURIComponent(targetId)}/dispatch-timeline/events/${encodeURIComponent(String(existing.timeline_id))}`,
      { method: 'DELETE' },
    );
    if (!deleteResponse.ok) {
      const errText = await deleteResponse.text();
      throw new Error(errText || `撤销时间线失败 (${deleteResponse.status})`);
    }
  } else {
    const body = {
      milestone_code: milestoneCode,
      occurred_at: options.value,
      leg_type: legType,
      client_action_id: buildDispatchTimelineClientActionId(
        targetId,
        milestoneCode,
        options.value,
      ),
      source: 'flight_monitor_manual',
      payload: {},
    };
    const postResponse = await options.authFetch(
      `${options.apiBase}/flights/${encodeURIComponent(targetId)}/dispatch-timeline/events`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      },
    );
    if (!postResponse.ok) {
      let message = `写入时间线失败 (${postResponse.status})`;
      try {
        const err = (await postResponse.json()) as { detail?: string; message?: string };
        message = err?.detail || err?.message || message;
      } catch {
        const text = await postResponse.text();
        if (text) {
          message = text;
        }
      }
      throw new Error(message);
    }
  }

  return loadDispatchTimelineForFlight(targetId, {
    ...options,
    force: true,
  });
}
